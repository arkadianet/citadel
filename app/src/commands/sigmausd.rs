use citadel_api::dto::{
    MintBuildRequest, MintBuildResponse, MintPreviewRequest, MintPreviewResponse,
    OraclePriceResponse, SigmaUsdBuildRequest, SigmaUsdBuildResponse, SigmaUsdPreviewRequest,
    SigmaUsdPreviewResponse, TxSummaryDto,
};
use citadel_api::AppState;
use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use sigmausd::{
    cost_to_mint_sigrsv, cost_to_mint_sigusd, erg_from_redeem_sigrsv, erg_from_redeem_sigusd,
    fetch::fetch_tx_context,
    fetch_oracle_price, fetch_sigmausd_state,
    tx_builder::{
        build_mint_sigrsv_tx, build_mint_sigusd_tx, build_redeem_sigrsv_tx, build_redeem_sigusd_tx,
        validate_mint_sigrsv, validate_mint_sigusd, validate_redeem_sigrsv, validate_redeem_sigusd,
        MintSigRsvRequest, MintSigUsdRequest, RedeemSigRsvRequest, RedeemSigUsdRequest, TxContext,
    },
    NftIds, SigmaUsdState,
};
use tauri::State;

/// Get SigmaUSD protocol state
#[tauri::command]
pub async fn get_sigmausd_state(state: State<'_, AppState>) -> Result<SigmaUsdState, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| e.to_string())
}

/// Get ERG/USD oracle price
#[tauri::command]
pub async fn get_oracle_price(state: State<'_, AppState>) -> Result<OraclePriceResponse, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("Oracle not available on {:?}", config.network))?;

    let price = fetch_oracle_price(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| e.to_string())?;

    Ok(OraclePriceResponse {
        nanoerg_per_usd: price.nanoerg_per_usd,
        erg_usd: price.erg_usd,
        oracle_box_id: price.oracle_box_id,
    })
}

/// Preview mint SigUSD (calculate cost without UTXOs)
#[tauri::command]
pub async fn preview_mint_sigusd(
    state: State<'_, AppState>,
    request: MintPreviewRequest,
) -> Result<MintPreviewResponse, String> {
    // Get protocol state for validation
    let sigmausd_state = get_sigmausd_state_internal(&state).await?;

    // Validate amount
    if request.amount <= 0 {
        return Err("Amount must be positive".to_string());
    }

    if !sigmausd_state.can_mint_sigusd {
        return Ok(MintPreviewResponse {
            erg_cost_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: "0".to_string(),
            total_cost_nano: "0".to_string(),
            can_execute: false,
            error: Some("Minting is currently disabled (reserve ratio too low)".to_string()),
        });
    }

    if request.amount > sigmausd_state.max_sigusd_mintable {
        return Ok(MintPreviewResponse {
            erg_cost_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: "0".to_string(),
            total_cost_nano: "0".to_string(),
            can_execute: false,
            error: Some(format!(
                "Amount exceeds maximum mintable: {} SigUSD",
                sigmausd_state.max_sigusd_mintable as f64 / 100.0
            )),
        });
    }

    // Calculate cost
    let calc = cost_to_mint_sigusd(request.amount, sigmausd_state.oracle_erg_per_usd_nano);
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.net_amount + tx_fee + min_box;

    Ok(MintPreviewResponse {
        erg_cost_nano: calc.net_amount.to_string(),
        protocol_fee_nano: calc.fee.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_cost_nano: total.to_string(),
        can_execute: true,
        error: None,
    })
}

