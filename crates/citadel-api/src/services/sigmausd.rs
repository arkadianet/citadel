//! SigmaUSD use-case orchestration: bank state, oracle price, mint/redeem preview and build.

use crate::dto::{
    MintBuildResponse, MintPreviewResponse, OraclePriceResponse, SigmaUsdBuildResponse,
    SigmaUsdPreviewResponse, TxSummaryDto,
};
use crate::services::error::{IntoServiceError, ServiceResult};
use crate::AppState;
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

pub async fn get_state(state: &AppState) -> ServiceResult<SigmaUsdState> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .into_service()
}

pub async fn get_oracle_price(state: &AppState) -> ServiceResult<OraclePriceResponse> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("Oracle not available on {:?}", config.network))?;

    let price = fetch_oracle_price(&client, &capabilities, &nft_ids)
        .await
        .into_service()?;

    Ok(OraclePriceResponse {
        nanoerg_per_usd: price.nanoerg_per_usd,
        erg_usd: price.erg_usd,
        oracle_box_id: price.oracle_box_id,
    })
}

pub async fn preview_mint_sigusd(
    state: &AppState,
    amount: i64,
) -> ServiceResult<MintPreviewResponse> {
    let sigmausd_state = get_state(state).await?;

    if amount <= 0 {
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

    if amount > sigmausd_state.max_sigusd_mintable {
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

    let calc = cost_to_mint_sigusd(amount, sigmausd_state.oracle_erg_per_usd_nano);
    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.net_amount + tx_fee + citadel_fee + min_box;

    Ok(MintPreviewResponse {
        erg_cost_nano: calc.net_amount.to_string(),
        protocol_fee_nano: calc.fee.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_cost_nano: total.to_string(),
        can_execute: true,
        error: None,
    })
}

pub async fn build_mint_sigusd(
    state: &AppState,
    amount: i64,
    user_address: String,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<MintBuildResponse> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    let sigmausd_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .into_service()?;

    validate_mint_sigusd(amount, &sigmausd_state).into_service()?;

    let tx_ctx = fetch_tx_context(&client, &capabilities, &nft_ids)
        .await
        .into_service()?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

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
        amount,
        user_address,
        user_ergo_tree,
        user_inputs: user_utxos,
        current_height,
        recipient_ergo_tree: None,
    };

    let result = build_mint_sigusd_tx(&mint_request, &build_ctx, &sigmausd_state).into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;

    Ok(MintBuildResponse {
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

pub async fn preview_sigmausd_tx(
    state: &AppState,
    action: &str,
    amount: i64,
) -> ServiceResult<SigmaUsdPreviewResponse> {
    let sigmausd_state = get_state(state).await?;

    if amount <= 0 {
        return Err("Amount must be positive".to_string());
    }

    match action {
        "mint_sigusd" => preview_mint_sigusd_internal(&sigmausd_state, amount),
        "redeem_sigusd" => preview_redeem_sigusd_internal(&sigmausd_state, amount),
        "mint_sigrsv" => preview_mint_sigrsv_internal(&sigmausd_state, amount),
        "redeem_sigrsv" => preview_redeem_sigrsv_internal(&sigmausd_state, amount),
        _ => Err(format!("Unknown action: {}", action)),
    }
}

fn preview_mint_sigusd_internal(
    sigmausd_state: &SigmaUsdState,
    amount: i64,
) -> ServiceResult<SigmaUsdPreviewResponse> {
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
    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.net_amount + tx_fee + citadel_fee + min_box;

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
) -> ServiceResult<SigmaUsdPreviewResponse> {
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
    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let tx_fee = TX_FEE_NANO;
    // Negative total means user receives ERG (after miner + Citadel fees)
    let total = -(calc.net_amount as i64) + tx_fee + citadel_fee;

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
) -> ServiceResult<SigmaUsdPreviewResponse> {
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
    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.net_amount + tx_fee + citadel_fee + min_box;

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
) -> ServiceResult<SigmaUsdPreviewResponse> {
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
    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let tx_fee = TX_FEE_NANO;
    // Negative total means user receives ERG (after miner + Citadel fees)
    let total = -(calc.net_amount as i64) + tx_fee + citadel_fee;

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

#[allow(clippy::too_many_arguments)]
pub async fn build_sigmausd_tx(
    state: &AppState,
    action: &str,
    amount: i64,
    user_address: String,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
    recipient_address: Option<String>,
) -> ServiceResult<SigmaUsdBuildResponse> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network)
        .ok_or_else(|| format!("SigmaUSD not available on {:?}", config.network))?;

    let sigmausd_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .into_service()?;

    let tx_ctx = fetch_tx_context(&client, &capabilities, &nft_ids)
        .await
        .into_service()?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let recipient_ergo_tree = match &recipient_address {
        Some(addr) if !addr.is_empty() => Some(ergo_tx::address_to_ergo_tree(addr).into_service()?),
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

    let result = match action {
        "mint_sigusd" => {
            validate_mint_sigusd(amount, &sigmausd_state).into_service()?;
            let req = MintSigUsdRequest {
                amount,
                user_address,
                user_ergo_tree,
                user_inputs: user_utxos,
                current_height,
                recipient_ergo_tree,
            };
            build_mint_sigusd_tx(&req, &ctx, &sigmausd_state).into_service()?
        }
        "redeem_sigusd" => {
            validate_redeem_sigusd(amount, &sigmausd_state).into_service()?;
            let req = RedeemSigUsdRequest {
                amount,
                user_address,
                user_ergo_tree,
                user_inputs: user_utxos,
                current_height,
                recipient_ergo_tree,
            };
            build_redeem_sigusd_tx(&req, &ctx, &sigmausd_state).into_service()?
        }
        "mint_sigrsv" => {
            validate_mint_sigrsv(amount, &sigmausd_state).into_service()?;
            let req = MintSigRsvRequest {
                amount,
                user_address,
                user_ergo_tree,
                user_inputs: user_utxos,
                current_height,
                recipient_ergo_tree,
            };
            build_mint_sigrsv_tx(&req, &ctx, &sigmausd_state).into_service()?
        }
        "redeem_sigrsv" => {
            validate_redeem_sigrsv(amount, &sigmausd_state).into_service()?;
            let req = RedeemSigRsvRequest {
                amount,
                user_address,
                user_ergo_tree,
                user_inputs: user_utxos,
                current_height,
                recipient_ergo_tree,
            };
            build_redeem_sigrsv_tx(&req, &ctx, &sigmausd_state).into_service()?
        }
        _ => return Err(format!("Unknown action: {}", action)),
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
