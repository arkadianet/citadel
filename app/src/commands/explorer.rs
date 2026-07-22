use citadel_api::services::explorer as explorer_svc;
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn explorer_node_info(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    explorer_svc::explorer_node_info(&state).await
}

#[tauri::command]
pub async fn explorer_get_transaction(
    state: State<'_, AppState>,
    tx_id: String,
) -> Result<serde_json::Value, String> {
    explorer_svc::explorer_get_transaction(&state, tx_id).await
}

#[tauri::command]
pub async fn explorer_get_block(
    state: State<'_, AppState>,
    block_id: String,
) -> Result<serde_json::Value, String> {
    explorer_svc::explorer_get_block(&state, block_id).await
}

#[tauri::command]
pub async fn explorer_get_block_headers(
    state: State<'_, AppState>,
    count: u32,
) -> Result<Vec<serde_json::Value>, String> {
    explorer_svc::explorer_get_block_headers(&state, count).await
}

#[tauri::command]
pub async fn explorer_get_mempool(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    explorer_svc::explorer_get_mempool(&state).await
}

#[tauri::command]
pub async fn explorer_get_box(
    state: State<'_, AppState>,
    box_id: String,
) -> Result<serde_json::Value, String> {
    explorer_svc::explorer_get_box(&state, box_id).await
}

#[tauri::command]
pub async fn explorer_get_token(
    state: State<'_, AppState>,
    token_id: String,
) -> Result<serde_json::Value, String> {
    explorer_svc::explorer_get_token(&state, token_id).await
}

#[tauri::command]
pub async fn explorer_get_address(
    state: State<'_, AppState>,
    address: String,
    offset: u64,
    limit: u64,
) -> Result<serde_json::Value, String> {
    explorer_svc::explorer_get_address(&state, address, offset, limit).await
}

#[tauri::command]
pub async fn explorer_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<serde_json::Value, String> {
    explorer_svc::explorer_search(&state, query).await
}
