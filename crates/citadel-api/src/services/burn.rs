//! Token burn transaction building.

use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use serde::{Deserialize, Serialize};

use super::error::{IntoServiceError, ServiceResult};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub burned_token_id: String,
    #[serde(with = "crate::dto::u64_as_string")]
    pub burned_amount: u64,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
    pub change_erg: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiBurnBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub burned_tokens: Vec<BurnedTokenEntry>,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
    pub change_erg: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnedTokenEntry {
    pub token_id: String,
    #[serde(with = "crate::dto::u64_as_string")]
    pub amount: u64,
}

pub fn build_burn_tx(
    token_id: &str,
    burn_amount: u64,
    user_ergo_tree: &str,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<BurnBuildResponse> {
    if burn_amount == 0 {
        return Err("Burn amount must be greater than zero".to_string());
    }

    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let selected = ergo_tx::box_selector::select_inputs(
        &user_utxos,
        TX_FEE_NANO + citadel_fee + MIN_BOX_VALUE_NANO,
        Some((token_id, burn_amount as i64)),
    );

    if selected.is_empty() {
        return Err("No suitable UTXOs found for burn".to_string());
    }

    let selected_owned: Vec<ergo_tx::Eip12InputBox> = selected.into_iter().cloned().collect();

    let result = ergo_tx::build_burn_tx(
        &selected_owned,
        token_id,
        burn_amount,
        user_ergo_tree,
        current_height,
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(BurnBuildResponse {
        unsigned_tx: unsigned_tx_json,
        burned_token_id: result.summary.burned_token_id,
        burned_amount: result.summary.burned_amount,
        miner_fee: result.summary.miner_fee,
        citadel_fee_nano: result.summary.citadel_fee_nano,
        change_erg: result.summary.change_erg,
    })
}

pub fn build_multi_burn_tx(
    burn_items: Vec<BurnedTokenEntry>,
    user_ergo_tree: &str,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<MultiBurnBuildResponse> {
    if burn_items.is_empty() {
        return Err("Burn list must not be empty".to_string());
    }

    let required_tokens: Vec<(&str, u64)> = burn_items
        .iter()
        .map(|item| (item.token_id.as_str(), item.amount))
        .collect();

    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let min_erg = (TX_FEE_NANO + citadel_fee + MIN_BOX_VALUE_NANO) as u64;

    let selected =
        ergo_tx::select_multi_token_boxes(&user_utxos, &required_tokens, min_erg).into_service()?;

    let burn_items_for_builder: Vec<ergo_tx::BurnItem> = burn_items
        .iter()
        .map(|item| ergo_tx::BurnItem {
            token_id: item.token_id.clone(),
            amount: item.amount,
        })
        .collect();

    let result = ergo_tx::build_multi_burn_tx(
        &selected.boxes,
        &burn_items_for_builder,
        user_ergo_tree,
        current_height,
    )
    .into_service()?;

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
        citadel_fee_nano: result.summary.citadel_fee_nano,
        change_erg: result.summary.change_erg,
    })
}
