//! Node status and configuration endpoints

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use citadel_core::NodeConfig;

use crate::dto::{ApiError, NodeConfigRequest, NodeStatusResponse};
use crate::AppState;

/// Create node routes
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/configure", post(configure))
}

/// GET /node/status - Get current node status
pub async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<NodeStatusResponse>, (StatusCode, Json<ApiError>)> {
    let config = state.config().await;

    // Try to get node client and capabilities
    let client = state.node_client().await;

    match client {
        Some(client) => {
            let caps = client.capabilities().await;
            let node_name = client.node_name().await;

            match caps {
                Some(caps) => Ok(Json(NodeStatusResponse {
                    connected: caps.is_online,
                    url: config.node.url,
                    node_name,
                    network: config.network.as_str().to_string(),
                    chain_height: caps.chain_height,
                    indexed_height: caps.indexed_height,
                    capability_tier: caps.capability_tier.as_str().to_string(),
                    index_lag: caps.index_lag(),
                })),
                None => Ok(Json(NodeStatusResponse {
                    connected: true,
                    url: config.node.url,
                    node_name,
                    network: config.network.as_str().to_string(),
                    chain_height: 0,
                    indexed_height: None,
                    capability_tier: "Basic".to_string(),
                    index_lag: None,
                })),
            }
        }
        None => Ok(Json(NodeStatusResponse {
            connected: false,
            url: config.node.url,
            node_name: None,
            network: config.network.as_str().to_string(),
            chain_height: 0,
            indexed_height: None,
            capability_tier: "Basic".to_string(),
            index_lag: None,
        })),
    }
}

/// POST /node/configure - Update node configuration
pub async fn configure(
    State(state): State<AppState>,
    Json(request): Json<NodeConfigRequest>,
) -> Result<Json<NodeStatusResponse>, (StatusCode, Json<ApiError>)> {
    // Update config
    let node_config = NodeConfig {
        url: request.url.clone(),
        api_key: request.api_key,
    };
    state.set_node_config(node_config).await;

    // Refresh client and return status
    let _ = state.refresh_node_client().await;

    get_status(State(state)).await
}
