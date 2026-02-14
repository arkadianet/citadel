//! Duckpools Lending Protocol API Routes
//!
//! REST endpoints for the Duckpools lending protocol:
//! - GET /lending/markets - All pools with APY, utilization, TVL
//! - GET /lending/markets/{pool_id} - Single pool details
//! - GET /lending/positions/{address} - User positions across all pools
//! - POST /lending/lend/build - Build lend proxy transaction
//! - POST /lending/withdraw/build - Build withdraw proxy transaction
//! - POST /lending/borrow/build - Build borrow proxy transaction (stub)
//! - POST /lending/repay/build - Build repay proxy transaction
//! - POST /lending/refund/build - Build refund transaction

use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::dto::ApiError;
use crate::AppState;

use lending::tx_builder::{
    self, BuildError, BuildResponse, LendRequest, ProxyBoxData, RefundResponse, RepayRequest,
    UserUtxo, WithdrawRequest,
};
use lending::{constants, fetch_all_markets, PoolState};

// =============================================================================
// DTOs for Lending API
// =============================================================================

/// Markets response - all pools with metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketsResponse {
    pub pools: Vec<PoolInfo>,
    pub block_height: u32,
}

/// Pool information for markets endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolInfo {
    pub pool_id: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub is_erg_pool: bool,
    pub total_supplied: String,
    pub total_borrowed: String,
    pub available_liquidity: String,
    pub utilization_pct: f64,
    pub supply_apy: f64,
    pub borrow_apy: f64,
    pub pool_box_id: String,
    pub collateral_options: Vec<CollateralOptionInfo>,
}

/// Collateral option info for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollateralOptionInfo {
    pub token_id: String,
    pub token_name: String,
    pub liquidation_threshold: u64,
    pub liquidation_penalty: u64,
    pub dex_nft: Option<String>,
}

/// User positions response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionsResponse {
    pub address: String,
    pub lend_positions: Vec<LendPositionInfo>,
    pub borrow_positions: Vec<BorrowPositionInfo>,
    pub block_height: u32,
}

/// Lending position info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendPositionInfo {
    pub pool_id: String,
    pub pool_name: String,
    pub lp_tokens: String,
    pub underlying_value: String,
    pub unrealized_profit: String,
}

/// Borrow position info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowPositionInfo {
    pub pool_id: String,
    pub pool_name: String,
    pub collateral_box_id: String,
    pub collateral_token: String,
    pub collateral_name: String,
    pub collateral_amount: String,
    pub borrowed_amount: String,
    pub total_owed: String,
    pub health_factor: f64,
    pub health_status: String,
}

/// Lend build request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendBuildRequest {
    pub pool_id: String,
    pub amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
    /// Slippage tolerance in basis points (0-200 for 0%-2%), defaults to 0
    #[serde(default)]
    pub slippage_bps: u16,
}

/// Withdraw build request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawBuildRequest {
    pub pool_id: String,
    pub lp_amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

/// Borrow build request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowBuildRequest {
    pub pool_id: String,
    pub collateral_token: String,
    pub collateral_amount: u64,
    pub borrow_amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

/// Repay build request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepayBuildRequest {
    pub pool_id: String,
    pub collateral_box_id: String,
    pub repay_amount: u64,
    /// Total owed with interest. Determines full vs partial repay proxy.
    pub total_owed: u64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

/// Refund build request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundBuildRequest {
    pub proxy_box_id: String,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

/// Build response for all transaction types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: LendingTxSummary,
}

/// Transaction summary for lending operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingTxSummary {
    pub action: String,
    pub pool_id: String,
    pub pool_name: String,
    pub amount_in: String,
    pub amount_out_estimate: Option<String>,
    pub tx_fee_nano: String,
    pub refund_height: i32,
    /// Service fee formatted for display (e.g. "0.006250 SigUSD")
    pub service_fee: String,
    /// Service fee in base units as string (e.g. "6250")
    pub service_fee_nano: String,
    /// Total tokens/ERG user sends to proxy (amount + fee + slippage)
    pub total_to_send: String,
}

// =============================================================================
// Router
// =============================================================================

/// Create Lending router
pub fn router() -> Router<AppState> {
    Router::new()
        // Market endpoints
        .route("/markets", get(get_markets))
        .route("/markets/{pool_id}", get(get_market))
        // Position endpoint
        .route("/positions/{address}", get(get_positions))
        // Transaction build endpoints
        .route("/lend/build", post(build_lend))
        .route("/withdraw/build", post(build_withdraw))
        .route("/borrow/build", post(build_borrow))
        .route("/repay/build", post(build_repay))
        .route("/refund/build", post(build_refund))
}

