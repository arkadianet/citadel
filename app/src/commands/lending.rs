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
/// process to execute the borrow.
#[tauri::command]
pub async fn build_borrow_tx(
    _state: State<'_, AppState>,
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

    // Find matching collateral option
    // For token pools, collateral is ERG ("native"); for ERG pool, it's one of the supported tokens
    let collateral_options = if pool_config.liquidation_threshold > 0 {
        vec![lending::CollateralOption {
            token_id: "native".to_string(),
            token_name: "ERG".to_string(),
            liquidation_threshold: pool_config.liquidation_threshold,
            liquidation_penalty: 0,
            dex_nft: pool_config.collateral_dex_nft.map(|s| s.to_string()),
        }]
    } else {
        vec![]
    };

    // For ERG pool, the collateral is a token (user-specified)
    // For token pools, the collateral is ERG ("native")
    let collateral_config = if pool_config.is_erg_pool {
        // ERG pool: user provides token collateral to borrow ERG
        // Use a synthetic collateral option with the user's specified token
        lending::CollateralOption {
            token_id: request.collateral_token.clone(),
            token_name: "Collateral".to_string(),
            liquidation_threshold: 1250, // Default threshold for ERG pool borrowing
            liquidation_penalty: 0,
            dex_nft: pool_config.collateral_dex_nft.map(|s| s.to_string()),
        }
    } else {
        collateral_options.into_iter().next().ok_or_else(|| {
            format!(
                "Pool '{}' does not support borrowing (no collateral options)",
                request.pool_id
            )
        })?
    };

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
    _state: State<'_, AppState>,
    request: RefundBuildRequest,
) -> Result<LendingBuildResponse, String> {
    // The first UTXO should be the proxy box to refund
    if request.user_utxos.is_empty() {
        return Err("Proxy box data required in user_utxos for refund".to_string());
    }

    let proxy_utxo = &request.user_utxos[0];

    // Validate it matches the proxy_box_id
    let box_id = proxy_utxo["boxId"]
        .as_str()
        .or_else(|| proxy_utxo["box_id"].as_str())
        .ok_or_else(|| "Proxy box missing boxId".to_string())?;

    if box_id != request.proxy_box_id {
        return Err(format!(
            "First UTXO boxId '{}' does not match proxy_box_id '{}'",
            box_id, request.proxy_box_id
        ));
    }

    // Extract proxy box fields
    let tx_id = proxy_utxo["transactionId"]
        .as_str()
        .or_else(|| proxy_utxo["transaction_id"].as_str())
        .ok_or_else(|| "Proxy box missing transactionId".to_string())?
        .to_string();

    let index = proxy_utxo["index"]
        .as_u64()
        .ok_or_else(|| "Proxy box missing index".to_string())? as u16;

    let value: i64 = match &proxy_utxo["value"] {
        serde_json::Value::String(s) => s
            .parse()
            .map_err(|_| format!("Invalid proxy box value: {}", s))?,
        serde_json::Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| "Invalid proxy box value".to_string())?,
        _ => return Err("Proxy box missing value".to_string()),
    };

    let ergo_tree = proxy_utxo["ergoTree"]
        .as_str()
        .or_else(|| proxy_utxo["ergo_tree"].as_str())
        .ok_or_else(|| "Proxy box missing ergoTree".to_string())?
        .to_string();

    let creation_height = proxy_utxo["creationHeight"]
        .as_i64()
        .or_else(|| proxy_utxo["creation_height"].as_i64())
        .ok_or_else(|| "Proxy box missing creationHeight".to_string())?
        as i32;

    // Parse assets
    let assets: Vec<(String, i64)> = proxy_utxo["assets"]
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

    // Extract R4 (user's ErgoTree) and R6 (refund height) from registers
    let registers = proxy_utxo["additionalRegisters"]
        .as_object()
        .or_else(|| proxy_utxo["additional_registers"].as_object())
        .ok_or_else(|| "Proxy box missing additionalRegisters".to_string())?;

    let r4_encoded = registers
        .get("R4")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Proxy box missing R4 (user ErgoTree)".to_string())?;

    let r6_encoded = registers
        .get("R6")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Proxy box missing R6 (refund height)".to_string())?;

    // Decode R4: Coll[Byte] containing user's ErgoTree
    let r4_user_tree =
        decode_sigma_byte_array(r4_encoded).map_err(|e| format!("Invalid R4 encoding: {}", e))?;

    // Decode R6: Long containing refund height
    let r6_refund_height =
        decode_sigma_long(r6_encoded).map_err(|e| format!("Invalid R6 encoding: {}", e))?;

    // Build ProxyBoxData
    let proxy_box = lending_tx_builder::ProxyBoxData {
        box_id: request.proxy_box_id.clone(),
        tx_id,
        index,
        value,
        ergo_tree,
        assets,
        creation_height,
        r4_user_tree,
        r6_refund_height,
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

/// Decode a Sigma Long from register hex string
/// Format: 05 (type tag) + zigzag-encoded VLQ value
fn decode_sigma_long(hex_str: &str) -> Result<i64, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Invalid hex: {}", e))?;

    if bytes.is_empty() || bytes[0] != 0x05 {
        return Err("Not a Long type (expected 0x05 prefix)".to_string());
    }

    // Decode VLQ
    let mut idx = 1;
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

    // Decode zigzag to signed value
    let value = if zigzag & 1 == 0 {
        (zigzag >> 1) as i64
    } else {
        -((zigzag >> 1) as i64) - 1
    };

    Ok(value)
}
