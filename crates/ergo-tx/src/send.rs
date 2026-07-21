//! Simple wallet send (ERG and/or token) transaction builder.
//!
//! Builds an EIP-12 unsigned tx for Nautilus/ErgoPay signing. Does not manage keys.

use std::collections::HashMap;

use crate::dev_fee::{append_dev_fee_output, resolved_config};
use crate::eip12::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};

use citadel_core::constants::{MIN_BOX_VALUE_NANO as MIN_BOX_VALUE, TX_FEE_NANO as TX_FEE};

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("No inputs provided")]
    NoInputs,

    #[error("Must send ERG and/or a token")]
    EmptySend,

    #[error("Send ERG amount must be at least {min} nanoERG (min box value)")]
    BelowMinBoxValue { min: i64 },

    #[error("Token amount must be greater than zero")]
    ZeroTokenAmount,

    #[error("Insufficient ERG: have {have} nanoERG, need {need} nanoERG")]
    InsufficientErg { have: i64, need: i64 },

    #[error("Insufficient tokens: have {have} of {token_id}, need {need}")]
    InsufficientTokens {
        token_id: String,
        have: u64,
        need: u64,
    },

    #[error(
        "Change amount {change} nanoERG is below minimum box value of {min} nanoERG"
    )]
    ChangeBelowMin { change: i64, min: i64 },

    #[error("Citadel fee config error: {0}")]
    DevFee(String),
}

#[derive(Debug)]
pub struct SendSummary {
    pub recipient_erg: i64,
    pub token_id: Option<String>,
    pub token_amount: Option<u64>,
    pub change_erg: i64,
    pub miner_fee: i64,
    /// Citadel app fee in nanoERG (0 when disabled)
    pub citadel_fee_nano: i64,
    pub input_count: usize,
}

#[derive(Debug)]
pub struct SendBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: SendSummary,
}