/// Build mint SigUSD transaction (requires user UTXOs)
#[tauri::command]
pub async fn build_mint_sigusd(
    state: State<'_, AppState>,
    request: MintBuildRequest,
) -> Result<MintBuildResponse, String> {
    // Get node client
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    // Fetch current protocol state for validation
    let sigmausd_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| e.to_string())?;

    // Validate the mint request
    validate_mint_sigusd(request.amount, &sigmausd_state).map_err(|e| e.to_string())?;

    // Fetch tx context (bank and oracle boxes in EIP12 format)
    let tx_ctx = fetch_tx_context(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| e.to_string())?;

    // Convert user UTXOs from JSON to Eip12InputBox
    let user_inputs = super::parse_eip12_utxos(request.user_utxos)?;

    // Extract user's ErgoTree from first input
    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

    // Build the TxContext from fetch context
    let build_ctx = TxContext {
        nft_ids: nft_ids.clone(),
        bank_input: tx_ctx.bank_input,
        bank_erg_nano: tx_ctx.bank_erg_nano,
        sigusd_circulating: tx_ctx.sigusd_circulating,
        sigrsv_circulating: tx_ctx.sigrsv_circulating,
        sigusd_in_bank: tx_ctx.sigusd_in_bank,
        sigrsv_in_bank: tx_ctx.sigrsv_in_bank,
        oracle_data_input: tx_ctx.oracle_data_input,
        oracle_rate: tx_ctx.oracle_rate,
    };

    // Build the mint request
    let mint_request = MintSigUsdRequest {
        amount: request.amount,
        user_address: request.user_address,
        user_ergo_tree,
        user_inputs,
        current_height: request.current_height,
        recipient_ergo_tree: None,
    };

    // Build the transaction
    let result = build_mint_sigusd_tx(&mint_request, &build_ctx, &sigmausd_state)
        .map_err(|e| e.to_string())?;

    // Convert to JSON for response
    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;

    // Build summary DTO
    let summary = TxSummaryDto {
        action: result.summary.action,
        erg_amount_nano: result.summary.erg_amount_nano.to_string(),
        token_amount: result.summary.token_amount.to_string(),
        token_name: result.summary.token_name,
        protocol_fee_nano: result.summary.protocol_fee_nano.to_string(),
        tx_fee_nano: result.summary.tx_fee_nano.to_string(),
    };

    Ok(MintBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary,
    })
}

/// Internal helper to get SigmaUSD state
async fn get_sigmausd_state_internal(state: &State<'_, AppState>) -> Result<SigmaUsdState, String> {
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| e.to_string())
}

// =============================================================================
// Unified SigmaUSD Preview Command
// =============================================================================

/// Preview any SigmaUSD operation (unified command)
#[tauri::command]
pub async fn preview_sigmausd_tx(
    state: State<'_, AppState>,
    request: SigmaUsdPreviewRequest,
) -> Result<SigmaUsdPreviewResponse, String> {
    let sigmausd_state = get_sigmausd_state_internal(&state).await?;

    if request.amount <= 0 {
        return Err("Amount must be positive".to_string());
    }

    match request.action.as_str() {
        "mint_sigusd" => preview_mint_sigusd_internal(&sigmausd_state, request.amount),
        "redeem_sigusd" => preview_redeem_sigusd_internal(&sigmausd_state, request.amount),
        "mint_sigrsv" => preview_mint_sigrsv_internal(&sigmausd_state, request.amount),
        "redeem_sigrsv" => preview_redeem_sigrsv_internal(&sigmausd_state, request.amount),
        _ => Err(format!("Unknown action: {}", request.action)),
    }
}

/// Preview mint SigUSD operation
fn preview_mint_sigusd_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> Result<SigmaUsdPreviewResponse, String> {
    // Check if minting is allowed
    if !sigmausd_state.can_mint_sigusd {
        return Ok(SigmaUsdPreviewResponse {
            erg_amount_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_erg_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: "SigUSD".to_string(),
            can_execute: false,
            error: Some("Minting is currently disabled (reserve ratio too low)".to_string()),
        });
    }

    // Check against maximum mintable
    if amount > sigmausd_state.max_sigusd_mintable {
        return Ok(SigmaUsdPreviewResponse {
            erg_amount_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_erg_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: "SigUSD".to_string(),
            can_execute: false,
            error: Some(format!(
                "Amount exceeds maximum mintable: {:.2} SigUSD",
                sigmausd_state.max_sigusd_mintable as f64 / 100.0
            )),
        });
    }

    // Calculate cost
    let calc = cost_to_mint_sigusd(amount, sigmausd_state.oracle_erg_per_usd_nano);
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.net_amount + tx_fee + min_box;

    Ok(SigmaUsdPreviewResponse {
        erg_amount_nano: calc.net_amount.to_string(),
        protocol_fee_nano: calc.fee.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_erg_nano: total.to_string(),
        token_amount: amount.to_string(),
        token_name: "SigUSD".to_string(),
        can_execute: true,
        error: None,
    })
}

