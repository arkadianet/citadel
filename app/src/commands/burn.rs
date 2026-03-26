use citadel_api::AppState;
use serde::Serialize;
use tauri::State;

use super::StrErr;

/// Response for building a burn transaction
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub burned_token_id: String,
    pub burned_amount: u64,
    pub miner_fee: i64,
    pub change_erg: i64,
}

/// Build a token burn transaction
#[tauri::command]
pub async fn build_burn_tx(
    _state: State<'_, AppState>,
    token_id: String,
    burn_amount: u64,
    user_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<BurnBuildResponse, String> {
    if burn_amount == 0 {
        return Err("Burn amount must be greater than zero".to_string());
    }

    // Parse user UTXOs from JSON
    let inputs = super::parse_eip12_utxos(user_utxos)?;

    // Select inputs that have the token + enough ERG for fees
    let selected = ergo_tx::box_selector::select_inputs(
        &inputs,
        citadel_core::constants::TX_FEE_NANO + citadel_core::constants::MIN_BOX_VALUE_NANO,
        Some((&token_id, burn_amount as i64)),
    );

    if selected.is_empty() {
        return Err("No suitable UTXOs found for burn".to_string());
    }

    let selected_owned: Vec<ergo_tx::Eip12InputBox> = selected.into_iter().cloned().collect();

    let result = ergo_tx::build_burn_tx(
        &selected_owned,
        &token_id,
        burn_amount,
        &user_ergo_tree,
        current_height,
    )
    .str_err()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(BurnBuildResponse {
        unsigned_tx: unsigned_tx_json,
        burned_token_id: result.summary.burned_token_id,
        burned_amount: result.summary.burned_amount,
        miner_fee: result.summary.miner_fee,
        change_erg: result.summary.change_erg,
    })
}

// =============================================================================
// Multi-Token Burn
// =============================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiBurnBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub burned_tokens: Vec<BurnedTokenEntry>,
    pub miner_fee: i64,
    pub change_erg: i64,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnedTokenEntry {
    pub token_id: String,
    pub amount: u64,
}

/// Build a multi-token burn transaction
#[tauri::command]
pub async fn build_multi_burn_tx(
    _state: State<'_, AppState>,
    burn_items: Vec<serde_json::Value>,
    user_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<MultiBurnBuildResponse, String> {
    // Parse burn items from JSON
    let parsed_items: Vec<BurnedTokenEntry> = burn_items
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(|e| format!("Invalid burn item: {}", e)))
        .collect::<Result<Vec<_>, _>>()?;

    if parsed_items.is_empty() {
        return Err("Burn list must not be empty".to_string());
    }

    // Parse user UTXOs from JSON
    let inputs = super::parse_eip12_utxos(user_utxos)?;

    // Build required tokens list for selection
    let required_tokens: Vec<(&str, u64)> = parsed_items
        .iter()
        .map(|item| (item.token_id.as_str(), item.amount))
        .collect();

    let min_erg =
        (citadel_core::constants::TX_FEE_NANO + citadel_core::constants::MIN_BOX_VALUE_NANO) as u64;

    // Select inputs covering all required tokens + ERG for fees
    let selected = ergo_tx::select_multi_token_boxes(&inputs, &required_tokens, min_erg)
        .str_err()?;

    // Convert to BurnItem for the builder
    let burn_items: Vec<ergo_tx::BurnItem> = parsed_items
        .iter()
        .map(|item| ergo_tx::BurnItem {
            token_id: item.token_id.clone(),
            amount: item.amount,
        })
        .collect();

    let result = ergo_tx::build_multi_burn_tx(
        &selected.boxes,
        &burn_items,
        &user_ergo_tree,
        current_height,
    )
    .str_err()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(MultiBurnBuildResponse {
        unsigned_tx: unsigned_tx_json,
        burned_tokens: result
            .summary
            .burned_tokens
            .iter()
            .map(|b| BurnedTokenEntry {
                token_id: b.token_id.clone(),
                amount: b.amount,
            })
            .collect(),
        miner_fee: result.summary.miner_fee,
        change_erg: result.summary.change_erg,
    })
}

