//! Pool Setup Transaction Builders
//!
//! Builds the two-transaction chain needed to create a new AMM pool:
//!
//! **TX0 (Bootstrap):** Mints LP tokens. The new token ID equals the first
//! input's box_id (Ergo minting rule). Produces a "bootstrap box" at the
//! user's address containing LP tokens, Token Y (and Token X for T2T), plus
//! an R4 register with the LP token name.
//!
//! **TX1 (Pool Create):** Spends the bootstrap box. Mints a pool NFT (amount=1)
//! from the bootstrap box's box_id. Creates the pool box under the appropriate
//! pool contract ErgoTree, with R4 = fee_num.
//!
//! # Transaction Structures
//!
//! ## TX0 (Bootstrap)
//! Inputs:  [user_utxos...]
//! Outputs: [bootstrap_box, change_box, miner_fee]
//!
//! ## TX1 (Pool Create)
//! Inputs:  [bootstrap_box]
//! Outputs: [pool_box, user_lp_output, miner_fee]

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

/// Transaction fee in nanoERG (0.0011 ERG - standard)
const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;

/// Minimum box value in nanoERG (required for any output box)
const MIN_BOX_VALUE: u64 = citadel_core::constants::MIN_BOX_VALUE_NANO as u64;

// =============================================================================
// Types
// =============================================================================

/// Parameters for creating a new AMM pool
#[derive(Debug, Clone)]
pub struct PoolSetupParams {
    pub pool_type: PoolType,
    /// Token X ID. None for N2T pools where X is ERG.
    pub x_token_id: Option<String>,
    /// Amount of Token X (or ERG in nanoERG for N2T)
    pub x_amount: u64,
    /// Token Y ID
    pub y_token_id: String,
    /// Amount of Token Y
    pub y_amount: u64,
    /// Fee numerator (e.g. 997 for 0.3% fee, 980 for 2% fee)
    pub fee_num: i32,
}

/// Result of building the bootstrap transaction (TX0)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolBootstrapResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: PoolBootstrapSummary,
}

/// Summary of the bootstrap transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolBootstrapSummary {
    /// LP token ID (= first input box_id)
    pub lp_token_id: String,
    /// Total LP tokens minted (TOTAL_EMISSION - BURN_LP)
    pub lp_minted: u64,
    /// User's LP share (sqrt(x * y))
    pub user_lp_share: u64,
    /// Pool type as string
    pub pool_type: String,
    /// X amount deposited
    pub x_amount: u64,
    /// Y amount deposited
    pub y_amount: u64,
    /// Fee as percentage
    pub fee_percent: f64,
    /// Miner fee
    pub miner_fee: u64,
    /// Total ERG cost to user
    pub total_erg_cost: u64,
}

/// Result of building the pool create transaction (TX1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolCreateResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: PoolCreateSummary,
}

/// Summary of the pool create transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolCreateSummary {
    /// Pool NFT ID (= bootstrap box's box_id)
    pub pool_nft_id: String,
    /// LP token ID
    pub lp_token_id: String,
    /// Pool type as string
    pub pool_type: String,
    /// Fee numerator
    pub fee_num: i32,
}

// =============================================================================
// TX0: Bootstrap
// =============================================================================

