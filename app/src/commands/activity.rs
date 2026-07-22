use citadel_api::services::activity::{self, ProtocolInteraction};
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_protocol_activity(
    state: State<'_, AppState>,
    count: u64,
    max_age_secs: Option<u64>,
) -> Result<Vec<ProtocolInteraction>, String> {
    activity::get_protocol_activity(&state, count, max_age_secs).await
}

#[tauri::command]
pub async fn get_dexy_activity(
    state: State<'_, AppState>,
    count: u64,
) -> Result<Vec<ProtocolInteraction>, String> {
    activity::get_dexy_activity(&state, count).await
}

#[tauri::command]
pub async fn get_sigmausd_activity(
    state: State<'_, AppState>,
    count: u64,
) -> Result<Vec<ProtocolInteraction>, String> {
    activity::get_sigmausd_activity(&state, count).await
}
