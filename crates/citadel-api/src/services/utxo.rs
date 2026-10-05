//! UTXO consolidation, split, and restructure transaction building.

use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use serde::{Deserialize, Serialize};

use super::error::{IntoServiceError, ServiceResult};
use crate::AppState;

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
    build_restructure_tx_with_owned_trees(
        selected_utxos,
        outputs,
        user_ergo_tree,
        &[user_ergo_tree],
        current_height,
    )
}

/// Resolve allowed input and output addresses from the connected wallet state.
pub async fn build_restructure_tx_for_wallet(
    state: &AppState,
    selected_utxos: Vec<ergo_tx::Eip12InputBox>,
    outputs: Vec<RestructureOutputInput>,
    user_ergo_tree: &str,
    current_height: i32,
) -> ServiceResult<RestructureBuildResponse> {
    let wallet = state.wallet().await.ok_or("Wallet not connected")?;
    let owned_trees: Result<Vec<String>, String> = wallet
        .addresses
        .iter()
        .map(|address| ergo_tx::address_to_ergo_tree(address).into_service())
        .collect();
    let owned_trees = owned_trees?;
    let owned_tree_refs: Vec<&str> = owned_trees.iter().map(String::as_str).collect();
    build_restructure_tx_with_owned_trees(
        selected_utxos,
        outputs,
        user_ergo_tree,
        &owned_tree_refs,
        current_height,
    )
}

fn build_restructure_tx_with_owned_trees(
    selected_utxos: Vec<ergo_tx::Eip12InputBox>,
    outputs: Vec<RestructureOutputInput>,
    user_ergo_tree: &str,
    owned_ergo_trees: &[&str],
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

    let result = ergo_tx::build_restructure_tx_with_owned_trees(
        &selected_utxos,
        &specs,
        user_ergo_tree,
        owned_ergo_trees,
        current_height,
    )
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const PRIMARY_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const FUNDING_TREE: &str =
        "0008cd0379be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const KEY: &str = "12fb8192304383f32a5eb880b3e32536036e9ef9150137555ebdde25d03943d0";

    fn input(id: &str, tree: &str, value: i64, key: bool) -> ergo_tx::Eip12InputBox {
        ergo_tx::Eip12InputBox {
            box_id: id.to_string(),
            transaction_id: "tx".to_string(),
            index: 0,
            value: value.to_string(),
            ergo_tree: tree.to_string(),
            assets: if key {
                vec![ergo_tx::Eip12Asset::new(KEY, 1)]
            } else {
                vec![]
            },
            creation_height: 1000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    fn output() -> Vec<RestructureOutputInput> {
        vec![RestructureOutputInput {
            value: MIN_BOX_VALUE_NANO,
            tokens: vec![RestructureTokenInput {
                token_id: KEY.to_string(),
                amount: "1".to_string(),
            }],
        }]
    }

    #[tokio::test]
    async fn restructure_ownership_comes_from_connected_wallet() {
        let state = AppState::new();
        let source = input("source", PRIMARY_TREE, 39_960, true);
        let funding = input("funding", FUNDING_TREE, 10_000_000, false);
        let error = build_restructure_tx_for_wallet(
            &state,
            vec![source.clone(), funding.clone()],
            output(),
            PRIMARY_TREE,
            50000,
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Wallet not connected");

        let primary_address = ergo_tx::address::ergo_tree_to_address(PRIMARY_TREE).unwrap();
        let funding_address = ergo_tx::address::ergo_tree_to_address(FUNDING_TREE).unwrap();
        state.set_wallet(primary_address.clone()).await.unwrap();
        let error = build_restructure_tx_for_wallet(
            &state,
            vec![source.clone(), funding.clone()],
            output(),
            PRIMARY_TREE,
            50000,
        )
        .await
        .unwrap_err();
        assert!(error.contains("does not belong to the wallet"));
        let error = build_restructure_tx_for_wallet(
            &state,
            vec![source.clone()],
            output(),
            FUNDING_TREE,
            50000,
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Output address does not belong to the wallet");

        state
            .set_wallet_addresses(primary_address, vec![funding_address])
            .await
            .unwrap();
        let result = build_restructure_tx_for_wallet(
            &state,
            vec![source, funding],
            output(),
            PRIMARY_TREE,
            50000,
        )
        .await
        .unwrap();
        assert_eq!(result.input_count, 2);
        assert_eq!(result.output_count, 2);
        assert!(result.has_change);
        assert_eq!(result.total_erg_in, 10_039_960);
        let transaction: ergo_tx::Eip12UnsignedTx =
            serde_json::from_value(result.unsigned_tx).unwrap();
        assert_eq!(transaction.inputs[1].ergo_tree, FUNDING_TREE);
        assert_eq!(transaction.outputs[0].ergo_tree, PRIMARY_TREE);
        assert_eq!(transaction.outputs[0].assets[0].token_id, KEY);
    }
}
