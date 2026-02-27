//! Direct LP Redeem Transaction Builder
//!
//! Builds EIP-12 unsigned transactions that redeem LP tokens from a pool box
//! directly, receiving ERG and tokens in return.
//!
//! # Transaction Structure
//!
//! Inputs:  [pool_box, user_utxos...]
//! Outputs: [new_pool_box, user_output, miner_fee]
//!
//! The pool box contract validates:
//! 1. Same ErgoTree (propositionBytes preserved)
//! 2. Same R4 register (fee config preserved)
//! 3. Same 3 tokens: [pool_nft(1), lp_token(updated), token_y(updated)]
//! 4. Updated ERG value
//! 5. LP returned is proportional to shares redeemed

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

use crate::calculator;
use crate::constants::{fees, lp};
use crate::state::{AmmError, AmmPool, PoolType};
use ergo_tx::{
    collect_change_tokens, select_token_boxes, Eip12Asset, Eip12InputBox, Eip12Output,
    Eip12UnsignedTx,
};

/// Transaction fee in nanoERG (0.0011 ERG - standard)
const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;

/// Minimum box value in nanoERG (required for any output box)
const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

/// Build result for a direct LP redeem
#[derive(Debug)]
pub struct LpRedeemBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: LpRedeemSummary,
}

/// Summary of an LP redeem transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpRedeemSummary {
    pub lp_redeemed: u64,
    pub erg_received: u64,
    pub token_received: u64,
    pub token_name: String,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

