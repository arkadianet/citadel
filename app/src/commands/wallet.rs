use citadel_api::dto::{
    ConnectionStatusResponse, RecentTxsResponse, WalletBalanceResponse, WalletConnectResponse,
    WalletStatusResponse,
};
use citadel_api::services::wallet as wallet_svc;
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn start_wallet_connect(
    state: State<'_, AppState>,
) -> Result<WalletConnectResponse, String> {
    wallet_svc::start_wallet_connect(&state).await
}

#[tauri::command]
pub async fn get_wallet_status(state: State<'_, AppState>) -> Result<WalletStatusResponse, String> {
    wallet_svc::get_wallet_status(&state).await
}

#[tauri::command]
pub async fn get_connection_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<ConnectionStatusResponse, String> {
    wallet_svc::get_connection_status(&state, &request_id).await
}

#[tauri::command]
pub async fn disconnect_wallet(state: State<'_, AppState>) -> Result<(), String> {
    wallet_svc::disconnect_wallet(&state).await
}

#[tauri::command]
pub async fn get_wallet_balance(
    state: State<'_, AppState>,
) -> Result<WalletBalanceResponse, String> {
    wallet_svc::get_wallet_balance(&state).await
}

#[tauri::command]
pub async fn get_recent_transactions(
    state: State<'_, AppState>,
    limit: u64,
) -> Result<RecentTxsResponse, String> {
    wallet_svc::get_recent_transactions(&state, limit).await
}

#[tauri::command]
pub async fn build_send_tx(
    _state: State<'_, AppState>,
    recipient_address: String,
    change_address: String,
    erg_nano: String,
    token_id: Option<String>,
    token_amount: Option<String>,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let parsed = super::parse_eip12_utxos(user_utxos)?;
    let response = wallet_svc::build_send_tx(
        &recipient_address,
        &change_address,
        &erg_nano,
        token_id.as_deref(),
        token_amount.as_deref(),
        parsed,
        current_height,
    )?;
    serde_json::to_value(&response).map_err(|e| format!("Failed to serialize response: {}", e))
}

#[tauri::command]
pub async fn get_user_utxos(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let utxos = wallet_svc::get_user_utxos(&state).await?;
    utxos
        .into_iter()
        .map(|u| serde_json::to_value(u).map_err(|e| format!("Failed to serialize UTXO: {}", e)))
        .collect()
}

#[tauri::command]
pub async fn validate_ergo_address(address: String) -> Result<String, String> {
    wallet_svc::validate_ergo_address(&address)
}
