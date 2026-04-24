use std::collections::{HashMap, HashSet};

use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_tx::ergo_box_utils::{extract_long_coll, get_register_coll_byte_hex};

use crate::constants::{
    ERGOPAD_DECIMALS, ERGOPAD_TOKEN, STAKE_BOX_ADDRESS, STAKE_BOX_MAX_PAGES,
    STAKE_BOX_PAGE_SIZE, STAKE_STATE_ADDRESS, STAKE_STATE_NFT, STAKE_TOKEN,
};
use crate::state::{RecoverableStake, RecoveryError, RecoveryScan, StakeStateSnapshot};

/// Fetch the singleton StakeStateBox and parse its R4.
pub async fn fetch_stake_state(
    node: &ergo_node_client::NodeClient,
) -> Result<(ErgoBox, StakeStateSnapshot), RecoveryError> {
    let boxes = node
        .inner()
        .unspent_boxes_by_address(&STAKE_STATE_ADDRESS.to_string(), 0, 16)
        .await
        .map_err(|e| RecoveryError::NodeError(e.to_string()))?;

    let ergo_box = boxes
        .into_iter()
        .find(|b| {
            b.tokens
                .as_ref()
                .and_then(|t| t.get(0))
                .map(|first| hex::encode(first.token_id.as_ref()) == STAKE_STATE_NFT)
                .unwrap_or(false)
        })
        .ok_or(RecoveryError::StateBoxNotFound)?;

    let snapshot = parse_stake_state(&ergo_box)?;
    Ok((ergo_box, snapshot))
}

fn parse_stake_state(ergo_box: &ErgoBox) -> Result<StakeStateSnapshot, RecoveryError> {
    let r4 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()
        .flatten()
        .ok_or_else(|| RecoveryError::InvalidStateBox("missing R4".into()))?;
    let longs = extract_long_coll(&r4)
        .map_err(|e| RecoveryError::InvalidStateBox(format!("R4 decode: {e}")))?;
    if longs.len() < 5 {
        return Err(RecoveryError::InvalidStateBox(format!(
            "R4 has {} entries, expected 5",
            longs.len()
        )));
    }

    let tokens = ergo_box
        .tokens
        .as_ref()
        .ok_or_else(|| RecoveryError::InvalidStateBox("no tokens".into()))?;
    let stake_token_amount = tokens
        .iter()
        .find(|t| hex::encode(t.token_id.as_ref()) == STAKE_TOKEN)
        .map(|t| u64::from(t.amount) as i64)
        .ok_or_else(|| RecoveryError::InvalidStateBox("no stake token".into()))?;

    Ok(StakeStateSnapshot {
        state_box_id: hex::encode(ergo_box.box_id().as_ref()),
        state_box_value_nano: u64::from(ergo_box.value) as i64,
        total_staked_raw: longs[0],
        checkpoint: longs[1],
        num_stakers: longs[2],
        last_checkpoint_ts: longs[3],
        cycle_duration_ms: longs[4],
        stake_token_amount,
    })
}

/// Fetch the unspent StakeBox whose R5 equals `stake_key_id`.
pub async fn fetch_stake_box_by_key(
    node: &ergo_node_client::NodeClient,
    stake_key_id: &str,
) -> Result<ErgoBox, RecoveryError> {
    for page in 0..STAKE_BOX_MAX_PAGES {
        let boxes = node
            .inner()
            .unspent_boxes_by_address(
                &STAKE_BOX_ADDRESS.to_string(),
                page * STAKE_BOX_PAGE_SIZE,
                STAKE_BOX_PAGE_SIZE,
            )
            .await
            .map_err(|e| RecoveryError::NodeError(e.to_string()))?;

        let page_len = boxes.len() as u64;
        for ergo_box in boxes {
            if r5_matches(&ergo_box, stake_key_id) {
                return Ok(ergo_box);
            }
        }
        if page_len < STAKE_BOX_PAGE_SIZE {
            break;
        }
    }

    Err(RecoveryError::StakeBoxNotFound(stake_key_id.to_string()))
}

fn r5_matches(ergo_box: &ErgoBox, stake_key_id: &str) -> bool {
    get_register_coll_byte_hex(ergo_box, NonMandatoryRegisterId::R5)
        .map(|hex| hex == stake_key_id)
        .unwrap_or(false)
}

