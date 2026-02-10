//! Pool Discovery and Fetching
//!
//! Functions for discovering AMM pools from the Ergo node.

use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::mir::constant::Literal;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

use crate::constants::{
    fees, lp, pool_indices::n2t, pool_indices::t2t, pool_template_bytes, pool_templates,
    swap_template_bytes,
};
use crate::state::{
    AmmError, AmmPool, MempoolSwap, PendingSwapOrder, PoolType, SwapInput, SwapOrderType,
    TokenAmount,
};

/// Parse an N2T pool box into AmmPool
pub fn parse_n2t_pool(ergo_box: &ErgoBox) -> Result<AmmPool, AmmError> {
    let tokens = ergo_box.tokens.as_ref().ok_or(AmmError::InvalidLayout {
        expected: "tokens array",
        found: "no tokens",
    })?;

    if tokens.len() < 3 {
        return Err(AmmError::InvalidLayout {
            expected: "at least 3 tokens",
            found: "fewer than 3 tokens",
        });
    }

    // Extract pool NFT (index 0)
    let pool_nft = tokens.get(n2t::INDEX_NFT).ok_or(AmmError::InvalidLayout {
        expected: "pool NFT at index 0",
        found: "missing token",
    })?;
    let pool_id = hex::encode(pool_nft.token_id.as_ref());

    // Extract LP token (index 1)
    let lp_token = tokens.get(n2t::INDEX_LP).ok_or(AmmError::InvalidLayout {
        expected: "LP token at index 1",
        found: "missing token",
    })?;
    let lp_token_id = hex::encode(lp_token.token_id.as_ref());
    let lp_locked = u64::from(lp_token.amount);
    let lp_circulating = (lp::TOTAL_EMISSION as u64).saturating_sub(lp_locked);

    // Extract token Y (index 2)
    let token_y = tokens.get(n2t::INDEX_Y).ok_or(AmmError::InvalidLayout {
        expected: "token Y at index 2",
        found: "missing token",
    })?;
    let token_y_id = hex::encode(token_y.token_id.as_ref());
    let token_y_amount = u64::from(token_y.amount);

    // ERG reserves from box value
    let erg_reserves = u64::from(ergo_box.value);

    // Box ID
    let box_id = hex::encode(ergo_box.box_id().as_ref());

    // Read fee numerator from R4 register (Int)
    let fee_num = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()
        .flatten()
        .and_then(|c| match &c.v {
            Literal::Int(v) => Some(*v),
            _ => None,
        })
        .unwrap_or(fees::DEFAULT_FEE_NUM);

    Ok(AmmPool {
        pool_id,
        pool_type: PoolType::N2T,
        box_id,
        erg_reserves: Some(erg_reserves),
        token_x: None,
        token_y: TokenAmount {
            token_id: token_y_id,
            amount: token_y_amount,
            decimals: None,
            name: None,
        },
        lp_token_id,
        lp_circulating,
        fee_num,
        fee_denom: fees::DEFAULT_FEE_DENOM,
    })
}

