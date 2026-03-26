use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::dto::{
    AmmPoolDto, AmmPoolsResponse, ApiError, SwapBuildApiRequest, SwapBuildApiResponse,
    SwapInputDto, SwapQuoteRequest, SwapQuoteResponse, SwapSummaryDto,
};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/pools", get(get_pools))
        .route("/pools/{pool_id}", get(get_pool))
        .route("/quote", post(get_quote))
        .route("/swap/build", post(build_swap))
}

async fn get_pools(
    State(state): State<AppState>,
) -> Result<Json<AmmPoolsResponse>, (StatusCode, Json<ApiError>)> {
    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("node_unavailable", "Node not connected")),
        )
    })?;

    let pools = amm::discover_pools(&client).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal(e.to_string())),
        )
    })?;

    let pool_dtos: Vec<AmmPoolDto> = pools.into_iter().map(Into::into).collect();
    let count = pool_dtos.len();

    Ok(Json(AmmPoolsResponse {
        pools: pool_dtos,
        count,
    }))
}

async fn get_pool(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
) -> Result<Json<AmmPoolDto>, (StatusCode, Json<ApiError>)> {
    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("node_unavailable", "Node not connected")),
        )
    })?;

    let pools = amm::discover_pools(&client).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal(e.to_string())),
        )
    })?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(format!("Pool not found: {}", pool_id))),
            )
        })?;

    Ok(Json(pool.into()))
}

async fn get_quote(
    State(state): State<AppState>,
    Json(request): Json<SwapQuoteRequest>,
) -> Result<Json<SwapQuoteResponse>, (StatusCode, Json<ApiError>)> {
    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("node_unavailable", "Node not connected")),
        )
    })?;

    let pools = amm::discover_pools(&client).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal(e.to_string())),
        )
    })?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == request.pool_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(format!(
                    "Pool not found: {}",
                    request.pool_id
                ))),
            )
        })?;

    let input = match request.input {
        SwapInputDto::Erg { amount } => amm::SwapInput::Erg { amount },
        SwapInputDto::Token { token_id, amount } => amm::SwapInput::Token { token_id, amount },
    };

    let quote = amm::quote_swap(&pool, &input).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(
                "Cannot calculate quote for this swap",
            )),
        )
    })?;

    Ok(Json(quote.into()))
}

async fn build_swap(
    State(state): State<AppState>,
    Json(request): Json<SwapBuildApiRequest>,
) -> Result<Json<SwapBuildApiResponse>, (StatusCode, Json<ApiError>)> {
    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("node_unavailable", "Node not connected")),
        )
    })?;

    let pools = amm::discover_pools(&client).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal(e.to_string())),
        )
    })?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == request.pool_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(format!(
                    "Pool not found: {}",
                    request.pool_id
                ))),
            )
        })?;

    let input = match request.input {
        SwapInputDto::Erg { amount } => amm::SwapInput::Erg { amount },
        SwapInputDto::Token { token_id, amount } => amm::SwapInput::Token { token_id, amount },
    };

    let swap_request = amm::SwapRequest {
        pool_id: request.pool_id,
        input,
        min_output: request.min_output,
        redeemer_address: request.user_address,
    };

    let user_utxos: Vec<ergo_tx::Eip12InputBox> = request
        .user_utxos
        .into_iter()
        .map(|v| {
            serde_json::from_value(v).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError::bad_request(format!("Invalid UTXO: {}", e))),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let result = amm::build_swap_order_eip12(
        &swap_request,
        &pool,
        &user_utxos,
        &request.user_ergo_tree,
        &request.user_pk,
        request.current_height,
        None,
        None,
    )
    .map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(e.to_string())),
        )
    })?;

    let tx_json = serde_json::to_value(&result.unsigned_tx).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal(e.to_string())),
        )
    })?;

    Ok(Json(SwapBuildApiResponse {
        unsigned_tx: tx_json,
        summary: SwapSummaryDto {
            input_amount: result.summary.input_amount,
            input_token: result.summary.input_token,
            min_output: result.summary.min_output,
            output_token: result.summary.output_token,
            execution_fee: result.summary.execution_fee,
            miner_fee: result.summary.miner_fee,
            total_erg_cost: result.summary.total_erg_cost,
        },
    }))
}
