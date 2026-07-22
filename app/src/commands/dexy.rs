use citadel_api::dto::{
    DexyBuildRequest, DexyBuildResponse, DexyLpBuildResponse, DexyLpPreviewResponse,
    DexyPreviewRequest, DexyPreviewResponse, DexyStateResponse, DexySwapBuildResponse,
    DexySwapPreviewResponse,
};
use citadel_api::services::dexy as dexy_svc;
use citadel_api::AppState;
use dexy::rates::DexyRates;
use tauri::State;

#[tauri::command]
pub async fn get_dexy_state(
    state: State<'_, AppState>,
    variant: String,
) -> Result<DexyStateResponse, String> {
    dexy_svc::get_state(&state, &variant).await
}

#[tauri::command]
pub async fn get_dexy_rates(
    state: State<'_, AppState>,
    variant: String,
) -> Result<DexyRates, String> {
    dexy_svc::get_rates(&state, &variant).await
}

#[tauri::command]
pub async fn preview_mint_dexy(
    state: State<'_, AppState>,
    request: DexyPreviewRequest,
) -> Result<DexyPreviewResponse, String> {
    dexy_svc::preview_mint(&state, &request.variant, request.amount).await
}

#[tauri::command]
pub async fn build_mint_dexy(
    state: State<'_, AppState>,
    request: DexyBuildRequest,
) -> Result<DexyBuildResponse, String> {
    let user_utxos = super::parse_eip12_utxos(request.user_utxos)?;
    dexy_svc::build_mint(
        &state,
        &request.variant,
        request.amount,
        request.user_address,
        user_utxos,
        request.current_height,
        request.recipient_address,
    )
    .await
}

#[tauri::command]
pub async fn preview_dexy_swap(
    state: State<'_, AppState>,
    variant: String,
    direction: String,
    amount: i64,
    slippage: Option<f64>,
) -> Result<DexySwapPreviewResponse, String> {
    dexy_svc::preview_swap(&state, &variant, &direction, amount, slippage).await
}

#[tauri::command]
pub async fn build_dexy_swap_tx(
    state: State<'_, AppState>,
    variant: String,
    direction: String,
    amount: i64,
    min_output: i64,
    user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    recipient_address: Option<String>,
) -> Result<DexySwapBuildResponse, String> {
    let user_utxos = super::parse_eip12_utxos(user_utxos)?;
    dexy_svc::build_swap(
        &state,
        &variant,
        &direction,
        amount,
        min_output,
        user_address,
        user_utxos,
        current_height,
        recipient_address,
    )
    .await
}

#[tauri::command]
pub async fn preview_lp_deposit(
    state: State<'_, AppState>,
    variant: String,
    erg_amount: i64,
    dexy_amount: i64,
) -> Result<DexyLpPreviewResponse, String> {
    dexy_svc::preview_lp_deposit(&state, &variant, erg_amount, dexy_amount).await
}

#[tauri::command]
pub async fn build_lp_deposit_tx(
    state: State<'_, AppState>,
    variant: String,
    erg_amount: i64,
    dexy_amount: i64,
    user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    recipient_address: Option<String>,
) -> Result<DexyLpBuildResponse, String> {
    let user_utxos = super::parse_eip12_utxos(user_utxos)?;
    dexy_svc::build_lp_deposit(
        &state,
        &variant,
        erg_amount,
        dexy_amount,
        user_address,
        user_utxos,
        current_height,
        recipient_address,
    )
    .await
}

#[tauri::command]
pub async fn preview_lp_redeem(
    state: State<'_, AppState>,
    variant: String,
    lp_amount: i64,
) -> Result<DexyLpPreviewResponse, String> {
    dexy_svc::preview_lp_redeem(&state, &variant, lp_amount).await
}

#[tauri::command]
pub async fn build_lp_redeem_tx(
    state: State<'_, AppState>,
    variant: String,
    lp_amount: i64,
    user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    recipient_address: Option<String>,
) -> Result<DexyLpBuildResponse, String> {
    let user_utxos = super::parse_eip12_utxos(user_utxos)?;
    dexy_svc::build_lp_redeem(
        &state,
        &variant,
        lp_amount,
        user_address,
        user_utxos,
        current_height,
        recipient_address,
    )
    .await
}