/// Parse a T2T pool box into AmmPool
pub fn parse_t2t_pool(ergo_box: &ErgoBox) -> Result<AmmPool, AmmError> {
    let tokens = ergo_box.tokens.as_ref().ok_or(AmmError::InvalidLayout {
        expected: "tokens array",
        found: "no tokens",
    })?;

    if tokens.len() < 4 {
        return Err(AmmError::InvalidLayout {
            expected: "at least 4 tokens",
            found: "fewer than 4 tokens",
        });
    }

    // Extract pool NFT (index 0)
    let pool_nft = tokens.get(t2t::INDEX_NFT).ok_or(AmmError::InvalidLayout {
        expected: "pool NFT at index 0",
        found: "missing token",
    })?;
    let pool_id = hex::encode(pool_nft.token_id.as_ref());

    // Extract LP token (index 1)
    let lp_token = tokens.get(t2t::INDEX_LP).ok_or(AmmError::InvalidLayout {
        expected: "LP token at index 1",
        found: "missing token",
    })?;
    let lp_token_id = hex::encode(lp_token.token_id.as_ref());
    let lp_locked = u64::from(lp_token.amount);
    let lp_circulating = (lp::TOTAL_EMISSION as u64).saturating_sub(lp_locked);

    // Extract token X (index 2)
    let token_x = tokens.get(t2t::INDEX_X).ok_or(AmmError::InvalidLayout {
        expected: "token X at index 2",
        found: "missing token",
    })?;
    let token_x_id = hex::encode(token_x.token_id.as_ref());
    let token_x_amount = u64::from(token_x.amount);

    // Extract token Y (index 3)
    let token_y = tokens.get(t2t::INDEX_Y).ok_or(AmmError::InvalidLayout {
        expected: "token Y at index 3",
        found: "missing token",
    })?;
    let token_y_id = hex::encode(token_y.token_id.as_ref());
    let token_y_amount = u64::from(token_y.amount);

    // ERG from box value (for fees)
    let erg_reserves = u64::from(ergo_box.value);

    // Box ID
    let box_id = hex::encode(ergo_box.box_id().as_ref());

    // Read fee numerator from R4 register (Int)
    let fee_num = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()
        .flatten()
        .and_then(|c| match &c.v {
            Literal::Int(v) => Some(*v),
            _ => None,
        })
        .unwrap_or(fees::DEFAULT_FEE_NUM);

    Ok(AmmPool {
        pool_id,
        pool_type: PoolType::T2T,
        box_id,
        erg_reserves: Some(erg_reserves),
        token_x: Some(TokenAmount {
            token_id: token_x_id,
            amount: token_x_amount,
            decimals: None,
            name: None,
        }),
        token_y: TokenAmount {
            token_id: token_y_id,
            amount: token_y_amount,
            decimals: None,
            name: None,
        },
        lp_token_id,
        lp_circulating,
        fee_num,
        fee_denom: fees::DEFAULT_FEE_DENOM,
    })
}

/// Discover all N2T pools from the node
pub async fn discover_n2t_pools(
    node: &ergo_node_client::NodeClient,
) -> Result<Vec<AmmPool>, AmmError> {
    let boxes = node
        .inner()
        .unspent_boxes_by_ergo_tree(pool_templates::N2T_POOL_TEMPLATE, 0, 1000)
        .await
        .map_err(|e| AmmError::NodeError(e.to_string()))?;

    let mut pools = Vec::new();
    for ergo_box in &boxes {
        match parse_n2t_pool(ergo_box) {
            Ok(pool) => pools.push(pool),
            Err(e) => {
                tracing::warn!("Failed to parse N2T pool: {}", e);
            }
        }
    }

    if boxes.len() >= 1000 {
        tracing::warn!("N2T pool limit reached (1000). Some pools may be missing.");
    }
    tracing::info!("Discovered {} N2T pools", pools.len());
    Ok(pools)
}

/// Discover all T2T pools from the node
pub async fn discover_t2t_pools(
    node: &ergo_node_client::NodeClient,
) -> Result<Vec<AmmPool>, AmmError> {
    let boxes = node
        .inner()
        .unspent_boxes_by_ergo_tree(pool_templates::T2T_POOL_TEMPLATE, 0, 1000)
        .await
        .map_err(|e| AmmError::NodeError(e.to_string()))?;

    let mut pools = Vec::new();
    for ergo_box in &boxes {
        match parse_t2t_pool(ergo_box) {
            Ok(pool) => pools.push(pool),
            Err(e) => {
                tracing::warn!("Failed to parse T2T pool: {}", e);
            }
        }
    }

    if boxes.len() >= 1000 {
        tracing::warn!("T2T pool limit reached (1000). Some pools may be missing.");
    }
    tracing::info!("Discovered {} T2T pools", pools.len());
    Ok(pools)
}

/// Discover all pools (N2T and T2T) with resolved token names
pub async fn discover_pools(node: &ergo_node_client::NodeClient) -> Result<Vec<AmmPool>, AmmError> {
    let mut pools = discover_n2t_pools(node).await?;
    pools.extend(discover_t2t_pools(node).await?);

    // Collect unique token IDs that need name resolution
    let mut token_ids = std::collections::HashSet::new();
    for pool in &pools {
        token_ids.insert(pool.token_y.token_id.clone());
        if let Some(ref tx) = pool.token_x {
            token_ids.insert(tx.token_id.clone());
        }
    }

    // Resolve token info in parallel
    let mut token_info_map = std::collections::HashMap::new();
    for token_id in &token_ids {
        match node.get_token_info(token_id).await {
            Ok(info) => {
                token_info_map.insert(token_id.clone(), info);
            }
            Err(e) => {
                tracing::warn!("Failed to resolve token {}: {}", &token_id[..8], e);
            }
        }
    }

    // Populate pool token metadata
    for pool in &mut pools {
        if let Some(info) = token_info_map.get(&pool.token_y.token_id) {
            pool.token_y.name = info.name.clone();
            if let Some(d) = info.decimals {
                pool.token_y.decimals = Some(d as u8);
            }
        }
        if let Some(ref mut tx) = pool.token_x {
            if let Some(info) = token_info_map.get(&tx.token_id) {
                tx.name = info.name.clone();
                if let Some(d) = info.decimals {
                    tx.decimals = Some(d as u8);
                }
            }
        }
    }

    // Sort by ERG reserves descending (deepest liquidity first)
    pools.sort_by(|a, b| {
        b.erg_reserves
            .unwrap_or(0)
            .cmp(&a.erg_reserves.unwrap_or(0))
    });

    tracing::info!(
        "Resolved names for {} tokens across {} pools",
        token_info_map.len(),
        pools.len()
    );
    Ok(pools)
}

