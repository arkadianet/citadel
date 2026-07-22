use std::collections::HashMap;

use crate::dev_fee::{append_dev_fee_output, resolved_config};
use crate::eip12::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};

use citadel_core::constants::{MIN_BOX_VALUE_NANO as MIN_BOX_VALUE, TX_FEE_NANO as TX_FEE};

// App UX caps — Ergo protocol allows up to 32_767 inputs/outputs per tx;
// block cost/size may still limit practical size before that hard ceiling.
const MAX_SPLIT_OUTPUTS: usize = 30;
const MAX_RESTRUCTURE_OUTPUTS: usize = 120;
const MAX_TOKENS_PER_BOX: usize = 255;

#[derive(Debug, thiserror::Error)]
pub enum UtxoManagementError {
    #[error("No inputs provided")]
    NoInputs,

    #[error("Consolidation requires at least 2 inputs, got {0}")]
    TooFewInputs(usize),

    #[error("Insufficient ERG: have {have} nanoERG, need {need} nanoERG")]
    InsufficientErg { have: i64, need: i64 },

    #[error("Insufficient tokens: have {have} of {token_id}, need {need}")]
    InsufficientTokens {
        token_id: String,
        have: u64,
        need: u64,
    },

    #[error("Split count must be at least 1")]
    ZeroSplitCount,

    #[error("Split amount must be greater than zero")]
    ZeroSplitAmount,

    #[error("Too many outputs: {count} exceeds maximum of {max}")]
    TooManyOutputs { count: usize, max: usize },

    #[error("Output value {value} nanoERG is below minimum box value of {min} nanoERG")]
    BelowMinBoxValue { value: i64, min: i64 },

    #[error("Too many distinct token types: {count} exceeds maximum of {max}")]
    TooManyTokenTypes { count: usize, max: usize },

    #[error("Change amount {change} nanoERG is below minimum box value of {min} nanoERG (not enough to create change output)")]
    ChangeBelowMin { change: i64, min: i64 },

    #[error("Restructure requires at least 1 user output")]
    NoOutputs,

    #[error("Input box {box_id} does not belong to the wallet (ErgoTree mismatch)")]
    InputNotOwned { box_id: String },

    #[error("Token {token_id} over-allocated: assigned {assigned}, available {available}")]
    TokenOverAllocated {
        token_id: String,
        assigned: u64,
        available: u64,
    },

    #[error("Unassigned tokens remain ({count} type(s)); assign them to an output or leave ERG for automatic change")]
    UnassignedTokens { count: usize },

    #[error("ERG over-allocated: outputs use {allocated} nanoERG but only {available} available after fee")]
    ErgOverAllocated { allocated: i64, available: i64 },

    #[error("Citadel fee config error: {0}")]
    DevFee(String),
}

#[derive(Debug)]
pub struct ConsolidateSummary {
    pub input_count: usize,
    pub total_erg_in: i64,
    pub change_erg: i64,
    pub token_count: usize,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
}

#[derive(Debug)]
pub struct ConsolidateBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: ConsolidateSummary,
}