// =============================================================================
// Handlers
// =============================================================================

/// GET /lending/markets - Get all lending pools with metrics
async fn get_markets(
    State(state): State<AppState>,
) -> Result<Json<MarketsResponse>, (StatusCode, Json<ApiError>)> {
    // Get node client
    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("node_unavailable", "Node not connected")),
        )
    })?;

    // Get capabilities
    let capabilities = client.capabilities().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                "node_unavailable",
                "Node capabilities not available",
            )),
        )
    })?;

    // Fetch all markets from the lending crate
    let markets_response = fetch_all_markets(&client, &capabilities, None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("fetch_error", e.to_string())),
            )
        })?;

    // Convert PoolState to PoolInfo DTOs
    let pools: Vec<PoolInfo> = markets_response
        .pools
        .iter()
        .map(pool_state_to_info)
        .collect();

    Ok(Json(MarketsResponse {
        pools,
        block_height: markets_response.block_height,
    }))
}

/// Convert lending::PoolState to API PoolInfo DTO
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

/// GET /lending/markets/{pool_id} - Get single pool details
async fn get_market(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
) -> Result<Json<PoolInfo>, (StatusCode, Json<ApiError>)> {
    // Validate pool_id exists in configuration
    let pool_config = constants::get_pool(&pool_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!("Pool '{}' not found", pool_id))),
        )
    })?;

    // Get node client
    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("node_unavailable", "Node not connected")),
        )
    })?;

    // Get capabilities
    let capabilities = client.capabilities().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                "node_unavailable",
                "Node capabilities not available",
            )),
        )
    })?;

    // Fetch the specific pool state
    let pool_state = lending::fetch::fetch_pool_state(&client, &capabilities, pool_config)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("fetch_error", e.to_string())),
            )
        })?;

    Ok(Json(pool_state_to_info(&pool_state)))
}

/// GET /lending/positions/{address} - Get user positions across all pools
async fn get_positions(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<PositionsResponse>, (StatusCode, Json<ApiError>)> {
    // Get node client
    let client = state.node_client().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new("node_unavailable", "Node not connected")),
        )
    })?;

    // Get capabilities
    let capabilities = client.capabilities().await.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                "node_unavailable",
                "Node capabilities not available",
            )),
        )
    })?;

    // Fetch all markets with user positions
    let markets_response = fetch_all_markets(&client, &capabilities, Some(&address))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("fetch_error", e.to_string())),
            )
        })?;

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

    Ok(Json(PositionsResponse {
        address,
        lend_positions,
        borrow_positions,
        block_height: markets_response.block_height,
    }))
}

/// Convert health factor to UI status string for color coding
/// - "green": >= 1.5 (healthy)
/// - "amber": >= 1.2 and < 1.5 (warning)
/// - "red": < 1.2 (danger)
fn health_factor_to_status(health_factor: f64) -> String {
    if health_factor >= constants::health::HEALTHY_THRESHOLD {
        "green".to_string()
    } else if health_factor >= constants::health::WARNING_THRESHOLD {
        "amber".to_string()
    } else {
        "red".to_string()
    }
}

// =============================================================================
// Transaction Build Helpers
// =============================================================================

/// Parse user UTXOs from JSON to UserUtxo structs
///
/// The frontend sends UTXOs as EIP-12 JSON format. This function parses them
/// into the tx_builder's UserUtxo format.
fn parse_user_utxos(
    utxos_json: Vec<serde_json::Value>,
) -> Result<Vec<UserUtxo>, (StatusCode, Json<ApiError>)> {
    if utxos_json.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("No user UTXOs provided")),
        ));
    }

    utxos_json
        .into_iter()
        .enumerate()
        .map(|(idx, v)| parse_single_utxo(v, idx))
        .collect()
}

