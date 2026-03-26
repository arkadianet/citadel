//! Direct LP Deposit Transaction Builder
//!
//! Builds EIP-12 unsigned transactions that deposit ERG and tokens into a pool
//! box directly, receiving LP tokens in return.
//!
//! # Transaction Structure
//!
//! Inputs:  [pool_box, user_utxos...]
//! Outputs: [new_pool_box, user_output, miner_fee]
//!
//! ## N2T Pool
//! The pool box contract validates:
//! 1. Same ErgoTree (propositionBytes preserved)
//! 2. Same R4 register (fee config preserved)
//! 3. Same 3 tokens: [pool_nft(1), lp_token(updated), token_y(updated)]
//! 4. Updated ERG value
//! 5. LP reward is proportional to deposit
//!
//! ## T2T Pool
//! The pool box contract validates:
//! 1. Same ErgoTree (propositionBytes preserved)
//! 2. Same R4 register (fee config preserved)
//! 3. Same 4 tokens: [pool_nft(1), lp_token(updated), token_x(updated), token_y(updated)]
//! 4. ERG value unchanged (storage rent only)
//! 5. LP reward is proportional to deposit

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::calculator;
use crate::state::{AmmError, AmmPool, PoolType};
use ergo_tx::{
    collect_change_tokens, collect_multi_change_tokens, select_multi_token_boxes,
    select_token_boxes, Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;
const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

#[derive(Debug)]
pub struct LpDepositBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: LpDepositSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpDepositSummary {
    pub erg_deposited: u64,
    pub token_deposited: u64,
    pub token_name: String,
    pub lp_reward: u64,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn build_lp_deposit_eip12(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    erg_amount: u64,
    token_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<LpDepositBuildResult, AmmError> {
    match pool.pool_type {
        PoolType::N2T => build_n2t_lp_deposit(
            pool_box,
            pool,
            erg_amount,
            token_amount,
            user_utxos,
            user_ergo_tree,
            current_height,
        ),
        PoolType::T2T => build_t2t_lp_deposit(
            pool_box,
            pool,
            erg_amount, // For T2T this is Token X amount
            token_amount,
            user_utxos,
            user_ergo_tree,
            current_height,
        ),
    }
}

/// N2T pools have 3 tokens: [NFT, LP, Token_Y]. ERG is the X reserve.
#[allow(clippy::too_many_arguments)]
fn build_n2t_lp_deposit(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    erg_amount: u64,
    token_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<LpDepositBuildResult, AmmError> {
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

    let lp_locked: u64 = pool_lp
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool LP amount".to_string()))?;

    let pool_token_y_amount: u64 = pool_token_y
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token Y amount".to_string()))?;

    // Pool contract requires R4 to be valid even though we don't use the fee for LP deposits
    let _fee_num = crate::constants::parse_fee_num_from_r4(&pool_box.additional_registers)?;

    let supply_lp =
        calculator::calculate_lp_supply(lp_locked, crate::constants::lp::TOTAL_EMISSION);

    let lp_reward = calculator::calculate_lp_reward(
        pool_erg,
        pool_token_y_amount,
        supply_lp,
        erg_amount,
        token_amount,
    );

    if lp_reward == 0 {
        return Err(AmmError::TxBuildError(
            "LP deposit too small: reward would be 0 LP tokens".to_string(),
        ));
    }

    let new_pool_erg = pool_erg
        .checked_add(erg_amount)
        .ok_or_else(|| AmmError::TxBuildError("Pool ERG overflow".to_string()))?;

    let new_lp_locked = lp_locked
        .checked_sub(lp_reward)
        .ok_or_else(|| AmmError::TxBuildError("Pool LP underflow".to_string()))?;

    let new_pool_token_y = pool_token_y_amount
        .checked_add(token_amount)
        .ok_or_else(|| AmmError::TxBuildError("Pool token Y overflow".to_string()))?;

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

    let user_erg_needed = erg_amount
        .checked_add(MIN_BOX_VALUE)
        .and_then(|v| v.checked_add(TX_FEE))
        .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?;

    let selected =
        select_token_boxes(user_utxos, &pool.token_y.token_id, token_amount, user_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = Some((pool.token_y.token_id.as_str(), token_amount));
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    let user_erg = MIN_BOX_VALUE + change_erg;

    let mut user_assets = vec![Eip12Asset {
        token_id: pool.lp_token_id.clone(),
        amount: lp_reward.to_string(),
    }];
    user_assets.extend(change_tokens);

    let user_output = Eip12Output {
        value: user_erg.to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: user_assets,
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    // Pool box MUST be inputs[0] — pool contract validates SELF == INPUTS(0)
    let mut inputs = vec![pool_box.clone()];
    inputs.extend(selected.boxes);

    let outputs = vec![new_pool_output, user_output, fee_output];

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    let token_name = pool
        .token_y
        .name
        .clone()
        .unwrap_or_else(|| pool.token_y.token_id[..8].to_string());

    let summary = LpDepositSummary {
        erg_deposited: erg_amount,
        token_deposited: token_amount,
        token_name,
        lp_reward,
        miner_fee: TX_FEE,
        total_erg_cost: user_erg_needed,
    };

    Ok(LpDepositBuildResult {
        unsigned_tx,
        summary,
    })
}

/// T2T pools have 4 tokens: [NFT, LP, Token_X, Token_Y].
/// ERG stays unchanged (storage rent only); both reserves are tokens.
#[allow(clippy::too_many_arguments)]
fn build_t2t_lp_deposit(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    amount_x: u64,
    amount_y: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<LpDepositBuildResult, AmmError> {
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

    let lp_locked: u64 = pool_lp
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool LP amount".to_string()))?;

    let pool_token_x_amount: u64 = pool_token_x_asset
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token X amount".to_string()))?;

    let pool_token_y_amount: u64 = pool_token_y_asset
        .amount
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid pool token Y amount".to_string()))?;

    let token_x = pool.token_x.as_ref().ok_or_else(|| {
        AmmError::TxBuildError("T2T pool must have token_x defined".to_string())
    })?;

    let _fee_num = crate::constants::parse_fee_num_from_r4(&pool_box.additional_registers)?;

    let supply_lp =
        calculator::calculate_lp_supply(lp_locked, crate::constants::lp::TOTAL_EMISSION);

    // T2T uses token reserves (not ERG) for LP reward calculation
    let lp_reward = calculator::calculate_lp_reward(
        pool_token_x_amount,
        pool_token_y_amount,
        supply_lp,
        amount_x,
        amount_y,
    );

    if lp_reward == 0 {
        return Err(AmmError::TxBuildError(
            "LP deposit too small: reward would be 0 LP tokens".to_string(),
        ));
    }

    let new_lp_locked = lp_locked
        .checked_sub(lp_reward)
        .ok_or_else(|| AmmError::TxBuildError("Pool LP underflow".to_string()))?;

    let new_pool_token_x = pool_token_x_amount
        .checked_add(amount_x)
        .ok_or_else(|| AmmError::TxBuildError("Pool token X overflow".to_string()))?;

    let new_pool_token_y = pool_token_y_amount
        .checked_add(amount_y)
        .ok_or_else(|| AmmError::TxBuildError("Pool token Y overflow".to_string()))?;

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
                amount: new_lp_locked.to_string(),
            },
            Eip12Asset {
                token_id: pool_token_x_asset.token_id.clone(),
                amount: new_pool_token_x.to_string(),
            },
            Eip12Asset {
                token_id: pool_token_y_asset.token_id.clone(),
                amount: new_pool_token_y.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: pool_box.additional_registers.clone(),
    };

    // No ERG deposited into T2T pool, only need MIN_BOX_VALUE + TX_FEE
    let user_erg_needed = MIN_BOX_VALUE
        .checked_add(TX_FEE)
        .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?;

    let required_tokens = [
        (token_x.token_id.as_str(), amount_x),
        (pool.token_y.token_id.as_str(), amount_y),
    ];
    let selected =
        select_multi_token_boxes(user_utxos, &required_tokens, user_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let change_erg = selected.total_erg - user_erg_needed;
    let spent_tokens = [
        (token_x.token_id.as_str(), amount_x),
        (pool.token_y.token_id.as_str(), amount_y),
    ];
    let change_tokens = collect_multi_change_tokens(&selected.boxes, &spent_tokens);

    let user_erg = MIN_BOX_VALUE + change_erg;

    let mut user_assets = vec![Eip12Asset {
        token_id: pool.lp_token_id.clone(),
        amount: lp_reward.to_string(),
    }];
    user_assets.extend(change_tokens);

    let user_output = Eip12Output {
        value: user_erg.to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: user_assets,
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    let mut inputs = vec![pool_box.clone()];
    inputs.extend(selected.boxes);

    let outputs = vec![new_pool_output, user_output, fee_output];

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    // erg_deposited is actually token_x amount; frontend interprets based on pool type
    let token_name = pool
        .token_y
        .name
        .clone()
        .unwrap_or_else(|| pool.token_y.token_id[..8].to_string());

    let summary = LpDepositSummary {
        erg_deposited: amount_x,
        token_deposited: amount_y,
        token_name,
        lp_reward,
        miner_fee: TX_FEE,
        total_erg_cost: user_erg_needed,
    };

    Ok(LpDepositBuildResult {
        unsigned_tx,
        summary,
    })
}

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

    fn test_user_utxo_with_tokens() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "user_utxo_1".to_string(),
            transaction_id: "user_tx_1".to_string(),
            index: 0,
            value: "50000000000".to_string(), // 50 ERG
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![Eip12Asset {
                token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: "500000".to_string(), // 500k tokens
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_lp_deposit_basic() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo_with_tokens();

        let erg_amount = 10_000_000_000u64; // 10 ERG
        let token_amount = 100_000u64; // 100k tokens

        // Pool reserves: 100 ERG, 1_000_000 tokens
        // LP locked: 9223372036854774807
        // LP supply: TOTAL_EMISSION - locked = 9223372036854775807 - 9223372036854774807 = 1000
        let lp_locked: u64 = 9_223_372_036_854_774_807;
        let supply_lp = calculator::calculate_lp_supply(
            lp_locked,
            crate::constants::lp::TOTAL_EMISSION,
        );
        assert_eq!(supply_lp, 1000);

        let expected_lp_reward = calculator::calculate_lp_reward(
            100_000_000_000,
            1_000_000,
            supply_lp,
            erg_amount,
            token_amount,
        );
        assert!(expected_lp_reward > 0, "Expected non-zero LP reward");

        let result = build_lp_deposit_eip12(
            &pool_box,
            &pool,
            erg_amount,
            token_amount,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");
        assert_eq!(build.unsigned_tx.inputs[1].box_id, "user_utxo_1");

        let new_pool = &build.unsigned_tx.outputs[0];
        let new_pool_erg: u64 = new_pool.value.parse().unwrap();
        assert_eq!(new_pool_erg, 100_000_000_000 + erg_amount);
        let new_token_y: u64 = new_pool.assets[2].amount.parse().unwrap();
        assert_eq!(new_token_y, 1_000_000 + token_amount);
        let new_lp_locked: u64 = new_pool.assets[1].amount.parse().unwrap();
        assert_eq!(new_lp_locked, lp_locked - expected_lp_reward);

        let user_out = &build.unsigned_tx.outputs[1];
        assert!(
            user_out.assets.iter().any(|a| a.token_id == "lp_token"),
            "User output should contain LP tokens"
        );
        let lp_received: u64 = user_out
            .assets
            .iter()
            .find(|a| a.token_id == "lp_token")
            .unwrap()
            .amount
            .parse()
            .unwrap();
        assert_eq!(lp_received, expected_lp_reward);

        assert_eq!(build.summary.erg_deposited, erg_amount);
        assert_eq!(build.summary.token_deposited, token_amount);
        assert_eq!(build.summary.lp_reward, expected_lp_reward);
        assert_eq!(build.summary.miner_fee, TX_FEE);
    }

    #[test]
    fn test_lp_deposit_pool_box_preserved() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo_with_tokens();

        let result = build_lp_deposit_eip12(
            &pool_box,
            &pool,
            10_000_000_000,
            100_000,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        )
        .unwrap();

        let new_pool = &result.unsigned_tx.outputs[0];
        assert_eq!(new_pool.ergo_tree, "pool_ergo_tree_hex");
        assert_eq!(
            new_pool.additional_registers.get("R4"),
            Some(&"04ca0f".to_string())
        );
        assert_eq!(
            new_pool.assets[0].token_id,
            "pool_nft_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(new_pool.assets[0].amount, "1");
    }

    #[test]
    fn test_lp_deposit_user_receives_lp() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo_with_tokens();

        let result = build_lp_deposit_eip12(
            &pool_box,
            &pool,
            10_000_000_000,
            100_000,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        )
        .unwrap();

        let user_out = &result.unsigned_tx.outputs[1];
        assert_eq!(user_out.assets[0].token_id, "lp_token");
        let lp_amount: u64 = user_out.assets[0].amount.parse().unwrap();
        assert!(lp_amount > 0, "LP reward should be positive");

        let change_token = user_out
            .assets
            .iter()
            .find(|a| {
                a.token_id
                    == "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            });
        assert!(
            change_token.is_some(),
            "User should receive remaining token_y as change"
        );
        let remaining: u64 = change_token.unwrap().amount.parse().unwrap();
        assert_eq!(remaining, 500_000 - 100_000); // 500k - 100k deposited
    }

    #[test]
    fn test_lp_deposit_insufficient_erg() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = Eip12InputBox {
            value: "1000000".to_string(), // 0.001 ERG - not enough for 10 ERG deposit
            ..test_user_utxo_with_tokens()
        };

        let result = build_lp_deposit_eip12(
            &pool_box,
            &pool,
            10_000_000_000, // 10 ERG
            100_000,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Insufficient"),
            "Should report insufficient funds"
        );
    }

    #[test]
    fn test_lp_deposit_insufficient_tokens() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = Eip12InputBox {
            assets: vec![Eip12Asset {
                token_id: "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                amount: "100".to_string(), // Only 100, need 100k
            }],
            ..test_user_utxo_with_tokens()
        };

        let result = build_lp_deposit_eip12(
            &pool_box,
            &pool,
            10_000_000_000,
            100_000, // Need 100k tokens, only have 100
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Insufficient"),
            "Should report insufficient tokens"
        );
    }

    #[test]
    fn test_lp_deposit_zero_reward() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = test_user_utxo_with_tokens();

        // Tiny deposit: 1 nanoERG + 1 token => LP reward should be 0
        let result = build_lp_deposit_eip12(
            &pool_box,
            &pool,
            1,     // 1 nanoERG
            1,     // 1 token unit
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("reward would be 0"),
            "Should reject zero LP reward"
        );
    }

    fn test_t2t_pool() -> AmmPool {
        AmmPool {
            pool_id:
                "t2t_pool_nft_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
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
                    "token_y_id_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
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
            value: "10000000".to_string(), // 0.01 ERG storage rent
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
                    amount: "9223372036854774807".to_string(), // max LP supply minus 1000 circulating
                },
                Eip12Asset {
                    token_id:
                        "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .to_string(),
                    amount: "10000000".to_string(), // 10M token_x
                },
                Eip12Asset {
                    token_id:
                        "token_y_id_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                            .to_string(),
                    amount: "5000000".to_string(), // 5M token_y
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

    fn test_user_utxo_with_both_tokens() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "user_utxo_t2t_1".to_string(),
            transaction_id: "user_tx_t2t_1".to_string(),
            index: 0,
            value: "5000000000".to_string(), // 5 ERG
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![
                Eip12Asset {
                    token_id:
                        "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .to_string(),
                    amount: "500000".to_string(), // 500k token_x
                },
                Eip12Asset {
                    token_id:
                        "token_y_id_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                            .to_string(),
                    amount: "250000".to_string(), // 250k token_y
                },
            ],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_lp_deposit_t2t_basic() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = test_user_utxo_with_both_tokens();

        let amount_x = 100_000u64; // 100k token_x
        let amount_y = 50_000u64; // 50k token_y

        // Pool reserves: 10M token_x, 5M token_y
        // LP locked: 9223372036854774807
        // LP supply: TOTAL_EMISSION - locked = 1000
        let lp_locked: u64 = 9_223_372_036_854_774_807;
        let supply_lp = calculator::calculate_lp_supply(
            lp_locked,
            crate::constants::lp::TOTAL_EMISSION,
        );
        assert_eq!(supply_lp, 1000);

        let expected_lp_reward = calculator::calculate_lp_reward(
            10_000_000, // reserves_x = pool token_x
            5_000_000,  // reserves_y = pool token_y
            supply_lp,
            amount_x,
            amount_y,
        );
        assert!(expected_lp_reward > 0, "Expected non-zero LP reward");

        let result = build_lp_deposit_eip12(
            &pool_box,
            &pool,
            amount_x,
            amount_y,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.inputs[0].box_id, "t2t_pool_box_1");

        let new_pool = &build.unsigned_tx.outputs[0];
        let new_pool_erg: u64 = new_pool.value.parse().unwrap();
        assert_eq!(new_pool_erg, 10_000_000, "T2T pool ERG must stay unchanged");
        assert_eq!(new_pool.assets.len(), 4, "T2T pool must have 4 tokens");
        assert_eq!(new_pool.assets[0].amount, "1");
        let new_lp_locked: u64 = new_pool.assets[1].amount.parse().unwrap();
        assert_eq!(new_lp_locked, lp_locked - expected_lp_reward);
        let new_token_x: u64 = new_pool.assets[2].amount.parse().unwrap();
        assert_eq!(new_token_x, 10_000_000 + amount_x);
        let new_token_y: u64 = new_pool.assets[3].amount.parse().unwrap();
        assert_eq!(new_token_y, 5_000_000 + amount_y);

        let user_out = &build.unsigned_tx.outputs[1];
        assert!(
            user_out
                .assets
                .iter()
                .any(|a| a.token_id == "t2t_lp_token"),
            "User output should contain LP tokens"
        );
        let lp_received: u64 = user_out
            .assets
            .iter()
            .find(|a| a.token_id == "t2t_lp_token")
            .unwrap()
            .amount
            .parse()
            .unwrap();
        assert_eq!(lp_received, expected_lp_reward);

        assert_eq!(build.summary.erg_deposited, amount_x);
        assert_eq!(build.summary.token_deposited, amount_y);
        assert_eq!(build.summary.lp_reward, expected_lp_reward);
        assert_eq!(build.summary.miner_fee, TX_FEE);
        assert_eq!(build.summary.total_erg_cost, MIN_BOX_VALUE + TX_FEE);
    }

    #[test]
    fn test_lp_deposit_t2t_pool_erg_unchanged() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = test_user_utxo_with_both_tokens();

        let result = build_lp_deposit_eip12(
            &pool_box,
            &pool,
            100_000, // token_x amount
            50_000,  // token_y amount
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        )
        .unwrap();

        let new_pool = &result.unsigned_tx.outputs[0];
        assert_eq!(new_pool.value, "10000000", "T2T pool ERG must not change");
        assert_eq!(new_pool.ergo_tree, "t2t_pool_ergo_tree_hex");
        assert_eq!(
            new_pool.additional_registers.get("R4"),
            Some(&"04ca0f".to_string())
        );
        assert_eq!(
            new_pool.assets[0].token_id,
            "t2t_pool_nft_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(new_pool.assets[0].amount, "1");
    }
}
