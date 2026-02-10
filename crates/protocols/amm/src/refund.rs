//! Refund Transaction Builder
//!
//! Builds EIP-12 unsigned transactions to refund unexecuted swap orders.
//! Spends the proxy box via RefundProp (user's public key) and returns
//! all contents to the user's address.
//!
//! # Transaction Structure
//!
//! Inputs:  [proxy_box, (optional extra user UTXOs for fee)]
//! Outputs: [user_output, miner_fee]

use std::collections::HashMap;

use ergo_tx::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};
use serde::{Deserialize, Serialize};

use crate::state::AmmError;

/// Standard miner fee for refund transactions (0.0011 ERG)
const REFUND_TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;

/// Result of building a refund transaction
#[derive(Debug, Serialize, Deserialize)]
pub struct RefundBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: RefundSummary,
}

/// Summary of the refund for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundSummary {
    pub proxy_box_id: String,
    pub refunded_erg: u64,
    pub refunded_tokens: Vec<(String, u64)>,
    pub miner_fee: u64,
}

/// Build a refund transaction for an unexecuted swap order.
///
/// Spends the proxy box via RefundProp (user's public key) and sends
/// all contents back to the user's address.
///
/// # Arguments
///
/// * `proxy_box` - The swap proxy box to refund
/// * `user_ergo_tree` - User's ErgoTree hex (destination for refunded funds)
/// * `current_height` - Current blockchain height
/// * `additional_inputs` - Optional extra UTXOs if proxy box doesn't cover miner fee
pub fn build_refund_tx_eip12(
    proxy_box: &Eip12InputBox,
    user_ergo_tree: &str,
    current_height: i32,
    additional_inputs: &[Eip12InputBox],
) -> Result<RefundBuildResult, AmmError> {
    // 1. Calculate total input ERG
    let proxy_value: u64 = proxy_box
        .value
        .parse()
        .map_err(|_| AmmError::RefundError("Invalid proxy box value".to_string()))?;

    let additional_erg: u64 = additional_inputs
        .iter()
        .map(|u| u.value.parse::<u64>().unwrap_or(0))
        .sum();

    let total_input_erg = proxy_value + additional_erg;

    // 2. Validate enough for miner fee
    if total_input_erg <= REFUND_TX_FEE {
        return Err(AmmError::RefundError(format!(
            "Insufficient ERG for miner fee: have {} nanoERG, need more than {}",
            total_input_erg, REFUND_TX_FEE
        )));
    }

    let user_erg = total_input_erg - REFUND_TX_FEE;

    // 3. Collect all tokens from all inputs
    let mut token_totals: HashMap<String, u64> = HashMap::new();
    for input in std::iter::once(proxy_box).chain(additional_inputs.iter()) {
        for asset in &input.assets {
            let amount: u64 = asset.amount.parse().unwrap_or_else(|_| {
                tracing::warn!(
                    token_id = %asset.token_id,
                    raw = %asset.amount,
                    "Failed to parse token amount in refund, defaulting to 0"
                );
                0
            });
            *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
        }
    }

    let user_assets: Vec<Eip12Asset> = token_totals
        .iter()
        .filter(|(_, &amount)| amount > 0)
        .map(|(token_id, &amount)| Eip12Asset {
            token_id: token_id.clone(),
            amount: amount.to_string(),
        })
        .collect();

    let refunded_tokens: Vec<(String, u64)> = token_totals
        .iter()
        .filter(|(_, &amount)| amount > 0)
        .map(|(id, &amount)| (id.clone(), amount))
        .collect();

    // 4. Build user output
    let user_output = Eip12Output {
        value: user_erg.to_string(),
        ergo_tree: user_ergo_tree.to_string(),
        assets: user_assets,
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // 5. Build fee output
    let fee_output = Eip12Output::fee(REFUND_TX_FEE as i64, current_height);

    // 6. Build inputs list
    let mut inputs = vec![proxy_box.clone()];
    inputs.extend(additional_inputs.iter().cloned());

    // 7. Assemble transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs: vec![user_output, fee_output],
    };

    let summary = RefundSummary {
        proxy_box_id: proxy_box.box_id.clone(),
        refunded_erg: user_erg,
        refunded_tokens,
        miner_fee: REFUND_TX_FEE,
    };

    Ok(RefundBuildResult {
        unsigned_tx,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_proxy_box_erg_to_token() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "proxy_box_1".to_string(),
            transaction_id: "submit_tx_1".to_string(),
            index: 0,
            value: "1006000000".to_string(), // 1 ERG input + 2M exec + 4M proxy
            ergo_tree: "19fe04aabbccdd".to_string(),
            assets: vec![],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    fn test_proxy_box_token_to_erg() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "proxy_box_2".to_string(),
            transaction_id: "submit_tx_2".to_string(),
            index: 0,
            value: "6000000".to_string(), // 2M exec + 4M proxy
            ergo_tree: "198b04aabbccdd".to_string(),
            assets: vec![Eip12Asset {
                token_id: "0000000000000000000000000000000000000000000000000000000000000002"
                    .to_string(),
                amount: "10000".to_string(),
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    const USER_ERGO_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    #[test]
    fn test_build_refund_erg_to_token_order() {
        let proxy = test_proxy_box_erg_to_token();
        let result = build_refund_tx_eip12(&proxy, USER_ERGO_TREE, 1_000_000, &[]).unwrap();

        let tx = &result.unsigned_tx;
        assert_eq!(tx.outputs.len(), 2);
        assert_eq!(tx.inputs.len(), 1);
        assert!(tx.data_inputs.is_empty());

        // User output: proxy value - fee
        let user_output = &tx.outputs[0];
        let user_value: u64 = user_output.value.parse().unwrap();
        assert_eq!(user_value, 1_006_000_000 - REFUND_TX_FEE);
        assert_eq!(user_output.ergo_tree, USER_ERGO_TREE);
        assert!(user_output.assets.is_empty());

        // Fee output
        let fee_output = &tx.outputs[1];
        assert_eq!(fee_output.value, REFUND_TX_FEE.to_string());

        // Summary
        assert_eq!(result.summary.proxy_box_id, "proxy_box_1");
        assert_eq!(result.summary.refunded_erg, 1_006_000_000 - REFUND_TX_FEE);
        assert!(result.summary.refunded_tokens.is_empty());
        assert_eq!(result.summary.miner_fee, REFUND_TX_FEE);
    }

    #[test]
    fn test_build_refund_token_to_erg_order() {
        let proxy = test_proxy_box_token_to_erg();
        let result = build_refund_tx_eip12(&proxy, USER_ERGO_TREE, 1_000_000, &[]).unwrap();

        let tx = &result.unsigned_tx;
        assert_eq!(tx.outputs.len(), 2);

        // User output should have the token
        let user_output = &tx.outputs[0];
        let user_value: u64 = user_output.value.parse().unwrap();
        assert_eq!(user_value, 6_000_000 - REFUND_TX_FEE);
        assert_eq!(user_output.assets.len(), 1);
        assert_eq!(
            user_output.assets[0].token_id,
            "0000000000000000000000000000000000000000000000000000000000000002"
        );
        assert_eq!(user_output.assets[0].amount, "10000");

        // Summary
        assert_eq!(result.summary.refunded_tokens.len(), 1);
    }

    #[test]
    fn test_refund_preserves_all_tokens() {
        let proxy = Eip12InputBox {
            box_id: "proxy_multi".to_string(),
            transaction_id: "tx_multi".to_string(),
            index: 0,
            value: "10000000".to_string(),
            ergo_tree: "19aabbcc".to_string(),
            assets: vec![
                Eip12Asset {
                    token_id: "token_a".to_string(),
                    amount: "100".to_string(),
                },
                Eip12Asset {
                    token_id: "token_b".to_string(),
                    amount: "200".to_string(),
                },
                Eip12Asset {
                    token_id: "token_c".to_string(),
                    amount: "300".to_string(),
                },
            ],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        };

        let result = build_refund_tx_eip12(&proxy, USER_ERGO_TREE, 1_000_000, &[]).unwrap();

        let user_output = &result.unsigned_tx.outputs[0];
        assert_eq!(user_output.assets.len(), 3);

        // Check all tokens are present
        let mut token_ids: Vec<&str> = user_output
            .assets
            .iter()
            .map(|a| a.token_id.as_str())
            .collect();
        token_ids.sort();
        assert_eq!(token_ids, vec!["token_a", "token_b", "token_c"]);
    }

    #[test]
    fn test_refund_tx_has_correct_structure() {
        let proxy = test_proxy_box_erg_to_token();
        let result = build_refund_tx_eip12(&proxy, USER_ERGO_TREE, 1_000_000, &[]).unwrap();

        let tx = &result.unsigned_tx;
        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.inputs[0].box_id, "proxy_box_1");
        assert!(tx.data_inputs.is_empty());
        assert_eq!(tx.outputs.len(), 2);
    }

    #[test]
    fn test_refund_insufficient_value_for_fee() {
        let proxy = Eip12InputBox {
            box_id: "tiny_box".to_string(),
            transaction_id: "tx".to_string(),
            index: 0,
            value: "500000".to_string(), // 0.5M - less than 1.1M fee
            ergo_tree: "19aabb".to_string(),
            assets: vec![],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        };

        let result = build_refund_tx_eip12(&proxy, USER_ERGO_TREE, 1_000_000, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient"));
    }

    #[test]
    fn test_refund_with_extra_user_utxo_for_fee() {
        // Proxy box has just barely enough for min box value, not fee
        let proxy = Eip12InputBox {
            box_id: "low_erg_proxy".to_string(),
            transaction_id: "tx_low".to_string(),
            index: 0,
            value: "1000000".to_string(), // 1M - not enough for 1.1M fee alone
            ergo_tree: "19aabb".to_string(),
            assets: vec![Eip12Asset {
                token_id: "some_token".to_string(),
                amount: "5000".to_string(),
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        };

        // But user provides an extra UTXO with enough ERG
        let extra_utxo = Eip12InputBox {
            box_id: "extra_utxo".to_string(),
            transaction_id: "tx_extra".to_string(),
            index: 0,
            value: "5000000".to_string(), // 5M ERG
            ergo_tree: USER_ERGO_TREE.to_string(),
            assets: vec![],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        };

        let result =
            build_refund_tx_eip12(&proxy, USER_ERGO_TREE, 1_000_000, &[extra_utxo]).unwrap();

        let tx = &result.unsigned_tx;
        // inputs = [proxy_box, user_utxo]
        assert_eq!(tx.inputs.len(), 2);
        assert_eq!(tx.inputs[0].box_id, "low_erg_proxy");
        assert_eq!(tx.inputs[1].box_id, "extra_utxo");

        // User gets 1M + 5M - 1.1M fee = 4.9M ERG + the token
        let user_output = &tx.outputs[0];
        let user_value: u64 = user_output.value.parse().unwrap();
        assert_eq!(user_value, 6_000_000 - REFUND_TX_FEE);
        assert_eq!(user_output.assets.len(), 1);
        assert_eq!(user_output.assets[0].token_id, "some_token");
        assert_eq!(user_output.assets[0].amount, "5000");
    }
}
