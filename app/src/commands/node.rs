use citadel_api::dto::{HealthResponse, NodeConfigRequest, NodeStatusResponse};
use citadel_api::services::node as node_svc;
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn health_check() -> Result<HealthResponse, String> {
    Ok(node_svc::health_check())
}

#[tauri::command]
pub async fn get_node_status(state: State<'_, AppState>) -> Result<NodeStatusResponse, String> {
    node_svc::get_node_status(&state).await
}

#[tauri::command]
pub async fn configure_node(
    state: State<'_, AppState>,
    request: NodeConfigRequest,
) -> Result<NodeStatusResponse, String> {
    node_svc::configure_node(&state, request).await
}

#[tauri::command]
pub async fn discover_nodes(
    state: State<'_, AppState>,
) -> Result<Vec<ergo_node_client::NodeProbeResult>, String> {
    node_svc::discover_nodes(&state).await
}

#[tauri::command]
pub async fn probe_single_node(
    url: String,
) -> Result<Option<ergo_node_client::NodeProbeResult>, String> {
    node_svc::probe_single_node(&url).await
}