/// Match an ErgoTree against known swap order templates.
/// Returns the order type if the tree matches a known template.
pub fn match_swap_template(tree: &ErgoTree) -> Option<SwapOrderType> {
    let tmpl = match tree.template_bytes() {
        Ok(t) => t,
        Err(_) => return None,
    };
    if tmpl == *swap_template_bytes::N2T_SWAP_SELL {
        Some(SwapOrderType::N2tSwapSell)
    } else if tmpl == *swap_template_bytes::N2T_SWAP_BUY {
        Some(SwapOrderType::N2tSwapBuy)
    } else {
        None
    }
}

/// Extract key constants from a swap order ErgoTree.
/// Returns (pool_id_hex, redeemer_prop_bytes, base_amount, min_quote_amount).
pub fn parse_order_constants(
    tree: &ErgoTree,
    order_type: SwapOrderType,
) -> Result<(String, Vec<u8>, i64, i64), AmmError> {
    let (base_idx, pool_idx, redeemer_idx, min_quote_idx) = match order_type {
        SwapOrderType::N2tSwapSell => (3, 13, 14, 16),
        SwapOrderType::N2tSwapBuy => (1, 11, 12, 13),
    };
    let base_amount = extract_i64_constant(tree, base_idx, "BaseAmount")?;
    let pool_nft_bytes = extract_coll_byte_constant(tree, pool_idx, "PoolNFT")?;
    let redeemer_bytes = extract_coll_byte_constant(tree, redeemer_idx, "RedeemerPropBytes")?;
    let min_quote = extract_i64_constant(tree, min_quote_idx, "MinQuoteAmount")?;
    let pool_id = hex::encode(&pool_nft_bytes);
    Ok((pool_id, redeemer_bytes, base_amount, min_quote))
}

/// Check if the redeemer bytes match a user's ErgoTree hex.
pub fn redeemer_matches_user(redeemer_bytes: &[u8], user_ergo_tree_hex: &str) -> bool {
    match hex::decode(user_ergo_tree_hex) {
        Ok(user_bytes) => redeemer_bytes == user_bytes.as_slice(),
        Err(_) => false,
    }
}

fn extract_i64_constant(tree: &ErgoTree, idx: usize, name: &str) -> Result<i64, AmmError> {
    let constant = tree
        .get_constant(idx)
        .map_err(|e| {
            AmmError::TxBuildError(format!("Failed to get {} at index {}: {}", name, idx, e))
        })?
        .ok_or_else(|| {
            AmmError::TxBuildError(format!("No constant at index {} ({})", idx, name))
        })?;
    match &constant.v {
        ergo_lib::ergotree_ir::mir::constant::Literal::Long(v) => Ok(*v),
        other => Err(AmmError::TxBuildError(format!(
            "Expected Long at index {} ({}), got {:?}",
            idx, name, other
        ))),
    }
}

fn extract_coll_byte_constant(
    tree: &ErgoTree,
    idx: usize,
    name: &str,
) -> Result<Vec<u8>, AmmError> {
    let constant = tree
        .get_constant(idx)
        .map_err(|e| {
            AmmError::TxBuildError(format!("Failed to get {} at index {}: {}", name, idx, e))
        })?
        .ok_or_else(|| {
            AmmError::TxBuildError(format!("No constant at index {} ({})", idx, name))
        })?;
    match &constant.v {
        ergo_lib::ergotree_ir::mir::constant::Literal::Coll(coll) => match coll {
            ergo_lib::ergotree_ir::mir::value::CollKind::NativeColl(
                ergo_lib::ergotree_ir::mir::value::NativeColl::CollByte(bytes),
            ) => Ok(bytes.iter().map(|b| *b as u8).collect()),
            _ => Err(AmmError::TxBuildError(format!(
                "Expected Coll[Byte] at index {} ({}), got non-byte collection",
                idx, name
            ))),
        },
        other => Err(AmmError::TxBuildError(format!(
            "Expected Coll at index {} ({}), got {:?}",
            idx, name, other
        ))),
    }
}

