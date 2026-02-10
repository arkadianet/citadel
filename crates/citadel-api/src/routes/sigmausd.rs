//! SigmaUSD protocol endpoints

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use sigmausd::{constants::NftIds, cost_to_mint_sigusd, fetch_sigmausd_state, SigmaUsdState};

use crate::dto::{ApiError, MintPreviewRequest, MintPreviewResponse};
use crate::AppState;

/// Create SigmaUSD routes
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/state", get(get_state))
        .route("/mint/preview", post(mint_preview))
}

/// GET /sigmausd/state - Get current protocol state
pub async fn get_state(
    State(state): State<AppState>,
) -> Result<Json<SigmaUsdState>, (StatusCode, Json<ApiError>)> {
    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("node_unavailable", "Node not connected")),
        )
    })?;

    let capabilities = client.capabilities().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                "node_unavailable",
                "Node capabilities not available",
            )),
        )
    })?;

    let config = state.config().await;
    let nft_ids = NftIds::for_network(config.network).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "network_not_supported",
                format!("SigmaUSD not available on {:?}", config.network),
            )),
        )
    })?;

    let sigmausd_state = fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| {
            (
                StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(ApiError::new(e.error_code(), e.to_string())),
            )
        })?;

    Ok(Json(sigmausd_state))
}

/// POST /sigmausd/mint/preview - Preview a mint operation
pub async fn mint_preview(
    State(state): State<AppState>,
    Json(request): Json<MintPreviewRequest>,
) -> Result<Json<MintPreviewResponse>, (StatusCode, Json<ApiError>)> {
    // Validate amount
    if request.amount <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("Amount must be positive")),
        ));
    }

    // Get protocol state for oracle rate
    let sigmausd_state = get_state(State(state.clone())).await?.0;

    let calc = cost_to_mint_sigusd(request.amount, sigmausd_state.oracle_erg_per_usd_nano);
    let tx_fee = 1_100_000i64; // 0.0011 ERG
    let min_box = 1_000_000i64; // 0.001 ERG
    let total = calc.net_amount + tx_fee + min_box;

    Ok(Json(MintPreviewResponse {
        erg_cost_nano: calc.net_amount.to_string(),
        protocol_fee_nano: calc.fee.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_cost_nano: total.to_string(),
        can_execute: sigmausd_state.can_mint_sigusd,
        error: if !sigmausd_state.can_mint_sigusd {
            Some(format!(
                "Reserve ratio {:.1}% is below 400%",
                sigmausd_state.reserve_ratio_pct
            ))
        } else {
            None
        },
    }))
}