/// Build a direct LP redeem EIP-12 unsigned transaction.
///
/// This transaction spends the pool box directly, returning LP tokens to the
/// pool and receiving proportional ERG and tokens in return. The pool box must
/// be inputs[0] and the new pool box must be outputs[0].
///
/// # Arguments
///
/// * `pool_box` - The current pool UTXO (fetched via get_eip12_box_by_id)
/// * `pool` - Parsed pool state (reserves, token IDs, fees)
/// * `lp_amount` - Number of LP tokens to redeem
/// * `user_utxos` - User's UTXOs for funding (must contain LP tokens)
/// * `user_ergo_tree` - User's ErgoTree hex (for output/change)
/// * `current_height` - Current blockchain height
pub fn build_lp_redeem_eip12(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    lp_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<LpRedeemBuildResult, AmmError> {
    // 1. Reject T2T pools
    match pool.pool_type {
        PoolType::N2T => {}
        PoolType::T2T => {
            return Err(AmmError::TxBuildError(
                "Direct LP redeem not yet supported for T2T pools".to_string(),
            ));
        }
    }

    // 2. Parse pool box values directly (ground truth for the contract)
    let pool_erg: u64 = pool_box
        .value
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool box ERG value".to_string()))?;

    // 3. Validate pool box has at least 3 tokens: [pool_nft, lp_token, token_y]
    if pool_box.assets.len() < 3 {
        return Err(AmmError::TxBuildError(format!(
            "Pool box has {} tokens, expected at least 3",
            pool_box.assets.len()
        )));
    }

    let pool_nft = &pool_box.assets[0];
    let pool_lp = &pool_box.assets[1];
    let pool_token_y = &pool_box.assets[2];

    // 4. Parse LP locked amount from pool box
    let lp_locked: u64 = pool_lp
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool LP amount".to_string()))?;

    // 5. Parse pool token_y amount
    let pool_token_y_amount: u64 = pool_token_y
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token Y amount".to_string()))?;

    // 6. Parse fee_num from pool box R4 register (sigma-serialized Int)
    let _fee_num = parse_fee_num_from_r4(&pool_box.additional_registers)?;

    // 7. Calculate LP supply: TOTAL_EMISSION - locked
    let supply_lp = calculator::calculate_lp_supply(lp_locked, lp::TOTAL_EMISSION);

    // 8. Calculate redeem shares
    let (erg_out, token_out) =
        calculator::calculate_redeem_shares(pool_erg, pool_token_y_amount, supply_lp, lp_amount);

    // 9. Validate shares > 0
    if erg_out == 0 {
        return Err(AmmError::TxBuildError(
            "LP redeem too small: ERG share would be 0".to_string(),
        ));
    }
    if token_out == 0 {
        return Err(AmmError::TxBuildError(
            "LP redeem too small: token share would be 0".to_string(),
        ));
    }

    // 10. Validate new pool ERG >= MIN_BOX_VALUE after removing erg_out
    let new_pool_erg = pool_erg
        .checked_sub(erg_out)
        .ok_or_else(|| AmmError::TxBuildError("Pool ERG underflow".to_string()))?;

    if new_pool_erg < MIN_BOX_VALUE {
        return Err(AmmError::TxBuildError(
            "New pool box would have less than minimum ERG".to_string(),
        ));
    }

    // 11. Build new pool box output
    let new_lp_locked = lp_locked
        .checked_add(lp_amount)
        .ok_or_else(|| AmmError::TxBuildError("Pool LP overflow".to_string()))?;

    let new_pool_token_y = pool_token_y_amount
        .checked_sub(token_out)
        .ok_or_else(|| AmmError::TxBuildError("Pool token Y underflow".to_string()))?;

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
                amount: new_lp_locked.to_string(),
            },
            Eip12Asset {
                token_id: pool_token_y.token_id.clone(),
                amount: new_pool_token_y.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: pool_box.additional_registers.clone(),
    };

    // 12. User ERG needed for tx: just TX_FEE (the ERG output comes from the pool)
    let user_erg_needed = TX_FEE;

    // 13. Select UTXOs: user needs LP tokens AND ERG for fee
    let selected =
        select_token_boxes(user_utxos, &pool.lp_token_id, lp_amount, user_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    // 14. Calculate change
    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = Some((pool.lp_token_id.as_str(), lp_amount));
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    // 15. Build merged user output: erg_out (from pool) + change_erg + Token Y + change tokens
    let user_erg = erg_out + change_erg;

    // Token Y comes first, then any change tokens (including leftover LP tokens)
    let mut user_assets = vec![Eip12Asset {
        token_id: pool.token_y.token_id.clone(),
        amount: token_out.to_string(),
    }];
    user_assets.extend(change_tokens);

    let user_output = Eip12Output {
        value: user_erg.to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: user_assets,
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // 16. Miner fee output
    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    // 17. Assemble transaction: pool box MUST be inputs[0]
    let mut inputs = vec![pool_box.clone()];
    inputs.extend(selected.boxes);

    let outputs = vec![new_pool_output, user_output, fee_output];

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    // 18. Build summary
    let token_name = pool
        .token_y
        .name
        .clone()
        .unwrap_or_else(|| pool.token_y.token_id[..8].to_string());

    let summary = LpRedeemSummary {
        lp_redeemed: lp_amount,
        erg_received: erg_out,
        token_received: token_out,
        token_name,
        miner_fee: TX_FEE,
        total_erg_cost: user_erg_needed,
    };

    Ok(LpRedeemBuildResult {
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
    use crate::state::{AmmPool, PoolType, TokenAmount};

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
                    amount: "9223372036854774807".to_string(), // max LP - 1000 circulating
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
                m.insert("R4".to_string(), "04ca0f".to_string()); // fee_num=997
                m
            },
            extension: HashMap::new(),
        }
    }

    fn test_user_utxo_with_lp() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "user_utxo_1".to_string(),
            transaction_id: "user_tx_1".to_string(),
            index: 0,
            value: "5000000000".to_string(), // 5 ERG
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![Eip12Asset {
                token_id: "lp_token".to_string(),
                amount: "500".to_string(), // 500 LP tokens
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_lp_redeem_basic() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo_with_lp();

        let lp_amount = 100u64; // Redeem 100 LP = 10% of 1000 circulating

        // Pool reserves: 100 ERG, 1_000_000 tokens
        // LP locked: 9223372036854774807
        // LP supply: TOTAL_EMISSION - locked = 1000
        let lp_locked: u64 = 9_223_372_036_854_774_807;
        let supply_lp =
            calculator::calculate_lp_supply(lp_locked, lp::TOTAL_EMISSION);
        assert_eq!(supply_lp, 1000);

        let (expected_erg_out, expected_token_out) = calculator::calculate_redeem_shares(
            100_000_000_000,
            1_000_000,
            supply_lp,
            lp_amount,
        );
        // 10% of pool = 10 ERG + 100k tokens
        assert_eq!(expected_erg_out, 10_000_000_000);
        assert_eq!(expected_token_out, 100_000);

        let result = build_lp_redeem_eip12(
            &pool_box,
            &pool,
            lp_amount,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        // inputs[0] = pool box
        assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");
        // inputs[1] = user utxo
        assert_eq!(build.unsigned_tx.inputs[1].box_id, "user_utxo_1");

        // outputs[0] = new pool box
        let new_pool = &build.unsigned_tx.outputs[0];
        // New pool ERG = 100 - 10 = 90 ERG
        let new_pool_erg: u64 = new_pool.value.parse().unwrap();
        assert_eq!(new_pool_erg, 100_000_000_000 - expected_erg_out);
        // New pool token_y = 1_000_000 - 100_000
        let new_token_y: u64 = new_pool.assets[2].amount.parse().unwrap();
        assert_eq!(new_token_y, 1_000_000 - expected_token_out);
        // New pool LP locked = old + redeemed (LP returned to pool)
        let new_lp_locked: u64 = new_pool.assets[1].amount.parse().unwrap();
        assert_eq!(new_lp_locked, lp_locked + lp_amount);

        // Summary
        assert_eq!(build.summary.lp_redeemed, lp_amount);
        assert_eq!(build.summary.erg_received, expected_erg_out);
        assert_eq!(build.summary.token_received, expected_token_out);
        assert_eq!(build.summary.miner_fee, TX_FEE);
    }

    #[test]
    fn test_lp_redeem_pool_preserved() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo_with_lp();

        let result = build_lp_redeem_eip12(
            &pool_box,
            &pool,
            100,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        )
        .unwrap();

        let new_pool = &result.unsigned_tx.outputs[0];

        // ErgoTree preserved
        assert_eq!(new_pool.ergo_tree, "pool_ergo_tree_hex");

        // R4 register preserved
        assert_eq!(
            new_pool.additional_registers.get("R4"),
            Some(&"04ca0f".to_string())
        );

        // Pool NFT preserved
        assert_eq!(
            new_pool.assets[0].token_id,
            "pool_nft_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(new_pool.assets[0].amount, "1");
    }

    #[test]
    fn test_lp_redeem_user_receives_erg_and_token() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo_with_lp();

        let result = build_lp_redeem_eip12(
            &pool_box,
            &pool,
            100,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        )
        .unwrap();

        let user_out = &result.unsigned_tx.outputs[1];

        // User output should have Token Y as first asset
        assert_eq!(
            user_out.assets[0].token_id,
            "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let token_received: u64 = user_out.assets[0].amount.parse().unwrap();
        assert_eq!(token_received, 100_000); // 10% of 1M

        // User should also receive ERG (erg_out + change_erg)
        let user_erg: u64 = user_out.value.parse().unwrap();
        // erg_out = 10 ERG, change_erg = 5 ERG - TX_FEE
        let expected_erg = 10_000_000_000 + (5_000_000_000 - TX_FEE);
        assert_eq!(user_erg, expected_erg);

        // User should also have remaining LP tokens as change
        let lp_change = user_out
            .assets
            .iter()
            .find(|a| a.token_id == "lp_token");
        assert!(
            lp_change.is_some(),
            "User should receive remaining LP tokens as change"
        );
        let remaining_lp: u64 = lp_change.unwrap().amount.parse().unwrap();
        assert_eq!(remaining_lp, 500 - 100); // 500 - 100 redeemed
    }

    #[test]
    fn test_lp_redeem_insufficient_lp() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = Eip12InputBox {
            assets: vec![Eip12Asset {
                token_id: "lp_token".to_string(),
                amount: "10".to_string(), // Only 10 LP tokens
            }],
            ..test_user_utxo_with_lp()
        };

        let result = build_lp_redeem_eip12(
            &pool_box,
            &pool,
            500, // Need 500 LP, only have 10
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Insufficient"),
            "Should report insufficient LP tokens"
        );
    }

    #[test]
    fn test_lp_redeem_t2t_rejected() {
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
        let user_utxo = test_user_utxo_with_lp();

        let result = build_lp_redeem_eip12(
            &pool_box,
            &pool,
            100,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("T2T"));
    }
}