// NOTE: get_pool_by_id and get_pools_by_token stubs were removed.
// Callers use discover_pools() and filter client-side instead.
// When needed, implement by querying boxes by token ID (pool NFT).

/// Find pending swap orders for an address by scanning recent transactions.
///
/// Scans the user's recent transactions, identifies swap order outputs via
/// template matching, checks that the redeemer matches the user, and verifies
/// the box is still unspent (i.e. still pending).
pub async fn find_pending_orders(
    node: &ergo_node_client::NodeClient,
    user_address: &str,
    user_ergo_tree_hex: &str,
    tx_limit: u64,
) -> Result<Vec<PendingSwapOrder>, AmmError> {
    let txs = node
        .get_recent_transactions(user_address, tx_limit)
        .await
        .map_err(|e| AmmError::NodeError(format!("Failed to fetch transactions: {}", e)))?;

    let mut orders = Vec::new();

    for tx_json in &txs {
        let tx_id = tx_json["id"].as_str().unwrap_or_default().to_string();
        let outputs = match tx_json["outputs"].as_array() {
            Some(o) => o,
            None => continue,
        };

        for output in outputs {
            let ergo_tree_hex = match output["ergoTree"].as_str() {
                Some(s) => s,
                None => continue,
            };
            let tree_bytes = match hex::decode(ergo_tree_hex) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let tree = match ErgoTree::sigma_parse_bytes(&tree_bytes) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let order_type = match match_swap_template(&tree) {
                Some(ot) => ot,
                None => continue,
            };
            let (pool_id, redeemer_bytes, base_amount, min_quote) =
                match parse_order_constants(&tree, order_type) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("Failed to parse order constants: {}", e);
                        continue;
                    }
                };
            if !redeemer_matches_user(&redeemer_bytes, user_ergo_tree_hex) {
                continue;
            }
            let box_id = match output["boxId"].as_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            let is_unspent = node.inner().box_from_id_with_pool(&box_id).await.is_ok();
            if !is_unspent {
                continue;
            }
            let value_nano_erg = output["value"].as_u64().unwrap_or(0);
            let created_height = output["creationHeight"].as_u64().unwrap_or(0) as u32;
            let input = match order_type {
                SwapOrderType::N2tSwapSell => SwapInput::Erg {
                    amount: base_amount as u64,
                },
                SwapOrderType::N2tSwapBuy => {
                    let token_id = output["assets"]
                        .as_array()
                        .and_then(|assets| assets.first())
                        .and_then(|a| a["tokenId"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    SwapInput::Token {
                        token_id,
                        amount: base_amount as u64,
                    }
                }
            };
            let redeemer_address = user_address.to_string();

            orders.push(PendingSwapOrder {
                box_id,
                tx_id: tx_id.clone(),
                pool_id,
                input,
                min_output: min_quote as u64,
                redeemer_address,
                created_height,
                value_nano_erg,
                order_type,
            });
        }
    }

    tracing::info!(
        "Found {} pending swap orders for {}",
        orders.len(),
        &user_address[..8]
    );
    Ok(orders)
}