/// Preview redeem SigUSD operation
fn preview_redeem_sigusd_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> Result<SigmaUsdPreviewResponse, String> {
    // Check if there's enough circulating supply to redeem
    if amount > sigmausd_state.sigusd_circulating {
        return Ok(SigmaUsdPreviewResponse {
            erg_amount_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_erg_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: "SigUSD".to_string(),
            can_execute: false,
            error: Some(format!(
                "Amount exceeds circulating supply: {:.2} SigUSD",
                sigmausd_state.sigusd_circulating as f64 / 100.0
            )),
        });
    }

    // Calculate ERG received
    let calc = erg_from_redeem_sigusd(amount, sigmausd_state.oracle_erg_per_usd_nano);
    let tx_fee = TX_FEE_NANO;
    // For redeem: user receives ERG (negative total means user receives)
    let total = -(calc.net_amount as i64) + tx_fee;

    Ok(SigmaUsdPreviewResponse {
        erg_amount_nano: calc.net_amount.to_string(),
        protocol_fee_nano: calc.fee.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_erg_nano: total.to_string(),
        token_amount: amount.to_string(),
        token_name: "SigUSD".to_string(),
        can_execute: true,
        error: None,
    })
}

/// Preview mint SigRSV operation
fn preview_mint_sigrsv_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> Result<SigmaUsdPreviewResponse, String> {
    // Check if minting is allowed
    if !sigmausd_state.can_mint_sigrsv {
        return Ok(SigmaUsdPreviewResponse {
            erg_amount_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_erg_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: "SigRSV".to_string(),
            can_execute: false,
            error: Some("Minting is currently disabled (reserve ratio too high)".to_string()),
        });
    }

    // Check against maximum mintable
    if amount > sigmausd_state.max_sigrsv_mintable {
        return Ok(SigmaUsdPreviewResponse {
            erg_amount_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_erg_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: "SigRSV".to_string(),
            can_execute: false,
            error: Some(format!(
                "Amount exceeds maximum mintable: {} SigRSV",
                sigmausd_state.max_sigrsv_mintable
            )),
        });
    }

    // Calculate cost
    let calc = cost_to_mint_sigrsv(amount, sigmausd_state.sigrsv_price_nano);
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.net_amount + tx_fee + min_box;

    Ok(SigmaUsdPreviewResponse {
        erg_amount_nano: calc.net_amount.to_string(),
        protocol_fee_nano: calc.fee.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_erg_nano: total.to_string(),
        token_amount: amount.to_string(),
        token_name: "SigRSV".to_string(),
        can_execute: true,
        error: None,
    })
}

/// Preview redeem SigRSV operation
fn preview_redeem_sigrsv_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> Result<SigmaUsdPreviewResponse, String> {
    // Check if redeeming is allowed
    if !sigmausd_state.can_redeem_sigrsv {
        return Ok(SigmaUsdPreviewResponse {
            erg_amount_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_erg_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: "SigRSV".to_string(),
            can_execute: false,
            error: Some("Redeeming is currently disabled (reserve ratio too low)".to_string()),
        });
    }

    // Check if there's enough circulating supply to redeem
    if amount > sigmausd_state.sigrsv_circulating {
        return Ok(SigmaUsdPreviewResponse {
            erg_amount_nano: "0".to_string(),
            protocol_fee_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_erg_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: "SigRSV".to_string(),
            can_execute: false,
            error: Some(format!(
                "Amount exceeds circulating supply: {} SigRSV",
                sigmausd_state.sigrsv_circulating
            )),
        });
    }

    // Calculate ERG received
    let calc = erg_from_redeem_sigrsv(amount, sigmausd_state.sigrsv_price_nano);
    let tx_fee = TX_FEE_NANO;
    // For redeem: user receives ERG (negative total means user receives)
    let total = -(calc.net_amount as i64) + tx_fee;

    Ok(SigmaUsdPreviewResponse {
        erg_amount_nano: calc.net_amount.to_string(),
        protocol_fee_nano: calc.fee.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_erg_nano: total.to_string(),
        token_amount: amount.to_string(),
        token_name: "SigRSV".to_string(),
        can_execute: true,
        error: None,
    })
}

