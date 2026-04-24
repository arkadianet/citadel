use std::collections::HashMap;
use std::sync::Arc;

use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
use ergo_lib::ergotree_ir::mir::value::CollKind;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::ergotree_ir::types::stype::SType;
use ergo_tx::{select_erg_boxes, Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};

use crate::constants::{
    ERGOPAD_TOKEN, MIN_BOX_VALUE, MIN_CHANGE_VALUE, MIN_MINER_FEE, STAKE_STATE_NFT, STAKE_TOKEN,
};
use crate::state::{RecoverableStake, RecoveryError, StakeStateSnapshot};

/// Build the 3-input unstake tx, mirroring the shape of
/// `0e1f269f2fe8e75d1c75d6550b4bff13ec43cdde7cb5afc94690e5cc1139e032`.
///
/// Inputs:
///  - \[0\] StakeStateBox
///  - \[1\] matching StakeBox
///  - \[2..\] user P2PK boxes (first one MUST hold the stake key NFT)
///
/// Outputs:
///  - \[0\] new StakeStateBox (R4 updated, stake token amount +1)
///  - \[1\] user ERGOPAD payout (MIN_BOX_VALUE + released ERGOPAD)
///  - \[2?\] change box (stake key + remaining ERG/tokens, if any)
///  - \[last\] miner fee
pub fn build_recovery_tx_eip12(
    state_box: &Eip12InputBox,
    state: &StakeStateSnapshot,
    stake_box: &Eip12InputBox,
    stake: &RecoverableStake,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<Eip12UnsignedTx, RecoveryError> {
    validate_state_box(state_box)?;
    validate_stake_box(stake_box)?;

    let state_nft = &state_box.assets[0];
    let state_stake_tok = &state_box.assets[1];

    // --- Build new state box ---
    let new_r4 = encode_long_coll(vec![
        state.total_staked_raw - stake.ergopad_amount_raw,
        state.checkpoint,
        state.num_stakers - 1,
        state.last_checkpoint_ts,
        state.cycle_duration_ms,
    ])?;

    let mut new_registers = state_box.additional_registers.clone();
    new_registers.insert("R4".into(), new_r4);

    let state_value_nano: u64 = state_box
        .value
        .parse()
        .map_err(|_| RecoveryError::TxBuildError("Invalid state box value".into()))?;
    let stake_value_nano: u64 = stake_box
        .value
        .parse()
        .map_err(|_| RecoveryError::TxBuildError("Invalid stake box value".into()))?;
    let old_stake_token_amount: u64 = state_stake_tok
        .amount
        .parse()
        .map_err(|_| RecoveryError::TxBuildError("Invalid stake token amount".into()))?;
    let new_stake_token_amount = old_stake_token_amount + 1;

    let new_state_output = Eip12Output {
        value: state_value_nano.to_string(),
        ergo_tree: state_box.ergo_tree.clone(),
        assets: vec![
            Eip12Asset {
                token_id: state_nft.token_id.clone(),
                amount: state_nft.amount.clone(),
            },
            Eip12Asset {
                token_id: state_stake_tok.token_id.clone(),
                amount: new_stake_token_amount.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: new_registers,
    };

    // --- User's ERGOPAD payout ---
    let ergopad_output = Eip12Output::change(
        MIN_BOX_VALUE as i64,
        user_ergo_tree,
        vec![Eip12Asset::new(ERGOPAD_TOKEN, stake.ergopad_amount_raw)],
        current_height,
    );

    let fee_output = Eip12Output::fee(MIN_MINER_FEE as i64, current_height);

    // --- User input selection (stake key box first, then top-up) ---
    let key_idx = user_utxos
        .iter()
        .position(|b| b.assets.iter().any(|a| a.token_id == stake.stake_key_id))
        .ok_or_else(|| {
            RecoveryError::InsufficientFunds(format!(
                "Wallet does not contain stake key {}",
                stake.stake_key_id
            ))
        })?;
    let key_box = user_utxos[key_idx].clone();
    let key_erg: u64 = key_box
        .value
        .parse()
        .map_err(|_| RecoveryError::TxBuildError("Invalid key box value".into()))?;

    // Non-change output ERG burden, minus what the key box + stake box already cover.
    let target = MIN_BOX_VALUE + MIN_MINER_FEE;
    let already_have = stake_value_nano.saturating_add(key_erg);
    let additional_needed = target.saturating_sub(already_have);

    let other_utxos: Vec<Eip12InputBox> = user_utxos
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != key_idx)
        .map(|(_, b)| b.clone())
        .collect();

    let mut selected_boxes = vec![key_box];
    if additional_needed > 0 {
        let extra = select_erg_boxes(&other_utxos, additional_needed)
            .map_err(|e| RecoveryError::InsufficientFunds(e.to_string()))?;
        selected_boxes.extend(extra.boxes);
    }

    // --- ERG accounting: conserve total value ---
    let selected_total: u64 = selected_boxes
        .iter()
        .map(|b| b.value.parse::<u64>().unwrap_or(0))
        .sum();
    let inputs_total = state_value_nano
        .checked_add(stake_value_nano)
        .and_then(|s| s.checked_add(selected_total))
        .ok_or_else(|| RecoveryError::TxBuildError("ERG overflow".into()))?;
    let outputs_non_change = state_value_nano + MIN_BOX_VALUE + MIN_MINER_FEE;
    let change_erg = inputs_total
        .checked_sub(outputs_non_change)
        .ok_or_else(|| RecoveryError::TxBuildError("ERG underflow".into()))?;

    // --- All selected input tokens flow to the change box (including the stake key NFT) ---
    let mut change_assets_map: HashMap<String, u64> = HashMap::new();
    for b in &selected_boxes {
        for a in &b.assets {
            let amt: u64 = a.amount.parse().unwrap_or(0);
            *change_assets_map.entry(a.token_id.clone()).or_insert(0) += amt;
        }
    }
    let change_assets: Vec<Eip12Asset> = change_assets_map
        .into_iter()
        .map(|(k, v)| Eip12Asset {
            token_id: k,
            amount: v.to_string(),
        })
        .collect();

    let mut outputs = vec![new_state_output, ergopad_output];

    let needs_change_box = !change_assets.is_empty() || change_erg >= MIN_CHANGE_VALUE;
    if needs_change_box {
        if change_erg < MIN_BOX_VALUE {
            return Err(RecoveryError::TxBuildError(format!(
                "Change ERG {} below min box value {} (adjust funding)",
                change_erg, MIN_BOX_VALUE
            )));
        }
        outputs.push(Eip12Output {
            value: change_erg.to_string(),
            ergo_tree: user_ergo_tree.to_string(),
            assets: change_assets,
            creation_height: current_height,
            additional_registers: HashMap::new(),
        });
    } else if change_erg > 0 {
        // Dust ERG below MIN_CHANGE_VALUE but non-zero: fold into miner fee.
        let mut fee = fee_output;
        let new_fee_val = MIN_MINER_FEE + change_erg;
        fee.value = new_fee_val.to_string();
        outputs.push(fee);
        let mut inputs = vec![state_box.clone(), stake_box.clone()];
        inputs.extend(selected_boxes);
        return Ok(Eip12UnsignedTx {
            inputs,
            data_inputs: vec![],
            outputs,
        });
    }

    outputs.push(fee_output);

    let mut inputs = vec![state_box.clone(), stake_box.clone()];
    inputs.extend(selected_boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

fn validate_state_box(b: &Eip12InputBox) -> Result<(), RecoveryError> {
    if b.assets.len() < 2 {
        return Err(RecoveryError::TxBuildError(
            "State box must have 2 tokens".into(),
        ));
    }
    if b.assets[0].token_id != STAKE_STATE_NFT {
        return Err(RecoveryError::TxBuildError(
            "State box token[0] is not the StakeStateNFT".into(),
        ));
    }
    if b.assets[1].token_id != STAKE_TOKEN {
        return Err(RecoveryError::TxBuildError(
            "State box token[1] is not the stake token".into(),
        ));
    }
    Ok(())
}

fn validate_stake_box(b: &Eip12InputBox) -> Result<(), RecoveryError> {
    if b.assets.len() < 2 {
        return Err(RecoveryError::TxBuildError(
            "Stake box must have 2 tokens".into(),
        ));
    }
    if b.assets[0].token_id != STAKE_TOKEN {
        return Err(RecoveryError::TxBuildError(
            "Stake box token[0] is not the stake token".into(),
        ));
    }
    if b.assets[1].token_id != ERGOPAD_TOKEN {
        return Err(RecoveryError::TxBuildError(
            "Stake box token[1] is not ERGOPAD".into(),
        ));
    }
    Ok(())
}

fn encode_long_coll(values: Vec<i64>) -> Result<String, RecoveryError> {
    let items_vec: Vec<Literal> = values.into_iter().map(Literal::Long).collect();
    let items: Arc<[Literal]> = items_vec.into();
    let constant = Constant {
        tpe: SType::SColl(Arc::new(SType::SLong)),
        v: Literal::Coll(CollKind::WrappedColl {
            elem_tpe: SType::SLong,
            items,
        }),
    };
    let bytes = constant
        .sigma_serialize_bytes()
        .map_err(|e| RecoveryError::TxBuildError(format!("R4 serialize: {e}")))?;
    Ok(hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_coll_matches_empirical_r4() {
        // From StakeStateBox input `1449644d…` R4 before unstake:
        // [24832149129, 1095, 1982, 1740595860900, 86400000]
        let hex = encode_long_coll(vec![
            24832149129,
            1095,
            1982,
            1740595860900,
            86400000,
        ])
        .unwrap();
        assert_eq!(hex, "1105929ae481b9018e11fc1ec8d6c8b9a86580f0b252");
    }

    #[test]
    fn long_coll_matches_empirical_r4_after_unstake() {
        // Output state box R4 after 614682 ergopad unstaked:
        // [24831534447, 1095, 1981, 1740595860900, 86400000]
        let hex = encode_long_coll(vec![
            24831534447,
            1095,
            1981,
            1740595860900,
            86400000,
        ])
        .unwrap();
        assert_eq!(hex, "1105de959981b9018e11fa1ec8d6c8b9a86580f0b252");
    }
}
