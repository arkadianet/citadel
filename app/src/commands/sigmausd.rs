use citadel_api::dto::{
    MintBuildRequest, MintBuildResponse, MintPreviewRequest, MintPreviewResponse,
    OraclePriceResponse, SigmaUsdBuildRequest, SigmaUsdBuildResponse, SigmaUsdPreviewRequest,
    SigmaUsdPreviewResponse,
};
use citadel_api::services::sigmausd as sigmausd_svc;
use citadel_api::AppState;
use sigmausd::SigmaUsdState;
use tauri::State;

#[tauri::command]
pub async fn get_sigmausd_state(state: State<'_, AppState>) -> Result<SigmaUsdState, String> {
    sigmausd_svc::get_state(&state).await
}

#[tauri::command]
pub async fn get_oracle_price(state: State<'_, AppState>) -> Result<OraclePriceResponse, String> {
    sigmausd_svc::get_oracle_price(&state).await
}

#[tauri::command]
pub async fn preview_mint_sigusd(
    state: State<'_, AppState>,
    request: MintPreviewRequest,
) -> Result<MintPreviewResponse, String> {
    sigmausd_svc::preview_mint_sigusd(&state, request.amount).await
}

#[tauri::command]
pub async fn build_mint_sigusd(
    state: State<'_, AppState>,
    request: MintBuildRequest,
) -> Result<MintBuildResponse, String> {
    let user_utxos = super::parse_eip12_utxos(request.user_utxos)?;
    sigmausd_svc::build_mint_sigusd(
        &state,
        request.amount,
        request.user_address,
        user_utxos,
        request.current_height,
    )
    .await
}

#[tauri::command]
pub async fn preview_sigmausd_tx(
    state: State<'_, AppState>,
    request: SigmaUsdPreviewRequest,
) -> Result<SigmaUsdPreviewResponse, String> {
    sigmausd_svc::preview_sigmausd_tx(&state, &request.action, request.amount).await
}

#[tauri::command]
pub async fn build_sigmausd_tx(
    state: State<'_, AppState>,
    request: SigmaUsdBuildRequest,
) -> Result<SigmaUsdBuildResponse, String> {
    let user_utxos = super::parse_eip12_utxos(request.user_utxos)?;
    sigmausd_svc::build_sigmausd_tx(
        &state,
        &request.action,
        request.amount,
        request.user_address,
        user_utxos,
        request.current_height,
        request.recipient_address,
    )
    .await
}
