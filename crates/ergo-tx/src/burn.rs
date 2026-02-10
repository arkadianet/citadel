//! Token burn transaction builder
//!
//! Burns tokens by omitting them from transaction outputs.

use std::collections::HashMap;

use crate::eip12::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};

use citadel_core::constants::TX_FEE_NANO as TX_FEE;

/// Result of building a burn transaction
#[derive(Debug)]
pub struct BurnBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: BurnSummary,
}

/// Summary of what the burn transaction does
#[derive(Debug)]
pub struct BurnSummary {
    pub burned_token_id: String,
    pub burned_amount: u64,
    pub miner_fee: i64,
    pub change_erg: i64,
}

/// A single token+amount to burn in a multi-burn transaction
#[derive(Debug, Clone)]
pub struct BurnItem {
    pub token_id: String,
    pub amount: u64,
}

/// Summary of a multi-burn transaction
#[derive(Debug)]
pub struct MultiBurnSummary {
    pub burned_tokens: Vec<BurnItem>,
    pub miner_fee: i64,
    pub change_erg: i64,
}

/// Result of building a multi-burn transaction
#[derive(Debug)]
pub struct MultiBurnBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: MultiBurnSummary,
}

/// Errors from burn tx building
#[derive(Debug, thiserror::Error)]
pub enum BurnError {
    #[error("Insufficient token balance: have {have}, need {need}")]
    InsufficientTokens { have: u64, need: u64 },

    #[error("Insufficient ERG for fees: have {have}, need {need}")]
    InsufficientErg { have: i64, need: i64 },

    #[error("Burn amount must be greater than zero")]
    ZeroAmount,

    #[error("Burn list must not be empty")]
    EmptyBurnList,

    #[error("Duplicate token in burn list: {0}")]
    DuplicateToken(String),
}

