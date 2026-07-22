//! Dexy use-case orchestration: bank/LP state, mint preview/build, swap and LP deposit/redeem.

use crate::dto::{
    DexyBuildResponse, DexyLpBuildResponse, DexyLpPreviewResponse, DexyPreviewResponse,
    DexyStateResponse, DexySwapBuildResponse, DexySwapPreviewResponse, TxSummaryDto,
};
use crate::services::error::{IntoServiceError, ServiceResult};
use crate::AppState;
use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use dexy::{
    calculator::{calculate_lp_deposit, calculate_lp_redeem, can_redeem_lp, cost_to_mint_dexy},
    constants::{DexyIds, DexyVariant},
    fetch::{
        fetch_dexy_state, fetch_lp_tx_context, fetch_tx_context as fetch_dexy_tx_context,
        parse_lp_box, LpAction,
    },
    rates::DexyRates,
    tx_builder::{
        build_mint_dexy_tx, validate_mint_dexy, LpDepositRequest, LpRedeemRequest, MintDexyRequest,
    },
};

fn parse_variant(variant: &str) -> ServiceResult<DexyVariant> {
    variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}. Use 'gold' or 'usd'", variant))
}

fn recipient_ergo_tree(recipient_address: &Option<String>) -> ServiceResult<Option<String>> {
    match recipient_address {
        Some(addr) if !addr.is_empty() => {
            Ok(Some(ergo_tx::address_to_ergo_tree(addr).into_service()?))
        }
        _ => Ok(None),
    }
}

pub async fn get_state(state: &AppState, variant: &str) -> ServiceResult<DexyStateResponse> {
    let dexy_variant = parse_variant(variant)?;

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available on {:?}", variant, config.network))?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .into_service()?;

    Ok(DexyStateResponse {
        variant: dexy_variant.as_str().to_string(),
        bank_erg_nano: dexy_state.bank_erg_nano,
        dexy_in_bank: dexy_state.dexy_in_bank,
        bank_box_id: dexy_state.bank_box_id,
        dexy_token_id: dexy_state.dexy_token_id,
        free_mint_available: dexy_state.free_mint_available,
        free_mint_reset_height: dexy_state.free_mint_reset_height,
        current_height: dexy_state.current_height,
        oracle_rate_nano: dexy_state.oracle_rate_nano,
        oracle_box_id: dexy_state.oracle_box_id,
        lp_erg_reserves: dexy_state.lp_erg_reserves,
        lp_dexy_reserves: dexy_state.lp_dexy_reserves,
        lp_box_id: dexy_state.lp_box_id,
        lp_rate_nano: dexy_state.lp_rate_nano,
        lp_token_reserves: dexy_state.lp_token_reserves,
        lp_circulating: dexy_state.lp_circulating,
        can_redeem_lp: dexy_state.can_redeem_lp,
        can_mint: dexy_state.can_mint,
        rate_difference_pct: dexy_state.rate_difference_pct,
        dexy_circulating: dexy_state.dexy_circulating,
    })
}

pub async fn get_rates(state: &AppState, variant: &str) -> ServiceResult<DexyRates> {
    let dexy_variant = parse_variant(variant)?;

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available on {:?}", variant, config.network))?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .into_service()?;

    Ok(DexyRates::from_state(&dexy_state))
}

pub async fn preview_mint(
    state: &AppState,
    variant: &str,
    amount: i64,
) -> ServiceResult<DexyPreviewResponse> {
    let dexy_variant = parse_variant(variant)?;

    if amount <= 0 {
        return Ok(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: dexy_variant.token_name().to_string(),
            can_execute: false,
            error: Some("Amount must be positive".to_string()),
        });
    }

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| "Dexy not available on this network".to_string())?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .into_service()?;

    if !dexy_state.can_mint {
        return Ok(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: dexy_variant.token_name().to_string(),
            can_execute: false,
            error: Some("Minting is currently unavailable".to_string()),
        });
    }

    if amount > dexy_state.dexy_in_bank {
        return Ok(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: amount.to_string(),
            token_name: dexy_variant.token_name().to_string(),
            can_execute: false,
            error: Some(format!(
                "Amount exceeds available: {}",
                dexy_state.dexy_in_bank
            )),
        });
    }

    let calc = cost_to_mint_dexy(amount, dexy_state.oracle_rate_nano, dexy_variant.decimals());
    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.erg_amount + tx_fee + citadel_fee + min_box;

    Ok(DexyPreviewResponse {
        erg_cost_nano: calc.erg_amount.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_cost_nano: total.to_string(),
        token_amount: amount.to_string(),
        token_name: dexy_variant.token_name().to_string(),
        can_execute: true,
        error: None,
    })
}

