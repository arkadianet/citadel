//! HTTP request handlers for ErgoPay endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::nautilus_page::{generate_connect_page, generate_signing_page};
use crate::server::ServerState;
use crate::types::{ErgoPayResponse, MessageSeverity, RequestStatus, RequestType, TxCallback};

/// Query parameters for connect/tx endpoints
#[derive(Debug, Deserialize)]
pub struct AddressQuery {
    /// Wallet address (from #P2PK_ADDRESS# substitution)
    pub address: Option<String>,
}

/// Handle wallet connection request
/// GET /connect/{id}?address=9xxx
pub async fn handle_connect(
    State(state): State<Arc<ServerState>>,
    Path(request_id): Path<String>,
    Query(query): Query<AddressQuery>,
) -> Result<Json<ErgoPayResponse>, StatusCode> {
    let mut requests = state.pending_requests.write().await;

    let request = requests.get_mut(&request_id).ok_or(StatusCode::NOT_FOUND)?;

    // Check if expired
    if request.is_expired() {
        request.status = RequestStatus::Expired;
        return Err(StatusCode::GONE);
    }

    // Must be a connect request
    if !matches!(request.request_type, RequestType::Connect) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Extract address from query
    let address = query.address.ok_or(StatusCode::BAD_REQUEST)?;

    // Validate address format (basic check - starts with 9 for mainnet P2PK)
    if !address.starts_with('9') || address.len() < 40 {
        return Ok(Json(ErgoPayResponse {
            message: Some("Invalid address format".to_string()),
            message_severity: Some(MessageSeverity::Error),
            ..Default::default()
        }));
    }

    // Update status
    request.status = RequestStatus::AddressReceived(address.clone());

    tracing::info!("Wallet connected: {} for request {}", address, request_id);

    Ok(Json(ErgoPayResponse {
        message: Some("Wallet connected successfully!".to_string()),
        message_severity: Some(MessageSeverity::Information),
        address: Some(address),
        ..Default::default()
    }))
}

/// Handle transaction signing request
/// GET /tx/{id}?address=9xxx
pub async fn handle_tx(
    State(state): State<Arc<ServerState>>,
    Path(request_id): Path<String>,
    Query(query): Query<AddressQuery>,
) -> Result<Json<ErgoPayResponse>, StatusCode> {
    let requests = state.pending_requests.read().await;

    let request = requests.get(&request_id).ok_or(StatusCode::NOT_FOUND)?;

    // Check if expired
    if request.is_expired() {
        return Err(StatusCode::GONE);
    }

    // Must be a sign transaction request
    let (reduced_tx, message) = match &request.request_type {
        RequestType::SignTransaction {
            reduced_tx,
            unsigned_tx: _,
            message,
        } => (reduced_tx, message),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    // Build callback URL using LAN IP so mobile wallet can reach us
    let callback_url = format!(
        "http://{}:{}/callback/{}",
        state.host, state.port, request_id
    );

    // Encode reduced_tx as base64 URL-safe with padding
    // Mobile wallets expect standard Base64 (length must be multiple of 4)
    use base64::{engine::general_purpose::URL_SAFE, Engine};
    let encoded_tx = URL_SAFE.encode(reduced_tx);

    Ok(Json(ErgoPayResponse {
        reduced_tx: Some(encoded_tx),
        message: Some(message.clone()),
        message_severity: Some(MessageSeverity::Information),
        address: query.address,
        reply_to: Some(callback_url),
    }))
}

/// Handle callback from wallet after tx submission
/// POST /callback/{id}
pub async fn handle_callback(
    State(state): State<Arc<ServerState>>,
    Path(request_id): Path<String>,
    Json(payload): Json<TxCallback>,
) -> Result<StatusCode, StatusCode> {
    let mut requests = state.pending_requests.write().await;

    let request = requests.get_mut(&request_id).ok_or(StatusCode::NOT_FOUND)?;

    // Update status with tx_id
    request.status = RequestStatus::TxSubmitted {
        tx_id: payload.tx_id.clone(),
    };

    tracing::info!(
        "Transaction submitted: {} for request {}",
        payload.tx_id,
        request_id
    );

    Ok(StatusCode::OK)
}

/// Serve Nautilus signing page
/// GET /nautilus/sign/{id}
pub async fn handle_nautilus_page(
    State(state): State<Arc<ServerState>>,
    Path(request_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let requests = state.pending_requests.read().await;

    let request = requests.get(&request_id).ok_or(StatusCode::NOT_FOUND)?;

    if request.is_expired() {
        return Err(StatusCode::GONE);
    }

    let message = match &request.request_type {
        RequestType::SignTransaction { message, .. } => message.clone(),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let html = generate_signing_page(&request_id, &message, &state.host, state.port);
    Ok(Html(html))
}

/// Serve Nautilus connect page
/// GET /nautilus/connect/{id}
pub async fn handle_nautilus_connect_page(
    State(state): State<Arc<ServerState>>,
    Path(request_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let requests = state.pending_requests.read().await;

    let request = requests.get(&request_id).ok_or(StatusCode::NOT_FOUND)?;

    if request.is_expired() {
        return Err(StatusCode::GONE);
    }

    // Must be a connect request
    if !matches!(request.request_type, RequestType::Connect) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let html = generate_connect_page(&request_id, &state.host, state.port);
    Ok(Html(html))
}

/// Return unsigned transaction JSON for Nautilus
/// GET /nautilus/tx/{id}
pub async fn handle_nautilus_tx(
    State(state): State<Arc<ServerState>>,
    Path(request_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let requests = state.pending_requests.read().await;

    let request = requests.get(&request_id).ok_or(StatusCode::NOT_FOUND)?;

    if request.is_expired() {
        return Err(StatusCode::GONE);
    }

    match &request.request_type {
        RequestType::SignTransaction { unsigned_tx, .. } => Ok(Json(unsigned_tx.clone())),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}