/// Parse a single UTXO from JSON
fn parse_single_utxo(
    v: serde_json::Value,
    idx: usize,
) -> Result<UserUtxo, (StatusCode, Json<ApiError>)> {
    // Extract required fields
    let box_id = v["boxId"]
        .as_str()
        .or_else(|| v["box_id"].as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!("UTXO {} missing boxId", idx))),
            )
        })?
        .to_string();

    let tx_id = v["transactionId"]
        .as_str()
        .or_else(|| v["transaction_id"].as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!(
                    "UTXO {} missing transactionId",
                    idx
                ))),
            )
        })?
        .to_string();

    let index = v["index"].as_u64().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(format!("UTXO {} missing index", idx))),
        )
    })? as u16;

    // Value can be string or number
    let value: i64 = match &v["value"] {
        serde_json::Value::String(s) => s.parse().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!(
                    "UTXO {} has invalid value: {}",
                    idx, s
                ))),
            )
        })?,
        serde_json::Value::Number(n) => n.as_i64().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!(
                    "UTXO {} has invalid value",
                    idx
                ))),
            )
        })?,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!("UTXO {} missing value", idx))),
            ))
        }
    };

    let ergo_tree = v["ergoTree"]
        .as_str()
        .or_else(|| v["ergo_tree"].as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!(
                    "UTXO {} missing ergoTree",
                    idx
                ))),
            )
        })?
        .to_string();

    let creation_height = v["creationHeight"]
        .as_i64()
        .or_else(|| v["creation_height"].as_i64())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!(
                    "UTXO {} missing creationHeight",
                    idx
                ))),
            )
        })? as i32;

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
    let registers: HashMap<String, String> = v["additionalRegisters"]
        .as_object()
        .or_else(|| v["additional_registers"].as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    Ok(UserUtxo {
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
fn build_response_to_api(
    response: BuildResponse,
    _pool_config: &constants::PoolConfig,
) -> Result<LendingBuildResponse, (StatusCode, Json<ApiError>)> {
    let unsigned_tx: serde_json::Value =
        serde_json::from_str(&response.unsigned_tx).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(format!(
                    "Failed to parse unsigned_tx: {}",
                    e
                ))),
            )
        })?;

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

/// Convert RefundResponse to LendingBuildResponse
fn refund_response_to_api(
    response: RefundResponse,
    proxy_box_id: &str,
) -> Result<LendingBuildResponse, (StatusCode, Json<ApiError>)> {
    let unsigned_tx: serde_json::Value =
        serde_json::from_str(&response.unsigned_tx).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(format!(
                    "Failed to parse unsigned_tx: {}",
                    e
                ))),
            )
        })?;

    Ok(LendingBuildResponse {
        unsigned_tx,
        summary: LendingTxSummary {
            action: "refund".to_string(),
            pool_id: "".to_string(),
            pool_name: "Proxy Refund".to_string(),
            amount_in: proxy_box_id.to_string(),
            amount_out_estimate: Some("Refunded to wallet".to_string()),
            tx_fee_nano: response.fee_nano.to_string(),
            refund_height: response.refundable_after_height as i32,
            service_fee: String::new(),
            service_fee_nano: "0".to_string(),
            total_to_send: String::new(),
        },
    })
}

/// Convert BuildError to API error response
fn build_error_to_api(error: BuildError) -> (StatusCode, Json<ApiError>) {
    let status = match error.status_code() {
        400 => StatusCode::BAD_REQUEST,
        404 => StatusCode::NOT_FOUND,
        422 => StatusCode::UNPROCESSABLE_ENTITY,
        425 => StatusCode::from_u16(425).unwrap_or(StatusCode::BAD_REQUEST),
        503 => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (status, Json(ApiError::new(error.code(), error.to_string())))
}

/// POST /lending/lend/build - Build lend proxy transaction
async fn build_lend(
    State(_state): State<AppState>,
    Json(request): Json<LendBuildRequest>,
) -> Result<Json<LendingBuildResponse>, (StatusCode, Json<ApiError>)> {
    // Validate amount is non-zero
    if request.amount == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("Amount must be greater than 0")),
        ));
    }

    // Validate pool_id exists
    let pool_config = constants::get_pool(&request.pool_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!(
                "Pool '{}' not found",
                request.pool_id
            ))),
        )
    })?;

    // Parse user UTXOs
    let user_utxos = parse_user_utxos(request.user_utxos)?;

    // Build the lend request
    let lend_request = LendRequest {
        pool_id: request.pool_id.clone(),
        amount: request.amount,
        user_address: request.user_address,
        user_utxos,
        min_lp_tokens: None,
        slippage_bps: request.slippage_bps,
    };

    // Build the transaction
    let result = tx_builder::build_lend_tx(lend_request, pool_config, request.current_height)
        .map_err(build_error_to_api)?;

    // Convert to API response
    build_response_to_api(result, pool_config).map(Json)
}