// =============================================================================
// Unified SigmaUSD Build Command
// =============================================================================

/// Build any SigmaUSD transaction (unified command)
#[tauri::command]
pub async fn build_sigmausd_tx(
    state: State<'_, AppState>,
    request: SigmaUsdBuildRequest,
) -> Result<SigmaUsdBuildResponse, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or("Node capabilities not available")?;
    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    let sigmausd_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| e.to_string())?;

    let tx_ctx = fetch_tx_context(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| e.to_string())?;

    // Convert UTXOs
    let user_inputs = super::parse_eip12_utxos(request.user_utxos)?;

    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

    // Convert optional recipient address to ErgoTree
    let recipient_ergo_tree = match &request.recipient_address {
        Some(addr) if !addr.is_empty() => {
            Some(ergo_tx::address_to_ergo_tree(addr).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    // Build TxContext from fetch result
    let ctx = TxContext {
        nft_ids: nft_ids.clone(),
        bank_input: tx_ctx.bank_input,
        bank_erg_nano: tx_ctx.bank_erg_nano,
        sigusd_circulating: tx_ctx.sigusd_circulating,
        sigrsv_circulating: tx_ctx.sigrsv_circulating,
        sigusd_in_bank: tx_ctx.sigusd_in_bank,
        sigrsv_in_bank: tx_ctx.sigrsv_in_bank,
        oracle_data_input: tx_ctx.oracle_data_input,
        oracle_rate: tx_ctx.oracle_rate,
    };

    // Route to appropriate builder based on action
    let result = match request.action.as_str() {
        "mint_sigusd" => {
            validate_mint_sigusd(request.amount, &sigmausd_state).map_err(|e| e.to_string())?;
            let req = MintSigUsdRequest {
                amount: request.amount,
                user_address: request.user_address,
                user_ergo_tree,
                user_inputs,
                current_height: request.current_height,
                recipient_ergo_tree,
            };
            build_mint_sigusd_tx(&req, &ctx, &sigmausd_state).map_err(|e| e.to_string())?
        }
        "redeem_sigusd" => {
            validate_redeem_sigusd(request.amount, &sigmausd_state).map_err(|e| e.to_string())?;
            let req = RedeemSigUsdRequest {
                amount: request.amount,
                user_address: request.user_address,
                user_ergo_tree,
                user_inputs,
                current_height: request.current_height,
                recipient_ergo_tree,
            };
            build_redeem_sigusd_tx(&req, &ctx, &sigmausd_state).map_err(|e| e.to_string())?
        }
        "mint_sigrsv" => {
            validate_mint_sigrsv(request.amount, &sigmausd_state).map_err(|e| e.to_string())?;
            let req = MintSigRsvRequest {
                amount: request.amount,
                user_address: request.user_address,
                user_ergo_tree,
                user_inputs,
                current_height: request.current_height,
                recipient_ergo_tree,
            };
            build_mint_sigrsv_tx(&req, &ctx, &sigmausd_state).map_err(|e| e.to_string())?
        }
        "redeem_sigrsv" => {
            validate_redeem_sigrsv(request.amount, &sigmausd_state).map_err(|e| e.to_string())?;
            let req = RedeemSigRsvRequest {
                amount: request.amount,
                user_address: request.user_address,
                user_ergo_tree,
                user_inputs,
                current_height: request.current_height,
                recipient_ergo_tree,
            };
            build_redeem_sigrsv_tx(&req, &ctx, &sigmausd_state).map_err(|e| e.to_string())?
        }
        _ => return Err(format!("Unknown action: {}", request.action)),
    };

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(SigmaUsdBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: TxSummaryDto {
            action: result.summary.action,
            erg_amount_nano: result.summary.erg_amount_nano.to_string(),
            token_amount: result.summary.token_amount.to_string(),
            token_name: result.summary.token_name,
            protocol_fee_nano: result.summary.protocol_fee_nano.to_string(),
            tx_fee_nano: result.summary.tx_fee_nano.to_string(),
        },
    })
}
