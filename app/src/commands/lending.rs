use citadel_api::routes::lending::{
    BorrowBuildRequest, BorrowPositionInfo, CollateralOptionInfo, LendBuildRequest,
    LendPositionInfo, LendingBuildResponse, LendingTxSummary, MarketsResponse, PoolInfo,
    PositionsResponse, RefundBuildRequest, RepayBuildRequest, WithdrawBuildRequest,
};
use citadel_api::AppState;
use lending::{
    constants as lending_constants, fetch_all_markets, tx_builder as lending_tx_builder, PoolState,
};
use tauri::State;

/// Convert PoolState to PoolInfo for API response
fn pool_state_to_info(state: &PoolState) -> PoolInfo {
    PoolInfo {
        pool_id: state.pool_id.clone(),
        name: state.name.clone(),
        symbol: state.symbol.clone(),
        decimals: state.decimals,
        is_erg_pool: state.is_erg_pool,
        total_supplied: state.total_supplied.to_string(),
        total_borrowed: state.total_borrowed.to_string(),
        available_liquidity: state.available_liquidity.to_string(),
        utilization_pct: state.utilization_pct,
        supply_apy: state.supply_apy,
        borrow_apy: state.borrow_apy,
        pool_box_id: state.pool_box_id.clone(),
        collateral_options: state
            .collateral_options
            .iter()
            .map(|opt| CollateralOptionInfo {
                token_id: opt.token_id.clone(),
                token_name: opt.token_name.clone(),
                liquidation_threshold: opt.liquidation_threshold,
                liquidation_penalty: opt.liquidation_penalty,
                dex_nft: opt.dex_nft.clone(),
            })
            .collect(),
    }
}

/// Convert health factor to UI status string for color coding
fn health_factor_to_status(health_factor: f64) -> String {
    if health_factor >= lending_constants::health::HEALTHY_THRESHOLD {
        "green".to_string()
    } else if health_factor >= lending_constants::health::WARNING_THRESHOLD {
        "amber".to_string()
    } else {
        "red".to_string()
    }
}

/// Get all lending markets with pool metrics
#[tauri::command]
pub async fn get_lending_markets(state: State<'_, AppState>) -> Result<MarketsResponse, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let markets_response = fetch_all_markets(&client, &capabilities, None)
        .await
        .map_err(|e| e.to_string())?;

    let pools: Vec<PoolInfo> = markets_response
        .pools
        .iter()
        .map(pool_state_to_info)
        .collect();

    Ok(MarketsResponse {
        pools,
        block_height: markets_response.block_height,
    })
}

/// Get user lending positions for an address
#[tauri::command]
pub async fn get_lending_positions(
    state: State<'_, AppState>,
    address: String,
) -> Result<PositionsResponse, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let markets_response = fetch_all_markets(&client, &capabilities, Some(&address))
        .await
        .map_err(|e| e.to_string())?;

    // Extract lend positions from pools
    let lend_positions: Vec<LendPositionInfo> = markets_response
        .pools
        .iter()
        .filter_map(|pool| {
            pool.user_lend_position
                .as_ref()
                .map(|pos| LendPositionInfo {
                    pool_id: pool.pool_id.clone(),
                    pool_name: pool.name.clone(),
                    lp_tokens: pos.lp_tokens.to_string(),
                    underlying_value: pos.underlying_value.to_string(),
                    unrealized_profit: pos.unrealized_profit.to_string(),
                })
        })
        .collect();

    // Extract borrow positions from pools
    let borrow_positions: Vec<BorrowPositionInfo> = markets_response
        .pools
        .iter()
        .flat_map(|pool| {
            pool.user_borrow_positions.iter().map(|pos| {
                let health_status = health_factor_to_status(pos.health_factor);
                BorrowPositionInfo {
                    pool_id: pool.pool_id.clone(),
                    pool_name: pool.name.clone(),
                    collateral_box_id: pos.collateral_box_id.clone(),
                    collateral_token: pos.collateral_token.clone(),
                    collateral_name: pos.collateral_name.clone(),
                    collateral_amount: pos.collateral_amount.to_string(),
                    borrowed_amount: pos.borrowed_amount.to_string(),
                    total_owed: pos.total_owed.to_string(),
                    health_factor: pos.health_factor,
                    health_status,
                }
            })
        })
        .collect();

    Ok(PositionsResponse {
        address,
        lend_positions,
        borrow_positions,
        block_height: markets_response.block_height,
    })
}

