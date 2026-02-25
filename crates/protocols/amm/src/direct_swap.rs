//! Direct Swap Transaction Builder
//!
//! Builds EIP-12 unsigned transactions that spend the pool box directly,
//! rather than creating a proxy box for off-chain bots to execute.
//!
//! # Transaction Structure
//!
//! Inputs:  [pool_box, user_utxos...]
//! Outputs: [new_pool_box, user_swap_output, miner_fee, change?]
//!
//! The pool box contract validates:
//! 1. Same ErgoTree (propositionBytes preserved)
//! 2. Same R4 register (fee config preserved)
//! 3. Same 3 tokens: [pool_nft(1), lp_token(same), token_y(updated)]
//! 4. Updated ERG value
//! 5. Constant product invariant holds

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

use crate::calculator;
use crate::constants::fees;
use crate::state::{AmmError, AmmPool, PoolType, SwapInput};
use crate::tx_builder::MIN_CHANGE_VALUE;
use ergo_tx::{
    collect_change_tokens, select_erg_boxes, select_token_boxes, Eip12Asset, Eip12InputBox,
    Eip12Output, Eip12UnsignedTx,
};

/// Transaction fee in nanoERG (0.0011 ERG - standard)
const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;

/// Minimum box value in nanoERG (required for any output box)
const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

/// Build result for a direct swap
#[derive(Debug)]
pub struct DirectSwapBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: DirectSwapSummary,
}

/// Summary of a direct swap transaction
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