pub async fn build_mint(
    state: &AppState,
    variant: &str,
    amount: i64,
    user_address: String,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
    recipient_address: Option<String>,
) -> ServiceResult<DexyBuildResponse> {
    let dexy_variant = parse_variant(variant)?;

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| "Dexy not available on this network".to_string())?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .into_service()?;

    validate_mint_dexy(amount, &dexy_state).into_service()?;

    let tx_ctx = fetch_dexy_tx_context(&client, &capabilities, &ids)
        .await
        .into_service()?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();
    let recipient_ergo_tree = recipient_ergo_tree(&recipient_address)?;

    let mint_request = MintDexyRequest {
        variant: dexy_variant,
        amount,
        user_address,
        user_ergo_tree,
        user_inputs: user_utxos,
        current_height,
        recipient_ergo_tree,
    };

    let result = build_mint_dexy_tx(&mint_request, &tx_ctx, &dexy_state).into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(DexyBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: TxSummaryDto {
            action: result.summary.action,
            erg_amount_nano: result.summary.erg_amount_nano.to_string(),
            token_amount: result.summary.token_amount.to_string(),
            token_name: result.summary.token_name,
            protocol_fee_nano: "0".to_string(), // Dexy has no protocol fee
            tx_fee_nano: result.summary.tx_fee_nano.to_string(),
        },
    })
}