/// Build an EIP-12 unsigned transaction that burns a specified amount of a token.
///
/// The transaction spends user inputs and creates a single change output back to
/// the user, minus the burned tokens and miner fee.
pub fn build_burn_tx(
    user_inputs: &[Eip12InputBox],
    burn_token_id: &str,
    burn_amount: u64,
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<BurnBuildResult, BurnError> {
    if burn_amount == 0 {
        return Err(BurnError::ZeroAmount);
    }

    // Sum total ERG from inputs
    let total_erg: i64 = user_inputs
        .iter()
        .map(|b| b.value.parse::<i64>().unwrap_or(0))
        .sum();

    // Sum total of the burn token across all inputs
    let total_burn_token: u64 = user_inputs
        .iter()
        .flat_map(|b| b.assets.iter())
        .filter(|a| a.token_id == burn_token_id)
        .map(|a| a.amount.parse::<u64>().unwrap_or(0))
        .sum();

    if total_burn_token < burn_amount {
        return Err(BurnError::InsufficientTokens {
            have: total_burn_token,
            need: burn_amount,
        });
    }

    let min_erg_needed = TX_FEE + citadel_core::constants::MIN_BOX_VALUE_NANO;
    if total_erg < min_erg_needed {
        return Err(BurnError::InsufficientErg {
            have: total_erg,
            need: min_erg_needed,
        });
    }

    let change_erg = total_erg - TX_FEE;

    // Aggregate all tokens from all inputs, then subtract the burned amount
    let mut token_totals: HashMap<String, u64> = HashMap::new();
    for input in user_inputs {
        for asset in &input.assets {
            let amount = asset.amount.parse::<u64>().unwrap_or(0);
            *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
        }
    }

    // Subtract burn amount
    if let Some(balance) = token_totals.get_mut(burn_token_id) {
        *balance = balance.saturating_sub(burn_amount);
        if *balance == 0 {
            token_totals.remove(burn_token_id);
        }
    }

    // Build change output assets (remaining tokens)
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

    let fee_output = Eip12Output::fee(TX_FEE, current_height);

    let unsigned_tx = Eip12UnsignedTx {
        inputs: user_inputs.to_vec(),
        data_inputs: vec![],
        outputs: vec![change_output, fee_output],
    };

    Ok(BurnBuildResult {
        unsigned_tx,
        summary: BurnSummary {
            burned_token_id: burn_token_id.to_string(),
            burned_amount: burn_amount,
            miner_fee: TX_FEE,
            change_erg,
        },
    })
}

/// Build an EIP-12 unsigned transaction that burns multiple tokens at once.
///
/// All specified burn items are removed from the change output. Remaining tokens
/// and ERG (minus miner fee) go to a single change box.
pub fn build_multi_burn_tx(
    user_inputs: &[Eip12InputBox],
    burn_items: &[BurnItem],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<MultiBurnBuildResult, BurnError> {
    use std::collections::HashSet;

    if burn_items.is_empty() {
        return Err(BurnError::EmptyBurnList);
    }

    // Check for zero amounts and duplicates
    let mut seen = HashSet::new();
    for item in burn_items {
        if item.amount == 0 {
            return Err(BurnError::ZeroAmount);
        }
        if !seen.insert(&item.token_id) {
            return Err(BurnError::DuplicateToken(item.token_id.clone()));
        }
    }

    // Sum total ERG from inputs
    let total_erg: i64 = user_inputs
        .iter()
        .map(|b| b.value.parse::<i64>().unwrap_or(0))
        .sum();

    let min_erg_needed = TX_FEE + citadel_core::constants::MIN_BOX_VALUE_NANO;
    if total_erg < min_erg_needed {
        return Err(BurnError::InsufficientErg {
            have: total_erg,
            need: min_erg_needed,
        });
    }

    // Aggregate all tokens from all inputs
    let mut token_totals: HashMap<String, u64> = HashMap::new();
    for input in user_inputs {
        for asset in &input.assets {
            let amount = asset.amount.parse::<u64>().unwrap_or(0);
            *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
        }
    }

    // Validate each burn item has sufficient balance, then subtract
    for item in burn_items {
        let have = token_totals.get(&item.token_id).copied().unwrap_or(0);
        if have < item.amount {
            return Err(BurnError::InsufficientTokens {
                have,
                need: item.amount,
            });
        }

        let balance = token_totals.get_mut(&item.token_id).unwrap();
        *balance = balance.saturating_sub(item.amount);
        if *balance == 0 {
            token_totals.remove(&item.token_id);
        }
    }

    let change_erg = total_erg - TX_FEE;

    // Build change output assets (remaining tokens)
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

    let fee_output = Eip12Output::fee(TX_FEE, current_height);

    let unsigned_tx = Eip12UnsignedTx {
        inputs: user_inputs.to_vec(),
        data_inputs: vec![],
        outputs: vec![change_output, fee_output],
    };

    Ok(MultiBurnBuildResult {
        unsigned_tx,
        summary: MultiBurnSummary {
            burned_tokens: burn_items.to_vec(),
            miner_fee: TX_FEE,
            change_erg,
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
    fn test_burn_partial_amount() {
        let inputs = vec![mock_input("box1", 10_000_000_000, vec![(TOKEN_A, 1000)])];
        let result = build_burn_tx(&inputs, TOKEN_A, 300, USER_TREE, 50000).unwrap();

        // Burned 300 out of 1000
        assert_eq!(result.summary.burned_amount, 300);
        assert_eq!(result.summary.burned_token_id, TOKEN_A);
        assert_eq!(result.summary.miner_fee, TX_FEE);
        assert_eq!(result.summary.change_erg, 10_000_000_000 - TX_FEE);

        // Change output should have 700 tokens remaining
        let change = &result.unsigned_tx.outputs[0];
        assert_eq!(change.ergo_tree, USER_TREE);
        assert_eq!(change.assets.len(), 1);
        assert_eq!(change.assets[0].token_id, TOKEN_A);
        assert_eq!(change.assets[0].amount, "700");
    }

    #[test]
    fn test_burn_full_amount() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 500)])];
        let result = build_burn_tx(&inputs, TOKEN_A, 500, USER_TREE, 50000).unwrap();

        // Change output should have no tokens
        let change = &result.unsigned_tx.outputs[0];
        assert!(change.assets.is_empty());
    }

    #[test]
    fn test_burn_preserves_other_tokens() {
        let inputs = vec![mock_input(
            "box1",
            5_000_000_000,
            vec![(TOKEN_A, 1000), (TOKEN_B, 200)],
        )];
        let result = build_burn_tx(&inputs, TOKEN_A, 1000, USER_TREE, 50000).unwrap();

        // Only TOKEN_B should remain
        let change = &result.unsigned_tx.outputs[0];
        assert_eq!(change.assets.len(), 1);
        assert_eq!(change.assets[0].token_id, TOKEN_B);
        assert_eq!(change.assets[0].amount, "200");
    }

    #[test]
    fn test_burn_across_multiple_inputs() {
        let inputs = vec![
            mock_input("box1", 3_000_000_000, vec![(TOKEN_A, 400)]),
            mock_input("box2", 2_000_000_000, vec![(TOKEN_A, 600)]),
        ];
        let result = build_burn_tx(&inputs, TOKEN_A, 800, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.burned_amount, 800);
        let change = &result.unsigned_tx.outputs[0];
        assert_eq!(change.assets.len(), 1);
        assert_eq!(change.assets[0].amount, "200"); // 1000 - 800
        assert_eq!(result.unsigned_tx.inputs.len(), 2);
    }

    #[test]
    fn test_burn_insufficient_tokens() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 100)])];
        let err = build_burn_tx(&inputs, TOKEN_A, 500, USER_TREE, 50000).unwrap_err();
        match err {
            BurnError::InsufficientTokens { have, need } => {
                assert_eq!(have, 100);
                assert_eq!(need, 500);
            }
            _ => panic!("Expected InsufficientTokens, got {:?}", err),
        }
    }

    #[test]
    fn test_burn_insufficient_erg() {
        let inputs = vec![mock_input("box1", 1_000_000, vec![(TOKEN_A, 100)])];
        let err = build_burn_tx(&inputs, TOKEN_A, 50, USER_TREE, 50000).unwrap_err();
        match err {
            BurnError::InsufficientErg { .. } => {}
            _ => panic!("Expected InsufficientErg, got {:?}", err),
        }
    }

    #[test]
    fn test_burn_zero_amount_rejected() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 100)])];
        let err = build_burn_tx(&inputs, TOKEN_A, 0, USER_TREE, 50000).unwrap_err();
        match err {
            BurnError::ZeroAmount => {}
            _ => panic!("Expected ZeroAmount, got {:?}", err),
        }
    }

    #[test]
    fn test_burn_tx_structure() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 100)])];
        let result = build_burn_tx(&inputs, TOKEN_A, 50, USER_TREE, 50000).unwrap();

        // Should have exactly 2 outputs: change + fee
        assert_eq!(result.unsigned_tx.outputs.len(), 2);
        assert_eq!(result.unsigned_tx.data_inputs.len(), 0);

        // Output 0: change to user
        assert_eq!(result.unsigned_tx.outputs[0].ergo_tree, USER_TREE);

        // Output 1: miner fee
        assert_eq!(
            result.unsigned_tx.outputs[1].ergo_tree,
            citadel_core::constants::MINER_FEE_ERGO_TREE
        );
        assert_eq!(result.unsigned_tx.outputs[1].value, TX_FEE.to_string());
    }

    // =========================================================================
    // Multi-burn tests
    // =========================================================================

    #[test]
    fn test_multi_burn_two_tokens() {
        let inputs = vec![mock_input(
            "box1",
            10_000_000_000,
            vec![(TOKEN_A, 1000), (TOKEN_B, 500)],
        )];
        let items = vec![
            BurnItem {
                token_id: TOKEN_A.to_string(),
                amount: 300,
            },
            BurnItem {
                token_id: TOKEN_B.to_string(),
                amount: 200,
            },
        ];
        let result = build_multi_burn_tx(&inputs, &items, USER_TREE, 50000).unwrap();

        assert_eq!(result.summary.burned_tokens.len(), 2);
        assert_eq!(result.summary.miner_fee, TX_FEE);

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
        assert_eq!(a, 700);
        assert_eq!(b, 300);
    }

    #[test]
    fn test_multi_burn_full_amounts() {
        let inputs = vec![mock_input(
            "box1",
            5_000_000_000,
            vec![(TOKEN_A, 100), (TOKEN_B, 200)],
        )];
        let items = vec![
            BurnItem {
                token_id: TOKEN_A.to_string(),
                amount: 100,
            },
            BurnItem {
                token_id: TOKEN_B.to_string(),
                amount: 200,
            },
        ];
        let result = build_multi_burn_tx(&inputs, &items, USER_TREE, 50000).unwrap();

        let change = &result.unsigned_tx.outputs[0];
        assert!(change.assets.is_empty());
    }

    #[test]
    fn test_multi_burn_preserves_unburned() {
        let token_c = "cccc000000000000000000000000000000000000000000000000000000000000";
        let inputs = vec![mock_input(
            "box1",
            5_000_000_000,
            vec![(TOKEN_A, 100), (TOKEN_B, 200), (token_c, 50)],
        )];
        let items = vec![
            BurnItem {
                token_id: TOKEN_A.to_string(),
                amount: 100,
            },
            BurnItem {
                token_id: TOKEN_B.to_string(),
                amount: 200,
            },
        ];
        let result = build_multi_burn_tx(&inputs, &items, USER_TREE, 50000).unwrap();

        let change = &result.unsigned_tx.outputs[0];
        assert_eq!(change.assets.len(), 1);
        assert_eq!(change.assets[0].token_id, token_c);
        assert_eq!(change.assets[0].amount, "50");
    }

    #[test]
    fn test_multi_burn_across_inputs() {
        let inputs = vec![
            mock_input("box1", 3_000_000_000, vec![(TOKEN_A, 400)]),
            mock_input("box2", 2_000_000_000, vec![(TOKEN_B, 600)]),
        ];
        let items = vec![
            BurnItem {
                token_id: TOKEN_A.to_string(),
                amount: 400,
            },
            BurnItem {
                token_id: TOKEN_B.to_string(),
                amount: 300,
            },
        ];
        let result = build_multi_burn_tx(&inputs, &items, USER_TREE, 50000).unwrap();

        assert_eq!(result.unsigned_tx.inputs.len(), 2);
        let change = &result.unsigned_tx.outputs[0];
        let b: u64 = change
            .assets
            .iter()
            .filter(|a| a.token_id == TOKEN_B)
            .map(|a| a.amount.parse::<u64>().unwrap())
            .sum();
        assert_eq!(b, 300);
    }

    #[test]
    fn test_multi_burn_empty_list_error() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 100)])];
        let err = build_multi_burn_tx(&inputs, &[], USER_TREE, 50000).unwrap_err();
        match err {
            BurnError::EmptyBurnList => {}
            _ => panic!("Expected EmptyBurnList, got {:?}", err),
        }
    }

    #[test]
    fn test_multi_burn_duplicate_token_error() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 100)])];
        let items = vec![
            BurnItem {
                token_id: TOKEN_A.to_string(),
                amount: 50,
            },
            BurnItem {
                token_id: TOKEN_A.to_string(),
                amount: 30,
            },
        ];
        let err = build_multi_burn_tx(&inputs, &items, USER_TREE, 50000).unwrap_err();
        match err {
            BurnError::DuplicateToken(id) => assert_eq!(id, TOKEN_A),
            _ => panic!("Expected DuplicateToken, got {:?}", err),
        }
    }

    #[test]
    fn test_multi_burn_zero_amount_error() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 100)])];
        let items = vec![BurnItem {
            token_id: TOKEN_A.to_string(),
            amount: 0,
        }];
        let err = build_multi_burn_tx(&inputs, &items, USER_TREE, 50000).unwrap_err();
        match err {
            BurnError::ZeroAmount => {}
            _ => panic!("Expected ZeroAmount, got {:?}", err),
        }
    }

    #[test]
    fn test_multi_burn_insufficient_token_error() {
        let inputs = vec![mock_input("box1", 5_000_000_000, vec![(TOKEN_A, 10)])];
        let items = vec![BurnItem {
            token_id: TOKEN_A.to_string(),
            amount: 100,
        }];
        let err = build_multi_burn_tx(&inputs, &items, USER_TREE, 50000).unwrap_err();
        match err {
            BurnError::InsufficientTokens { have, need } => {
                assert_eq!(have, 10);
                assert_eq!(need, 100);
            }
            _ => panic!("Expected InsufficientTokens, got {:?}", err),
        }
    }
}
