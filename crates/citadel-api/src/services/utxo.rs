//! UTXO consolidation, split, and restructure transaction building.

use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use serde::{Deserialize, Serialize};

use super::error::{IntoServiceError, ServiceResult};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidateBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub input_count: usize,
    pub total_erg_in: i64,
    pub change_erg: i64,
    pub token_count: usize,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub split_count: usize,
    pub amount_per_box: String,
    pub total_split: String,
    pub change_erg: i64,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestructureOutputInput {
    pub value: i64,
    pub tokens: Vec<RestructureTokenInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestructureTokenInput {
    pub token_id: String,
    pub amount: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestructureBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub input_count: usize,
    pub output_count: usize,
    pub total_erg_in: i64,
    pub allocated_erg: i64,
    pub change_erg: i64,
    pub has_change: bool,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
}

pub fn build_consolidate_tx(
    selected_utxos: Vec<ergo_tx::Eip12InputBox>,
    user_ergo_tree: &str,
    current_height: i32,
) -> ServiceResult<ConsolidateBuildResponse> {
    let result = ergo_tx::build_consolidate_tx(&selected_utxos, user_ergo_tree, current_height)
        .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(ConsolidateBuildResponse {
        unsigned_tx: unsigned_tx_json,
        input_count: result.summary.input_count,
        total_erg_in: result.summary.total_erg_in,
        change_erg: result.summary.change_erg,
        token_count: result.summary.token_count,
        miner_fee: result.summary.miner_fee,
        citadel_fee_nano: result.summary.citadel_fee_nano,
    })
}

pub fn build_split_tx(
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    user_ergo_tree: &str,
    current_height: i32,
    split_mode: &str,
    amount_per_box: &str,
    count: usize,
    token_id: Option<&str>,
    erg_per_box: Option<i64>,
) -> ServiceResult<SplitBuildResponse> {
    let mode = match split_mode {
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
                token_id: tid.to_string(),
                amount_per_box: amount,
                erg_per_box: epb,
            }
        }
        _ => return Err(format!("Unknown split_mode: {}", split_mode)),
    };

    let citadel_fee = ergo_tx::resolved_dev_fee_config().budget();
    let selected = match &mode {
        ergo_tx::SplitMode::Erg { amount_per_box } => {
            let total_needed =
                (*amount_per_box * count as i64 + TX_FEE_NANO + citadel_fee + MIN_BOX_VALUE_NANO)
                    as u64;
            ergo_tx::select_erg_boxes(&user_utxos, total_needed).into_service()?
        }
        ergo_tx::SplitMode::Token {
            token_id,
            amount_per_box,
            erg_per_box,
        } => {
            let total_tokens = *amount_per_box * count as u64;
            let total_erg =
                (*erg_per_box * count as i64 + TX_FEE_NANO + citadel_fee + MIN_BOX_VALUE_NANO)
                    as u64;
            ergo_tx::select_token_boxes(&user_utxos, token_id, total_tokens, total_erg)
                .into_service()?
        }
    };

    let result = ergo_tx::build_split_tx(
        &selected.boxes,
        &mode,
        count,
        user_ergo_tree,
        current_height,
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(SplitBuildResponse {
        unsigned_tx: unsigned_tx_json,
        split_count: result.summary.split_count,
        amount_per_box: result.summary.amount_per_box,
        total_split: result.summary.total_split,
        change_erg: result.summary.change_erg,
        miner_fee: result.summary.miner_fee,
        citadel_fee_nano: result.summary.citadel_fee_nano,
    })
}

pub fn build_restructure_tx(
    selected_utxos: Vec<ergo_tx::Eip12InputBox>,
    outputs: Vec<RestructureOutputInput>,
    user_ergo_tree: &str,
    current_height: i32,
) -> ServiceResult<RestructureBuildResponse> {
    let specs: Result<Vec<ergo_tx::RestructureOutputSpec>, String> = outputs
        .into_iter()
        .map(|o| {
            let tokens: Result<Vec<(String, u64)>, String> = o
                .tokens
                .into_iter()
                .map(|t| {
                    let amt: u64 = t
                        .amount
                        .parse()
                        .map_err(|_| format!("Invalid token amount for {}", t.token_id))?;
                    Ok((t.token_id, amt))
                })
                .collect();
            Ok(ergo_tx::RestructureOutputSpec {
                value: o.value,
                tokens: tokens?,
            })
        })
        .collect();
    let specs = specs?;

    let result =
        ergo_tx::build_restructure_tx(&selected_utxos, &specs, user_ergo_tree, current_height)
            .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(RestructureBuildResponse {
        unsigned_tx: unsigned_tx_json,
        input_count: result.summary.input_count,
        output_count: result.summary.output_count,
        total_erg_in: result.summary.total_erg_in,
        allocated_erg: result.summary.allocated_erg,
        change_erg: result.summary.change_erg,
        has_change: result.summary.has_change,
        miner_fee: result.summary.miner_fee,
        citadel_fee_nano: result.summary.citadel_fee_nano,
    })
}
