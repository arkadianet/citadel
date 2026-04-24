//! Direct LP token redemption from Spectrum AMM pool boxes.
//!
//! Inputs:  [pool_box, user_utxos...]
//! Outputs: [new_pool_box, user_output, miner_fee]
//!
//! Pool contract validates: same ErgoTree, same R4, same token IDs,
//! proportional LP-to-reserve ratio preserved.
//! N2T pools: 3 tokens [NFT, LP, token_y], ERG is X reserve.
//! T2T pools: 4 tokens [NFT, LP, token_x, token_y], ERG unchanged.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::calculator;
use crate::constants::lp;
use crate::state::{AmmError, AmmPool, PoolType};
use ergo_tx::{
    collect_change_tokens, select_token_boxes, Eip12Asset, Eip12InputBox, Eip12Output,
    Eip12UnsignedTx,
};

const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;
const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

/// Spectrum N2T pool V1 hard-codes a `OUTPUTS(0).value > 10_000_000` check in
/// the ErgoTree. Any state transition that leaves the pool with <= 0.01 ERG
/// triggers "Script reduced to false". Confirmed by decompiling a live pool
/// ErgoTree (see the `10000000: SLong` constant in the contract body).
const POOL_MIN_ERG_STRICT: u64 = 10_000_000;

#[derive(Debug)]
pub struct LpRedeemBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: LpRedeemSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpRedeemSummary {
    pub lp_redeemed: u64,
    pub erg_received: u64,
    pub token_received: u64,
    pub token_name: String,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

/// Pool box must be inputs[0], new pool box must be outputs[0].
pub fn build_lp_redeem_eip12(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    lp_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<LpRedeemBuildResult, AmmError> {
    match pool.pool_type {
        PoolType::N2T => build_n2t_lp_redeem(
            pool_box,
            pool,
            lp_amount,
            user_utxos,
            user_ergo_tree,
            current_height,
        ),
        PoolType::T2T => build_t2t_lp_redeem(
            pool_box,
            pool,
            lp_amount,
            user_utxos,
            user_ergo_tree,
            current_height,
        ),
    }
}

/// N2T: 3 tokens [NFT, LP, Token_Y]. ERG is the X reserve.
fn build_n2t_lp_redeem(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    lp_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<LpRedeemBuildResult, AmmError> {
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

    // Validates R4 is parseable (pool contract requires it preserved)
    let _fee_num = crate::constants::parse_fee_num_from_r4(&pool_box.additional_registers)?;

    let supply_lp = calculator::calculate_lp_supply(lp_locked, lp::TOTAL_EMISSION);
    let (erg_out, token_out) =
        calculator::calculate_redeem_shares(pool_erg, pool_token_y_amount, supply_lp, lp_amount);

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

    let new_pool_erg = pool_erg
        .checked_sub(erg_out)
        .ok_or_else(|| AmmError::TxBuildError("Pool ERG underflow".to_string()))?;

    if new_pool_erg <= POOL_MIN_ERG_STRICT {
        return Err(AmmError::TxBuildError(format!(
            "New pool box ERG ({}) must be strictly greater than {} nano (Spectrum V1 pool contract invariant)",
            new_pool_erg, POOL_MIN_ERG_STRICT
        )));
    }
    // Belt-and-suspenders: also reject anything the network per-byte rule would reject.
    if new_pool_erg < MIN_BOX_VALUE {
        return Err(AmmError::TxBuildError(
            "New pool box would have less than minimum ERG".to_string(),
        ));
    }

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

    // User only needs TX_FEE -- ERG output comes from pool
    let user_erg_needed = TX_FEE;
    let selected =
        select_token_boxes(user_utxos, &pool.lp_token_id, lp_amount, user_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = Some((pool.lp_token_id.as_str(), lp_amount));
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    let user_erg = erg_out + change_erg;
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

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    // Pool box MUST be inputs[0] (contract requirement)
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

/// T2T: 4 tokens [NFT, LP, Token_X, Token_Y]. ERG unchanged (storage rent only).
fn build_t2t_lp_redeem(
    pool_box: &Eip12InputBox,
    pool: &AmmPool,
    lp_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<LpRedeemBuildResult, AmmError> {
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

    let supply_lp = calculator::calculate_lp_supply(lp_locked, lp::TOTAL_EMISSION);
    let (x_out, y_out) = calculator::calculate_redeem_shares(
        pool_token_x_amount,
        pool_token_y_amount,
        supply_lp,
        lp_amount,
    );

    if x_out == 0 {
        return Err(AmmError::TxBuildError(
            "LP redeem too small: Token X share would be 0".to_string(),
        ));
    }
    if y_out == 0 {
        return Err(AmmError::TxBuildError(
            "LP redeem too small: Token Y share would be 0".to_string(),
        ));
    }

    let new_lp_locked = lp_locked
        .checked_add(lp_amount)
        .ok_or_else(|| AmmError::TxBuildError("Pool LP overflow".to_string()))?;

    let new_pool_token_x = pool_token_x_amount
        .checked_sub(x_out)
        .ok_or_else(|| AmmError::TxBuildError("Pool token X underflow".to_string()))?;

    let new_pool_token_y = pool_token_y_amount
        .checked_sub(y_out)
        .ok_or_else(|| AmmError::TxBuildError("Pool token Y underflow".to_string()))?;

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

    // No ERG comes from pool in T2T -- user pays MIN_BOX_VALUE + TX_FEE
    let user_erg_needed = MIN_BOX_VALUE
        .checked_add(TX_FEE)
        .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?;

    let selected =
        select_token_boxes(user_utxos, &pool.lp_token_id, lp_amount, user_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = Some((pool.lp_token_id.as_str(), lp_amount));
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    let user_erg = MIN_BOX_VALUE + change_erg;
    let mut user_assets = vec![
        Eip12Asset {
            token_id: token_x.token_id.clone(),
            amount: x_out.to_string(),
        },
        Eip12Asset {
            token_id: pool.token_y.token_id.clone(),
            amount: y_out.to_string(),
        },
    ];
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

    // erg_received holds x_out for T2T (frontend interprets based on pool type)
    let token_name = pool
        .token_y
        .name
        .clone()
        .unwrap_or_else(|| pool.token_y.token_id[..8].to_string());

    let summary = LpRedeemSummary {
        lp_redeemed: lp_amount,
        erg_received: x_out,
        token_received: y_out,
        token_name,
        miner_fee: TX_FEE,
        total_erg_cost: user_erg_needed,
    };

    Ok(LpRedeemBuildResult {
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
            erg_reserves: Some(100_000_000_000),
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
            value: "100000000000".to_string(),
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
                    amount: "9223372036854774807".to_string(),
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
            value: "5000000000".to_string(),
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![Eip12Asset {
                token_id: "lp_token".to_string(),
                amount: "500".to_string(),
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

        let lp_amount = 100u64;
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

        assert_eq!(build.unsigned_tx.inputs[0].box_id, "pool_box_1");
        assert_eq!(build.unsigned_tx.inputs[1].box_id, "user_utxo_1");

        let new_pool = &build.unsigned_tx.outputs[0];
        let new_pool_erg: u64 = new_pool.value.parse().unwrap();
        assert_eq!(new_pool_erg, 100_000_000_000 - expected_erg_out);
        let new_token_y: u64 = new_pool.assets[2].amount.parse().unwrap();
        assert_eq!(new_token_y, 1_000_000 - expected_token_out);
        let new_lp_locked: u64 = new_pool.assets[1].amount.parse().unwrap();
        assert_eq!(new_lp_locked, lp_locked + lp_amount);

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
        assert_eq!(
            user_out.assets[0].token_id,
            "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let token_received: u64 = user_out.assets[0].amount.parse().unwrap();
        assert_eq!(token_received, 100_000);

        let user_erg: u64 = user_out.value.parse().unwrap();
        let expected_erg = 10_000_000_000 + (5_000_000_000 - TX_FEE);
        assert_eq!(user_erg, expected_erg);

        let lp_change = user_out
            .assets
            .iter()
            .find(|a| a.token_id == "lp_token");
        assert!(lp_change.is_some(), "Should have LP change tokens");
        let remaining_lp: u64 = lp_change.unwrap().amount.parse().unwrap();
        assert_eq!(remaining_lp, 400);
    }

    #[test]
    fn test_lp_redeem_insufficient_lp() {
        let pool = test_n2t_pool();
        let pool_box = test_pool_box();
        let user_utxo = Eip12InputBox {
            assets: vec![Eip12Asset {
                token_id: "lp_token".to_string(),
                amount: "10".to_string(),
            }],
            ..test_user_utxo_with_lp()
        };

        let result = build_lp_redeem_eip12(
            &pool_box,
            &pool,
            500,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient"));
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
            value: "10000000".to_string(),
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

    fn test_user_utxo_with_t2t_lp() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "user_utxo_t2t_1".to_string(),
            transaction_id: "user_tx_t2t_1".to_string(),
            index: 0,
            value: "5000000000".to_string(),
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![Eip12Asset {
                token_id: "t2t_lp_token".to_string(),
                amount: "500".to_string(),
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_lp_redeem_t2t_basic() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = test_user_utxo_with_t2t_lp();

        let lp_amount = 100u64;

        let lp_locked: u64 = 9_223_372_036_854_774_807;
        let supply_lp =
            calculator::calculate_lp_supply(lp_locked, lp::TOTAL_EMISSION);
        assert_eq!(supply_lp, 1000);

        let (expected_x_out, expected_y_out) = calculator::calculate_redeem_shares(
            10_000_000,
            5_000_000,
            supply_lp,
            lp_amount,
        );
        assert_eq!(expected_x_out, 1_000_000);
        assert_eq!(expected_y_out, 500_000);

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

        assert_eq!(build.unsigned_tx.inputs[0].box_id, "t2t_pool_box_1");

        let new_pool = &build.unsigned_tx.outputs[0];
        let new_pool_erg: u64 = new_pool.value.parse().unwrap();
        assert_eq!(new_pool_erg, 10_000_000, "T2T pool ERG must stay unchanged");
        assert_eq!(new_pool.assets.len(), 4);
        assert_eq!(new_pool.assets[0].amount, "1");
        let new_lp_locked: u64 = new_pool.assets[1].amount.parse().unwrap();
        assert_eq!(new_lp_locked, lp_locked + lp_amount);
        let new_token_x: u64 = new_pool.assets[2].amount.parse().unwrap();
        assert_eq!(new_token_x, 10_000_000 - expected_x_out);
        let new_token_y: u64 = new_pool.assets[3].amount.parse().unwrap();
        assert_eq!(new_token_y, 5_000_000 - expected_y_out);

        let user_out = &build.unsigned_tx.outputs[1];
        assert_eq!(
            user_out.assets[0].token_id,
            "token_x_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let x_received: u64 = user_out.assets[0].amount.parse().unwrap();
        assert_eq!(x_received, expected_x_out);
        assert_eq!(
            user_out.assets[1].token_id,
            "token_y_id_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let y_received: u64 = user_out.assets[1].amount.parse().unwrap();
        assert_eq!(y_received, expected_y_out);

        let lp_change = user_out
            .assets
            .iter()
            .find(|a| a.token_id == "t2t_lp_token");
        assert!(lp_change.is_some(), "Should have LP change tokens");
        let remaining_lp: u64 = lp_change.unwrap().amount.parse().unwrap();
        assert_eq!(remaining_lp, 400);

        assert_eq!(build.summary.lp_redeemed, lp_amount);
        assert_eq!(build.summary.erg_received, expected_x_out);
        assert_eq!(build.summary.token_received, expected_y_out);
        assert_eq!(build.summary.miner_fee, TX_FEE);
        assert_eq!(build.summary.total_erg_cost, MIN_BOX_VALUE + TX_FEE);
    }

    #[test]
    fn test_lp_redeem_t2t_pool_erg_unchanged() {
        let pool = test_t2t_pool();
        let pool_box = test_t2t_pool_box();
        let user_utxo = test_user_utxo_with_t2t_lp();

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
