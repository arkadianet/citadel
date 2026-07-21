use std::collections::{HashMap, HashSet};

use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_tx::ergo_box_utils::{extract_long_coll, get_register_coll_byte_hex};

use crate::constants::{StakeProtocolConfig, PROTOCOLS, STAKE_BOX_MAX_PAGES, STAKE_BOX_PAGE_SIZE};
use crate::state::{RecoverableStake, RecoveryError, RecoveryScan, StakeStateSnapshot};

/// Fetch the singleton StakeStateBox for `cfg` and parse its R4. Paginates the
/// state address rather than assuming the box is among the first page: the
/// address is a P2S anyone can send arbitrary boxes to (dust/spam), so the real
/// singleton is not guaranteed to be near the front of an old, multi-year address.
pub async fn fetch_stake_state(
    node: &ergo_node_client::NodeClient,
    cfg: &StakeProtocolConfig,
) -> Result<(ErgoBox, StakeStateSnapshot), RecoveryError> {
    for page in 0..STAKE_BOX_MAX_PAGES {
        let boxes = node
            .unspent_boxes_by_address(
                &cfg.stake_state_address.to_string(),
                page * STAKE_BOX_PAGE_SIZE,
                STAKE_BOX_PAGE_SIZE,
            )
            .await
            .map_err(|e| RecoveryError::NodeError(e.to_string()))?;

        let page_len = boxes.len() as u64;
        if let Some(ergo_box) = boxes.into_iter().find(|b| {
            b.tokens
                .as_ref()
                .and_then(|t| t.get(0))
                .map(|first| hex::encode(first.token_id.as_ref()) == cfg.stake_state_nft)
                .unwrap_or(false)
        }) {
            let snapshot = parse_stake_state(&ergo_box, cfg)?;
            return Ok((ergo_box, snapshot));
        }
        if page_len < STAKE_BOX_PAGE_SIZE {
            break;
        }
    }

    Err(RecoveryError::StateBoxNotFound(cfg.name.to_string()))
}

