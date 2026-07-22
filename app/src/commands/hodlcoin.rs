use citadel_api::services::hodlcoin as hodl_svc;
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_hodlcoin_banks(
    state: State<'_, AppState>,
) -> Result<Vec<hodlcoin::HodlBankState>, String> {
    hodl_svc::get_banks(&state).await
}

#[tauri::command]
pub async fn preview_hodlcoin_mint(
    state: State<'_, AppState>,
    singleton_token_id: String,
    erg_amount: i64,
) -> Result<hodlcoin::HodlMintPreview, String> {
    hodl_svc::preview_mint(&state, &singleton_token_id, erg_amount).await
}

#[tauri::command]
pub async fn preview_hodlcoin_burn(
    state: State<'_, AppState>,
    singleton_token_id: String,
    hodl_amount: i64,
) -> Result<hodlcoin::HodlBurnPreview, String> {
    hodl_svc::preview_burn(&state, &singleton_token_id, hodl_amount).await
}

#[tauri::command]
pub async fn build_hodlcoin_mint_tx(
    state: State<'_, AppState>,
    singleton_token_id: String,
    erg_amount: i64,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let parsed = super::parse_eip12_utxos(user_utxos)?;
    let tx = hodl_svc::build_mint_tx(
        &state,
        &singleton_token_id,
        erg_amount,
        parsed,
        current_height,
    )
    .await?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize transaction: {}", e))
}

#[tauri::command]
pub async fn build_hodlcoin_burn_tx(
    state: State<'_, AppState>,
    singleton_token_id: String,
    hodl_amount: i64,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let parsed = super::parse_eip12_utxos(user_utxos)?;
    let tx = hodl_svc::build_burn_tx(
        &state,
        &singleton_token_id,
        hodl_amount,
        parsed,
        current_height,
    )
    .await?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize transaction: {}", e))
}