pub async fn preview_swap(
    state: &AppState,
    variant: &str,
    direction: &str,
    amount: i64,
    slippage: Option<f64>,
) -> ServiceResult<DexySwapPreviewResponse> {
    let dexy_variant = parse_variant(variant)?;

    if amount <= 0 {
        return Err("Amount must be positive".to_string());
    }

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available", variant))?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .into_service()?;

    let (output_amount, reserves_sold, reserves_bought) = match direction {
        "erg_to_dexy" => {
            let out = dexy::calculator::calculate_lp_swap_output(
                amount,
                dexy_state.lp_erg_reserves,
                dexy_state.lp_dexy_reserves,
                dexy::constants::LP_SWAP_FEE_NUM,
                dexy::constants::LP_SWAP_FEE_DENOM,
            );
            (out, dexy_state.lp_erg_reserves, dexy_state.lp_dexy_reserves)
        }
        "dexy_to_erg" => {
            let out = dexy::calculator::calculate_lp_swap_output(
                amount,
                dexy_state.lp_dexy_reserves,
                dexy_state.lp_erg_reserves,
                dexy::constants::LP_SWAP_FEE_NUM,
                dexy::constants::LP_SWAP_FEE_DENOM,
            );
            (out, dexy_state.lp_dexy_reserves, dexy_state.lp_erg_reserves)
        }
        _ => {
            return Err(format!(
                "Invalid direction: {}. Use 'erg_to_dexy' or 'dexy_to_erg'",
                direction
            ))
        }
    };

    let slippage_pct = slippage.unwrap_or(0.5);
    let min_output = (output_amount as f64 * (1.0 - slippage_pct / 100.0)) as i64;

    let price_impact = dexy::calculator::calculate_lp_swap_price_impact(
        amount,
        reserves_sold,
        reserves_bought,
        dexy::constants::LP_SWAP_FEE_NUM,
        dexy::constants::LP_SWAP_FEE_DENOM,
    );

    let (output_token_name, output_decimals) = match direction {
        "erg_to_dexy" => (
            dexy_variant.token_name().to_string(),
            dexy_variant.decimals(),
        ),
        _ => ("ERG".to_string(), 9),
    };

    Ok(DexySwapPreviewResponse {
        variant: variant.to_string(),
        direction: direction.to_string(),
        input_amount: amount,
        output_amount,
        output_token_name,
        output_decimals,
        min_output,
        price_impact,
        fee_pct: dexy::constants::LP_SWAP_FEE_NUM as f64
            / dexy::constants::LP_SWAP_FEE_DENOM as f64
            * 100.0,
        miner_fee_nano: TX_FEE_NANO,
        citadel_fee_nano: ergo_tx::resolved_dev_fee_config().budget(),
        lp_erg_reserves: dexy_state.lp_erg_reserves,
        lp_dexy_reserves: dexy_state.lp_dexy_reserves,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn build_swap(
    state: &AppState,
    variant: &str,
    direction: &str,
    amount: i64,
    min_output: i64,
    user_address: String,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
    recipient_address: Option<String>,
) -> ServiceResult<DexySwapBuildResponse> {
    let dexy_variant = parse_variant(variant)?;

    let swap_direction = match direction {
        "erg_to_dexy" => dexy::tx_builder::SwapDirection::ErgToDexy,
        "dexy_to_erg" => dexy::tx_builder::SwapDirection::DexyToErg,
        _ => return Err(format!("Invalid direction: {}", direction)),
    };

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available", variant))?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .into_service()?;

    let swap_ctx = dexy::fetch::fetch_swap_tx_context(&client, &capabilities, &ids)
        .await
        .into_service()?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();
    let recipient_ergo_tree = recipient_ergo_tree(&recipient_address)?;

    let request = dexy::tx_builder::SwapDexyRequest {
        variant: dexy_variant,
        direction: swap_direction,
        input_amount: amount,
        min_output,
        user_address,
        user_ergo_tree,
        user_inputs: user_utxos,
        current_height,
        recipient_ergo_tree,
    };

    let result =
        dexy::tx_builder::build_swap_dexy_tx(&request, &swap_ctx, &dexy_state).into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(DexySwapBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: result.summary,
    })
}

pub async fn preview_lp_deposit(
    state: &AppState,
    variant: &str,
    erg_amount: i64,
    dexy_amount: i64,
) -> ServiceResult<DexyLpPreviewResponse> {
    let dexy_variant = parse_variant(variant)?;

    if erg_amount <= 0 || dexy_amount <= 0 {
        return Ok(DexyLpPreviewResponse {
            variant: variant.to_string(),
            action: "lp_deposit".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: "0".to_string(),
            redemption_fee_pct: None,
            can_execute: false,
            error: Some("Both ERG and Dexy amounts must be positive".to_string()),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available on {:?}", variant, config.network))?;

    let lp_token_id = citadel_core::TokenId::new(&ids.lp_nft);
    let lp_box = client
        .get_box_by_token_id(&capabilities, &lp_token_id)
        .await
        .map_err(|e| format!("LP box not found: {}", e))?;
    let lp_data = parse_lp_box(&lp_box, &ids).into_service()?;

    let calc = calculate_lp_deposit(
        erg_amount,
        dexy_amount,
        lp_data.erg_reserves,
        lp_data.dexy_reserves,
        lp_data.lp_token_reserves,
        dexy_variant.initial_lp(),
    );

    if calc.lp_tokens_out <= 0 {
        return Ok(DexyLpPreviewResponse {
            variant: variant.to_string(),
            action: "lp_deposit".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: "0".to_string(),
            redemption_fee_pct: None,
            can_execute: false,
            error: Some("Deposit too small: would receive 0 LP tokens".to_string()),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    Ok(DexyLpPreviewResponse {
        variant: variant.to_string(),
        action: "lp_deposit".to_string(),
        erg_amount: calc.consumed_erg.to_string(),
        dexy_amount: calc.consumed_dexy.to_string(),
        lp_tokens: calc.lp_tokens_out.to_string(),
        redemption_fee_pct: None,
        can_execute: true,
        error: None,
        miner_fee_nano: TX_FEE_NANO.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn build_lp_deposit(
    state: &AppState,
    variant: &str,
    erg_amount: i64,
    dexy_amount: i64,
    user_address: String,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
    recipient_address: Option<String>,
) -> ServiceResult<DexyLpBuildResponse> {
    let dexy_variant = parse_variant(variant)?;

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available", variant))?;

    let ctx = fetch_lp_tx_context(&client, &capabilities, &ids, LpAction::Deposit)
        .await
        .into_service()?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();
    let recipient_ergo_tree = recipient_ergo_tree(&recipient_address)?;

    let request = LpDepositRequest {
        variant: dexy_variant,
        deposit_erg: erg_amount,
        deposit_dexy: dexy_amount,
        user_address,
        user_ergo_tree,
        user_inputs: user_utxos,
        current_height,
        recipient_ergo_tree,
    };

    let result = dexy::tx_builder::build_lp_deposit_tx(
        &request,
        &ctx,
        &ids.dexy_token,
        &ids.lp_token_id,
        dexy_variant.initial_lp(),
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(DexyLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: result.summary,
    })
}

pub async fn preview_lp_redeem(
    state: &AppState,
    variant: &str,
    lp_amount: i64,
) -> ServiceResult<DexyLpPreviewResponse> {
    let dexy_variant = parse_variant(variant)?;

    if lp_amount <= 0 {
        return Ok(DexyLpPreviewResponse {
            variant: variant.to_string(),
            action: "lp_redeem".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: "0".to_string(),
            redemption_fee_pct: Some(2.0),
            can_execute: false,
            error: Some("LP token amount must be positive".to_string()),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available on {:?}", variant, config.network))?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .into_service()?;

    let lp_token_id = citadel_core::TokenId::new(&ids.lp_nft);
    let lp_box = client
        .get_box_by_token_id(&capabilities, &lp_token_id)
        .await
        .map_err(|e| format!("LP box not found: {}", e))?;
    let lp_data = parse_lp_box(&lp_box, &ids).into_service()?;

    if !can_redeem_lp(
        dexy_state.lp_erg_reserves,
        dexy_state.lp_dexy_reserves,
        dexy_state.oracle_rate_nano,
    ) {
        return Ok(DexyLpPreviewResponse {
            variant: variant.to_string(),
            action: "lp_redeem".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: lp_amount.to_string(),
            redemption_fee_pct: Some(2.0),
            can_execute: false,
            error: Some(
                "LP redeem blocked: LP rate below 98% of oracle rate (depeg protection)"
                    .to_string(),
            ),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    let calc = calculate_lp_redeem(
        lp_amount,
        lp_data.erg_reserves,
        lp_data.dexy_reserves,
        lp_data.lp_token_reserves,
        dexy_variant.initial_lp(),
    );

    if calc.erg_out <= 0 || calc.dexy_out <= 0 {
        return Ok(DexyLpPreviewResponse {
            variant: variant.to_string(),
            action: "lp_redeem".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: lp_amount.to_string(),
            redemption_fee_pct: Some(2.0),
            can_execute: false,
            error: Some("Redeem too small: would receive 0 ERG or Dexy tokens".to_string()),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    Ok(DexyLpPreviewResponse {
        variant: variant.to_string(),
        action: "lp_redeem".to_string(),
        erg_amount: calc.erg_out.to_string(),
        dexy_amount: calc.dexy_out.to_string(),
        lp_tokens: lp_amount.to_string(),
        redemption_fee_pct: Some(2.0),
        can_execute: true,
        error: None,
        miner_fee_nano: TX_FEE_NANO.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn build_lp_redeem(
    state: &AppState,
    variant: &str,
    lp_amount: i64,
    user_address: String,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
    recipient_address: Option<String>,
) -> ServiceResult<DexyLpBuildResponse> {
    let dexy_variant = parse_variant(variant)?;

    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available", variant))?;

    let ctx = fetch_lp_tx_context(&client, &capabilities, &ids, LpAction::Redeem)
        .await
        .into_service()?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();
    let recipient_ergo_tree = recipient_ergo_tree(&recipient_address)?;

    let request = LpRedeemRequest {
        variant: dexy_variant,
        lp_to_burn: lp_amount,
        user_address,
        user_ergo_tree,
        user_inputs: user_utxos,
        current_height,
        recipient_ergo_tree,
    };

    let result = dexy::tx_builder::build_lp_redeem_tx(
        &request,
        &ctx,
        &ids.dexy_token,
        &ids.lp_token_id,
        dexy_variant.initial_lp(),
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(DexyLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: result.summary,
    })
}