/// Parse user UTXOs from JSON to tx_builder's UserUtxo format
fn parse_lending_utxos(
    utxos_json: Vec<serde_json::Value>,
) -> Result<Vec<lending_tx_builder::UserUtxo>, String> {
    if utxos_json.is_empty() {
        return Err("No user UTXOs provided".to_string());
    }

    utxos_json
        .into_iter()
        .enumerate()
        .map(|(idx, v)| parse_single_lending_utxo(v, idx))
        .collect()
}

/// Parse a single UTXO from JSON
fn parse_single_lending_utxo(
    v: serde_json::Value,
    idx: usize,
) -> Result<lending_tx_builder::UserUtxo, String> {
    let box_id = v["boxId"]
        .as_str()
        .or_else(|| v["box_id"].as_str())
        .ok_or_else(|| format!("UTXO {} missing boxId", idx))?
        .to_string();

    let tx_id = v["transactionId"]
        .as_str()
        .or_else(|| v["transaction_id"].as_str())
        .ok_or_else(|| format!("UTXO {} missing transactionId", idx))?
        .to_string();

    let index = v["index"]
        .as_u64()
        .ok_or_else(|| format!("UTXO {} missing index", idx))? as u16;

    let value: i64 = match &v["value"] {
        serde_json::Value::String(s) => s
            .parse()
            .map_err(|_| format!("UTXO {} has invalid value: {}", idx, s))?,
        serde_json::Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| format!("UTXO {} has invalid value", idx))?,
        _ => return Err(format!("UTXO {} missing value", idx)),
    };

    let ergo_tree = v["ergoTree"]
        .as_str()
        .or_else(|| v["ergo_tree"].as_str())
        .ok_or_else(|| format!("UTXO {} missing ergoTree", idx))?
        .to_string();

    let creation_height = v["creationHeight"]
        .as_i64()
        .or_else(|| v["creation_height"].as_i64())
        .ok_or_else(|| format!("UTXO {} missing creationHeight", idx))?
        as i32;

    // Parse assets (optional)
    let assets: Vec<(String, i64)> = v["assets"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let token_id = a["tokenId"]
                        .as_str()
                        .or_else(|| a["token_id"].as_str())?
                        .to_string();
                    let amount: i64 = match &a["amount"] {
                        serde_json::Value::String(s) => s.parse().ok()?,
                        serde_json::Value::Number(n) => n.as_i64()?,
                        _ => return None,
                    };
                    Some((token_id, amount))
                })
                .collect()
        })
        .unwrap_or_default();

    // Parse registers (optional)
    let registers: std::collections::HashMap<String, String> = v["additionalRegisters"]
        .as_object()
        .or_else(|| v["additional_registers"].as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    Ok(lending_tx_builder::UserUtxo {
        box_id,
        tx_id,
        index,
        value,
        ergo_tree,
        assets,
        creation_height,
        registers,
    })
}

/// Convert BuildResponse to LendingBuildResponse
fn lending_build_response_to_api(
    response: lending_tx_builder::BuildResponse,
) -> Result<LendingBuildResponse, String> {
    let unsigned_tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx)
        .map_err(|e| format!("Failed to parse unsigned_tx: {}", e))?;

    Ok(LendingBuildResponse {
        unsigned_tx,
        summary: LendingTxSummary {
            action: response.summary.action,
            pool_id: response.summary.pool_id,
            pool_name: response.summary.pool_name,
            amount_in: response.summary.amount_in,
            amount_out_estimate: response.summary.amount_out_estimate,
            tx_fee_nano: response.fee_nano.to_string(),
            refund_height: response.summary.refund_height,
            service_fee: response.summary.service_fee_display,
            service_fee_nano: response.summary.service_fee_raw.to_string(),
            total_to_send: response.summary.total_to_send_display,
        },
    })
}