/// Build an EIP-12 unsigned send tx from already-selected inputs.
///
/// - `send_erg` is the ERG attached to the recipient output (must be >= MIN_BOX_VALUE).
/// - Optional token is placed on the recipient output.
/// - Leftover ERG/tokens go to `change_ergo_tree` (wallet primary address).
/// - When enabled, appends Citadel app fee (0.011 ERG) before miner fee.
pub fn build_send_tx(
    user_inputs: &[Eip12InputBox],
    recipient_ergo_tree: &str,
    change_ergo_tree: &str,
    send_erg: i64,
    send_token: Option<(&str, u64)>,
    current_height: i32,
) -> Result<SendBuildResult, SendError> {
    if user_inputs.is_empty() {
        return Err(SendError::NoInputs);
    }

    let sending_token = send_token.is_some();
    if send_erg <= 0 && !sending_token {
        return Err(SendError::EmptySend);
    }
    if send_erg < MIN_BOX_VALUE {
        return Err(SendError::BelowMinBoxValue {
            min: MIN_BOX_VALUE,
        });
    }
    if let Some((_, amt)) = send_token {
        if amt == 0 {
            return Err(SendError::ZeroTokenAmount);
        }
    }

    let total_erg: i64 = user_inputs
        .iter()
        .map(|b| b.value.parse::<i64>().unwrap_or(0))
        .sum();

    let mut token_totals: HashMap<String, u64> = HashMap::new();
    for input in user_inputs {
        for asset in &input.assets {
            let amount = asset.amount.parse::<u64>().unwrap_or(0);
            *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
        }
    }

    if let Some((token_id, amount)) = send_token {
        let have = token_totals.get(token_id).copied().unwrap_or(0);
        if have < amount {
            return Err(SendError::InsufficientTokens {
                token_id: token_id.to_string(),
                have,
                need: amount,
            });
        }
    }

    let fee_cfg = resolved_config();
    let citadel_fee = fee_cfg.budget();

    let min_needed = send_erg + TX_FEE + citadel_fee;
    if total_erg < min_needed {
        return Err(SendError::InsufficientErg {
            have: total_erg,
            need: min_needed,
        });
    }

    let remainder = total_erg - send_erg - TX_FEE - citadel_fee;

    // Subtract sent token from totals for change
    if let Some((token_id, amount)) = send_token {
        if let Some(balance) = token_totals.get_mut(token_id) {
            *balance = balance.saturating_sub(amount);
            if *balance == 0 {
                token_totals.remove(token_id);
            }
        }
    }

    let has_change_tokens = !token_totals.is_empty();
    let need_change = remainder > 0 || has_change_tokens;

    if need_change {
        if has_change_tokens && remainder < MIN_BOX_VALUE {
            return Err(SendError::InsufficientErg {
                have: total_erg,
                need: send_erg + TX_FEE + citadel_fee + MIN_BOX_VALUE,
            });
        }
        if remainder > 0 && remainder < MIN_BOX_VALUE {
            return Err(SendError::ChangeBelowMin {
                change: remainder,
                min: MIN_BOX_VALUE,
            });
        }
    }

    let recipient_assets: Vec<Eip12Asset> = match send_token {
        Some((token_id, amount)) => vec![Eip12Asset::new(token_id, amount as i64)],
        None => vec![],
    };

    let mut outputs = Vec::with_capacity(4);
    outputs.push(Eip12Output {
        value: send_erg.to_string(),
        ergo_tree: recipient_ergo_tree.to_string(),
        assets: recipient_assets,
        creation_height: current_height,
        additional_registers: HashMap::new(),
    });

    let change_erg = if need_change {
        let change_value = if remainder > 0 {
            remainder
        } else {
            MIN_BOX_VALUE
        };
        let change_assets: Vec<Eip12Asset> = token_totals
            .into_iter()
            .map(|(id, amt)| Eip12Asset::new(id, amt as i64))
            .collect();
        outputs.push(Eip12Output {
            value: change_value.to_string(),
            ergo_tree: change_ergo_tree.to_string(),
            assets: change_assets,
            creation_height: current_height,
            additional_registers: HashMap::new(),
        });
        change_value
    } else {
        0
    };

    append_dev_fee_output(&mut outputs, &fee_cfg, current_height)
        .map_err(|e| SendError::DevFee(e.to_string()))?;
    outputs.push(Eip12Output::fee(TX_FEE, current_height));

    let unsigned_tx = Eip12UnsignedTx {
        inputs: user_inputs.to_vec(),
        data_inputs: vec![],
        outputs,
    };

    Ok(SendBuildResult {
        unsigned_tx,
        summary: SendSummary {
            recipient_erg: send_erg,
            token_id: send_token.map(|(id, _)| id.to_string()),
            token_amount: send_token.map(|(_, amt)| amt),
            change_erg,
            miner_fee: TX_FEE,
            citadel_fee_nano: citadel_fee,
            input_count: user_inputs.len(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev_fee::{with_test_dev_fee, DevFeeConfig};
    use crate::eip12::Eip12Asset;
    use citadel_core::constants::DEV_FEE_NANO;
    use std::collections::HashMap;

    const USER_TREE: &str = "0008cduser";
    const RECIPIENT_TREE: &str = "0008cdrecip";

    fn make_box(value: &str, assets: Vec<(&str, &str)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: "b".to_string(),
            transaction_id: "t".to_string(),
            index: 0,
            value: value.to_string(),
            ergo_tree: USER_TREE.to_string(),
            assets: assets
                .into_iter()
                .map(|(id, amt)| Eip12Asset {
                    token_id: id.to_string(),
                    amount: amt.to_string(),
                })
                .collect(),
            creation_height: 1,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn send_erg_only() {
        let inputs = vec![make_box("10000000000", vec![])]; // 10 ERG
        let result = build_send_tx(
            &inputs,
            RECIPIENT_TREE,
            USER_TREE,
            2_000_000_000, // 2 ERG
            None,
            50000,
        )
        .unwrap();

        assert_eq!(result.summary.recipient_erg, 2_000_000_000);
        assert_eq!(result.summary.miner_fee, TX_FEE);
        assert_eq!(result.summary.citadel_fee_nano, 0);
        assert_eq!(
            result.summary.change_erg,
            10_000_000_000 - 2_000_000_000 - TX_FEE
        );
        assert_eq!(result.unsigned_tx.outputs.len(), 3); // recipient + change + fee
        assert_eq!(result.unsigned_tx.outputs[0].ergo_tree, RECIPIENT_TREE);
        assert_eq!(result.unsigned_tx.outputs[1].ergo_tree, USER_TREE);
    }

    #[test]
    fn send_erg_with_citadel_fee() {
        with_test_dev_fee(DevFeeConfig::enabled_default(), || {
            let inputs = vec![make_box("10000000000", vec![])];
            let result = build_send_tx(
                &inputs,
                RECIPIENT_TREE,
                USER_TREE,
                2_000_000_000,
                None,
                50000,
            )
            .unwrap();

            assert_eq!(result.summary.citadel_fee_nano, DEV_FEE_NANO);
            assert_eq!(
                result.summary.change_erg,
                10_000_000_000 - 2_000_000_000 - TX_FEE - DEV_FEE_NANO
            );
            // recipient + change + citadel + miner
            assert_eq!(result.unsigned_tx.outputs.len(), 4);
            let fee_out = &result.unsigned_tx.outputs[2];
            assert_eq!(fee_out.value, DEV_FEE_NANO.to_string());
            assert_eq!(
                fee_out.ergo_tree,
                crate::dev_fee::DEFAULT_DEV_FEE_ERGO_TREE
            );
        });
    }

    #[test]
    fn send_insufficient_when_citadel_fee_enabled() {
        with_test_dev_fee(DevFeeConfig::enabled_default(), || {
            let send = 5_000_000_000i64;
            // Exactly send + miner — missing citadel fee
            let inputs = vec![make_box(&(send + TX_FEE).to_string(), vec![])];
            let err = build_send_tx(
                &inputs,
                RECIPIENT_TREE,
                USER_TREE,
                send,
                None,
                50000,
            )
            .unwrap_err();
            match err {
                SendError::InsufficientErg { need, .. } => {
                    assert_eq!(need, send + TX_FEE + DEV_FEE_NANO);
                }
                other => panic!("expected InsufficientErg, got {other:?}"),
            }
        });
    }

    #[test]
    fn send_token_with_min_erg() {
        let inputs = vec![make_box(
            "5000000000",
            vec![("tok_a", "100")],
        )];
        let result = build_send_tx(
            &inputs,
            RECIPIENT_TREE,
            USER_TREE,
            MIN_BOX_VALUE,
            Some(("tok_a", 40)),
            50000,
        )
        .unwrap();

        assert_eq!(result.summary.token_amount, Some(40));
        assert_eq!(result.unsigned_tx.outputs[0].assets.len(), 1);
        assert_eq!(result.unsigned_tx.outputs[0].assets[0].amount, "40");
        // change keeps remaining 60 tokens
        assert_eq!(result.unsigned_tx.outputs[1].assets.len(), 1);
        assert_eq!(result.unsigned_tx.outputs[1].assets[0].amount, "60");
    }

    #[test]
    fn reject_below_min_box() {
        let inputs = vec![make_box("10000000000", vec![])];
        let err = build_send_tx(
            &inputs,
            RECIPIENT_TREE,
            USER_TREE,
            500_000,
            None,
            50000,
        )
        .unwrap_err();
        assert!(matches!(err, SendError::BelowMinBoxValue { .. }));
    }

    #[test]
    fn reject_insufficient_token() {
        let inputs = vec![make_box("5000000000", vec![("tok_a", "10")])];
        let err = build_send_tx(
            &inputs,
            RECIPIENT_TREE,
            USER_TREE,
            MIN_BOX_VALUE,
            Some(("tok_a", 50)),
            50000,
        )
        .unwrap_err();
        assert!(matches!(err, SendError::InsufficientTokens { .. }));
    }

    #[test]
    fn exact_spend_no_change() {
        // send + fee exactly consumes inputs, no leftover tokens
        let send = 5_000_000_000i64;
        let inputs = vec![make_box(&(send + TX_FEE).to_string(), vec![])];
        let result = build_send_tx(
            &inputs,
            RECIPIENT_TREE,
            USER_TREE,
            send,
            None,
            50000,
        )
        .unwrap();
        assert_eq!(result.summary.change_erg, 0);
        assert_eq!(result.unsigned_tx.outputs.len(), 2); // recipient + fee only
    }
}