/// Check if an ErgoTree hex matches one of the known pool templates.
fn is_pool_ergo_tree(ergo_tree_hex: &str) -> bool {
    let tree_bytes = match hex::decode(ergo_tree_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let tree = match ErgoTree::sigma_parse_bytes(&tree_bytes) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let tmpl = match tree.template_bytes() {
        Ok(t) => t,
        Err(_) => return false,
    };
    tmpl == *pool_template_bytes::N2T_POOL || tmpl == *pool_template_bytes::T2T_POOL
}

/// Find direct swap transactions in the mempool for a given user.
///
/// Scans unconfirmed transactions involving the user's address and identifies
/// direct swaps: transactions where outputs[0] is a pool box and another output
/// goes to the user's address.
pub async fn find_mempool_swaps(
    node: &ergo_node_client::NodeClient,
    user_address: &str,
    user_ergo_tree_hex: &str,
) -> Result<Vec<MempoolSwap>, AmmError> {
    let txs = match node.get_unconfirmed_by_address(user_address).await {
        Ok(txs) => txs,
        Err(e) => {
            tracing::warn!("Mempool query failed, skipping direct swap scan: {}", e);
            return Ok(Vec::new());
        }
    };

    let mut swaps = Vec::new();

    for tx in &txs {
        let tx_id = match tx["id"].as_str() {
            Some(id) => id.to_string(),
            None => continue,
        };
        let outputs = match tx["outputs"].as_array() {
            Some(o) if !o.is_empty() => o,
            _ => continue,
        };

        // Check if outputs[0] is a pool box
        let first_ergo_tree = match outputs[0]["ergoTree"].as_str() {
            Some(s) => s,
            None => continue,
        };
        if !is_pool_ergo_tree(first_ergo_tree) {
            continue;
        }

        // Extract pool NFT ID from outputs[0].assets[0].tokenId
        let pool_id = match outputs[0]["assets"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|a| a["tokenId"].as_str())
        {
            Some(id) => id.to_string(),
            None => continue,
        };

        // Find the user's output box (ergoTree matches user)
        let user_output = outputs[1..]
            .iter()
            .find(|o| o["ergoTree"].as_str() == Some(user_ergo_tree_hex));
        let user_output = match user_output {
            Some(o) => o,
            None => continue,
        };

        let receiving_erg = user_output["value"].as_u64().unwrap_or(0);
        let receiving_tokens: Vec<(String, u64)> = user_output["assets"]
            .as_array()
            .map(|assets| {
                assets
                    .iter()
                    .filter_map(|a| {
                        let tid = a["tokenId"].as_str()?.to_string();
                        let amt = a["amount"].as_u64()?;
                        Some((tid, amt))
                    })
                    .collect()
            })
            .unwrap_or_default();

        swaps.push(MempoolSwap {
            tx_id,
            pool_id,
            receiving_erg,
            receiving_tokens,
        });
    }

    tracing::info!(
        "Found {} direct swap(s) in mempool for {}",
        swaps.len(),
        &user_address[..user_address.len().min(8)]
    );
    Ok(swaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::swap_templates;

    fn parse_tree(hex_str: &str) -> ErgoTree {
        let bytes = hex::decode(hex_str).unwrap();
        ErgoTree::sigma_parse_bytes(&bytes).unwrap()
    }

    #[test]
    fn test_template_matching_sell_positive() {
        let tree = parse_tree(swap_templates::N2T_SWAP_SELL_TEMPLATE);
        assert_eq!(match_swap_template(&tree), Some(SwapOrderType::N2tSwapSell));
    }

    #[test]
    fn test_template_matching_buy_positive() {
        let tree = parse_tree(swap_templates::N2T_SWAP_BUY_TEMPLATE);
        assert_eq!(match_swap_template(&tree), Some(SwapOrderType::N2tSwapBuy));
    }

    #[test]
    fn test_template_matching_p2pk_returns_none() {
        let p2pk_hex = "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let tree = parse_tree(p2pk_hex);
        assert_eq!(match_swap_template(&tree), None);
    }

    #[test]
    fn test_template_matching_pool_returns_none() {
        let tree = parse_tree(crate::constants::pool_templates::N2T_POOL_TEMPLATE);
        assert_eq!(match_swap_template(&tree), None);
    }

    #[test]
    fn test_extract_constants_sell() {
        let tree = parse_tree(swap_templates::N2T_SWAP_SELL_TEMPLATE);
        let result = parse_order_constants(&tree, SwapOrderType::N2tSwapSell);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        let (pool_id, redeemer_bytes, _base, _min) = result.unwrap();
        assert!(!pool_id.is_empty());
        assert!(!redeemer_bytes.is_empty());
    }

    #[test]
    fn test_extract_constants_buy() {
        let tree = parse_tree(swap_templates::N2T_SWAP_BUY_TEMPLATE);
        let result = parse_order_constants(&tree, SwapOrderType::N2tSwapBuy);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        let (pool_id, redeemer_bytes, _base, _min) = result.unwrap();
        assert!(!pool_id.is_empty());
        assert!(!redeemer_bytes.is_empty());
    }

    #[test]
    fn test_redeemer_matching() {
        let user_tree = "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let redeemer_bytes = hex::decode(user_tree).unwrap();
        assert!(redeemer_matches_user(&redeemer_bytes, user_tree));
        let wrong = "0008cdaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(!redeemer_matches_user(&redeemer_bytes, wrong));
    }
}