/// Build the bootstrap transaction (TX0) that mints LP tokens.
///
/// The LP token ID will equal `selected_inputs[0].box_id` per Ergo's minting
/// rule. The bootstrap box is placed at the user's address and contains the LP
/// tokens plus the deposited tokens.
///
/// # Arguments
///
/// * `params` - Pool creation parameters
/// * `user_utxos` - User's available UTXOs for funding
/// * `user_ergo_tree` - User's ErgoTree hex string
/// * `current_height` - Current blockchain height
pub fn build_pool_bootstrap_eip12(
    params: &PoolSetupParams,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<PoolBootstrapResult, AmmError> {
    // 1. Validate parameters
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

    // 2. Compute LP amounts
    let lp_minted = (TOTAL_EMISSION - BURN_LP) as u64;
    let user_lp_share = calculate_initial_lp_share(params.x_amount, params.y_amount);
    if user_lp_share == 0 {
        return Err(AmmError::TxBuildError(
            "Initial LP share would be 0".to_string(),
        ));
    }

    // 3. Determine bootstrap box ERG value and select UTXOs
    let (bootstrap_box_erg, selected) = match params.pool_type {
        PoolType::N2T => {
            // N2T: x_amount is ERG, bootstrap box holds the ERG deposit
            let bootstrap_erg = params.x_amount;
            // User needs: bootstrap_erg + change output + tx fee
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
            // T2T: both tokens are non-ERG, bootstrap box holds MIN_BOX_VALUE
            let bootstrap_erg = MIN_BOX_VALUE;
            let x_token_id = params.x_token_id.as_deref().ok_or_else(|| {
                AmmError::TxBuildError("T2T pool requires x_token_id".to_string())
            })?;

            // User needs: bootstrap_erg + change output + tx fee
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

    // 4. LP token ID = first selected input's box_id
    let lp_token_id = selected.boxes[0].box_id.clone();

    // 5. Build bootstrap box assets: LP tokens + deposited tokens
    let mut bootstrap_assets = vec![Eip12Asset {
        token_id: lp_token_id.clone(),
        amount: lp_minted.to_string(),
    }];

    // For T2T, include Token X
    if let PoolType::T2T = params.pool_type {
        let x_token_id = params.x_token_id.as_ref().unwrap();
        bootstrap_assets.push(Eip12Asset {
            token_id: x_token_id.clone(),
            amount: params.x_amount.to_string(),
        });
    }

    // Always include Token Y
    bootstrap_assets.push(Eip12Asset {
        token_id: params.y_token_id.clone(),
        amount: params.y_amount.to_string(),
    });

    // 6. Build bootstrap box with R4 = LP token name
    let lp_name = b"LP";
    let r4_hex = encode_sigma_coll_byte(lp_name);

    let mut bootstrap_registers = HashMap::new();
    bootstrap_registers.insert("R4".to_string(), r4_hex);

    let bootstrap_output = Eip12Output {
        value: bootstrap_box_erg.to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: bootstrap_assets,
        creation_height: current_height,
        additional_registers: bootstrap_registers,
    };

    // 7. Build change output
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

    // Collect change tokens (subtract what we're putting in the bootstrap box)
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

    // 8. Fee output
    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    // 9. Assemble transaction
    let inputs = selected.boxes;
    let outputs = vec![bootstrap_output, change_output, fee_output];

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    // 10. Build summary
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

// =============================================================================
// TX1: Pool Create
// =============================================================================

/// Build the pool create transaction (TX1) that creates the on-chain pool box.
///
/// The pool NFT ID equals `bootstrap_box.box_id` per Ergo's minting rule.
/// The pool box is placed under the appropriate pool contract ErgoTree with
/// R4 set to the fee numerator.
///
/// # Arguments
///
/// * `bootstrap_box` - The bootstrap box (TX0 output[0]), must be confirmed
/// * `params` - Pool creation parameters (same as TX0)
/// * `lp_token_id` - LP token ID from TX0
/// * `user_lp_share` - User's LP share from TX0
/// * `user_ergo_tree` - User's ErgoTree hex string
/// * `current_height` - Current blockchain height
pub fn build_pool_create_eip12(
    bootstrap_box: &Eip12InputBox,
    params: &PoolSetupParams,
    lp_token_id: &str,
    user_lp_share: u64,
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<PoolCreateResult, AmmError> {
    // 1. Pool NFT ID = bootstrap box's box_id (Ergo minting rule)
    let pool_nft_id = bootstrap_box.box_id.clone();

    // 2. Compute LP locked in pool = total minted - user share
    let lp_minted = (TOTAL_EMISSION - BURN_LP) as u64;
    let pool_lp_locked = lp_minted
        .checked_sub(user_lp_share)
        .ok_or_else(|| AmmError::TxBuildError("LP share exceeds minted amount".to_string()))?;

    // 3. Parse bootstrap box ERG
    let bootstrap_erg: u64 = bootstrap_box
        .value
        .parse()
        .map_err(|_| AmmError::TxBuildError("Invalid bootstrap box ERG value".to_string()))?;

    // 4. Determine pool box ERG and user output ERG
    let (pool_box_erg, user_output_erg) = match params.pool_type {
        PoolType::N2T => {
            // Pool gets bootstrap ERG minus user output and fee
            let pool_erg = bootstrap_erg
                .checked_sub(MIN_BOX_VALUE)
                .and_then(|v| v.checked_sub(TX_FEE))
                .ok_or_else(|| {
                    AmmError::TxBuildError("Insufficient ERG in bootstrap box".to_string())
                })?;
            (pool_erg, MIN_BOX_VALUE)
        }
        PoolType::T2T => {
            // Pool gets MIN_BOX_VALUE, user gets leftover
            let user_erg = bootstrap_erg
                .checked_sub(MIN_BOX_VALUE)
                .and_then(|v| v.checked_sub(TX_FEE))
                .ok_or_else(|| {
                    AmmError::TxBuildError("Insufficient ERG in bootstrap box".to_string())
                })?;
            (MIN_BOX_VALUE, user_erg)
        }
    };

    // 5. Build pool box assets
    let mut pool_assets = vec![
        // NFT (amount=1)
        Eip12Asset {
            token_id: pool_nft_id.clone(),
            amount: "1".to_string(),
        },
        // LP tokens (locked)
        Eip12Asset {
            token_id: lp_token_id.to_string(),
            amount: pool_lp_locked.to_string(),
        },
    ];

    // For T2T, include Token X
    if let PoolType::T2T = params.pool_type {
        let x_token_id = params.x_token_id.as_ref().ok_or_else(|| {
            AmmError::TxBuildError("T2T pool requires x_token_id".to_string())
        })?;
        pool_assets.push(Eip12Asset {
            token_id: x_token_id.clone(),
            amount: params.x_amount.to_string(),
        });
    }

    // Token Y
    pool_assets.push(Eip12Asset {
        token_id: params.y_token_id.clone(),
        amount: params.y_amount.to_string(),
    });

    // 6. Pool ErgoTree based on pool type
    let pool_ergo_tree = match params.pool_type {
        PoolType::N2T => N2T_POOL_TEMPLATE,
        PoolType::T2T => T2T_POOL_TEMPLATE,
    };

    // 7. Pool R4 = fee_num as sigma Int
    let r4_hex = encode_sigma_int(params.fee_num);
    let mut pool_registers = HashMap::new();
    pool_registers.insert("R4".to_string(), r4_hex);

    let pool_output = Eip12Output {
        value: pool_box_erg.to_string(),
        ergo_tree: pool_ergo_tree.to_string(),
        assets: pool_assets,
        creation_height: current_height,
        additional_registers: pool_registers,
    };

    // 8. User LP output
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

    // 9. Fee output
    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    // 10. Assemble transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs: vec![bootstrap_box.clone()],
        data_inputs: vec![],
        outputs: vec![pool_output, user_output, fee_output],
    };

    // 11. Summary
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const USER_ERGO_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    /// Helper: create a test UTXO with the given box_id, ERG value, and tokens.
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

    // 64-char hex box IDs for realistic tests
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

    // =========================================================================
    // N2T Bootstrap
    // =========================================================================

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

        // LP token ID = first input box_id
        assert_eq!(build.summary.lp_token_id, BOX_ID_1);

        // 3 outputs: bootstrap, change, fee
        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        // Bootstrap box (output[0]) should have 2 assets: LP + TokenY
        let bootstrap = &build.unsigned_tx.outputs[0];
        assert_eq!(bootstrap.assets.len(), 2);
        assert_eq!(bootstrap.assets[0].token_id, BOX_ID_1); // LP token
        assert_eq!(bootstrap.assets[1].token_id, TOKEN_Y_ID);

        // LP minted = TOTAL_EMISSION - BURN_LP
        let lp_minted: u64 = bootstrap.assets[0].amount.parse().unwrap();
        assert_eq!(lp_minted, (TOTAL_EMISSION - BURN_LP) as u64);

        // Bootstrap box ERG = x_amount (the ERG deposit)
        let bootstrap_erg: u64 = bootstrap.value.parse().unwrap();
        assert_eq!(bootstrap_erg, params.x_amount);

        // R4 should be set
        assert!(bootstrap.additional_registers.contains_key("R4"));

        // User LP share should be sqrt(10e9 * 1e6) = sqrt(10e15) = 100_000_000 (approx)
        assert!(build.summary.user_lp_share > 0);
    }

    // =========================================================================
    // T2T Bootstrap
    // =========================================================================

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

        // Bootstrap box should have 3 assets: LP + TokenX + TokenY
        let bootstrap = &build.unsigned_tx.outputs[0];
        assert_eq!(bootstrap.assets.len(), 3);
        assert_eq!(bootstrap.assets[0].token_id, BOX_ID_1); // LP
        assert_eq!(bootstrap.assets[1].token_id, TOKEN_X_ID);
        assert_eq!(bootstrap.assets[2].token_id, TOKEN_Y_ID);

        // Bootstrap box ERG = MIN_BOX_VALUE for T2T
        let bootstrap_erg: u64 = bootstrap.value.parse().unwrap();
        assert_eq!(bootstrap_erg, MIN_BOX_VALUE);

        // Fee percent for 997/1000
        let expected_fee_pct = (1.0 - 997.0 / 1000.0) * 100.0;
        assert!(
            (build.summary.fee_percent - expected_fee_pct).abs() < 0.001,
            "Fee percent should be ~0.3%, got {}",
            build.summary.fee_percent
        );
    }

    // =========================================================================
    // Pool Create N2T
    // =========================================================================

    #[test]
    fn test_pool_create_n2t() {
        let params = n2t_params();
        let lp_minted = (TOTAL_EMISSION - BURN_LP) as u64;
        let user_lp_share = calculate_initial_lp_share(params.x_amount, params.y_amount);
        let lp_token_id = BOX_ID_1;

        // Simulate a bootstrap box (TX0 output[0])
        let bootstrap_box = Eip12InputBox {
            box_id: BOX_ID_2.to_string(),
            transaction_id: BOX_ID_1.to_string(),
            index: 0,
            value: params.x_amount.to_string(), // N2T: bootstrap holds ERG deposit
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
            additional_registers: {
                let mut m = HashMap::new();
                m.insert("R4".to_string(), encode_sigma_coll_byte(b"LP"));
                m
            },
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

        // Pool NFT ID = bootstrap box's box_id
        assert_eq!(build.summary.pool_nft_id, BOX_ID_2);

        // 3 outputs: pool, user LP, fee
        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        // Pool box (output[0])
        let pool_box = &build.unsigned_tx.outputs[0];

        // Pool should have 3 tokens: NFT, LP, TokenY
        assert_eq!(pool_box.assets.len(), 3);
        assert_eq!(pool_box.assets[0].token_id, BOX_ID_2); // NFT
        assert_eq!(pool_box.assets[0].amount, "1");
        assert_eq!(pool_box.assets[1].token_id, lp_token_id); // LP locked
        assert_eq!(pool_box.assets[2].token_id, TOKEN_Y_ID);

        // LP locked = minted - user_share
        let lp_locked: u64 = pool_box.assets[1].amount.parse().unwrap();
        assert_eq!(lp_locked, lp_minted - user_lp_share);

        // Pool ErgoTree should be N2T template
        assert_eq!(pool_box.ergo_tree, N2T_POOL_TEMPLATE);

        // R4 should exist (fee_num encoded)
        assert!(pool_box.additional_registers.contains_key("R4"));

        // User LP output (output[1])
        let user_out = &build.unsigned_tx.outputs[1];
        assert_eq!(user_out.assets.len(), 1);
        assert_eq!(user_out.assets[0].token_id, lp_token_id);
        let user_lp: u64 = user_out.assets[0].amount.parse().unwrap();
        assert_eq!(user_lp, user_lp_share);

        // Pool box ERG = bootstrap_erg - MIN_BOX_VALUE - TX_FEE
        let pool_erg: u64 = pool_box.value.parse().unwrap();
        assert_eq!(pool_erg, params.x_amount - MIN_BOX_VALUE - TX_FEE);
    }

    // =========================================================================
    // Validation tests
    // =========================================================================

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

        // Also test zero y_amount
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

        // fee_num >= 1000
        let mut params = n2t_params();
        params.fee_num = 1000;

        let result =
            build_pool_bootstrap_eip12(&params, &utxos, USER_ERGO_TREE, 1_000_000);

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("fee_num"),
            "Should reject fee_num >= 1000"
        );

        // fee_num == 0
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

    // =========================================================================
    // Pool Create T2T
    // =========================================================================

    #[test]
    fn test_pool_create_t2t() {
        let params = t2t_params();
        let lp_minted = (TOTAL_EMISSION - BURN_LP) as u64;
        let user_lp_share = calculate_initial_lp_share(params.x_amount, params.y_amount);
        let lp_token_id = BOX_ID_1;

        // Simulate a T2T bootstrap box
        let bootstrap_erg = 3_000_000u64; // MIN_BOX_VALUE + extra
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

        // Pool box should have 4 tokens: NFT, LP, TokenX, TokenY
        let pool_box = &build.unsigned_tx.outputs[0];
        assert_eq!(pool_box.assets.len(), 4);
        assert_eq!(pool_box.assets[0].token_id, BOX_ID_2); // NFT
        assert_eq!(pool_box.assets[0].amount, "1");

        // Pool ErgoTree should be T2T template
        assert_eq!(pool_box.ergo_tree, T2T_POOL_TEMPLATE);

        // Pool ERG = MIN_BOX_VALUE for T2T
        let pool_erg: u64 = pool_box.value.parse().unwrap();
        assert_eq!(pool_erg, MIN_BOX_VALUE);

        // User gets leftover ERG
        let user_out = &build.unsigned_tx.outputs[1];
        let user_erg: u64 = user_out.value.parse().unwrap();
        assert_eq!(user_erg, bootstrap_erg - MIN_BOX_VALUE - TX_FEE);
    }
}
