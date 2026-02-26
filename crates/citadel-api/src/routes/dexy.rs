//! Dexy Protocol API Routes

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::dto::{
    ApiError, DexyBuildRequest, DexyBuildResponse, DexyPreviewRequest, DexyPreviewResponse,
    DexyStateResponse, TxSummaryDto,
};
use crate::AppState;

use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use dexy::{
    calculator::cost_to_mint_dexy,
    constants::{DexyIds, DexyVariant},
    fetch::{fetch_dexy_state, fetch_tx_context},
    tx_builder::{build_mint_dexy_tx, validate_mint_dexy, MintDexyRequest},
};

/// Create Dexy router
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/state/{variant}", get(get_state))
        .route("/rates/{variant}", get(get_rates))
        .route("/mint/preview", post(mint_preview))
        .route("/mint/build", post(build_mint))
}

/// Get Dexy protocol state for a variant
async fn get_state(
    State(state): State<AppState>,
    Path(variant_str): Path<String>,
) -> Result<Json<DexyStateResponse>, (StatusCode, Json<ApiError>)> {
    let variant = variant_str.parse::<DexyVariant>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(format!(
                "Invalid variant: {}. Use 'gold' or 'usd'",
                variant_str
            ))),
        )
    })?;

    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::internal("Node not connected")),
        )
    })?;

    let capabilities = client.capabilities().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::internal("Node capabilities not available")),
        )
    })?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(variant, config.network).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(format!(
                "Dexy {} not available on {:?}",
                variant_str, config.network
            ))),
        )
    })?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(e.to_string())),
            )
        })?;

    Ok(Json(DexyStateResponse {
        variant: variant.as_str().to_string(),
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
    }))
}

/// Get Dexy rates for all minting paths
async fn get_rates(
    State(state): State<AppState>,
    Path(variant_str): Path<String>,
) -> Result<Json<dexy::DexyRates>, (StatusCode, Json<ApiError>)> {
    let variant = variant_str.parse::<DexyVariant>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(format!(
                "Invalid variant: {}. Use 'gold' or 'usd'",
                variant_str
            ))),
        )
    })?;

    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::internal("Node not connected")),
        )
    })?;

    let capabilities = client.capabilities().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::internal("Node capabilities not available")),
        )
    })?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(variant, config.network).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(format!(
                "Dexy {} not available on {:?}",
                variant_str, config.network
            ))),
        )
    })?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(e.to_string())),
            )
        })?;

    let rates = dexy::DexyRates::from_state(&dexy_state);
    Ok(Json(rates))
}

/// Preview mint operation
async fn mint_preview(
    State(state): State<AppState>,
    Json(request): Json<DexyPreviewRequest>,
) -> Result<Json<DexyPreviewResponse>, (StatusCode, Json<ApiError>)> {
    let variant = request.variant.parse::<DexyVariant>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(format!(
                "Invalid variant: {}",
                request.variant
            ))),
        )
    })?;

    if request.amount <= 0 {
        return Ok(Json(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: request.amount.to_string(),
            token_name: variant.token_name().to_string(),
            can_execute: false,
            error: Some("Amount must be positive".to_string()),
        }));
    }

    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::internal("Node not connected")),
        )
    })?;

    let capabilities = client.capabilities().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::internal("Node capabilities not available")),
        )
    })?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(variant, config.network).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("Dexy not available on this network")),
        )
    })?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(e.to_string())),
            )
        })?;

    // Check if can mint
    if !dexy_state.can_mint {
        return Ok(Json(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: request.amount.to_string(),
            token_name: variant.token_name().to_string(),
            can_execute: false,
            error: Some("Minting is currently unavailable".to_string()),
        }));
    }

    // Check amount against available
    if request.amount > dexy_state.dexy_in_bank {
        return Ok(Json(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: request.amount.to_string(),
            token_name: variant.token_name().to_string(),
            can_execute: false,
            error: Some(format!(
                "Amount exceeds available: {} in bank",
                dexy_state.dexy_in_bank
            )),
        }));
    }

    // Calculate cost
    let calc = cost_to_mint_dexy(
        request.amount,
        dexy_state.oracle_rate_nano,
        variant.decimals(),
    );
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.erg_amount + tx_fee + min_box;

    Ok(Json(DexyPreviewResponse {
        erg_cost_nano: calc.erg_amount.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_cost_nano: total.to_string(),
        token_amount: request.amount.to_string(),
        token_name: variant.token_name().to_string(),
        can_execute: true,
        error: None,
    }))
}

/// Build mint transaction
async fn build_mint(
    State(state): State<AppState>,
    Json(request): Json<DexyBuildRequest>,
) -> Result<Json<DexyBuildResponse>, (StatusCode, Json<ApiError>)> {
    let variant = request.variant.parse::<DexyVariant>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(format!(
                "Invalid variant: {}",
                request.variant
            ))),
        )
    })?;

    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::internal("Node not connected")),
        )
    })?;

    let capabilities = client.capabilities().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::internal("Node capabilities not available")),
        )
    })?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(variant, config.network).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("Dexy not available on this network")),
        )
    })?;

    // Fetch state and context
    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(e.to_string())),
            )
        })?;

    // Validate
    validate_mint_dexy(request.amount, &dexy_state).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(e.to_string())),
        )
    })?;

    let tx_ctx = fetch_tx_context(&client, &capabilities, &ids)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(e.to_string())),
            )
        })?;

    // Parse user UTXOs
    let user_inputs: Vec<ergo_tx::Eip12InputBox> = request
        .user_utxos
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(|e| format!("Invalid UTXO: {}", e)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError::bad_request(e))))?;

    if user_inputs.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("No user UTXOs provided")),
        ));
    }

    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

    // Use fresh height from node to ensure R4 calculation is accurate
    // This minimizes the delay between fetching height and transaction validation
    let fresh_height = capabilities.chain_height as i32;

    let mint_request = MintDexyRequest {
        variant,
        amount: request.amount,
        user_address: request.user_address,
        user_ergo_tree,
        user_inputs,
        current_height: fresh_height,
        recipient_ergo_tree: None,
    };

    let result = build_mint_dexy_tx(&mint_request, &tx_ctx, &dexy_state).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal(e.to_string())),
        )
    })?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal(format!("Failed to serialize tx: {}", e))),
        )
    })?;

    Ok(Json(DexyBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: TxSummaryDto {
            action: result.summary.action,
            erg_amount_nano: result.summary.erg_amount_nano.to_string(),
            token_amount: result.summary.token_amount.to_string(),
            token_name: result.summary.token_name,
            protocol_fee_nano: "0".to_string(), // Dexy has no protocol fee
            tx_fee_nano: result.summary.tx_fee_nano.to_string(),
        },
    }))
}