/// Build a direct swap EIP-12 unsigned transaction.
///
/// This transaction spends the pool box directly (no proxy/bot pattern).
/// The pool box must be inputs[0] and the new pool box must be outputs[0].
///
/// # Arguments
///
/// * `pool_box` - The current pool UTXO (fetched via get_eip12_box_by_id)
/// * `pool` - Parsed pool state (reserves, token IDs, fees)
/// * `input` - What the user is swapping (ERG or Token)
/// * `min_output` - Minimum acceptable output (slippage protection)
/// * `user_utxos` - User's UTXOs for funding
/// * `user_ergo_tree` - User's ErgoTree hex (for output/change)
/// * `current_height` - Current blockchain height
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
        PoolType::N2T => {}
        PoolType::T2T => {
            return Err(AmmError::TxBuildError(
                "Direct swap not yet supported for T2T pools".to_string(),
            ));
        }
    }

    // Parse pool box values directly (ground truth for the contract)
    let pool_erg: u64 = pool_box
        .value
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool box ERG value".to_string()))?;

    // Find pool tokens (expected: [pool_nft, lp_token, token_y])
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

    // Parse fee_num from pool box R4 register (sigma-serialized Int)
    // This is the ground truth the contract uses for the constant product check.
    let fee_num = parse_fee_num_from_r4(&pool_box.additional_registers)?;
    let fee_denom = fees::DEFAULT_FEE_DENOM;

    // Calculate output amount using pool box reserves and fee from R4
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

    // Validate min_output
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

    // Build new pool box output
    let (new_pool_erg, new_pool_token_y_amount) = if is_erg_to_token {
        // ERG -> Token: pool gains ERG, loses token_y
        let new_erg = pool_erg
            .checked_add(input_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool ERG overflow".to_string()))?;
        let new_token_y = pool_token_y_amount
            .checked_sub(output_amount)
            .ok_or_else(|| AmmError::TxBuildError("Pool token Y underflow".to_string()))?;
        (new_erg, new_token_y)
    } else {
        // Token -> ERG: pool loses ERG, gains token_y
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

    // New pool box: same ErgoTree, same registers, updated value + tokens
    let new_pool_output = Eip12Output {
        value: new_pool_erg.to_string(),
        ergo_tree: pool_box.ergo_tree.clone(),
        assets: vec![
            Eip12Asset {
                token_id: pool_nft.token_id.clone(),
                amount: pool_nft.amount.clone(), // same NFT count (1)
            },
            Eip12Asset {
                token_id: pool_lp.token_id.clone(),
                amount: pool_lp.amount.clone(), // same LP amount
            },
            Eip12Asset {
                token_id: pool_token_y.token_id.clone(),
                amount: new_pool_token_y_amount.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: pool_box.additional_registers.clone(),
    };

    // Build user swap output (goes to recipient if set)
    let output_tree = recipient_ergo_tree.unwrap_or(user_ergo_tree);
    let user_swap_output = if is_erg_to_token {
        // User receives tokens
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
        // User receives ERG
        Eip12Output {
            value: output_amount.to_string(),
            ergo_tree: output_tree.to_string(),
            assets: vec![],
            creation_height: current_height,
            additional_registers: HashMap::new(),
        }
    };

    // Miner fee output
    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    // Calculate user ERG needed and select minimum UTXOs
    let user_erg_needed = if is_erg_to_token {
        // User provides: input_amount (goes to pool) + MIN_BOX_VALUE (for swap output box) + TX_FEE
        input_amount
            .checked_add(MIN_BOX_VALUE)
            .and_then(|v| v.checked_add(TX_FEE))
            .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?
    } else {
        // User provides: TX_FEE only (output ERG comes from pool)
        TX_FEE
    };

    let selected = match input {
        SwapInput::Erg { .. } => select_erg_boxes(user_utxos, user_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?,
        SwapInput::Token { token_id, amount } => {
            select_token_boxes(user_utxos, token_id, *amount, user_erg_needed)
                .map_err(|e| AmmError::TxBuildError(e.to_string()))?
        }
    };

    // Change calculation
    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = match input {
        SwapInput::Erg { .. } => None,
        SwapInput::Token { token_id, amount } => Some((token_id.as_str(), *amount)),
    };
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    if !change_tokens.is_empty() && change_erg < MIN_CHANGE_VALUE {
        return Err(AmmError::TxBuildError(format!(
            "Change tokens exist but not enough ERG for change box (need {}, have {})",
            MIN_CHANGE_VALUE, change_erg
        )));
    }

    // If change ERG is too small for a separate change box and there are no change
    // tokens, fold it into the user swap output to avoid losing ERG.
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

    // Build outputs list
    let mut outputs = vec![new_pool_output, user_swap_output, fee_output];

    if change_erg >= MIN_CHANGE_VALUE || !change_tokens.is_empty() {
        let change_output = Eip12Output {
            value: change_erg.to_string(),
            ergo_tree: user_ergo_tree.to_string(),
            assets: change_tokens,
            creation_height: current_height,
            additional_registers: HashMap::new(),
        };
        outputs.push(change_output);
    }

    // Build transaction: pool box MUST be inputs[0]
    let mut inputs = vec![pool_box.clone()];
    inputs.extend(selected.boxes);

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    // Build summary
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

/// Parse the fee numerator (Int) from pool box R4 register hex.
///
/// The register value is a sigma-serialized Constant. Falls back to
/// `DEFAULT_FEE_NUM` if R4 is missing or not an Int.
fn parse_fee_num_from_r4(registers: &HashMap<String, String>) -> Result<i32, AmmError> {
    let r4_hex = match registers.get("R4") {
        Some(hex) => hex,
        None => return Ok(fees::DEFAULT_FEE_NUM),
    };
    let r4_bytes = hex::decode(r4_hex)
        .map_err(|e| AmmError::TxBuildError(format!("Invalid R4 hex: {}", e)))?;
    let constant = Constant::sigma_parse_bytes(&r4_bytes)
        .map_err(|e| AmmError::TxBuildError(format!("Failed to parse R4 constant: {}", e)))?;
    match &constant.v {
        Literal::Int(v) => Ok(*v),
        _ => Ok(fees::DEFAULT_FEE_NUM),
    }
}

// =============================================================================
// Tests
// =============================================================================

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

        // inputs[0] = pool box
        assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");
        // inputs[1] = user utxo
        assert_eq!(build.unsigned_tx.inputs[1].box_id, "user_utxo_1");

        // outputs[0] = new pool box (same ergo_tree)
        assert_eq!(build.unsigned_tx.outputs[0].ergo_tree, "pool_ergo_tree_hex");
        // New pool ERG = 100 + 1 = 101 ERG
        let new_pool_erg: u64 = build.unsigned_tx.outputs[0].value.parse().unwrap();
        assert_eq!(new_pool_erg, 100_000_000_000 + 1_000_000_000);
        // Pool token_y decreased
        let new_token_y: u64 = build.unsigned_tx.outputs[0].assets[2]
            .amount
            .parse()
            .unwrap();
        assert_eq!(new_token_y, 1_000_000 - output);

        // outputs[1] = user swap output (receives tokens)
        assert_eq!(build.unsigned_tx.outputs[1].assets.len(), 1);
        let user_token_received: u64 = build.unsigned_tx.outputs[1].assets[0]
            .amount
            .parse()
            .unwrap();
        assert_eq!(user_token_received, output);
        // User output box has MIN_BOX_VALUE ERG
        assert_eq!(
            build.unsigned_tx.outputs[1].value,
            MIN_BOX_VALUE.to_string()
        );

        // outputs[2] = miner fee
        assert_eq!(build.unsigned_tx.outputs[2].value, TX_FEE.to_string());

        // Summary
        assert_eq!(build.summary.input_amount, 1_000_000_000);
        assert_eq!(build.summary.input_token, "ERG");
        assert_eq!(build.summary.output_amount, output);
        assert_eq!(build.summary.output_token, "TestToken");
        assert_eq!(build.summary.miner_fee, TX_FEE);
        // No execution fee
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

        // Pool box is input[0]
        assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");

        // New pool box: ERG decreased, token_y increased
        let new_pool_erg: u64 = build.unsigned_tx.outputs[0].value.parse().unwrap();
        assert_eq!(new_pool_erg, 100_000_000_000 - output);
        let new_token_y: u64 = build.unsigned_tx.outputs[0].assets[2]
            .amount
            .parse()
            .unwrap();
        assert_eq!(new_token_y, 1_000_000 + 10000);

        // User receives ERG
        let user_erg_received: u64 = build.unsigned_tx.outputs[1].value.parse().unwrap();
        assert_eq!(user_erg_received, output);
        assert!(build.unsigned_tx.outputs[1].assets.is_empty());

        // Change should have remaining tokens
        let change = &build.unsigned_tx.outputs[3]; // pool, swap_output, fee, change
        let change_token: &Eip12Asset = change
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
        // Set min_output absurdly high
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

        // R4 should be preserved in the new pool box
        assert_eq!(
            result.unsigned_tx.outputs[0].additional_registers.get("R4"),
            Some(&"04ca0f".to_string())
        );
    }

    #[test]
    fn test_direct_swap_t2t_not_supported() {
        let pool = AmmPool {
            pool_type: PoolType::T2T,
            token_x: Some(TokenAmount {
                token_id: "token_x".to_string(),
                amount: 1000,
                decimals: None,
                name: None,
            }),
            ..test_n2t_pool()
        };
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
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("T2T"));
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

        // User UTXO with exactly the swap tokens and small ERG (just above TX_FEE)
        let user_utxo = Eip12InputBox {
            value: "1956185".to_string(), // small: 1,956,185 nanoERG
            assets: vec![Eip12Asset {
                token_id: token_id.clone(),
                amount: "2192".to_string(), // exact swap amount, no leftover tokens
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

        // Should have exactly 3 outputs: pool, user swap, miner fee
        // (no separate change box since change_erg < MIN_CHANGE_VALUE)
        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        // Verify total ERG balances: inputs == outputs
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

        // User swap output should include the leftover ERG
        let change_erg = 1_956_185u64 - TX_FEE;
        let user_swap_value: u64 = build.unsigned_tx.outputs[1].value.parse().unwrap();
        assert_eq!(user_swap_value, output + change_erg);
    }
}