/// Build lend (deposit) transaction
#[tauri::command]
pub async fn build_lend_tx(
    _state: State<'_, AppState>,
    request: LendBuildRequest,
) -> Result<LendingBuildResponse, String> {
    // Validate amount is non-zero
    if request.amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    // Validate pool_id exists
    let pool_config = lending_constants::get_pool(&request.pool_id)
        .ok_or_else(|| format!("Pool '{}' not found", request.pool_id))?;

    // Parse user UTXOs
    let user_utxos = parse_lending_utxos(request.user_utxos)?;

    // Build the lend request
    let lend_request = lending_tx_builder::LendRequest {
        pool_id: request.pool_id.clone(),
        amount: request.amount,
        user_address: request.user_address,
        user_utxos,
        min_lp_tokens: None,
        slippage_bps: request.slippage_bps,
    };

    // Build the transaction
    let result =
        lending_tx_builder::build_lend_tx(lend_request, pool_config, request.current_height)
            .map_err(|e| e.to_string())?;

    lending_build_response_to_api(result)
}

/// Build withdraw (redeem LP tokens) transaction
#[tauri::command]
pub async fn build_withdraw_tx(
    _state: State<'_, AppState>,
    request: WithdrawBuildRequest,
) -> Result<LendingBuildResponse, String> {
    // Validate amount is non-zero
    if request.lp_amount == 0 {
        return Err("LP amount must be greater than 0".to_string());
    }

    // Validate pool_id exists
    let pool_config = lending_constants::get_pool(&request.pool_id)
        .ok_or_else(|| format!("Pool '{}' not found", request.pool_id))?;

    // Parse user UTXOs
    let user_utxos = parse_lending_utxos(request.user_utxos)?;

    // Build the withdraw request
    let withdraw_request = lending_tx_builder::WithdrawRequest {
        pool_id: request.pool_id.clone(),
        lp_amount: request.lp_amount,
        user_address: request.user_address,
        user_utxos,
        min_output: None,
    };

    // Build the transaction
    let result = lending_tx_builder::build_withdraw_tx(
        withdraw_request,
        pool_config,
        request.current_height,
    )
    .map_err(|e| e.to_string())?;

    lending_build_response_to_api(result)
}

