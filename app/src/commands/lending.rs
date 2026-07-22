use citadel_api::dto::lending::{
    BorrowBuildRequest, LendBuildRequest, LendingBuildResponse, MarketsResponse, PositionsResponse,
    RefundBuildRequest, RepayBuildRequest, WithdrawBuildRequest,
};
use citadel_api::services::lending as lending_svc;
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_lending_markets(state: State<'_, AppState>) -> Result<MarketsResponse, String> {
    lending_svc::get_markets(&state).await
}

#[tauri::command]
pub async fn get_lending_positions(
    state: State<'_, AppState>,
    address: String,
) -> Result<PositionsResponse, String> {
    lending_svc::get_positions(&state, address).await
}

#[tauri::command]
pub async fn build_lend_tx(
    state: State<'_, AppState>,
    request: LendBuildRequest,
) -> Result<LendingBuildResponse, String> {
    lending_svc::build_lend(&state, request).await
}

#[tauri::command]
pub async fn build_withdraw_tx(
    state: State<'_, AppState>,
    request: WithdrawBuildRequest,
) -> Result<LendingBuildResponse, String> {
    lending_svc::build_withdraw(&state, request).await
}

#[tauri::command]
pub async fn build_borrow_tx(
    state: State<'_, AppState>,
    request: BorrowBuildRequest,
) -> Result<LendingBuildResponse, String> {
    lending_svc::build_borrow(&state, request).await
}

#[tauri::command]
pub async fn build_repay_tx(
    state: State<'_, AppState>,
    request: RepayBuildRequest,
) -> Result<LendingBuildResponse, String> {
    lending_svc::build_repay(&state, request).await
}

#[tauri::command]
pub async fn build_refund_tx(
    state: State<'_, AppState>,
    request: RefundBuildRequest,
) -> Result<LendingBuildResponse, String> {
    lending_svc::build_refund(&state, request).await
}

#[tauri::command]
pub async fn check_proxy_box(
    state: State<'_, AppState>,
    box_id: String,
) -> Result<serde_json::Value, String> {
    lending_svc::check_proxy_box(&state, box_id).await
}

#[tauri::command]
pub async fn discover_stuck_proxies(
    state: State<'_, AppState>,
    user_address: String,
) -> Result<serde_json::Value, String> {
    lending_svc::discover_stuck_proxies(&state, user_address).await
}

#[tauri::command]
pub async fn get_dex_price(
    state: State<'_, AppState>,
    dex_nft: String,
) -> Result<serde_json::Value, String> {
    lending_svc::get_dex_price(&state, dex_nft).await
}
