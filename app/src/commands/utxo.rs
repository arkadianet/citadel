use citadel_api::dto::{MintSignRequest, MintSignResponse, MintTxStatusResponse};
use citadel_api::services::signing as sign_svc;
use citadel_api::services::utxo as utxo_svc;
use citadel_api::AppState;
use tauri::State;

pub use utxo_svc::{
    ConsolidateBuildResponse, RestructureBuildResponse, RestructureOutputInput,
    RestructureTokenInput, SplitBuildResponse,
};

#[tauri::command]
pub async fn build_consolidate_tx(
    _state: State<'_, AppState>,
    selected_utxos: Vec<serde_json::Value>,
    user_ergo_tree: String,
    current_height: i32,
) -> Result<ConsolidateBuildResponse, String> {
    let inputs = super::parse_eip12_utxos(selected_utxos)?;
    utxo_svc::build_consolidate_tx(inputs, &user_ergo_tree, current_height)
}

#[tauri::command]
pub async fn build_split_tx(
    _state: State<'_, AppState>,
    user_utxos: Vec<serde_json::Value>,
    user_ergo_tree: String,
    current_height: i32,
    split_mode: String,
    amount_per_box: String,
    count: usize,
    token_id: Option<String>,
    erg_per_box: Option<i64>,
) -> Result<SplitBuildResponse, String> {
    let all_inputs = super::parse_eip12_utxos(user_utxos)?;
    utxo_svc::build_split_tx(
        all_inputs,
        &user_ergo_tree,
        current_height,
        &split_mode,
        &amount_per_box,
        count,
        token_id.as_deref(),
        erg_per_box,
    )
}

#[tauri::command]
pub async fn build_restructure_tx(
    _state: State<'_, AppState>,
    selected_utxos: Vec<serde_json::Value>,
    outputs: Vec<RestructureOutputInput>,
    user_ergo_tree: String,
    current_height: i32,
) -> Result<RestructureBuildResponse, String> {
    let inputs = super::parse_eip12_utxos(selected_utxos)?;
    utxo_svc::build_restructure_tx(inputs, outputs, &user_ergo_tree, current_height)
}

#[tauri::command]
pub async fn start_utxo_mgmt_sign(
    state: State<'_, AppState>,
    unsigned_tx: serde_json::Value,
    message: String,
) -> Result<MintSignResponse, String> {
    sign_svc::start_mint_sign(
        &state,
        MintSignRequest {
            unsigned_tx,
            message,
        },
    )
    .await
}

#[tauri::command]
pub async fn get_utxo_mgmt_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    sign_svc::get_mint_tx_status(&state, &request_id).await
}