/// Build borrow transaction
///
/// Creates a proxy box with collateral tokens and registers that Duckpools bots
/// process to execute the borrow. Fetches liquidation threshold/penalty from
/// the on-chain parameter box to ensure they match what the pool contract validates.
#[tauri::command]
pub async fn build_borrow_tx(
    state: State<'_, AppState>,
    request: BorrowBuildRequest,
) -> Result<LendingBuildResponse, String> {
    // Validate amounts
    if request.borrow_amount == 0 {
        return Err("Borrow amount must be greater than 0".to_string());
    }
    if request.collateral_amount == 0 {
        return Err("Collateral amount must be greater than 0".to_string());
    }

    // Validate pool_id exists
    let pool_config = lending_constants::get_pool(&request.pool_id)
        .ok_or_else(|| format!("Pool '{}' not found", request.pool_id))?;

    // Get node client to fetch on-chain parameter box
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    // Fetch real threshold/penalty from on-chain parameter box
    let collateral_options = lending::fetch_collateral_from_parameter_box(
        &client,
        &capabilities,
        pool_config,
    )
    .await
    .map_err(|e| e.to_string())?;

    if collateral_options.is_empty() {
        return Err(format!(
            "Pool '{}' does not support borrowing (no collateral options in parameter box)",
            request.pool_id
        ));
    }

    // Find the matching collateral option for the user's chosen token
    // Token pools: collateral_token = "native" (ERG)
    // ERG pool: collateral_token = token_id of the collateral token
    let collateral_config = collateral_options
        .iter()
        .find(|opt| opt.token_id == request.collateral_token)
        .ok_or_else(|| {
            format!(
                "Collateral token '{}' not found in parameter box for pool '{}'.\nAvailable: {}",
                request.collateral_token,
                request.pool_id,
                collateral_options
                    .iter()
                    .map(|o| format!("{} ({})", o.token_name, o.token_id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?
        .clone();

    // Parse user UTXOs
    let user_utxos = parse_lending_utxos(request.user_utxos)?;

    // Build the borrow request
    let borrow_request = lending_tx_builder::BorrowRequest {
        pool_id: request.pool_id.clone(),
        collateral_token: request.collateral_token,
        collateral_amount: request.collateral_amount,
        borrow_amount: request.borrow_amount,
        user_address: request.user_address,
        user_utxos,
    };

    // Build the transaction
    let result = lending_tx_builder::build_borrow_tx(
        borrow_request,
        pool_config,
        &collateral_config,
        request.current_height,
    )
    .map_err(|e| e.to_string())?;

    lending_build_response_to_api(result)
}

/// Build repay transaction
#[tauri::command]
pub async fn build_repay_tx(
    _state: State<'_, AppState>,
    request: RepayBuildRequest,
) -> Result<LendingBuildResponse, String> {
    // Validate amount is non-zero
    if request.repay_amount == 0 {
        return Err("Repay amount must be greater than 0".to_string());
    }

    // Validate pool_id exists
    let pool_config = lending_constants::get_pool(&request.pool_id)
        .ok_or_else(|| format!("Pool '{}' not found", request.pool_id))?;

    // Parse user UTXOs
    let user_utxos = parse_lending_utxos(request.user_utxos)?;

    // Build the repay request
    let repay_request = lending_tx_builder::RepayRequest {
        pool_id: request.pool_id.clone(),
        collateral_box_id: request.collateral_box_id,
        repay_amount: request.repay_amount,
        total_owed: request.total_owed,
        user_address: request.user_address,
        user_utxos,
    };

    // Build the transaction
    let result =
        lending_tx_builder::build_repay_tx(repay_request, pool_config, request.current_height)
            .map_err(|e| e.to_string())?;

    lending_build_response_to_api(result)
}

/// Build refund transaction for stuck proxy box
#[tauri::command]
pub async fn build_refund_tx(
    state: State<'_, AppState>,
    request: RefundBuildRequest,
) -> Result<LendingBuildResponse, String> {
    // Fetch the proxy box directly from the node by its box ID.
    // The proxy box lives at a contract address, not the user's wallet,
    // so it won't appear in get_user_utxos.
    let client = state.node_client().await.ok_or("Node not connected")?;
    let proxy_eip12 = client
        .get_eip12_box_by_id(&request.proxy_box_id)
        .await
        .map_err(|e| format!("Failed to fetch proxy box {}: {}", request.proxy_box_id, e))?;

    let proxy_utxo = serde_json::to_value(&proxy_eip12)
        .map_err(|e| format!("Failed to serialize proxy box: {}", e))?;

    // Extract proxy box fields from EIP-12 format
    let value: i64 = proxy_eip12
        .value
        .parse()
        .map_err(|_| format!("Invalid proxy box value: {}", proxy_eip12.value))?;

    // Parse assets
    let assets: Vec<(String, i64)> = proxy_eip12
        .assets
        .iter()
        .filter_map(|a| {
            let amount: i64 = a.amount.parse().ok()?;
            Some((a.token_id.clone(), amount))
        })
        .collect();

    // Extract user's ErgoTree and refund height from registers.
    // Different proxy types store user ErgoTree in different registers:
    // - Lend/Withdraw/Borrow: R4 = Coll[Byte] (user ErgoTree)
    // - Repay/PartialRepay:   R5 = Coll[Byte] (user ErgoTree), R4 = Long
    let registers = proxy_utxo["additionalRegisters"]
        .as_object()
        .or_else(|| proxy_utxo["additional_registers"].as_object())
        .ok_or_else(|| "Proxy box missing additionalRegisters".to_string())?;

    let r4_encoded = registers
        .get("R4")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Proxy box missing R4".to_string())?;

    let r6_encoded = registers
        .get("R6")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Proxy box missing R6 (refund height)".to_string())?;

    // Try R4 as Coll[Byte] first (lend/withdraw/borrow proxies).
    // If that fails (repay proxies store a Long in R4), fall back to R5.
    let user_ergo_tree = match decode_sigma_byte_array(r4_encoded) {
        Ok(tree) => tree,
        Err(_) => {
            // Repay/PartialRepay proxy: user ErgoTree is in R5
            let r5_encoded = registers
                .get("R5")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Proxy box missing user ErgoTree in R4 or R5".to_string())?;
            decode_sigma_byte_array(r5_encoded)
                .map_err(|e| format!("Invalid R5 encoding: {}", e))?
        }
    };

    // Decode R6: Int or Long containing refund height
    let r6_refund_height =
        decode_sigma_int_or_long(r6_encoded).map_err(|e| format!("Invalid R6 encoding: {}", e))?;

    // Build ProxyBoxData — include all registers for correct box ID verification
    let proxy_box = lending_tx_builder::ProxyBoxData {
        box_id: request.proxy_box_id.clone(),
        tx_id: proxy_eip12.transaction_id,
        index: proxy_eip12.index,
        value,
        ergo_tree: proxy_eip12.ergo_tree,
        assets,
        creation_height: proxy_eip12.creation_height,
        user_ergo_tree,
        r6_refund_height,
        additional_registers: proxy_eip12.additional_registers,
    };

    // Build the refund transaction
    let result = lending_tx_builder::build_refund_tx(proxy_box, request.current_height)
        .map_err(|e| e.to_string())?;

    // Convert RefundResponse to LendingBuildResponse
    let unsigned_tx: serde_json::Value = serde_json::from_str(&result.unsigned_tx)
        .map_err(|e| format!("Failed to parse unsigned_tx: {}", e))?;

    Ok(LendingBuildResponse {
        unsigned_tx,
        summary: LendingTxSummary {
            action: "refund".to_string(),
            pool_id: "".to_string(),
            pool_name: "Proxy Refund".to_string(),
            amount_in: request.proxy_box_id,
            amount_out_estimate: Some("Refunded to wallet".to_string()),
            tx_fee_nano: result.fee_nano.to_string(),
            refund_height: result.refundable_after_height as i32,
            service_fee: String::new(),
            service_fee_nano: "0".to_string(),
            total_to_send: String::new(),
        },
    })
}

/// Check if a box is a valid proxy box and return its info
///
/// Fetches the box from the node, parses registers, and returns
/// value, refund height, and whether it looks like a proxy box.
#[tauri::command]
pub async fn check_proxy_box(
    state: State<'_, AppState>,
    box_id: String,
) -> Result<serde_json::Value, String> {
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;
    use ergo_tx::ergo_box_utils;

    let client = state.node_client().await.ok_or("Node not connected")?;

    // Fetch the box by ID (works without extraIndex)
    let ergo_box_id = citadel_core::BoxId::new(&box_id);
    let ergo_box = ergo_node_client::queries::get_box_by_id(client.inner(), &ergo_box_id)
        .await
        .map_err(|e| e.to_string())?;

    let value_nano = ergo_box.value.as_i64();

    // Try to parse R6 as refund height (Int or Long)
    let refund_height: i64 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R6)
        .ok()
        .flatten()
        .and_then(|c| {
            ergo_box_utils::extract_int(&c)
                .map(|v| v as i64)
                .or_else(|_| ergo_box_utils::extract_long(&c))
                .ok()
        })
        .unwrap_or(0);

    // A proxy box should have R4 (user ErgoTree) and R6 (refund height)
    let has_r4 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()
        .flatten()
        .is_some();
    let is_proxy_box = has_r4 && refund_height > 0;

    Ok(serde_json::json!({
        "value_nano": value_nano,
        "refund_height": refund_height,
        "is_proxy_box": is_proxy_box,
    }))
}

/// Discover stuck proxy boxes belonging to the user across all Duckpools proxy contracts.
#[tauri::command]
pub async fn discover_stuck_proxies(
    state: State<'_, AppState>,
    user_address: String,
) -> Result<serde_json::Value, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let current_height = client.current_height().await.map_err(|e| e.to_string())? as u32;

    let stuck_boxes =
        lending::fetch::discover_stuck_proxy_boxes(&client, &user_address, current_height)
            .await
            .map_err(|e| e.to_string())?;

    serde_json::to_value(&stuck_boxes).map_err(|e| e.to_string())
}

/// Get DEX pool price for collateral calculation.
///
/// Fetches the Spectrum DEX pool box by its NFT and returns the
/// ERG/token price ratio, used to auto-calculate borrow collateral amounts.
#[tauri::command]
pub async fn get_dex_price(
    state: State<'_, AppState>,
    dex_nft: String,
) -> Result<serde_json::Value, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let token_id = citadel_core::TokenId::new(&dex_nft);
    let dex_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        &capabilities,
        &token_id,
    )
    .await
    .map_err(|e| format!("DEX box not found for NFT {}: {}", dex_nft, e))?;

    // ERG reserves from box value
    let erg_reserves = dex_box.value.as_i64() as f64;

    // Token reserves from tokens[2]
    let tokens = dex_box.tokens.as_ref().ok_or("DEX box has no tokens")?;
    if tokens.len() < 3 {
        return Err("DEX box has fewer than 3 tokens".to_string());
    }
    let token_reserves = u64::from(tokens.as_slice()[2].amount) as f64;

    if erg_reserves <= 0.0 || token_reserves <= 0.0 {
        return Err("DEX pool has zero reserves".to_string());
    }

    let erg_per_token = erg_reserves / token_reserves;
    let token_per_erg = token_reserves / erg_reserves;

    serde_json::to_value(serde_json::json!({
        "erg_per_token": erg_per_token,
        "token_per_erg": token_per_erg,
        "erg_reserves": erg_reserves as u64,
        "token_reserves": token_reserves as u64,
    }))
    .map_err(|e| e.to_string())
}

// =============================================================================
// Sigma Decoding Helpers (for refund transactions)
// =============================================================================

/// Decode a Sigma Coll[Byte] from register hex string
/// Format: 0e (type tag) + VLQ length + data bytes
fn decode_sigma_byte_array(hex_str: &str) -> Result<String, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Invalid hex: {}", e))?;

    if bytes.is_empty() || bytes[0] != 0x0e {
        return Err("Not a Coll[Byte] type (expected 0x0e prefix)".to_string());
    }

    // Decode VLQ length
    let mut idx = 1;
    let mut length: usize = 0;
    let mut shift = 0;

    while idx < bytes.len() {
        if shift >= 64 {
            return Err("VLQ value too large".to_string());
        }
        let byte = bytes[idx];
        length |= ((byte & 0x7f) as usize) << shift;
        idx += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    if idx + length > bytes.len() {
        return Err(format!(
            "Invalid length: expected {} bytes, only {} available",
            length,
            bytes.len() - idx
        ));
    }

    // Extract the data bytes and return as hex
    Ok(hex::encode(&bytes[idx..idx + length]))
}

/// Decode a zigzag-VLQ encoded integer from sigma-serialized bytes.
/// Used for both Int (0x04) and Long (0x05) types.
fn decode_zigzag_vlq(bytes: &[u8], start: usize) -> Result<i64, String> {
    let mut idx = start;
    let mut zigzag: u64 = 0;
    let mut shift = 0;

    while idx < bytes.len() {
        if shift >= 64 {
            return Err("VLQ value too large".to_string());
        }
        let byte = bytes[idx];
        zigzag |= ((byte & 0x7f) as u64) << shift;
        idx += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    let value = if zigzag & 1 == 0 {
        (zigzag >> 1) as i64
    } else {
        -((zigzag >> 1) as i64) - 1
    };

    Ok(value)
}

/// Decode a Sigma Int or Long from register hex string
/// Handles both type 0x04 (Int) and 0x05 (Long)
fn decode_sigma_int_or_long(hex_str: &str) -> Result<i64, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Invalid hex: {}", e))?;
    if bytes.is_empty() {
        return Err("Empty register value".to_string());
    }
    match bytes[0] {
        0x04 => decode_zigzag_vlq(&bytes, 1),
        0x05 => decode_zigzag_vlq(&bytes, 1),
        other => Err(format!("Expected Int (0x04) or Long (0x05), got 0x{:02x}", other)),
    }
}