/// POST /lending/withdraw/build - Build withdraw proxy transaction
async fn build_withdraw(
    State(_state): State<AppState>,
    Json(request): Json<WithdrawBuildRequest>,
) -> Result<Json<LendingBuildResponse>, (StatusCode, Json<ApiError>)> {
    // Validate amount is non-zero
    if request.lp_amount == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("LP amount must be greater than 0")),
        ));
    }

    // Validate pool_id exists
    let pool_config = constants::get_pool(&request.pool_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!(
                "Pool '{}' not found",
                request.pool_id
            ))),
        )
    })?;

    // Parse user UTXOs
    let user_utxos = parse_user_utxos(request.user_utxos)?;

    // Build the withdraw request
    let withdraw_request = WithdrawRequest {
        pool_id: request.pool_id.clone(),
        lp_amount: request.lp_amount,
        user_address: request.user_address,
        user_utxos,
        min_output: None, // Could add to API request later
    };

    // Build the transaction
    let result =
        tx_builder::build_withdraw_tx(withdraw_request, pool_config, request.current_height)
            .map_err(build_error_to_api)?;

    // Convert to API response
    build_response_to_api(result, pool_config).map(Json)
}

/// POST /lending/borrow/build - Build borrow proxy transaction
///
/// Note: Borrowing is currently stubbed in the protocol layer as it requires
/// complex Sigma encoding for GroupElement (R9) that is not yet implemented.
/// This endpoint returns a descriptive error directing users to the Duckpools
/// web interface for borrowing functionality.
async fn build_borrow(
    State(_state): State<AppState>,
    Json(request): Json<BorrowBuildRequest>,
) -> Result<Json<LendingBuildResponse>, (StatusCode, Json<ApiError>)> {
    // Validate pool_id exists (provide helpful feedback)
    let _pool_config = constants::get_pool(&request.pool_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!(
                "Pool '{}' not found",
                request.pool_id
            ))),
        )
    })?;

    // Borrow requires complex Sigma encoding not yet implemented:
    // - R7: (Long, Long) tuple for (threshold, penalty)
    // - R9: GroupElement for user's public key
    //
    // Direct users to Duckpools web interface for borrowing.
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiError::new(
            "borrow_not_available",
            format!(
                "Borrowing from pool '{}' is not yet available in this interface. \
                 Please use the Duckpools web interface at https://duckpools.io to borrow. \
                 Lending, withdrawing, and repaying existing loans are fully supported here.",
                request.pool_id
            ),
        )),
    ))
}

/// POST /lending/repay/build - Build repay proxy transaction
async fn build_repay(
    State(_state): State<AppState>,
    Json(request): Json<RepayBuildRequest>,
) -> Result<Json<LendingBuildResponse>, (StatusCode, Json<ApiError>)> {
    // Validate amount is non-zero
    if request.repay_amount == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("Repay amount must be greater than 0")),
        ));
    }

    // Validate pool_id exists
    let pool_config = constants::get_pool(&request.pool_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!(
                "Pool '{}' not found",
                request.pool_id
            ))),
        )
    })?;

    // Parse user UTXOs
    let user_utxos = parse_user_utxos(request.user_utxos)?;

    // Build the repay request
    let repay_request = RepayRequest {
        pool_id: request.pool_id.clone(),
        collateral_box_id: request.collateral_box_id,
        repay_amount: request.repay_amount,
        total_owed: request.total_owed,
        user_address: request.user_address,
        user_utxos,
    };

    // Build the transaction
    let result = tx_builder::build_repay_tx(repay_request, pool_config, request.current_height)
        .map_err(build_error_to_api)?;

    // Convert to API response
    build_response_to_api(result, pool_config).map(Json)
}

