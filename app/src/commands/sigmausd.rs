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
use super::StrErr;
use tauri::State;

#[tauri::command]
pub async fn get_sigmausd_state(state: State<'_, AppState>) -> Result<SigmaUsdState, String> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .str_err()
}

#[tauri::command]
pub async fn get_oracle_price(state: State<'_, AppState>) -> Result<OraclePriceResponse, String> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("Oracle not available on {:?}", config.network))?;

    let price = fetch_oracle_price(&client, &capabilities, &nft_ids)
        .await
        .str_err()?;

    Ok(OraclePriceResponse {
        nanoerg_per_usd: price.nanoerg_per_usd,
        erg_usd: price.erg_usd,
        oracle_box_id: price.oracle_box_id,
    })
}

#[tauri::command]
pub async fn preview_mint_sigusd(
    state: State<'_, AppState>,
    request: MintPreviewRequest,
) -> Result<MintPreviewResponse, String> {
    let sigmausd_state = get_sigmausd_state_internal(&state).await?;

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

#[tauri::command]
pub async fn build_mint_sigusd(
    state: State<'_, AppState>,
    request: MintBuildRequest,
) -> Result<MintBuildResponse, String> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    let sigmausd_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .str_err()?;

    validate_mint_sigusd(request.amount, &sigmausd_state).str_err()?;

    let tx_ctx = fetch_tx_context(&client, &capabilities, &nft_ids)
        .await
        .str_err()?;

    let user_inputs = super::parse_eip12_utxos(request.user_utxos)?;
    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

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

    let mint_request = MintSigUsdRequest {
        amount: request.amount,
        user_address: request.user_address,
        user_ergo_tree,
        user_inputs,
        current_height: request.current_height,
        recipient_ergo_tree: None,
    };

    let result = build_mint_sigusd_tx(&mint_request, &build_ctx, &sigmausd_state)
        .str_err()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;

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

async fn get_sigmausd_state_internal(state: &State<'_, AppState>) -> Result<SigmaUsdState, String> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .str_err()
}

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

fn preview_mint_sigusd_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> Result<SigmaUsdPreviewResponse, String> {
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

fn preview_redeem_sigusd_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> Result<SigmaUsdPreviewResponse, String> {
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

    let calc = erg_from_redeem_sigusd(amount, sigmausd_state.oracle_erg_per_usd_nano);
    let tx_fee = TX_FEE_NANO;
    // Negative total means user receives ERG
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

fn preview_mint_sigrsv_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> Result<SigmaUsdPreviewResponse, String> {
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

fn preview_redeem_sigrsv_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> Result<SigmaUsdPreviewResponse, String> {
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

    let calc = erg_from_redeem_sigrsv(amount, sigmausd_state.sigrsv_price_nano);
    let tx_fee = TX_FEE_NANO;
    // Negative total means user receives ERG
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

#[tauri::command]
pub async fn build_sigmausd_tx(
    state: State<'_, AppState>,
    request: SigmaUsdBuildRequest,
) -> Result<SigmaUsdBuildResponse, String> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    let sigmausd_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .str_err()?;

    let tx_ctx = fetch_tx_context(&client, &capabilities, &nft_ids)
        .await
        .str_err()?;

    let user_inputs = super::parse_eip12_utxos(request.user_utxos)?;
    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

    let recipient_ergo_tree = match &request.recipient_address {
        Some(addr) if !addr.is_empty() => {
            Some(ergo_tx::address_to_ergo_tree(addr).str_err()?)
        }
        _ => None,
    };

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

    let result = match request.action.as_str() {
        "mint_sigusd" => {
            validate_mint_sigusd(request.amount, &sigmausd_state).str_err()?;
            let req = MintSigUsdRequest {
                amount: request.amount,
                user_address: request.user_address,
                user_ergo_tree,
                user_inputs,
                current_height: request.current_height,
                recipient_ergo_tree,
            };
            build_mint_sigusd_tx(&req, &ctx, &sigmausd_state).str_err()?
        }
        "redeem_sigusd" => {
            validate_redeem_sigusd(request.amount, &sigmausd_state).str_err()?;
            let req = RedeemSigUsdRequest {
                amount: request.amount,
                user_address: request.user_address,
                user_ergo_tree,
                user_inputs,
                current_height: request.current_height,
                recipient_ergo_tree,
            };
            build_redeem_sigusd_tx(&req, &ctx, &sigmausd_state).str_err()?
        }
        "mint_sigrsv" => {
            validate_mint_sigrsv(request.amount, &sigmausd_state).str_err()?;
            let req = MintSigRsvRequest {
                amount: request.amount,
                user_address: request.user_address,
                user_ergo_tree,
                user_inputs,
                current_height: request.current_height,
                recipient_ergo_tree,
            };
            build_mint_sigrsv_tx(&req, &ctx, &sigmausd_state).str_err()?
        }
        "redeem_sigrsv" => {
            validate_redeem_sigrsv(request.amount, &sigmausd_state).str_err()?;
            let req = RedeemSigRsvRequest {
                amount: request.amount,
                user_address: request.user_address,
                user_ergo_tree,
                user_inputs,
                current_height: request.current_height,
                recipient_ergo_tree,
            };
            build_redeem_sigrsv_tx(&req, &ctx, &sigmausd_state).str_err()?
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
