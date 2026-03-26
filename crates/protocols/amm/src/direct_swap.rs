//! Direct swap tx builder -- spends pool box directly (no proxy/bot).
//!
//! N2T: inputs[pool, user...] -> outputs[pool', user_out, fee]
//! T2T: same structure, but ERG stays unchanged (storage rent only)
//!
//! Pool contract validates: same ErgoTree, same R4, same NFT/LP,
//! updated reserves, constant product invariant.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::calculator;
use crate::constants::fees;
use crate::state::{AmmError, AmmPool, PoolType, SwapInput};
use crate::tx_builder::MIN_CHANGE_VALUE;
use ergo_tx::{
    collect_change_tokens, select_inputs_for_spend, select_token_boxes, Eip12Asset,
    Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;
const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

#[derive(Debug)]
pub struct DirectSwapBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: DirectSwapSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectSwapSummary {
    pub input_amount: u64,
    pub input_token: String,
    pub output_amount: u64,
    pub min_output: u64,
    pub output_token: String,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

/// Pool box must be inputs[0], new pool box must be outputs[0].
#[allow(clippy::too_many_arguments)]
pub fn build_direct_swap_eip12(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    input: &SwapInput,
    min_output: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
    recipient_ergo_tree: Option<&str>,
) -> Result<DirectSwapBuildResult, AmmError> {
    match pool.pool_type {
        PoolType::N2T => build_n2t_direct_swap(
            pool_box,
            pool,
            input,
            min_output,
            user_utxos,
            user_ergo_tree,
            current_height,
            recipient_ergo_tree,
        ),
        PoolType::T2T => build_t2t_direct_swap(
            pool_box,
            pool,
            input,
            min_output,
            user_utxos,
            user_ergo_tree,
            current_height,
            recipient_ergo_tree,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_n2t_direct_swap(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    input: &SwapInput,
    min_output: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
    recipient_ergo_tree: Option<&str>,
) -> Result<DirectSwapBuildResult, AmmError> {
    let pool_erg: u64 = pool_box
        .value
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool box ERG value".to_string()))?;

    if pool_box.assets.len() < 3 {
        return Err(AmmError::TxBuildError(format!(
            "Pool box has {} tokens, expected at least 3",
            pool_box.assets.len()
        )));
    }

    let pool_nft = &pool_box.assets[0];
    let pool_lp = &pool_box.assets[1];
    let pool_token_y = &pool_box.assets[2];

    let pool_token_y_amount: u64 = pool_token_y
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token Y amount".to_string()))?;

    // fee_num from R4 is the ground truth for the constant product check
    let fee_num = crate::constants::parse_fee_num_from_r4(&pool_box.additional_registers)?;
    let fee_denom = fees::DEFAULT_FEE_DENOM;

    let (output_amount, is_erg_to_token) = match input {
        SwapInput::Erg { amount } => {
            let output = calculator::calculate_output(
                pool_erg,
                pool_token_y_amount,
                *amount,
                fee_num,
                fee_denom,
            );
            if output == 0 {
                return Err(AmmError::InsufficientLiquidity);
            }
            (output, true)
        }
        SwapInput::Token { amount, .. } => {
            let output = calculator::calculate_output(
                pool_token_y_amount,
                pool_erg,
                *amount,
                fee_num,
                fee_denom,
            );
            if output == 0 {
                return Err(AmmError::InsufficientLiquidity);
            }
            (output, false)
        }
    };

    if output_amount < min_output {
        return Err(AmmError::SlippageExceeded {
            got: output_amount,
            min: min_output,
        });
    }

    let input_amount = match input {
        SwapInput::Erg { amount } => *amount,
        SwapInput::Token { amount, .. } => *amount,
    };

    let (new_pool_erg, new_pool_token_y_amount) = if is_erg_to_token {
        let new_erg = pool_erg
            .checked_add(input_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool ERG overflow".to_string()))?;
        let new_token_y = pool_token_y_amount
            .checked_sub(output_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token Y underflow".to_string()))?;
        (new_erg, new_token_y)
    } else {
        let new_erg = pool_erg
            .checked_sub(output_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool ERG underflow".to_string()))?;
        if new_erg < MIN_BOX_VALUE {
            return Err(AmmError::TxBuildError(
                "New pool box would have less than minimum ERG".to_string(),
            ));
        }
        let new_token_y = pool_token_y_amount
            .checked_add(input_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token Y overflow".to_string()))?;
        (new_erg, new_token_y)
    };

    let new_pool_output = Eip12Output {
        value: new_pool_erg.to_string(),
        ergo_tree: pool_box.ergo_tree.clone(),
        assets: vec![
            Eip12Asset {
                token_id: pool_nft.token_id.clone(),
                amount: pool_nft.amount.clone(),
            },
            Eip12Asset {
                token_id: pool_lp.token_id.clone(),
                amount: pool_lp.amount.clone(),
            },
            Eip12Asset {
                token_id: pool_token_y.token_id.clone(),
                amount: new_pool_token_y_amount.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: pool_box.additional_registers.clone(),
    };

    let output_tree = recipient_ergo_tree.unwrap_or(user_ergo_tree);
    let user_swap_output = if is_erg_to_token {
        Eip12Output {
            value: MIN_BOX_VALUE.to_string(),
            ergo_tree: output_tree.to_string(),
            assets: vec![Eip12Asset {
                token_id: pool.token_y.token_id.clone(),
                amount: output_amount.to_string(),
            }],
            creation_height: current_height,
            additional_registers: HashMap::new(),
        }
    } else {
        Eip12Output {
            value: output_amount.to_string(),
            ergo_tree: output_tree.to_string(),
            assets: vec![],
            creation_height: current_height,
            additional_registers: HashMap::new(),
        }
    };

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    let user_erg_needed = if is_erg_to_token {
        input_amount
            .checked_add(MIN_BOX_VALUE)
            .and_then(|v| v.checked_add(TX_FEE))
            .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?
    } else {
        TX_FEE
    };

    let token_requirement = match input {
        SwapInput::Erg { .. } => None,
        SwapInput::Token { token_id, amount } => Some((token_id.as_str(), *amount)),
    };
    let selected = select_inputs_for_spend(user_utxos, user_erg_needed, token_requirement)
        .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = match input {
        SwapInput::Erg { .. } => None,
        SwapInput::Token { token_id, amount } => Some((token_id.as_str(), *amount)),
    };
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    // When no separate recipient, merge swap output + change into one box
    let mut outputs = vec![new_pool_output];

    if recipient_ergo_tree.is_none() {
        let user_erg = if is_erg_to_token {
            MIN_BOX_VALUE + change_erg
        } else {
            output_amount + change_erg
        };

        let mut user_tokens = if is_erg_to_token {
            vec![Eip12Asset {
                token_id: pool.token_y.token_id.clone(),
                amount: output_amount.to_string(),
            }]
        } else {
            vec![]
        };
        user_tokens.extend(change_tokens);

        outputs.push(Eip12Output {
            value: user_erg.to_string(),
            ergo_tree: user_ergo_tree.to_string(),
            assets: user_tokens,
            creation_height: current_height,
            additional_registers: HashMap::new(),
        });
    } else {
        if !change_tokens.is_empty() && change_erg < MIN_CHANGE_VALUE {
            return Err(AmmError::TxBuildError(format!(
                "Change tokens exist but not enough ERG for change box (need {}, have {})",
                MIN_CHANGE_VALUE, change_erg
            )));
        }

        // If change ERG is too small for a separate box and there are no change
        // tokens, fold it into the swap output to avoid losing ERG.
        let user_swap_output = if change_erg > 0
            && change_erg < MIN_CHANGE_VALUE
            && change_tokens.is_empty()
        {
            let base_value: u64 = user_swap_output
                .value
                .parse()
                .map_err(|_| AmmError::TxBuildError("Invalid swap output value".to_string()))?;
            Eip12Output {
                value: (base_value + change_erg).to_string(),
                ..user_swap_output
            }
        } else {
            user_swap_output
        };

        outputs.push(user_swap_output);

        if change_erg >= MIN_CHANGE_VALUE || !change_tokens.is_empty() {
            outputs.push(Eip12Output {
                value: change_erg.to_string(),
                ergo_tree: user_ergo_tree.to_string(),
                assets: change_tokens,
                creation_height: current_height,
                additional_registers: HashMap::new(),
            });
        }
    }

    outputs.push(fee_output);

    let mut inputs = vec![pool_box.clone()];
    inputs.extend(selected.boxes);

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    let (input_token_name, output_token_name) = match input {
        SwapInput::Erg { .. } => (
            "ERG".to_string(),
            pool.token_y
                .name
                .clone()
                .unwrap_or_else(|| pool.token_y.token_id[..8].to_string()),
        ),
        SwapInput::Token { .. } => (
            pool.token_y
                .name
                .clone()
                .unwrap_or_else(|| pool.token_y.token_id[..8].to_string()),
            "ERG".to_string(),
        ),
    };

    let summary = DirectSwapSummary {
        input_amount,
        input_token: input_token_name,
        output_amount,
        min_output,
        output_token: output_token_name,
        miner_fee: TX_FEE,
        total_erg_cost: user_erg_needed,
    };

    Ok(DirectSwapBuildResult {
        unsigned_tx,
        summary,
    })
}

/// T2T pools: 4 tokens [NFT, LP, X, Y], ERG unchanged (storage rent only).
#[allow(clippy::too_many_arguments)]
fn build_t2t_direct_swap(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    input: &SwapInput,
    min_output: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
    recipient_ergo_tree: Option<&str>,
) -> Result<DirectSwapBuildResult, AmmError> {
    let (input_token_id, input_amount) = match input {
        SwapInput::Erg { .. } => {
            return Err(AmmError::InvalidToken(
                "ERG input is not valid for T2T pools -- use a token".to_string(),
            ));
        }
        SwapInput::Token { token_id, amount } => (token_id.as_str(), *amount),
    };

    let pool_erg: u64 = pool_box
        .value
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool box ERG value".to_string()))?;

    if pool_box.assets.len() < 4 {
        return Err(AmmError::TxBuildError(format!(
            "T2T pool box has {} tokens, expected at least 4",
            pool_box.assets.len()
        )));
    }

    let pool_nft = &pool_box.assets[0];
    let pool_lp = &pool_box.assets[1];
    let pool_token_x_asset = &pool_box.assets[2];
    let pool_token_y_asset = &pool_box.assets[3];

    let pool_token_x_amount: u64 = pool_token_x_asset
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token X amount".to_string()))?;
    let pool_token_y_amount: u64 = pool_token_y_asset
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token Y amount".to_string()))?;

    let token_x_meta = pool.token_x.as_ref().ok_or_else(|| {
        AmmError::TxBuildError("T2T pool missing token_x metadata".to_string())
    })?;

    let fee_num = crate::constants::parse_fee_num_from_r4(&pool_box.additional_registers)?;
    let fee_denom = fees::DEFAULT_FEE_DENOM;

    let is_x_to_y = if input_token_id == token_x_meta.token_id {
        true
    } else if input_token_id == pool.token_y.token_id {
        false
    } else {
        return Err(AmmError::InvalidToken(format!(
            "Token {} does not match pool token_x ({}) or token_y ({})",
            input_token_id,
            &token_x_meta.token_id[..8],
            &pool.token_y.token_id[..8],
        )));
    };

    let (reserves_in, reserves_out) = if is_x_to_y {
        (pool_token_x_amount, pool_token_y_amount)
    } else {
        (pool_token_y_amount, pool_token_x_amount)
    };

    let output_amount = calculator::calculate_output(
        reserves_in,
        reserves_out,
        input_amount,
        fee_num,
        fee_denom,
    );
    if output_amount == 0 {
        return Err(AmmError::InsufficientLiquidity);
    }

    if output_amount < min_output {
        return Err(AmmError::SlippageExceeded {
            got: output_amount,
            min: min_output,
        });
    }

    let (new_token_x_amount, new_token_y_amount) = if is_x_to_y {
        let new_x = pool_token_x_amount
            .checked_add(input_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token X overflow".to_string()))?;
        let new_y = pool_token_y_amount
            .checked_sub(output_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token Y underflow".to_string()))?;
        (new_x, new_y)
    } else {
        let new_x = pool_token_x_amount
            .checked_sub(output_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token X underflow".to_string()))?;
        let new_y = pool_token_y_amount
            .checked_add(input_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token Y overflow".to_string()))?;
        (new_x, new_y)
    };

    let new_pool_output = Eip12Output {
        value: pool_erg.to_string(),
        ergo_tree: pool_box.ergo_tree.clone(),
        assets: vec![
            Eip12Asset {
                token_id: pool_nft.token_id.clone(),
                amount: pool_nft.amount.clone(),
            },
            Eip12Asset {
                token_id: pool_lp.token_id.clone(),
                amount: pool_lp.amount.clone(),
            },
            Eip12Asset {
                token_id: pool_token_x_asset.token_id.clone(),
                amount: new_token_x_amount.to_string(),
            },
            Eip12Asset {
                token_id: pool_token_y_asset.token_id.clone(),
                amount: new_token_y_amount.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: pool_box.additional_registers.clone(),
    };

    let output_token_id = if is_x_to_y {
        &pool.token_y.token_id
    } else {
        &token_x_meta.token_id
    };

    let output_tree = recipient_ergo_tree.unwrap_or(user_ergo_tree);
    let user_swap_output = Eip12Output {
        value: MIN_BOX_VALUE.to_string(),
        ergo_tree: output_tree.to_string(),
        assets: vec![Eip12Asset {
            token_id: output_token_id.clone(),
            amount: output_amount.to_string(),
        }],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    let user_erg_needed = MIN_BOX_VALUE
        .checked_add(TX_FEE)
        .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?;

    let selected = select_token_boxes(user_utxos, input_token_id, input_amount, user_erg_needed)
        .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = Some((input_token_id, input_amount));
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    let mut outputs = vec![new_pool_output];

    if recipient_ergo_tree.is_none() {
        let user_erg = MIN_BOX_VALUE + change_erg;

        let mut user_tokens = vec![Eip12Asset {
            token_id: output_token_id.clone(),
            amount: output_amount.to_string(),
        }];
        user_tokens.extend(change_tokens);

        outputs.push(Eip12Output {
            value: user_erg.to_string(),
            ergo_tree: user_ergo_tree.to_string(),
            assets: user_tokens,
            creation_height: current_height,
            additional_registers: HashMap::new(),
        });
    } else {
        if !change_tokens.is_empty() && change_erg < MIN_CHANGE_VALUE {
            return Err(AmmError::TxBuildError(format!(
                "Change tokens exist but not enough ERG for change box (need {}, have {})",
                MIN_CHANGE_VALUE, change_erg
            )));
        }

        // If change ERG is too small for a separate box and there are no change
        // tokens, fold it into the swap output to avoid losing ERG.
        let user_swap_output = if change_erg > 0
            && change_erg < MIN_CHANGE_VALUE
            && change_tokens.is_empty()
        {
            let base_value: u64 = user_swap_output
                .value
                .parse()
                .map_err(|_| AmmError::TxBuildError("Invalid swap output value".to_string()))?;
            Eip12Output {
                value: (base_value + change_erg).to_string(),
                ..user_swap_output
            }
        } else {
            user_swap_output
        };

        outputs.push(user_swap_output);

        if change_erg >= MIN_CHANGE_VALUE || !change_tokens.is_empty() {
            outputs.push(Eip12Output {
                value: change_erg.to_string(),
                ergo_tree: user_ergo_tree.to_string(),
                assets: change_tokens,
                creation_height: current_height,
                additional_registers: HashMap::new(),
            });
        }
    }

    outputs.push(fee_output);

    let mut inputs = vec![pool_box.clone()];
    inputs.extend(selected.boxes);

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    let (input_token_name, output_token_name) = if is_x_to_y {
        (
            token_x_meta
                .name
                .clone()
                .unwrap_or_else(|| token_x_meta.token_id[..8].to_string()),
            pool.token_y
                .name
                .clone()
                .unwrap_or_else(|| pool.token_y.token_id[..8].to_string()),
        )
    } else {
        (
            pool.token_y
                .name
                .clone()
                .unwrap_or_else(|| pool.token_y.token_id[..8].to_string()),
            token_x_meta
                .name
                .clone()
                .unwrap_or_else(|| token_x_meta.token_id[..8].to_string()),
        )
    };

    let summary = DirectSwapSummary {
        input_amount,
        input_token: input_token_name,
        output_amount,
        min_output,
        output_token: output_token_name,
        miner_fee: TX_FEE,
        total_erg_cost: user_erg_needed,
    };

    Ok(DirectSwapBuildResult {
        unsigned_tx,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AmmPool, PoolType, SwapInput, TokenAmount};

    fn test_n2t_pool() -> AmmPool {
        AmmPool {
            pool_id: "pool_nft_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            pool_type: PoolType::N2T,
            box_id: "pool_box_1".to_string(),
            erg_reserves: Some(100_000_000_000), // 100 ERG
            token_x: None,
            token_y: TokenAmount {
                token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: 1_000_000,
                decimals: Some(6),
                name: Some("TestToken".to_string()),
            },
            lp_token_id: "lp_token".to_string(),
            lp_circulating: 1000,
            fee_num: 997,
            fee_denom: 1000,
        }
    }

    fn test_pool_box() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "pool_box_1".to_string(),
            transaction_id: "pool_tx_1".to_string(),
            index: 0,
            value: "100000000000".to_string(), // 100 ERG
            ergo_tree: "pool_ergo_tree_hex".to_string(),
            assets: vec![
                Eip12Asset {
                    token_id:
                        "pool_nft_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .to_string(),
                    amount: "1".to_string(),
                },
                Eip12Asset {
                    token_id: "lp_token".to_string(),
                    amount: "9223372036854774807".to_string(), // max LP supply minus circulating
                },
                Eip12Asset {
                    token_id:
                        "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .to_string(),
                    amount: "1000000".to_string(),
                },
            ],
            creation_height: 999_000,
            additional_registers: {
                let mut m = HashMap::new();
                m.insert("R4".to_string(), "04ca0f".to_string()); // fee_num=997 (sigma Int)
                m
            },
            extension: HashMap::new(),
        }
    }

    fn test_user_utxo() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "user_utxo_1".to_string(),
            transaction_id: "user_tx_1".to_string(),
            index: 0,
            value: "10000000000".to_string(), // 10 ERG
            ergo_tree: "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                .to_string(),
            assets: vec![],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_direct_swap_erg_to_token() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo();

        let input = SwapInput::Erg {
            amount: 1_000_000_000,
        }; // 1 ERG
        let output =
            calculator::calculate_output(100_000_000_000, 1_000_000, 1_000_000_000, 997, 1000);
        let min_output = calculator::apply_slippage(output, 0.5);

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            min_output,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");
        assert_eq!(build.unsigned_tx.inputs[1].box_id, "user_utxo_1");

        assert_eq!(build.unsigned_tx.outputs[0].ergo_tree, "pool_ergo_tree_hex");
        let new_pool_erg: u64 = build.unsigned_tx.outputs[0].value.parse().unwrap();
        assert_eq!(new_pool_erg, 100_000_000_000 + 1_000_000_000);
        let new_token_y: u64 = build.unsigned_tx.outputs[0].assets[2]
            .amount
            .parse()
            .unwrap();
        assert_eq!(new_token_y, 1_000_000 - output);

        assert_eq!(build.unsigned_tx.outputs[1].assets.len(), 1);
        let user_token_received: u64 = build.unsigned_tx.outputs[1].assets[0]
            .amount
            .parse()
            .unwrap();
        assert_eq!(user_token_received, output);
        let user_out_erg: u64 = build.unsigned_tx.outputs[1].value.parse().unwrap();
        assert!(user_out_erg > MIN_BOX_VALUE, "Change ERG should be folded in");

        assert_eq!(build.unsigned_tx.outputs[2].value, TX_FEE.to_string());

        assert_eq!(build.summary.input_amount, 1_000_000_000);
        assert_eq!(build.summary.input_token, "ERG");
        assert_eq!(build.summary.output_amount, output);
        assert_eq!(build.summary.output_token, "TestToken");
        assert_eq!(build.summary.miner_fee, TX_FEE);
    }

    #[test]
    fn test_direct_swap_token_to_erg() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let token_id =
            "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let user_utxo = Eip12InputBox {
            assets: vec![Eip12Asset {
                token_id: token_id.clone(),
                amount: "50000".to_string(),
            }],
            ..test_user_utxo()
        };

        let input = SwapInput::Token {
            token_id: token_id.clone(),
            amount: 10000,
        };
        let output = calculator::calculate_output(1_000_000, 100_000_000_000, 10000, 997, 1000);
        let min_output = calculator::apply_slippage(output, 0.5);

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            min_output,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");

        let new_pool_erg: u64 = build.unsigned_tx.outputs[0].value.parse().unwrap();
        assert_eq!(new_pool_erg, 100_000_000_000 - output);
        let new_token_y: u64 = build.unsigned_tx.outputs[0].assets[2]
            .amount
            .parse()
            .unwrap();
        assert_eq!(new_token_y, 1_000_000 + 10000);

        let user_out = &build.unsigned_tx.outputs[1];
        let user_erg_received: u64 = user_out.value.parse().unwrap();
        assert!(user_erg_received > output, "Change ERG should be folded in");

        let change_token: &Eip12Asset = user_out
            .assets
            .iter()
            .find(|a| a.token_id == token_id)
            .unwrap();
        assert_eq!(change_token.amount, "40000"); // 50000 - 10000
    }

    #[test]
    fn test_direct_swap_insufficient_erg() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = Eip12InputBox {
            value: "1000000".to_string(), // 0.001 ERG - not enough
            ..test_user_utxo()
        };

        let input = SwapInput::Erg {
            amount: 1_000_000_000,
        };
        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            1,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient"));
    }

    #[test]
    fn test_direct_swap_slippage_exceeded() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo();

        let input = SwapInput::Erg {
            amount: 1_000_000_000,
        };
        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            u64::MAX,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("below minimum") || err.contains("Output below minimum"),
            "Got: {}",
            err
        );
    }

    #[test]
    fn test_direct_swap_pool_registers_preserved() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo();

        let input = SwapInput::Erg {
            amount: 1_000_000_000,
        };
        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            1,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        )
        .unwrap();

        assert_eq!(
            result.unsigned_tx.outputs[0].additional_registers.get("R4"),
            Some(&"04ca0f".to_string())
        );
    }

    fn test_t2t_pool() -> AmmPool {
        AmmPool {
            pool_id: "t2t_pool_nft_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            pool_type: PoolType::T2T,
            box_id: "t2t_pool_box_1".to_string(),
            erg_reserves: None,
            token_x: Some(TokenAmount {
                token_id:
                    "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_string(),
                amount: 10_000_000,
                decimals: Some(6),
                name: Some("TokenX".to_string()),
            }),
            token_y: TokenAmount {
                token_id:
                    "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_string(),
                amount: 5_000_000,
                decimals: Some(6),
                name: Some("TokenY".to_string()),
            },
            lp_token_id: "t2t_lp_token".to_string(),
            lp_circulating: 1000,
            fee_num: 997,
            fee_denom: 1000,
        }
    }

    fn test_t2t_pool_box() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "t2t_pool_box_1".to_string(),
            transaction_id: "t2t_pool_tx_1".to_string(),
            index: 0,
            value: "10000000".to_string(), // 0.01 ERG (storage rent only)
            ergo_tree: "t2t_pool_ergo_tree_hex".to_string(),
            assets: vec![
                Eip12Asset {
                    token_id:
                        "t2t_pool_nft_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .to_string(),
                    amount: "1".to_string(),
                },
                Eip12Asset {
                    token_id: "t2t_lp_token".to_string(),
                    amount: "9223372036854774807".to_string(),
                },
                Eip12Asset {
                    token_id:
                        "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .to_string(),
                    amount: "10000000".to_string(),
                },
                Eip12Asset {
                    token_id:
                        "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .to_string(),
                    amount: "5000000".to_string(),
                },
            ],
            creation_height: 999_000,
            additional_registers: {
                let mut m = HashMap::new();
                m.insert("R4".to_string(), "04ca0f".to_string()); // fee_num=997
                m
            },
            extension: HashMap::new(),
        }
    }

    fn test_user_utxo_with_token_x() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "user_utxo_token_x".to_string(),
            transaction_id: "user_tx_2".to_string(),
            index: 0,
            value: "5000000000".to_string(), // 5 ERG
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![Eip12Asset {
                token_id:
                    "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_string(),
                amount: "500000".to_string(),
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    fn test_user_utxo_with_token_y() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "user_utxo_token_y".to_string(),
            transaction_id: "user_tx_3".to_string(),
            index: 0,
            value: "5000000000".to_string(), // 5 ERG
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![Eip12Asset {
                token_id:
                    "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_string(),
                amount: "500000".to_string(),
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_direct_swap_t2t_x_to_y() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = test_user_utxo_with_token_x();

        let input_amount = 100_000u64;
        let input = SwapInput::Token {
            token_id: "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: input_amount,
        };
        let output =
            calculator::calculate_output(10_000_000, 5_000_000, input_amount, 997, 1000);
        let min_output = calculator::apply_slippage(output, 0.5);

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            min_output,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_ok(), "Should build T2T X->Y swap: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.inputs[0].box_id, "t2t_pool_box_1");

        let new_pool = &build.unsigned_tx.outputs[0];
        assert_eq!(new_pool.ergo_tree, "t2t_pool_ergo_tree_hex");
        assert_eq!(new_pool.value, "10000000");
        assert_eq!(new_pool.assets.len(), 4);

        let new_x: u64 = new_pool.assets[2].amount.parse().unwrap();
        assert_eq!(new_x, 10_000_000 + input_amount);

        let new_y: u64 = new_pool.assets[3].amount.parse().unwrap();
        assert_eq!(new_y, 5_000_000 - output);

        let user_out = &build.unsigned_tx.outputs[1];
        let received_token = user_out
            .assets
            .iter()
            .find(|a| {
                a.token_id
                    == "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            })
            .expect("User should receive token_y");
        assert_eq!(received_token.amount, output.to_string());

        assert_eq!(build.summary.input_amount, input_amount);
        assert_eq!(build.summary.input_token, "TokenX");
        assert_eq!(build.summary.output_amount, output);
        assert_eq!(build.summary.output_token, "TokenY");
    }

    #[test]
    fn test_direct_swap_t2t_y_to_x() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = test_user_utxo_with_token_y();

        let input_amount = 100_000u64;
        let input = SwapInput::Token {
            token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: input_amount,
        };
        let output =
            calculator::calculate_output(5_000_000, 10_000_000, input_amount, 997, 1000);
        let min_output = calculator::apply_slippage(output, 0.5);

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            min_output,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_ok(), "Should build T2T Y->X swap: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.outputs[0].value, "10000000");
        assert_eq!(build.unsigned_tx.outputs[0].assets.len(), 4);

        let new_x: u64 = build.unsigned_tx.outputs[0].assets[2].amount.parse().unwrap();
        assert_eq!(new_x, 10_000_000 - output);

        let new_y: u64 = build.unsigned_tx.outputs[0].assets[3].amount.parse().unwrap();
        assert_eq!(new_y, 5_000_000 + input_amount);

        let user_out = &build.unsigned_tx.outputs[1];
        let received_token = user_out
            .assets
            .iter()
            .find(|a| {
                a.token_id
                    == "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            })
            .expect("User should receive token_x");
        assert_eq!(received_token.amount, output.to_string());

        assert_eq!(build.summary.input_token, "TokenY");
        assert_eq!(build.summary.output_token, "TokenX");
    }

    #[test]
    fn test_direct_swap_t2t_pool_erg_unchanged() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = test_user_utxo_with_token_x();

        let input = SwapInput::Token {
            token_id: "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: 50_000,
        };

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            1, // any output is fine
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        )
        .unwrap();

        let pool_erg_in: u64 = pool_box.value.parse().unwrap();
        let pool_erg_out: u64 = result.unsigned_tx.outputs[0].value.parse().unwrap();
        assert_eq!(pool_erg_in, pool_erg_out, "T2T pool ERG must be unchanged");

        assert_eq!(
            result.unsigned_tx.outputs[0].additional_registers.get("R4"),
            Some(&"04ca0f".to_string()),
            "R4 fee register must be preserved"
        );

        assert_eq!(
            result.unsigned_tx.outputs[0].ergo_tree,
            pool_box.ergo_tree,
            "Pool ErgoTree must be preserved"
        );
        assert_eq!(
            result.unsigned_tx.outputs[0].assets[0].amount,
            pool_box.assets[0].amount,
            "NFT amount must be unchanged"
        );
        assert_eq!(
            result.unsigned_tx.outputs[0].assets[1].amount,
            pool_box.assets[1].amount,
            "LP amount must be unchanged"
        );
    }

    #[test]
    fn test_direct_swap_t2t_wrong_token() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = Eip12InputBox {
            assets: vec![Eip12Asset {
                token_id: "wrong_token_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: "500000".to_string(),
            }],
            ..test_user_utxo_with_token_x()
        };

        let input = SwapInput::Token {
            token_id: "wrong_token_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount: 100_000,
        };

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            1,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("does not match") || err.contains("Invalid token"),
            "Expected token mismatch error, got: {}",
            err
        );
    }

    #[test]
    fn test_direct_swap_t2t_erg_input_rejected() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = test_user_utxo(); // plain ERG utxo

        let input = SwapInput::Erg {
            amount: 1_000_000_000,
        };

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            1,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("ERG input") || err.contains("not valid for T2T"),
            "Expected ERG rejection error, got: {}",
            err
        );
    }

    #[test]
    fn test_direct_swap_token_to_erg_small_change_folded_into_output() {
        // Regression: when user's ERG input minus TX_FEE is below MIN_CHANGE_VALUE
        // and there are no change tokens, the leftover ERG must be folded into the
        // user swap output instead of being silently dropped.
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let token_id =
            "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();

        let user_utxo = Eip12InputBox {
            value: "1956185".to_string(),
            assets: vec![Eip12Asset {
                token_id: token_id.clone(),
                amount: "2192".to_string(),
            }],
            ..test_user_utxo()
        };

        let input = SwapInput::Token {
            token_id: token_id.clone(),
            amount: 2192,
        };
        let output = calculator::calculate_output(1_000_000, 100_000_000_000, 2192, 997, 1000);

        let result = build_direct_swap_eip12(
            &pool_box,
            &pool,
            &input,
            1,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        let total_input_erg: u64 = build
            .unsigned_tx
            .inputs
            .iter()
            .map(|i| i.value.parse::<u64>().unwrap())
            .sum();
        let total_output_erg: u64 = build
            .unsigned_tx
            .outputs
            .iter()
            .map(|o| o.value.parse::<u64>().unwrap())
            .sum();
        assert_eq!(
            total_input_erg, total_output_erg,
            "ERG inputs ({}) must equal outputs ({})",
            total_input_erg, total_output_erg
        );

        let change_erg = 1_956_185u64 - TX_FEE;
        let user_swap_value: u64 = build.unsigned_tx.outputs[1].value.parse().unwrap();
        assert_eq!(user_swap_value, output + change_erg);
    }
}
