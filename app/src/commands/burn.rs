use citadel_api::services::burn as burn_svc;
use citadel_api::AppState;
use tauri::State;

pub use burn_svc::{BurnBuildResponse, BurnedTokenEntry, MultiBurnBuildResponse};

#[tauri::command]
pub async fn build_burn_tx(
    _state: State<'_, AppState>,
    token_id: String,
    burn_amount: String,
    user_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<BurnBuildResponse, String> {
    let burn_amount: u64 = burn_amount
        .parse()
        .map_err(|e| format!("Invalid burn amount '{}': {}", burn_amount, e))?;
    let inputs = super::parse_eip12_utxos(user_utxos)?;
    burn_svc::build_burn_tx(
        &token_id,
        burn_amount,
        &user_ergo_tree,
        inputs,
        current_height,
    )
}

#[tauri::command]
pub async fn build_multi_burn_tx(
    _state: State<'_, AppState>,
    burn_items: Vec<serde_json::Value>,
    user_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<MultiBurnBuildResponse, String> {
    let parsed_items: Vec<BurnedTokenEntry> = burn_items
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(|e| format!("Invalid burn item: {}", e)))
        .collect::<Result<Vec<_>, _>>()?;
    let inputs = super::parse_eip12_utxos(user_utxos)?;
    burn_svc::build_multi_burn_tx(parsed_items, &user_ergo_tree, inputs, current_height)
}
