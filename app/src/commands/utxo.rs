use citadel_api::dto::{MintSignRequest, MintSignResponse, MintTxStatusResponse};
use citadel_api::AppState;
use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use serde::Serialize;
use tauri::State;

/// Response for building a consolidation transaction
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidateBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub input_count: usize,
    pub total_erg_in: i64,
    pub change_erg: i64,
    pub token_count: usize,
    pub miner_fee: i64,
}

/// Response for building a split transaction
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub split_count: usize,
    pub amount_per_box: String,
    pub total_split: String,
    pub change_erg: i64,
    pub miner_fee: i64,
}

/// Build a UTXO consolidation transaction
#[tauri::command]
pub async fn build_consolidate_tx(
    _state: State<'_, AppState>,
    selected_utxos: Vec<serde_json::Value>,
    user_ergo_tree: String,
    current_height: i32,
) -> Result<ConsolidateBuildResponse, String> {
    let inputs = super::parse_eip12_utxos(selected_utxos)?;

    let result = ergo_tx::build_consolidate_tx(&inputs, &user_ergo_tree, current_height)
        .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(ConsolidateBuildResponse {
        unsigned_tx: unsigned_tx_json,
        input_count: result.summary.input_count,
        total_erg_in: result.summary.total_erg_in,
        change_erg: result.summary.change_erg,
        token_count: result.summary.token_count,
        miner_fee: result.summary.miner_fee,
    })
}

/// Build a UTXO split transaction
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

    let mode = match split_mode.as_str() {
        "erg" => {
            let amount: i64 = amount_per_box
                .parse()
                .map_err(|_| "Invalid amount_per_box".to_string())?;
            ergo_tx::SplitMode::Erg {
                amount_per_box: amount,
            }
        }
        "token" => {
            let tid = token_id.ok_or("token_id is required for token split")?;
            let amount: u64 = amount_per_box
                .parse()
                .map_err(|_| "Invalid amount_per_box".to_string())?;
            let epb = erg_per_box.unwrap_or(MIN_BOX_VALUE_NANO);
            ergo_tx::SplitMode::Token {
                token_id: tid,
                amount_per_box: amount,
                erg_per_box: epb,
            }
        }
        _ => return Err(format!("Unknown split_mode: {}", split_mode)),
    };

    // Select inputs based on mode
    let selected = match &mode {
        ergo_tx::SplitMode::Erg { amount_per_box } => {
            let total_needed =
                (*amount_per_box * count as i64 + TX_FEE_NANO + MIN_BOX_VALUE_NANO) as u64;
            ergo_tx::select_erg_boxes(&all_inputs, total_needed).map_err(|e| e.to_string())?
        }
        ergo_tx::SplitMode::Token {
            token_id,
            amount_per_box,
            erg_per_box,
        } => {
            let total_tokens = *amount_per_box * count as u64;
            let total_erg = (*erg_per_box * count as i64 + TX_FEE_NANO + MIN_BOX_VALUE_NANO) as u64;
            ergo_tx::select_token_boxes(&all_inputs, token_id, total_tokens, total_erg)
                .map_err(|e| e.to_string())?
        }
    };

    let result = ergo_tx::build_split_tx(
        &selected.boxes,
        &mode,
        count,
        &user_ergo_tree,
        current_height,
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(SplitBuildResponse {
        unsigned_tx: unsigned_tx_json,
        split_count: result.summary.split_count,
        amount_per_box: result.summary.amount_per_box,
        total_split: result.summary.total_split,
        change_erg: result.summary.change_erg,
        miner_fee: result.summary.miner_fee,
    })
}

/// Start ErgoPay signing flow for a UTXO management transaction
#[tauri::command]
pub async fn start_utxo_mgmt_sign(
    state: State<'_, AppState>,
    unsigned_tx: serde_json::Value,
    message: String,
) -> Result<MintSignResponse, String> {
    super::start_mint_sign(
        state,
        MintSignRequest {
            unsigned_tx,
            message,
        },
    )
    .await
}

/// Get status of a UTXO management transaction signing request
#[tauri::command]
pub async fn get_utxo_mgmt_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    super::get_mint_tx_status(state, request_id).await
}