/// Scan the v1 stake P2S, matching R5 values against `candidate_token_ids`.
/// Returns a snapshot of the live state plus every candidate that had a live StakeBox.
pub async fn discover_recoverable_stakes(
    node: &ergo_node_client::NodeClient,
    candidate_token_ids: &[String],
) -> Result<RecoveryScan, RecoveryError> {
    let (_state_box, state_snapshot) = fetch_stake_state(node).await?;

    let wanted: HashSet<&str> = candidate_token_ids.iter().map(|s| s.as_str()).collect();
    let candidates_checked = wanted.len() as u64;
    if wanted.is_empty() {
        return Ok(RecoveryScan {
            state: state_snapshot,
            stakes: vec![],
            candidates_checked: 0,
            boxes_scanned: 0,
            pages_fetched: 0,
            hit_page_limit: false,
        });
    }

    let mut found: HashMap<String, RecoverableStake> = HashMap::new();
    let mut boxes_scanned: u64 = 0;
    let mut pages_fetched: u64 = 0;
    let mut hit_page_limit = true;

    'outer: for page in 0..STAKE_BOX_MAX_PAGES {
        let boxes = node
            .inner()
            .unspent_boxes_by_address(
                &STAKE_BOX_ADDRESS.to_string(),
                page * STAKE_BOX_PAGE_SIZE,
                STAKE_BOX_PAGE_SIZE,
            )
            .await
            .map_err(|e| RecoveryError::NodeError(e.to_string()))?;

        let page_len = boxes.len() as u64;
        pages_fetched += 1;
        boxes_scanned += page_len;

        for ergo_box in boxes {
            match parse_stake_box(&ergo_box) {
                Ok(stake) if wanted.contains(stake.stake_key_id.as_str()) => {
                    found.insert(stake.stake_key_id.clone(), stake);
                    if found.len() == wanted.len() {
                        hit_page_limit = false;
                        break 'outer;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!("Skipping unparseable stake box: {}", e);
                }
            }
        }
        if page_len < STAKE_BOX_PAGE_SIZE {
            hit_page_limit = false;
            break;
        }
    }

    let mut stakes: Vec<RecoverableStake> = found.into_values().collect();
    stakes.sort_by(|a, b| b.ergopad_amount_raw.cmp(&a.ergopad_amount_raw));

    tracing::info!(
        candidates = candidates_checked,
        scanned = boxes_scanned,
        pages = pages_fetched,
        matched = stakes.len(),
        hit_page_limit,
        "Ergopad recovery scan complete"
    );

    Ok(RecoveryScan {
        state: state_snapshot,
        stakes,
        candidates_checked,
        boxes_scanned,
        pages_fetched,
        hit_page_limit,
    })
}

pub fn parse_stake_box(ergo_box: &ErgoBox) -> Result<RecoverableStake, RecoveryError> {
    let stake_key_id = get_register_coll_byte_hex(ergo_box, NonMandatoryRegisterId::R5)
        .map_err(|e| RecoveryError::InvalidStakeBox(format!("R5: {e}")))?;

    let r4 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()
        .flatten()
        .ok_or_else(|| RecoveryError::InvalidStakeBox("missing R4".into()))?;
    let longs = extract_long_coll(&r4)
        .map_err(|e| RecoveryError::InvalidStakeBox(format!("R4 decode: {e}")))?;
    if longs.len() < 2 {
        return Err(RecoveryError::InvalidStakeBox(format!(
            "R4 has {} entries, expected 2",
            longs.len()
        )));
    }
    let checkpoint = longs[0];
    let stake_time_ms = longs[1];

    let tokens = ergo_box
        .tokens
        .as_ref()
        .ok_or_else(|| RecoveryError::InvalidStakeBox("no tokens".into()))?;
    let ergopad_amount_raw = tokens
        .iter()
        .find(|t| hex::encode(t.token_id.as_ref()) == ERGOPAD_TOKEN)
        .map(|t| u64::from(t.amount) as i64)
        .ok_or_else(|| RecoveryError::InvalidStakeBox("no ERGOPAD".into()))?;

    let divisor = 10_i64.pow(ERGOPAD_DECIMALS);
    let whole = ergopad_amount_raw / divisor;
    let frac = (ergopad_amount_raw.abs() % divisor) as u64;
    let ergopad_amount_display = format!("{}.{:02}", whole, frac);

    Ok(RecoverableStake {
        stake_key_id,
        stake_box_id: hex::encode(ergo_box.box_id().as_ref()),
        stake_box_value_nano: u64::from(ergo_box.value) as i64,
        ergopad_amount_raw,
        checkpoint,
        stake_time_ms,
        ergopad_amount_display,
    })
}
