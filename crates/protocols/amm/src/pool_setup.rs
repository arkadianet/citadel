//! Two-tx pool creation: TX0 mints LP tokens into a bootstrap box,
//! TX1 spends it to create the on-chain pool box with NFT and R4=fee_num.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::calculator::calculate_initial_lp_share;
use crate::constants::fees::DEFAULT_FEE_DENOM;
use crate::constants::lp::{BURN_LP, TOTAL_EMISSION};
use crate::constants::pool_templates::{N2T_POOL_TEMPLATE, T2T_POOL_TEMPLATE};
use crate::state::{AmmError, PoolType};
use ergo_tx::sigma::{encode_sigma_coll_byte, encode_sigma_int};
use ergo_tx::{
    collect_multi_change_tokens, select_multi_token_boxes, select_token_boxes, Eip12Asset,
    Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;
const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

#[derive(Debug, Clone)]
pub struct PoolSetupParams {
    pub pool_type: PoolType,
    /// None for N2T pools where X is ERG
    pub x_token_id: Option<String>,
    /// ERG in nanoERG for N2T, token amount for T2T
    pub x_amount: u64,
    pub y_token_id: String,
    pub y_amount: u64,
    /// e.g. 997 for 0.3% fee
    pub fee_num: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolBootstrapResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: PoolBootstrapSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolBootstrapSummary {
    pub lp_token_id: String,
    pub lp_minted: u64,
    pub user_lp_share: u64,
    pub pool_type: String,
    pub x_amount: u64,
    pub y_amount: u64,
    pub fee_percent: f64,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolCreateResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: PoolCreateSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolCreateSummary {
    pub pool_nft_id: String,
    pub lp_token_id: String,
    pub pool_type: String,
    pub fee_num: i32,
}

/// LP token ID = first input's box_id per Ergo minting rule.
pub fn build_pool_bootstrap_eip12(
    params: &PoolSetupParams,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<PoolBootstrapResult, AmmError> {
    if params.x_amount == 0 || params.y_amount == 0 {
        return Err(AmmError::TxBuildError(
            "Token amounts must be greater than 0".to_string(),
        ));
    }
    if params.fee_num <= 0 || params.fee_num >= DEFAULT_FEE_DENOM {
        return Err(AmmError::TxBuildError(format!(
            "fee_num must be in (0, {}), got {}",
            DEFAULT_FEE_DENOM, params.fee_num
        )));
    }

    let lp_minted = (TOTAL_EMISSION - BURN_LP) as u64;
    let user_lp_share = calculate_initial_lp_share(params.x_amount, params.y_amount);
    if user_lp_share == 0 {
        return Err(AmmError::TxBuildError(
            "Initial LP share would be 0".to_string(),
        ));
    }

    let (bootstrap_box_erg, selected) = match params.pool_type {
        PoolType::N2T => {
            let bootstrap_erg = params.x_amount;
            let user_erg_needed = bootstrap_erg
                .checked_add(MIN_BOX_VALUE)
                .and_then(|v| v.checked_add(TX_FEE))
                .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?;

            let sel = select_token_boxes(
                user_utxos,
                &params.y_token_id,
                params.y_amount,
                user_erg_needed,
            )
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

            (bootstrap_erg, sel)
        }
        PoolType::T2T => {
            let bootstrap_erg = MIN_BOX_VALUE;
            let x_token_id = params.x_token_id.as_deref().ok_or_else(|| {
                AmmError::TxBuildError("T2T pool requires x_token_id".to_string())
            })?;

            let user_erg_needed = bootstrap_erg
                .checked_add(MIN_BOX_VALUE)
                .and_then(|v| v.checked_add(TX_FEE))
                .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?;

            let required_tokens = vec![
                (x_token_id, params.x_amount),
                (params.y_token_id.as_str(), params.y_amount),
            ];
            let sel =
                select_multi_token_boxes(user_utxos, &required_tokens, user_erg_needed)
                    .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

            (bootstrap_erg, sel)
        }
    };

    // LP token ID = first input's box_id (Ergo minting rule)
    let lp_token_id = selected.boxes[0].box_id.clone();

    let mut bootstrap_assets = vec![Eip12Asset {
        token_id: lp_token_id.clone(),
        amount: lp_minted.to_string(),
    }];

    if let PoolType::T2T = params.pool_type {
        let x_token_id = params.x_token_id.as_ref().unwrap();
        bootstrap_assets.push(Eip12Asset {
            token_id: x_token_id.clone(),
            amount: params.x_amount.to_string(),
        });
    }

    bootstrap_assets.push(Eip12Asset {
        token_id: params.y_token_id.clone(),
        amount: params.y_amount.to_string(),
    });

    let lp_name = b"LP";
    let r4_hex = encode_sigma_coll_byte(lp_name);

    let bootstrap_registers = ergo_tx::sigma_registers!("R4" => r4_hex);

    let bootstrap_output = Eip12Output {
        value: bootstrap_box_erg.to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: bootstrap_assets,
        creation_height: current_height,
        additional_registers: bootstrap_registers,
    };

    let total_erg_needed = match params.pool_type {
        PoolType::N2T => params
            .x_amount
            .checked_add(MIN_BOX_VALUE)
            .and_then(|v| v.checked_add(TX_FEE))
            .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?,
        PoolType::T2T => MIN_BOX_VALUE
            .checked_add(MIN_BOX_VALUE)
            .and_then(|v| v.checked_add(TX_FEE))
            .ok_or_else(|| AmmError::TxBuildError("ERG cost overflow".to_string()))?,
    };
    let change_erg = selected.total_erg - total_erg_needed;

    let mut spent_tokens: Vec<(&str, u64)> = vec![(params.y_token_id.as_str(), params.y_amount)];
    if let Some(ref x_id) = params.x_token_id {
        spent_tokens.push((x_id.as_str(), params.x_amount));
    }
    let change_tokens = collect_multi_change_tokens(&selected.boxes, &spent_tokens);

    let change_output = Eip12Output {
        value: (MIN_BOX_VALUE + change_erg).to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: change_tokens,
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    let inputs = selected.boxes;
    let outputs = vec![bootstrap_output, change_output, fee_output];

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    let fee_percent =
        (1.0 - params.fee_num as f64 / DEFAULT_FEE_DENOM as f64) * 100.0;

    let summary = PoolBootstrapSummary {
        lp_token_id,
        lp_minted,
        user_lp_share,
        pool_type: format!("{:?}", params.pool_type),
        x_amount: params.x_amount,
        y_amount: params.y_amount,
        fee_percent,
        miner_fee: TX_FEE,
        total_erg_cost: total_erg_needed,
    };

    Ok(PoolBootstrapResult {
        unsigned_tx,
        summary,
    })
}

/// Pool NFT ID = bootstrap box's box_id per Ergo minting rule.
pub fn build_pool_create_eip12(
    bootstrap_box: &Eip12InputBox,
    params: &PoolSetupParams,
    lp_token_id: &str,
    user_lp_share: u64,
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<PoolCreateResult, AmmError> {
    let pool_nft_id = bootstrap_box.box_id.clone();

    let lp_minted = (TOTAL_EMISSION - BURN_LP) as u64;
    let pool_lp_locked = lp_minted
        .checked_sub(user_lp_share)
        .ok_or_else(|| AmmError::TxBuildError("LP share exceeds minted amount".to_string()))?;

    let bootstrap_erg: u64 = bootstrap_box
        .value
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid bootstrap box ERG value".to_string()))?;

    let (pool_box_erg, user_output_erg) = match params.pool_type {
        PoolType::N2T => {
            let pool_erg = bootstrap_erg
                .checked_sub(MIN_BOX_VALUE)
                .and_then(|v| v.checked_sub(TX_FEE))
                .ok_or_else(|| {
                    AmmError::TxBuildError("Insufficient ERG in bootstrap box".to_string())
                })?;
            (pool_erg, MIN_BOX_VALUE)
        }
        PoolType::T2T => {
            let user_erg = bootstrap_erg
                .checked_sub(MIN_BOX_VALUE)
                .and_then(|v| v.checked_sub(TX_FEE))
                .ok_or_else(|| {
                    AmmError::TxBuildError("Insufficient ERG in bootstrap box".to_string())
                })?;
            (MIN_BOX_VALUE, user_erg)
        }
    };

    let mut pool_assets = vec![
        Eip12Asset {
            token_id: pool_nft_id.clone(),
            amount: "1".to_string(),
        },
        Eip12Asset {
            token_id: lp_token_id.to_string(),
            amount: pool_lp_locked.to_string(),
        },
    ];

    if let PoolType::T2T = params.pool_type {
        let x_token_id = params.x_token_id.as_ref().ok_or_else(|| {
            AmmError::TxBuildError("T2T pool requires x_token_id".to_string())
        })?;
        pool_assets.push(Eip12Asset {
            token_id: x_token_id.clone(),
            amount: params.x_amount.to_string(),
        });
    }

    pool_assets.push(Eip12Asset {
        token_id: params.y_token_id.clone(),
        amount: params.y_amount.to_string(),
    });

    let pool_ergo_tree = match params.pool_type {
        PoolType::N2T => N2T_POOL_TEMPLATE,
        PoolType::T2T => T2T_POOL_TEMPLATE,
    };

    // Pool contract validates R4 == fee_num
    let r4_hex = encode_sigma_int(params.fee_num);
    let pool_registers = ergo_tx::sigma_registers!("R4" => r4_hex);

    let pool_output = Eip12Output {
        value: pool_box_erg.to_string(),
        ergo_tree: pool_ergo_tree.to_string(),
        assets: pool_assets,
        creation_height: current_height,
        additional_registers: pool_registers,
    };

    let user_output = Eip12Output {
        value: user_output_erg.to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: vec![Eip12Asset {
            token_id: lp_token_id.to_string(),
            amount: user_lp_share.to_string(),
        }],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    let unsigned_tx = Eip12UnsignedTx {
        inputs: vec![bootstrap_box.clone()],
        data_inputs: vec![],
        outputs: vec![pool_output, user_output, fee_output],
    };

    let summary = PoolCreateSummary {
        pool_nft_id,
        lp_token_id: lp_token_id.to_string(),
        pool_type: format!("{:?}", params.pool_type),
        fee_num: params.fee_num,
    };

    Ok(PoolCreateResult {
        unsigned_tx,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_ERGO_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    fn make_utxo(box_id: &str, erg: u64, tokens: Vec<(&str, u64)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: box_id.to_string(),
            transaction_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            index: 0,
            value: erg.to_string(),
            ergo_tree: USER_ERGO_TREE.to_string(),
            assets: tokens
                .into_iter()
                .map(|(id, amt)| Eip12Asset {
                    token_id: id.to_string(),
                    amount: amt.to_string(),
                })
                .collect(),
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    const BOX_ID_1: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BOX_ID_2: &str =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const TOKEN_Y_ID: &str =
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const TOKEN_X_ID: &str =
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    fn n2t_params() -> PoolSetupParams {
        PoolSetupParams {
            pool_type: PoolType::N2T,
            x_token_id: None,
            x_amount: 10_000_000_000, // 10 ERG
            y_token_id: TOKEN_Y_ID.to_string(),
            y_amount: 1_000_000,
            fee_num: 997,
        }
    }

    fn t2t_params() -> PoolSetupParams {
        PoolSetupParams {
            pool_type: PoolType::T2T,
            x_token_id: Some(TOKEN_X_ID.to_string()),
            x_amount: 500_000,
            y_token_id: TOKEN_Y_ID.to_string(),
            y_amount: 1_000_000,
            fee_num: 997,
        }
    }

    #[test]
    fn test_n2t_bootstrap_basic() {
        let utxos = vec![make_utxo(
            BOX_ID_1,
            50_000_000_000, // 50 ERG
            vec![(TOKEN_Y_ID, 5_000_000)],
        )];

        let params = n2t_params();
        let result =
            build_pool_bootstrap_eip12(&params, &utxos, USER_ERGO_TREE, 1_000_000);

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.summary.lp_token_id, BOX_ID_1);
        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        let bootstrap = &build.unsigned_tx.outputs[0];
        assert_eq!(bootstrap.assets.len(), 2);
        assert_eq!(bootstrap.assets[0].token_id, BOX_ID_1); // LP token
        assert_eq!(bootstrap.assets[1].token_id, TOKEN_Y_ID);

        let lp_minted: u64 = bootstrap.assets[0].amount.parse().unwrap();
        assert_eq!(lp_minted, (TOTAL_EMISSION - BURN_LP) as u64);

        let bootstrap_erg: u64 = bootstrap.value.parse().unwrap();
        assert_eq!(bootstrap_erg, params.x_amount);

        assert!(bootstrap.additional_registers.contains_key("R4"));
        assert!(build.summary.user_lp_share > 0);
    }

    #[test]
    fn test_t2t_bootstrap_basic() {
        let utxos = vec![make_utxo(
            BOX_ID_1,
            50_000_000_000,
            vec![(TOKEN_X_ID, 1_000_000), (TOKEN_Y_ID, 5_000_000)],
        )];

        let params = t2t_params();
        let result =
            build_pool_bootstrap_eip12(&params, &utxos, USER_ERGO_TREE, 1_000_000);

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        let bootstrap = &build.unsigned_tx.outputs[0];
        assert_eq!(bootstrap.assets.len(), 3);
        assert_eq!(bootstrap.assets[0].token_id, BOX_ID_1); // LP
        assert_eq!(bootstrap.assets[1].token_id, TOKEN_X_ID);
        assert_eq!(bootstrap.assets[2].token_id, TOKEN_Y_ID);

        let bootstrap_erg: u64 = bootstrap.value.parse().unwrap();
        assert_eq!(bootstrap_erg, MIN_BOX_VALUE);

        let expected_fee_pct = (1.0 - 997.0 / 1000.0) * 100.0;
        assert!(
            (build.summary.fee_percent - expected_fee_pct).abs() < 0.001,
            "Fee percent should be ~0.3%, got {}",
            build.summary.fee_percent
        );
    }

    #[test]
    fn test_pool_create_n2t() {
        let params = n2t_params();
        let lp_minted = (TOTAL_EMISSION - BURN_LP) as u64;
        let user_lp_share = calculate_initial_lp_share(params.x_amount, params.y_amount);
        let lp_token_id = BOX_ID_1;

        let bootstrap_box = Eip12InputBox {
            box_id: BOX_ID_2.to_string(),
            transaction_id: BOX_ID_1.to_string(),
            index: 0,
            value: params.x_amount.to_string(),
            ergo_tree: USER_ERGO_TREE.to_string(),
            assets: vec![
                Eip12Asset {
                    token_id: lp_token_id.to_string(),
                    amount: lp_minted.to_string(),
                },
                Eip12Asset {
                    token_id: TOKEN_Y_ID.to_string(),
                    amount: params.y_amount.to_string(),
                },
            ],
            creation_height: 1_000_000,
            additional_registers: ergo_tx::sigma_registers!("R4" => encode_sigma_coll_byte(b"LP")),
            extension: HashMap::new(),
        };

        let result = build_pool_create_eip12(
            &bootstrap_box,
            &params,
            lp_token_id,
            user_lp_share,
            USER_ERGO_TREE,
            1_000_001,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.summary.pool_nft_id, BOX_ID_2);
        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        let pool_box = &build.unsigned_tx.outputs[0];
        assert_eq!(pool_box.assets.len(), 3);
        assert_eq!(pool_box.assets[0].token_id, BOX_ID_2);
        assert_eq!(pool_box.assets[0].amount, "1");
        assert_eq!(pool_box.assets[1].token_id, lp_token_id);
        assert_eq!(pool_box.assets[2].token_id, TOKEN_Y_ID);

        let lp_locked: u64 = pool_box.assets[1].amount.parse().unwrap();
        assert_eq!(lp_locked, lp_minted - user_lp_share);

        assert_eq!(pool_box.ergo_tree, N2T_POOL_TEMPLATE);
        assert!(pool_box.additional_registers.contains_key("R4"));

        let user_out = &build.unsigned_tx.outputs[1];
        assert_eq!(user_out.assets.len(), 1);
        assert_eq!(user_out.assets[0].token_id, lp_token_id);
        let user_lp: u64 = user_out.assets[0].amount.parse().unwrap();
        assert_eq!(user_lp, user_lp_share);

        let pool_erg: u64 = pool_box.value.parse().unwrap();
        assert_eq!(pool_erg, params.x_amount - MIN_BOX_VALUE - TX_FEE);
    }

    #[test]
    fn test_zero_amount_rejected() {
        let utxos = vec![make_utxo(BOX_ID_1, 50_000_000_000, vec![(TOKEN_Y_ID, 5_000_000)])];

        let mut params = n2t_params();
        params.x_amount = 0;

        let result =
            build_pool_bootstrap_eip12(&params, &utxos, USER_ERGO_TREE, 1_000_000);

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("greater than 0"),
            "Should reject zero amounts"
        );

        let mut params2 = n2t_params();
        params2.y_amount = 0;

        let result2 =
            build_pool_bootstrap_eip12(&params2, &utxos, USER_ERGO_TREE, 1_000_000);

        assert!(result2.is_err());
        assert!(
            result2.unwrap_err().to_string().contains("greater than 0"),
            "Should reject zero y_amount"
        );
    }

    #[test]
    fn test_invalid_fee_rejected() {
        let utxos = vec![make_utxo(BOX_ID_1, 50_000_000_000, vec![(TOKEN_Y_ID, 5_000_000)])];

        let mut params = n2t_params();
        params.fee_num = 1000;

        let result =
            build_pool_bootstrap_eip12(&params, &utxos, USER_ERGO_TREE, 1_000_000);

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("fee_num"),
            "Should reject fee_num >= 1000"
        );

        let mut params2 = n2t_params();
        params2.fee_num = 0;

        let result2 =
            build_pool_bootstrap_eip12(&params2, &utxos, USER_ERGO_TREE, 1_000_000);

        assert!(result2.is_err());
        assert!(
            result2.unwrap_err().to_string().contains("fee_num"),
            "Should reject fee_num == 0"
        );
    }

    #[test]
    fn test_pool_create_t2t() {
        let params = t2t_params();
        let lp_minted = (TOTAL_EMISSION - BURN_LP) as u64;
        let user_lp_share = calculate_initial_lp_share(params.x_amount, params.y_amount);
        let lp_token_id = BOX_ID_1;

        let bootstrap_erg = 3_000_000u64;
        let bootstrap_box = Eip12InputBox {
            box_id: BOX_ID_2.to_string(),
            transaction_id: BOX_ID_1.to_string(),
            index: 0,
            value: bootstrap_erg.to_string(),
            ergo_tree: USER_ERGO_TREE.to_string(),
            assets: vec![
                Eip12Asset {
                    token_id: lp_token_id.to_string(),
                    amount: lp_minted.to_string(),
                },
                Eip12Asset {
                    token_id: TOKEN_X_ID.to_string(),
                    amount: params.x_amount.to_string(),
                },
                Eip12Asset {
                    token_id: TOKEN_Y_ID.to_string(),
                    amount: params.y_amount.to_string(),
                },
            ],
            creation_height: 1_000_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        };

        let result = build_pool_create_eip12(
            &bootstrap_box,
            &params,
            lp_token_id,
            user_lp_share,
            USER_ERGO_TREE,
            1_000_001,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        let pool_box = &build.unsigned_tx.outputs[0];
        assert_eq!(pool_box.assets.len(), 4);
        assert_eq!(pool_box.assets[0].token_id, BOX_ID_2);
        assert_eq!(pool_box.assets[0].amount, "1");
        assert_eq!(pool_box.ergo_tree, T2T_POOL_TEMPLATE);

        let pool_erg: u64 = pool_box.value.parse().unwrap();
        assert_eq!(pool_erg, MIN_BOX_VALUE);

        let user_out = &build.unsigned_tx.outputs[1];
        let user_erg: u64 = user_out.value.parse().unwrap();
        assert_eq!(user_erg, bootstrap_erg - MIN_BOX_VALUE - TX_FEE);
    }
}