/// POST /lending/refund/build - Build refund transaction for stuck proxy box
///
/// Refund allows users to reclaim funds from proxy boxes that weren't processed
/// by the Duckpools bots (e.g., due to insufficient liquidity or other issues).
/// The proxy contract allows refunds after the refund_height stored in R6.
async fn build_refund(
    State(_state): State<AppState>,
    Json(request): Json<RefundBuildRequest>,
) -> Result<Json<LendingBuildResponse>, (StatusCode, Json<ApiError>)> {
    // The RefundBuildRequest should contain proxy box details
    // We need to construct ProxyBoxData from the request

    // For refund, we need the proxy box information. The frontend must provide:
    // - proxy_box_id: The box ID being refunded
    // - proxy_box data: Full box details for building the transaction
    //
    // The request includes user_utxos which should contain the proxy box as the first element
    // with all necessary register data.

    // Parse the proxy box from user_utxos (first element should be the proxy box)
    if request.user_utxos.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(
                "Proxy box data required in user_utxos for refund",
            )),
        ));
    }

    // The first UTXO should be the proxy box to refund
    let proxy_utxo = &request.user_utxos[0];

    // Validate it matches the proxy_box_id
    let box_id = proxy_utxo["boxId"]
        .as_str()
        .or_else(|| proxy_utxo["box_id"].as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request("Proxy box missing boxId")),
            )
        })?;

    if box_id != request.proxy_box_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(format!(
                "First UTXO boxId '{}' does not match proxy_box_id '{}'",
                box_id, request.proxy_box_id
            ))),
        ));
    }

    // Extract proxy box fields
    let tx_id = proxy_utxo["transactionId"]
        .as_str()
        .or_else(|| proxy_utxo["transaction_id"].as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request("Proxy box missing transactionId")),
            )
        })?
        .to_string();

    let index = proxy_utxo["index"].as_u64().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("Proxy box missing index")),
        )
    })? as u16;

    let value: i64 = match &proxy_utxo["value"] {
        serde_json::Value::String(s) => s.parse().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!(
                    "Invalid proxy box value: {}",
                    s
                ))),
            )
        })?,
        serde_json::Value::Number(n) => n.as_i64().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request("Invalid proxy box value")),
            )
        })?,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request("Proxy box missing value")),
            ))
        }
    };

    let ergo_tree = proxy_utxo["ergoTree"]
        .as_str()
        .or_else(|| proxy_utxo["ergo_tree"].as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request("Proxy box missing ergoTree")),
            )
        })?
        .to_string();

    let creation_height = proxy_utxo["creationHeight"]
        .as_i64()
        .or_else(|| proxy_utxo["creation_height"].as_i64())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request("Proxy box missing creationHeight")),
            )
        })? as i32;

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
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(
                    "Proxy box missing additionalRegisters",
                )),
            )
        })?;

    let r4_encoded = registers.get("R4").and_then(|v| v.as_str());
    let r5_encoded = registers.get("R5").and_then(|v| v.as_str());

    let r6_encoded = registers
        .get("R6")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(
                    "Proxy box missing R6 (refund height)",
                )),
            )
        })?;

    // Decode user ErgoTree from R4 or R5.
    // Lend/Withdraw/Borrow proxies store it in R4 (Coll[Byte]).
    // Repay/PartialRepay proxies store it in R5 (R4 is a Long).
    let r4_user_tree = r4_encoded
        .and_then(|r4| decode_sigma_byte_array(r4).ok())
        .or_else(|| r5_encoded.and_then(|r5| decode_sigma_byte_array(r5).ok()))
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(
                    "Proxy box missing valid user ErgoTree in R4 or R5",
                )),
            )
        })?;

    // Decode R6: Int or Long containing refund height
    // Old proxies used Long (0x05), new proxies use Int (0x04) after the encoding fix
    let r6_refund_height = decode_sigma_long(r6_encoded)
        .or_else(|_| decode_sigma_int(r6_encoded).map(|v| v as i64))
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(format!("Invalid R6 encoding: {}", e))),
            )
        })?;

    // Build ProxyBoxData
    let proxy_box = ProxyBoxData {
        box_id: request.proxy_box_id.clone(),
        tx_id,
        index,
        value,
        ergo_tree,
        assets,
        creation_height,
        r4_user_tree,
        r6_refund_height,
        additional_registers: registers
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .collect(),
    };

    // Build the refund transaction
    let result = tx_builder::build_refund_tx(proxy_box, request.current_height)
        .map_err(build_error_to_api)?;

    // Convert to API response
    refund_response_to_api(result, &request.proxy_box_id).map(Json)
}

// =============================================================================
// Sigma Decoding Helpers
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

/// Decode a Sigma Int from register hex string
/// Format: 04 (type tag) + zigzag-encoded VLQ value
fn decode_sigma_int(hex_str: &str) -> Result<i32, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Invalid hex: {}", e))?;

    if bytes.is_empty() || bytes[0] != 0x04 {
        return Err("Not an Int type (expected 0x04 prefix)".to_string());
    }

    let mut idx = 1;
    let mut zigzag: u32 = 0;
    let mut shift = 0;

    while idx < bytes.len() {
        if shift >= 32 {
            return Err("VLQ value too large for Int".to_string());
        }
        let byte = bytes[idx];
        zigzag |= ((byte & 0x7f) as u32) << shift;
        idx += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    let value = if zigzag & 1 == 0 {
        (zigzag >> 1) as i32
    } else {
        -((zigzag >> 1) as i32) - 1
    };

    Ok(value)
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