pub fn build_consolidate_tx(
    user_inputs: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<ConsolidateBuildResult, UtxoManagementError> {
    if user_inputs.is_empty() {
        return Err(UtxoManagementError::NoInputs);
    }
    if user_inputs.len() < 2 {
        return Err(UtxoManagementError::TooFewInputs(user_inputs.len()));
    }

    let total_erg: i64 = user_inputs
        .iter()
        .map(|b| b.value.parse::<i64>().unwrap_or(0))
        .sum();

    let fee_cfg = resolved_config();
    let citadel_fee = fee_cfg.budget();

    let min_needed = TX_FEE + citadel_fee + MIN_BOX_VALUE;
    if total_erg < min_needed {
        return Err(UtxoManagementError::InsufficientErg {
            have: total_erg,
            need: min_needed,
        });
    }

    let mut token_totals: HashMap<String, u64> = HashMap::new();
    for input in user_inputs {
        for asset in &input.assets {
            let amount = asset.amount.parse::<u64>().unwrap_or(0);
            *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
        }
    }

    if token_totals.len() > 255 {
        return Err(UtxoManagementError::TooManyTokenTypes {
            count: token_totals.len(),
            max: 255,
        });
    }

    let change_erg = total_erg - TX_FEE - citadel_fee;
    let token_count = token_totals.len();

    let change_assets: Vec<Eip12Asset> = token_totals
        .into_iter()
        .map(|(id, amt)| Eip12Asset::new(id, amt as i64))
        .collect();

    let change_output = Eip12Output {
        value: change_erg.to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: change_assets,
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let mut outputs = vec![change_output];
    append_dev_fee_output(&mut outputs, &fee_cfg, current_height)
        .map_err(|e| UtxoManagementError::DevFee(e.to_string()))?;
    outputs.push(Eip12Output::fee(TX_FEE, current_height));

    let unsigned_tx = Eip12UnsignedTx {
        inputs: user_inputs.to_vec(),
        data_inputs: vec![],
        outputs,
    };

    Ok(ConsolidateBuildResult {
        unsigned_tx,
        summary: ConsolidateSummary {
            input_count: user_inputs.len(),
            total_erg_in: total_erg,
            change_erg,
            token_count,
            miner_fee: TX_FEE,
            citadel_fee_nano: citadel_fee,
        },
    })
}

#[derive(Debug, Clone)]
pub enum SplitMode {
    Erg { amount_per_box: i64 },
    Token {
        token_id: String,
        amount_per_box: u64,
        erg_per_box: i64,
    },
}

#[derive(Debug)]
pub struct SplitSummary {
    pub split_count: usize,
    pub amount_per_box: String,
    pub total_split: String,
    pub change_erg: i64,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
}

#[derive(Debug)]
pub struct SplitBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: SplitSummary,
}

pub fn build_split_tx(
    user_inputs: &[Eip12InputBox],
    mode: &SplitMode,
    count: usize,
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<SplitBuildResult, UtxoManagementError> {
    if user_inputs.is_empty() {
        return Err(UtxoManagementError::NoInputs);
    }
    if count == 0 {
        return Err(UtxoManagementError::ZeroSplitCount);
    }
    if count > MAX_SPLIT_OUTPUTS {
        return Err(UtxoManagementError::TooManyOutputs {
            count,
            max: MAX_SPLIT_OUTPUTS,
        });
    }

    let total_erg: i64 = user_inputs
        .iter()
        .map(|b| b.value.parse::<i64>().unwrap_or(0))
        .sum();

    match mode {
        SplitMode::Erg { amount_per_box } => {
            if *amount_per_box <= 0 {
                return Err(UtxoManagementError::ZeroSplitAmount);
            }
            if *amount_per_box < MIN_BOX_VALUE {
                return Err(UtxoManagementError::BelowMinBoxValue {
                    value: *amount_per_box,
                    min: MIN_BOX_VALUE,
                });
            }

            let fee_cfg = resolved_config();
            let citadel_fee = fee_cfg.budget();

            let split_total = *amount_per_box * count as i64;
            let min_without_change = split_total + TX_FEE + citadel_fee;
            if total_erg < min_without_change {
                return Err(UtxoManagementError::InsufficientErg {
                    have: total_erg,
                    need: min_without_change,
                });
            }

            let remainder = total_erg - split_total - TX_FEE - citadel_fee;

            let mut token_totals: HashMap<String, u64> = HashMap::new();
            for input in user_inputs {
                for asset in &input.assets {
                    let amount = asset.amount.parse::<u64>().unwrap_or(0);
                    *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
                }
            }
            let has_tokens = !token_totals.is_empty();

            let need_change = remainder > 0 || has_tokens;
            if need_change && remainder > 0 && remainder < MIN_BOX_VALUE {
                return Err(UtxoManagementError::ChangeBelowMin {
                    change: remainder,
                    min: MIN_BOX_VALUE,
                });
            }
            if has_tokens && remainder < MIN_BOX_VALUE {
                return Err(UtxoManagementError::InsufficientErg {
                    have: total_erg,
                    need: split_total + TX_FEE + citadel_fee + MIN_BOX_VALUE,
                });
            }

            let mut outputs = Vec::with_capacity(count + 3);

            for _ in 0..count {
                outputs.push(Eip12Output {
                    value: amount_per_box.to_string(),
                    ergo_tree: user_ergo_tree.to_string(),
                    assets: vec![],
                    creation_height: current_height,
                    additional_registers: HashMap::new(),
                });
            }

            if need_change && (remainder > 0 || has_tokens) {
                let change_assets: Vec<Eip12Asset> = token_totals
                    .into_iter()
                    .map(|(id, amt)| Eip12Asset::new(id, amt as i64))
                    .collect();

                let change_value = if remainder > 0 {
                    remainder
                } else {
                    MIN_BOX_VALUE
                };
                outputs.push(Eip12Output {
                    value: change_value.to_string(),
                    ergo_tree: user_ergo_tree.to_string(),
                    assets: change_assets,
                    creation_height: current_height,
                    additional_registers: HashMap::new(),
                });
            }

            append_dev_fee_output(&mut outputs, &fee_cfg, current_height)
                .map_err(|e| UtxoManagementError::DevFee(e.to_string()))?;
            outputs.push(Eip12Output::fee(TX_FEE, current_height));

            let unsigned_tx = Eip12UnsignedTx {
                inputs: user_inputs.to_vec(),
                data_inputs: vec![],
                outputs,
            };

            Ok(SplitBuildResult {
                unsigned_tx,
                summary: SplitSummary {
                    split_count: count,
                    amount_per_box: amount_per_box.to_string(),
                    total_split: split_total.to_string(),
                    change_erg: remainder,
                    miner_fee: TX_FEE,
                    citadel_fee_nano: citadel_fee,
                },
            })
        }

        SplitMode::Token {
            token_id,
            amount_per_box,
            erg_per_box,
        } => {
            if *amount_per_box == 0 {
                return Err(UtxoManagementError::ZeroSplitAmount);
            }
            if *erg_per_box < MIN_BOX_VALUE {
                return Err(UtxoManagementError::BelowMinBoxValue {
                    value: *erg_per_box,
                    min: MIN_BOX_VALUE,
                });
            }

            let fee_cfg = resolved_config();
            let citadel_fee = fee_cfg.budget();

            let total_token_needed = *amount_per_box * count as u64;
            let erg_for_splits = *erg_per_box * count as i64;

            let total_token: u64 = user_inputs
                .iter()
                .flat_map(|b| b.assets.iter())
                .filter(|a| a.token_id == *token_id)
                .map(|a| a.amount.parse::<u64>().unwrap_or(0))
                .sum();

            if total_token < total_token_needed {
                return Err(UtxoManagementError::InsufficientTokens {
                    token_id: token_id.clone(),
                    have: total_token,
                    need: total_token_needed,
                });
            }

            let min_erg = erg_for_splits + TX_FEE + citadel_fee + MIN_BOX_VALUE;
            if total_erg < min_erg {
                return Err(UtxoManagementError::InsufficientErg {
                    have: total_erg,
                    need: min_erg,
                });
            }

            let change_erg = total_erg - erg_for_splits - TX_FEE - citadel_fee;

            let mut token_totals: HashMap<String, u64> = HashMap::new();
            for input in user_inputs {
                for asset in &input.assets {
                    let amount = asset.amount.parse::<u64>().unwrap_or(0);
                    *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
                }
            }

            if let Some(balance) = token_totals.get_mut(token_id) {
                *balance = balance.saturating_sub(total_token_needed);
                if *balance == 0 {
                    token_totals.remove(token_id);
                }
            }

            let mut outputs = Vec::with_capacity(count + 3);

            for _ in 0..count {
                outputs.push(Eip12Output {
                    value: erg_per_box.to_string(),
                    ergo_tree: user_ergo_tree.to_string(),
                    assets: vec![Eip12Asset::new(token_id.clone(), *amount_per_box as i64)],
                    creation_height: current_height,
                    additional_registers: HashMap::new(),
                });
            }

            let change_assets: Vec<Eip12Asset> = token_totals
                .into_iter()
                .map(|(id, amt)| Eip12Asset::new(id, amt as i64))
                .collect();

            outputs.push(Eip12Output {
                value: change_erg.to_string(),
                ergo_tree: user_ergo_tree.to_string(),
                assets: change_assets,
                creation_height: current_height,
                additional_registers: HashMap::new(),
            });

            append_dev_fee_output(&mut outputs, &fee_cfg, current_height)
                .map_err(|e| UtxoManagementError::DevFee(e.to_string()))?;
            outputs.push(Eip12Output::fee(TX_FEE, current_height));

            let unsigned_tx = Eip12UnsignedTx {
                inputs: user_inputs.to_vec(),
                data_inputs: vec![],
                outputs,
            };

            Ok(SplitBuildResult {
                unsigned_tx,
                summary: SplitSummary {
                    split_count: count,
                    amount_per_box: amount_per_box.to_string(),
                    total_split: total_token_needed.to_string(),
                    change_erg,
                    miner_fee: TX_FEE,
                    citadel_fee_nano: citadel_fee,
                },
            })
        }
    }
}

/// One user-defined output for a restructure transaction.
#[derive(Debug, Clone)]
pub struct RestructureOutputSpec {
    /// nanoERG for this output (must be ≥ MIN_BOX_VALUE)
    pub value: i64,
    /// Tokens assigned to this output
    pub tokens: Vec<(String, u64)>,
}

#[derive(Debug)]
pub struct RestructureSummary {
    pub input_count: usize,
    pub output_count: usize,
    pub total_erg_in: i64,
    pub allocated_erg: i64,
    pub change_erg: i64,
    pub has_change: bool,
    pub miner_fee: i64,
    pub citadel_fee_nano: i64,
}

#[derive(Debug)]
pub struct RestructureBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: RestructureSummary,
}

/// Build a free-form restructure tx from selected inputs and user-defined outputs.
///
/// - All inputs must match `user_ergo_tree` (ownership check).
/// - User outputs receive the specified ERG + tokens (capped at
///   `MAX_RESTRUCTURE_OUTPUTS` — an app UX limit, not an Ergo protocol ceiling).
/// - Unassigned ERG (after fee) and any unassigned tokens form an automatic change
///   output when needed. If tokens remain unassigned but change ERG would be 0,
///   returns an error (need room for a change box).
/// - Miner fee is always `TX_FEE`. Citadel app fee is appended before miner fee when enabled.
pub fn build_restructure_tx(
    user_inputs: &[Eip12InputBox],
    outputs: &[RestructureOutputSpec],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<RestructureBuildResult, UtxoManagementError> {
    if user_inputs.is_empty() {
        return Err(UtxoManagementError::NoInputs);
    }
    if outputs.is_empty() {
        return Err(UtxoManagementError::NoOutputs);
    }
    if outputs.len() > MAX_RESTRUCTURE_OUTPUTS {
        return Err(UtxoManagementError::TooManyOutputs {
            count: outputs.len(),
            max: MAX_RESTRUCTURE_OUTPUTS,
        });
    }

    for input in user_inputs {
        if input.ergo_tree != user_ergo_tree {
            return Err(UtxoManagementError::InputNotOwned {
                box_id: input.box_id.clone(),
            });
        }
    }

    let total_erg: i64 = user_inputs
        .iter()
        .map(|b| b.value.parse::<i64>().unwrap_or(0))
        .sum();

    let mut available_tokens: HashMap<String, u64> = HashMap::new();
    for input in user_inputs {
        for asset in &input.assets {
            let amount = asset.amount.parse::<u64>().unwrap_or(0);
            *available_tokens
                .entry(asset.token_id.clone())
                .or_insert(0) += amount;
        }
    }

    let mut assigned_tokens: HashMap<String, u64> = HashMap::new();
    let mut allocated_erg: i64 = 0;

    for out in outputs {
        if out.value < MIN_BOX_VALUE {
            return Err(UtxoManagementError::BelowMinBoxValue {
                value: out.value,
                min: MIN_BOX_VALUE,
            });
        }
        if out.tokens.len() > MAX_TOKENS_PER_BOX {
            return Err(UtxoManagementError::TooManyTokenTypes {
                count: out.tokens.len(),
                max: MAX_TOKENS_PER_BOX,
            });
        }

        // Merge duplicate token ids within one output
        let mut out_tokens: HashMap<String, u64> = HashMap::new();
        for (tid, amt) in &out.tokens {
            if *amt == 0 {
                return Err(UtxoManagementError::ZeroSplitAmount);
            }
            *out_tokens.entry(tid.clone()).or_insert(0) += *amt;
        }
        if out_tokens.len() > MAX_TOKENS_PER_BOX {
            return Err(UtxoManagementError::TooManyTokenTypes {
                count: out_tokens.len(),
                max: MAX_TOKENS_PER_BOX,
            });
        }

        for (tid, amt) in out_tokens {
            *assigned_tokens.entry(tid).or_insert(0) += amt;
        }

        allocated_erg = allocated_erg.checked_add(out.value).ok_or(
            UtxoManagementError::InsufficientErg {
                have: total_erg,
                need: i64::MAX,
            },
        )?;
    }

    for (tid, assigned) in &assigned_tokens {
        let available = available_tokens.get(tid).copied().unwrap_or(0);
        if *assigned > available {
            return Err(UtxoManagementError::TokenOverAllocated {
                token_id: tid.clone(),
                assigned: *assigned,
                available,
            });
        }
    }

    let fee_cfg = resolved_config();
    let citadel_fee = fee_cfg.budget();

    let available_after_fee = total_erg - TX_FEE - citadel_fee;
    if available_after_fee < MIN_BOX_VALUE {
        return Err(UtxoManagementError::InsufficientErg {
            have: total_erg,
            need: TX_FEE + citadel_fee + MIN_BOX_VALUE,
        });
    }
    if allocated_erg > available_after_fee {
        return Err(UtxoManagementError::ErgOverAllocated {
            allocated: allocated_erg,
            available: available_after_fee,
        });
    }

    let change_erg = available_after_fee - allocated_erg;

    let mut remaining_tokens: HashMap<String, u64> = HashMap::new();
    for (tid, avail) in &available_tokens {
        let assigned = assigned_tokens.get(tid).copied().unwrap_or(0);
        let rem = avail.saturating_sub(assigned);
        if rem > 0 {
            remaining_tokens.insert(tid.clone(), rem);
        }
    }

    let need_change = change_erg > 0 || !remaining_tokens.is_empty();
    if need_change {
        if change_erg > 0 && change_erg < MIN_BOX_VALUE {
            return Err(UtxoManagementError::ChangeBelowMin {
                change: change_erg,
                min: MIN_BOX_VALUE,
            });
        }
        if !remaining_tokens.is_empty() && change_erg < MIN_BOX_VALUE {
            // Tokens need a change box but no ERG left for it
            return Err(UtxoManagementError::UnassignedTokens {
                count: remaining_tokens.len(),
            });
        }
        if remaining_tokens.len() > MAX_TOKENS_PER_BOX {
            return Err(UtxoManagementError::TooManyTokenTypes {
                count: remaining_tokens.len(),
                max: MAX_TOKENS_PER_BOX,
            });
        }
    }

    let mut tx_outputs = Vec::with_capacity(outputs.len() + 3);

    for out in outputs {
        let mut merged: HashMap<String, u64> = HashMap::new();
        for (tid, amt) in &out.tokens {
            *merged.entry(tid.clone()).or_insert(0) += *amt;
        }
        let assets: Vec<Eip12Asset> = merged
            .into_iter()
            .map(|(id, amt)| Eip12Asset::new(id, amt as i64))
            .collect();

        tx_outputs.push(Eip12Output {
            value: out.value.to_string(),
            ergo_tree: user_ergo_tree.to_string(),
            assets,
            creation_height: current_height,
            additional_registers: HashMap::new(),
        });
    }

    let has_change = need_change;
    if has_change {
        let change_assets: Vec<Eip12Asset> = remaining_tokens
            .into_iter()
            .map(|(id, amt)| Eip12Asset::new(id, amt as i64))
            .collect();
        tx_outputs.push(Eip12Output {
            value: change_erg.to_string(),
            ergo_tree: user_ergo_tree.to_string(),
            assets: change_assets,
            creation_height: current_height,
            additional_registers: HashMap::new(),
        });
    }

    append_dev_fee_output(&mut tx_outputs, &fee_cfg, current_height)
        .map_err(|e| UtxoManagementError::DevFee(e.to_string()))?;
    tx_outputs.push(Eip12Output::fee(TX_FEE, current_height));

    let unsigned_tx = Eip12UnsignedTx {
        inputs: user_inputs.to_vec(),
        data_inputs: vec![],
        outputs: tx_outputs,
    };

    Ok(RestructureBuildResult {
        unsigned_tx,
        summary: RestructureSummary {
            input_count: user_inputs.len(),
            output_count: outputs.len() + if has_change { 1 } else { 0 },
            total_erg_in: total_erg,
            allocated_erg,
            change_erg,
            has_change,
            miner_fee: TX_FEE,
            citadel_fee_nano: citadel_fee,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const TOKEN_A: &str = "aaaa000000000000000000000000000000000000000000000000000000000000";
    const TOKEN_B: &str = "bbbb000000000000000000000000000000000000000000000000000000000000";

    fn mock_input(box_id: &str, erg: i64, assets: Vec<(&str, i64)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: box_id.to_string(),
            transaction_id: "tx123".to_string(),
            index: 0,
            value: erg.to_string(),
            ergo_tree: USER_TREE.to_string(),
            assets: assets
                .into_iter()
                .map(|(id, amt)| Eip12Asset::new(id, amt))
                .collect(),
            creation_height: 1000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_consolidate_two_boxes() {
        let inputs = vec![
            mock_input("box1", 3_000_000_000, vec![]),
            mock_input("box2", 2_000_000_000, vec![]),
        ];
        let result = build_consolidate_tx(&inputs, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.input_count, 2);
        assert_eq!(result.summary.total_erg_in, 5_000_000_000);
        assert_eq!(result.summary.change_erg, 5_000_000_000 - TX_FEE);
        assert_eq!(result.summary.token_count, 0);
        assert_eq!(result.summary.miner_fee, TX_FEE);

        assert_eq!(result.unsigned_tx.outputs.len(), 2);
        assert_eq!(result.unsigned_tx.outputs[0].ergo_tree, USER_TREE);
        assert_eq!(
            result.unsigned_tx.outputs[1].ergo_tree,
            citadel_core::constants::MINER_FEE_ERGO_TREE
        );
    }

    #[test]
    fn test_consolidate_preserves_tokens() {
        let inputs = vec![
            mock_input("box1", 3_000_000_000, vec![(TOKEN_A, 100)]),
            mock_input("box2", 2_000_000_000, vec![(TOKEN_B, 200)]),
        ];
        let result = build_consolidate_tx(&inputs, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.token_count, 2);
        let change = &result.unsigned_tx.outputs[0];
        assert_eq!(change.assets.len(), 2);
    }

    #[test]
    fn test_consolidate_merges_same_token() {
        let inputs = vec![
            mock_input("box1", 3_000_000_000, vec![(TOKEN_A, 100)]),
            mock_input("box2", 2_000_000_000, vec![(TOKEN_A, 200)]),
        ];
        let result = build_consolidate_tx(&inputs, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.token_count, 1);
        let change = &result.unsigned_tx.outputs[0];
        assert_eq!(change.assets.len(), 1);
        assert_eq!(change.assets[0].amount, "300");
    }

    #[test]
    fn test_consolidate_insufficient_erg() {
        let inputs = vec![
            mock_input("box1", 500_000, vec![]),
            mock_input("box2", 500_000, vec![]),
        ];
        let err = build_consolidate_tx(&inputs, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::InsufficientErg { .. } => {}
            _ => panic!("Expected InsufficientErg, got {:?}", err),
        }
    }

    #[test]
    fn test_consolidate_single_input_rejected() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![])];
        let err = build_consolidate_tx(&inputs, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::TooFewInputs(1) => {}
            _ => panic!("Expected TooFewInputs(1), got {:?}", err),
        }
    }

    #[test]
    fn test_consolidate_no_inputs() {
        let inputs: Vec<Eip12InputBox> = vec![];
        let err = build_consolidate_tx(&inputs, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::NoInputs => {}
            _ => panic!("Expected NoInputs, got {:?}", err),
        }
    }

    #[test]
    fn test_consolidate_three_boxes_with_mixed_tokens() {
        let inputs = vec![
            mock_input("box1", 1_000_000_000, vec![(TOKEN_A, 50), (TOKEN_B, 10)]),
            mock_input("box2", 2_000_000_000, vec![(TOKEN_A, 30)]),
            mock_input("box3", 1_500_000_000, vec![(TOKEN_B, 20)]),
        ];
        let result = build_consolidate_tx(&inputs, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.input_count, 3);
        assert_eq!(result.summary.total_erg_in, 4_500_000_000);
        assert_eq!(result.summary.token_count, 2);

        let change = &result.unsigned_tx.outputs[0];
        let a: u64 = change
            .assets
            .iter()
            .filter(|a| a.token_id == TOKEN_A)
            .map(|a| a.amount.parse::<u64>().unwrap())
            .sum();
        let b: u64 = change
            .assets
            .iter()
            .filter(|a| a.token_id == TOKEN_B)
            .map(|a| a.amount.parse::<u64>().unwrap())
            .sum();
        assert_eq!(a, 80);
        assert_eq!(b, 30);
    }

    #[test]
    fn test_split_erg_basic() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![])];
        let mode = SplitMode::Erg {
            amount_per_box: 1_000_000_000,
        };
        let result = build_split_tx(&inputs, &mode, 5, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.split_count, 5);
        assert_eq!(result.summary.amount_per_box, "1000000000");
        assert_eq!(result.summary.total_split, "5000000000");
        assert_eq!(result.summary.miner_fee, TX_FEE);

        assert_eq!(result.unsigned_tx.outputs.len(), 7);
        for i in 0..5 {
            assert_eq!(result.unsigned_tx.outputs[i].value, "1000000000");
            assert!(result.unsigned_tx.outputs[i].assets.is_empty());
        }
    }

    #[test]
    fn test_split_erg_with_change() {
        let inputs = vec![mock_input("box1", 6_000_000_000, vec![])];
        let mode = SplitMode::Erg {
            amount_per_box: 1_000_000_000,
        };
        let result = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap();

        let change_erg = 6_000_000_000 - 3_000_000_000 - TX_FEE;
        assert_eq!(result.summary.change_erg, change_erg);
    }

    #[test]
    fn test_split_erg_insufficient() {
        let inputs = vec![mock_input("box1", 1_000_000_000, vec![])];
        let mode = SplitMode::Erg {
            amount_per_box: 1_000_000_000,
        };
        let err = build_split_tx(&inputs, &mode, 5, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::InsufficientErg { .. } => {}
            _ => panic!("Expected InsufficientErg, got {:?}", err),
        }
    }

    #[test]
    fn test_split_erg_below_min() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![])];
        let mode = SplitMode::Erg {
            amount_per_box: 500_000,
        };
        let err = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::BelowMinBoxValue { .. } => {}
            _ => panic!("Expected BelowMinBoxValue, got {:?}", err),
        }
    }

    #[test]
    fn test_split_erg_preserves_tokens_in_change() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![(TOKEN_A, 100)])];
        let mode = SplitMode::Erg {
            amount_per_box: 2_000_000_000,
        };
        let result = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap();

        for i in 0..3 {
            assert!(result.unsigned_tx.outputs[i].assets.is_empty());
        }

        let change = &result.unsigned_tx.outputs[3];
        assert_eq!(change.assets.len(), 1);
        assert_eq!(change.assets[0].token_id, TOKEN_A);
        assert_eq!(change.assets[0].amount, "100");
    }

    #[test]
    fn test_split_count_exceeds_max() {
        let inputs = vec![mock_input("box1", 100_000_000_000, vec![])];
        let mode = SplitMode::Erg {
            amount_per_box: 1_000_000_000,
        };
        let err = build_split_tx(&inputs, &mode, 31, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::TooManyOutputs { count: 31, max: 30 } => {}
            _ => panic!("Expected TooManyOutputs, got {:?}", err),
        }
    }

    #[test]
    fn test_split_zero_count() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![])];
        let mode = SplitMode::Erg {
            amount_per_box: 1_000_000_000,
        };
        let err = build_split_tx(&inputs, &mode, 0, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::ZeroSplitCount => {}
            _ => panic!("Expected ZeroSplitCount, got {:?}", err),
        }
    }

    #[test]
    fn test_split_no_inputs() {
        let inputs: Vec<Eip12InputBox> = vec![];
        let mode = SplitMode::Erg {
            amount_per_box: 1_000_000_000,
        };
        let err = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::NoInputs => {}
            _ => panic!("Expected NoInputs, got {:?}", err),
        }
    }

    #[test]
    fn test_split_token_basic() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![(TOKEN_A, 1000)])];
        let mode = SplitMode::Token {
            token_id: TOKEN_A.to_string(),
            amount_per_box: 100,
            erg_per_box: 1_000_000,
        };
        let result = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.split_count, 3);
        assert_eq!(result.summary.amount_per_box, "100");
        assert_eq!(result.summary.total_split, "300");

        assert_eq!(result.unsigned_tx.outputs.len(), 5);
        for i in 0..3 {
            assert_eq!(result.unsigned_tx.outputs[i].value, "1000000");
            assert_eq!(result.unsigned_tx.outputs[i].assets.len(), 1);
            assert_eq!(result.unsigned_tx.outputs[i].assets[0].token_id, TOKEN_A);
            assert_eq!(result.unsigned_tx.outputs[i].assets[0].amount, "100");
        }

        let change = &result.unsigned_tx.outputs[3];
        let remaining: u64 = change
            .assets
            .iter()
            .filter(|a| a.token_id == TOKEN_A)
            .map(|a| a.amount.parse::<u64>().unwrap())
            .sum();
        assert_eq!(remaining, 700);
    }

    #[test]
    fn test_split_token_preserves_other_tokens() {
        let inputs = vec![mock_input(
            "box1",
            10_000_000_000,
            vec![(TOKEN_A, 1000), (TOKEN_B, 500)],
        )];
        let mode = SplitMode::Token {
            token_id: TOKEN_A.to_string(),
            amount_per_box: 200,
            erg_per_box: 1_000_000,
        };
        let result = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap();

        for i in 0..3 {
            assert_eq!(result.unsigned_tx.outputs[i].assets.len(), 1);
            assert_eq!(result.unsigned_tx.outputs[i].assets[0].token_id, TOKEN_A);
        }

        let change = &result.unsigned_tx.outputs[3];
        let remaining_a: u64 = change
            .assets
            .iter()
            .filter(|a| a.token_id == TOKEN_A)
            .map(|a| a.amount.parse::<u64>().unwrap())
            .sum();
        let remaining_b: u64 = change
            .assets
            .iter()
            .filter(|a| a.token_id == TOKEN_B)
            .map(|a| a.amount.parse::<u64>().unwrap())
            .sum();
        assert_eq!(remaining_a, 400);
        assert_eq!(remaining_b, 500);
    }

    #[test]
    fn test_split_token_insufficient() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![(TOKEN_A, 50)])];
        let mode = SplitMode::Token {
            token_id: TOKEN_A.to_string(),
            amount_per_box: 100,
            erg_per_box: 1_000_000,
        };
        let err = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::InsufficientTokens {
                have: 50,
                need: 300,
                ..
            } => {}
            _ => panic!("Expected InsufficientTokens, got {:?}", err),
        }
    }

    #[test]
    fn test_split_token_insufficient_erg() {
        let inputs = vec![mock_input("box1", 2_000_000, vec![(TOKEN_A, 1000)])];
        let mode = SplitMode::Token {
            token_id: TOKEN_A.to_string(),
            amount_per_box: 100,
            erg_per_box: 1_000_000,
        };
        let err = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::InsufficientErg { .. } => {}
            _ => panic!("Expected InsufficientErg, got {:?}", err),
        }
    }

    #[test]
    fn test_split_token_zero_amount() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![(TOKEN_A, 1000)])];
        let mode = SplitMode::Token {
            token_id: TOKEN_A.to_string(),
            amount_per_box: 0,
            erg_per_box: 1_000_000,
        };
        let err = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::ZeroSplitAmount => {}
            _ => panic!("Expected ZeroSplitAmount, got {:?}", err),
        }
    }

    #[test]
    fn test_split_token_erg_below_min() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![(TOKEN_A, 1000)])];
        let mode = SplitMode::Token {
            token_id: TOKEN_A.to_string(),
            amount_per_box: 100,
            erg_per_box: 500_000,
        };
        let err = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::BelowMinBoxValue { .. } => {}
            _ => panic!("Expected BelowMinBoxValue, got {:?}", err),
        }
    }

    #[test]
    fn test_split_erg_exact_no_change() {
        let total = 3_000_000_000 + TX_FEE;
        let inputs = vec![mock_input("box1", total, vec![])];
        let mode = SplitMode::Erg {
            amount_per_box: 1_000_000_000,
        };
        let result = build_split_tx(&inputs, &mode, 3, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.change_erg, 0);
        assert_eq!(result.unsigned_tx.outputs.len(), 4);
    }

    #[test]
    fn test_restructure_two_outputs() {
        let inputs = vec![
            mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 100)]),
            mock_input("box2", 3_000_000_000, vec![(TOKEN_B, 50)]),
        ];
        let outs = vec![
            RestructureOutputSpec {
                value: 2_000_000_000,
                tokens: vec![(TOKEN_A.to_string(), 100)],
            },
            RestructureOutputSpec {
                value: 1_000_000_000,
                tokens: vec![(TOKEN_B.to_string(), 50)],
            },
        ];
        let result = build_restructure_tx(&inputs, &outs, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.input_count, 2);
        assert_eq!(result.summary.allocated_erg, 3_000_000_000);
        assert!(result.summary.has_change);
        assert_eq!(
            result.summary.change_erg,
            8_000_000_000 - TX_FEE - 3_000_000_000
        );
        // 2 user outs + change + fee
        assert_eq!(result.unsigned_tx.outputs.len(), 4);
        assert_eq!(result.unsigned_tx.outputs[0].assets[0].token_id, TOKEN_A);
        assert_eq!(result.unsigned_tx.outputs[1].assets[0].token_id, TOKEN_B);
    }

    #[test]
    fn test_restructure_exact_no_change() {
        let total = 2_000_000_000 + TX_FEE;
        let inputs = vec![mock_input("box1", total, vec![])];
        let outs = vec![
            RestructureOutputSpec {
                value: 1_000_000_000,
                tokens: vec![],
            },
            RestructureOutputSpec {
                value: 1_000_000_000,
                tokens: vec![],
            },
        ];
        let result = build_restructure_tx(&inputs, &outs, USER_TREE, 50000).unwrap();
        assert!(!result.summary.has_change);
        assert_eq!(result.summary.change_erg, 0);
        assert_eq!(result.unsigned_tx.outputs.len(), 3); // 2 + fee
    }

    #[test]
    fn test_restructure_rejects_unowned_input() {
        let mut inputs = vec![mock_input("box1", 5_000_000_000, vec![])];
        inputs[0].ergo_tree = "0008cd00deadbeef".to_string();
        let outs = vec![RestructureOutputSpec {
            value: 1_000_000_000,
            tokens: vec![],
        }];
        let err = build_restructure_tx(&inputs, &outs, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::InputNotOwned { .. } => {}
            _ => panic!("Expected InputNotOwned, got {:?}", err),
        }
    }

    #[test]
    fn test_restructure_token_over_alloc() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 10)])];
        let outs = vec![RestructureOutputSpec {
            value: 1_000_000_000,
            tokens: vec![(TOKEN_A.to_string(), 50)],
        }];
        let err = build_restructure_tx(&inputs, &outs, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::TokenOverAllocated { .. } => {}
            _ => panic!("Expected TokenOverAllocated, got {:?}", err),
        }
    }

    #[test]
    fn test_restructure_unassigned_tokens_need_change_erg() {
        let inputs = vec![mock_input("box1", TX_FEE + MIN_BOX_VALUE, vec![(TOKEN_A, 10)])];
        let outs = vec![RestructureOutputSpec {
            value: MIN_BOX_VALUE,
            tokens: vec![],
        }];
        let err = build_restructure_tx(&inputs, &outs, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::UnassignedTokens { count: 1 } => {}
            _ => panic!("Expected UnassignedTokens, got {:?}", err),
        }
    }

    #[test]
    fn test_restructure_erg_over_alloc() {
        let inputs = vec![mock_input("box1", 3_000_000_000, vec![])];
        let outs = vec![RestructureOutputSpec {
            value: 5_000_000_000,
            tokens: vec![],
        }];
        let err = build_restructure_tx(&inputs, &outs, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::ErgOverAllocated { .. } => {}
            _ => panic!("Expected ErgOverAllocated, got {:?}", err),
        }
    }

    #[test]
    fn test_restructure_splits_token_across_outputs() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![(TOKEN_A, 100)])];
        let outs = vec![
            RestructureOutputSpec {
                value: 1_000_000_000,
                tokens: vec![(TOKEN_A.to_string(), 40)],
            },
            RestructureOutputSpec {
                value: 1_000_000_000,
                tokens: vec![(TOKEN_A.to_string(), 60)],
            },
        ];
        let result = build_restructure_tx(&inputs, &outs, USER_TREE, 50000).unwrap();
        assert_eq!(result.unsigned_tx.outputs[0].assets[0].amount, "40");
        assert_eq!(result.unsigned_tx.outputs[1].assets[0].amount, "60");
        // change has no leftover tokens
        let change = &result.unsigned_tx.outputs[2];
        assert!(change.assets.is_empty());
    }

    #[test]
    fn test_restructure_count_exceeds_max() {
        let n = MAX_RESTRUCTURE_OUTPUTS + 1;
        let total = (n as i64) * MIN_BOX_VALUE + TX_FEE;
        let inputs = vec![mock_input("box1", total, vec![])];
        let outs = vec![
            RestructureOutputSpec {
                value: MIN_BOX_VALUE,
                tokens: vec![],
            };
            n
        ];
        let err = build_restructure_tx(&inputs, &outs, USER_TREE, 50000).unwrap_err();
        match err {
            UtxoManagementError::TooManyOutputs {
                count,
                max: MAX_RESTRUCTURE_OUTPUTS,
            } if count == n => {}
            _ => panic!("Expected TooManyOutputs, got {:?}", err),
        }
    }
}