fn parse_stake_state(
    ergo_box: &ErgoBox,
    cfg: &StakeProtocolConfig,
) -> Result<StakeStateSnapshot, RecoveryError> {
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
        .find(|t| hex::encode(t.token_id.as_ref()) == cfg.stake_token)
        .map(|t| u64::from(t.amount) as i64)
        .ok_or_else(|| RecoveryError::InvalidStateBox("no stake token".into()))?;

    Ok(StakeStateSnapshot {
        protocol: cfg.name.to_string(),
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

/// Fetch the unspent StakeBox whose R5 equals `stake_key_id` on protocol `cfg`.
pub async fn fetch_stake_box_by_key(
    node: &ergo_node_client::NodeClient,
    cfg: &StakeProtocolConfig,
    stake_key_id: &str,
) -> Result<ErgoBox, RecoveryError> {
    for page in 0..STAKE_BOX_MAX_PAGES {
        let boxes = node
            .unspent_boxes_by_address(
                &cfg.stake_box_address.to_string(),
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

/// Auto-detect which registered protocol holds an unspent StakeBox for `stake_key_id`.
/// Tries each protocol's stake P2S in turn; returns the owning config plus the box.
///
/// An R5 match alone isn't proof the box belongs to `cfg`: a P2S address accepts
/// arbitrary boxes from anyone, so a foreign/decoy box could coincidentally carry
/// this key in R5. Each candidate is validated against `cfg`'s expected
/// register/token layout via [`parse_stake_box`] before being accepted; a
/// candidate that fails validation is skipped rather than returned, so a real
/// match at a later protocol is never shadowed by an earlier false one.
pub async fn find_stake_box_by_key(
    node: &ergo_node_client::NodeClient,
    stake_key_id: &str,
) -> Result<(&'static StakeProtocolConfig, ErgoBox), RecoveryError> {
    for cfg in PROTOCOLS {
        match fetch_stake_box_by_key(node, cfg, stake_key_id).await {
            Ok(ergo_box) => {
                if parse_stake_box(&ergo_box, cfg).is_ok() {
                    return Ok((cfg, ergo_box));
                }
            }
            Err(RecoveryError::StakeBoxNotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Err(RecoveryError::StakeBoxNotFound(stake_key_id.to_string()))
}

fn r5_matches(ergo_box: &ErgoBox, stake_key_id: &str) -> bool {
    get_register_coll_byte_hex(ergo_box, NonMandatoryRegisterId::R5)
        .map(|hex| hex == stake_key_id)
        .unwrap_or(false)
}

/// Scan every registered protocol's stake P2S, matching R5 values against
/// `candidate_token_ids`. Returns each reachable protocol's live state plus every
/// candidate that had a live StakeBox (tagged with its owning protocol).
pub async fn discover_recoverable_stakes(
    node: &ergo_node_client::NodeClient,
    candidate_token_ids: &[String],
) -> Result<RecoveryScan, RecoveryError> {
    let wanted: HashSet<&str> = candidate_token_ids.iter().map(|s| s.as_str()).collect();
    let candidates_checked = wanted.len() as u64;

    let mut states: Vec<StakeStateSnapshot> = Vec::new();
    let mut found: HashMap<String, RecoverableStake> = HashMap::new();
    let mut boxes_scanned: u64 = 0;
    let mut pages_fetched: u64 = 0;
    let mut hit_page_limit = false;

    for cfg in PROTOCOLS {
        // A protocol whose state box can't be found is treated as inactive: record
        // nothing and move on rather than failing the whole scan.
        let state_snapshot = match fetch_stake_state(node, cfg).await {
            Ok((_box, snap)) => snap,
            Err(RecoveryError::StateBoxNotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        states.push(state_snapshot);

        if wanted.is_empty() {
            continue;
        }

        let mut protocol_hit_limit = true;
        'outer: for page in 0..STAKE_BOX_MAX_PAGES {
            let boxes = node
                .unspent_boxes_by_address(
                    &cfg.stake_box_address.to_string(),
                    page * STAKE_BOX_PAGE_SIZE,
                    STAKE_BOX_PAGE_SIZE,
                )
                .await
                .map_err(|e| RecoveryError::NodeError(e.to_string()))?;

            let page_len = boxes.len() as u64;
            pages_fetched += 1;
            boxes_scanned += page_len;

            for ergo_box in boxes {
                match parse_stake_box(&ergo_box, cfg) {
                    Ok(stake) if wanted.contains(stake.stake_key_id.as_str()) => {
                        found.insert(stake.stake_key_id.clone(), stake);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!("Skipping unparseable stake box: {}", e);
                    }
                }
            }
            if page_len < STAKE_BOX_PAGE_SIZE {
                protocol_hit_limit = false;
                break 'outer;
            }
        }
        hit_page_limit |= protocol_hit_limit;
    }

    let mut stakes: Vec<RecoverableStake> = found.into_values().collect();
    stakes.sort_by_key(|s| std::cmp::Reverse(s.reward_amount_raw));

    tracing::info!(
        candidates = candidates_checked,
        scanned = boxes_scanned,
        pages = pages_fetched,
        matched = stakes.len(),
        hit_page_limit,
        "Stake recovery scan complete"
    );

    Ok(RecoveryScan {
        states,
        stakes,
        candidates_checked,
        boxes_scanned,
        pages_fetched,
        hit_page_limit,
    })
}

pub fn parse_stake_box(
    ergo_box: &ErgoBox,
    cfg: &StakeProtocolConfig,
) -> Result<RecoverableStake, RecoveryError> {
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
    let reward_amount_raw = tokens
        .iter()
        .find(|t| hex::encode(t.token_id.as_ref()) == cfg.reward_token)
        .map(|t| u64::from(t.amount) as i64)
        .ok_or_else(|| {
            RecoveryError::InvalidStakeBox(format!("no {} reward token", cfg.reward_token_name))
        })?;

    let reward_amount_display = format_reward(reward_amount_raw, cfg.reward_decimals);

    Ok(RecoverableStake {
        protocol: cfg.name.to_string(),
        reward_token_name: cfg.reward_token_name.to_string(),
        stake_key_id,
        stake_box_id: hex::encode(ergo_box.box_id().as_ref()),
        stake_box_value_nano: u64::from(ergo_box.value) as i64,
        reward_amount_raw,
        checkpoint,
        stake_time_ms,
        reward_amount_display,
    })
}

/// Format a raw reward amount with `decimals` fractional digits, e.g. `614.68`.
fn format_reward(raw: i64, decimals: u32) -> String {
    if decimals == 0 {
        return raw.to_string();
    }
    let divisor = 10_i64.pow(decimals);
    let whole = raw / divisor;
    let frac = (raw.abs() % divisor) as u64;
    format!("{}.{:0width$}", whole, frac, width = decimals as usize)
}
