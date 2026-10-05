//! Discover unfinished Paideia recoveries directly from their live proxy boxes.
use std::collections::HashSet;

use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_tx::ergo_box_utils::{extract_long_coll, get_register_coll_byte_hex};

use crate::constants::{
    PAIDEIA_PROXY_ADDRESS, PAIDEIA_PROXY_ERGO_TREE, STAKE_BOX_MAX_PAGES, STAKE_BOX_PAGE_SIZE,
};
use crate::pending::PendingBoxes;
use crate::RecoveryError;

pub struct PaideiaProxyCandidate {
    pub proxy_box: ErgoBox,
    pub recipient_ergo_tree: String,
    pub stake_key_id: String,
    pub reward_amount_raw: i64,
    pub confirmed: bool,
}

pub struct PaideiaProxyDiscovery {
    pub candidates: Vec<PaideiaProxyCandidate>,
    pub boxes_scanned: u64,
    pub pages_fetched: u64,
    pub hit_page_limit: bool,
}

/// An unfinished recovery is an unspent proxy addressed to the wallet in R5.
/// Querying these boxes finds old recoveries regardless of wallet history size,
/// excludes completed transactions, and includes newly submitted step-one boxes.
pub async fn discover_pending_paideia_proxies(
    node: &ergo_node_client::NodeClient,
    recipient_ergo_trees: &[String],
) -> Result<PaideiaProxyDiscovery, RecoveryError> {
    if node
        .capabilities()
        .await
        .is_some_and(|caps| caps.has_extra_index == Some(false))
    {
        return Err(RecoveryError::NodeError(
            "Automatic Paideia discovery requires a node with extraIndex enabled".into(),
        ));
    }
    let wanted: HashSet<&str> = recipient_ergo_trees.iter().map(String::as_str).collect();
    let pending = PendingBoxes::load(node).await?;
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut boxes_scanned = 0;
    let mut pages_fetched = 0;
    let mut hit_page_limit = true;

    for b in pending.outputs_for(PAIDEIA_PROXY_ERGO_TREE) {
        boxes_scanned += 1;
        if let Some(candidate) = owned_proxy(b, &wanted, false) {
            if seen.insert(b.box_id().to_string()) {
                candidates.push(candidate);
            }
        }
    }
    for page in 0..STAKE_BOX_MAX_PAGES {
        let boxes = node
            .confirmed_unspent_boxes_by_address(
                PAIDEIA_PROXY_ADDRESS,
                page * STAKE_BOX_PAGE_SIZE,
                STAKE_BOX_PAGE_SIZE,
            )
            .await
            .map_err(|e| RecoveryError::NodeError(e.to_string()))?;
        pages_fetched += 1;
        let page_len = boxes.len() as u64;
        boxes_scanned += page_len;
        for b in boxes.into_iter().filter(|b| pending.is_unspent(b)) {
            if let Some(candidate) = owned_proxy(&b, &wanted, true) {
                if seen.insert(b.box_id().to_string()) {
                    candidates.push(candidate);
                }
            }
        }
        if page_len < STAKE_BOX_PAGE_SIZE {
            hit_page_limit = false;
            break;
        }
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.proxy_box.creation_height));
    Ok(PaideiaProxyDiscovery {
        candidates,
        boxes_scanned,
        pages_fetched,
        hit_page_limit,
    })
}

fn owned_proxy(
    b: &ErgoBox,
    wanted: &HashSet<&str>,
    confirmed: bool,
) -> Option<PaideiaProxyCandidate> {
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    if hex::encode(b.ergo_tree.sigma_serialize_bytes().ok()?) != PAIDEIA_PROXY_ERGO_TREE {
        return None;
    }
    let tokens = b.tokens.as_ref()?;
    if tokens.len() != 1 || u64::from(tokens.get(0)?.amount) != 1 {
        return None;
    }
    let recipient_ergo_tree = get_register_coll_byte_hex(b, NonMandatoryRegisterId::R5).ok()?;
    if !wanted.contains(recipient_ergo_tree.as_str()) {
        return None;
    }
    let r4 = b
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()??;
    let amount = extract_long_coll(&r4).ok()?;
    if amount.len() != 1 || amount[0] <= 0 {
        return None;
    }
    Some(PaideiaProxyCandidate {
        proxy_box: b.clone(),
        recipient_ergo_tree,
        stake_key_id: hex::encode(tokens.get(0)?.token_id.as_ref()),
        reward_amount_raw: amount[0],
        confirmed,
    })
}
