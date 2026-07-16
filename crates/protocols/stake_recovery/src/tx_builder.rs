use std::collections::HashMap;
use std::sync::Arc;

use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
use ergo_lib::ergotree_ir::mir::value::{CollKind, NativeColl};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::ergotree_ir::types::stype::SType;
use ergo_tx::{select_erg_boxes, Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};

use crate::constants::{
    protocol_by_name, RecoveryMechanism, StakeProtocolConfig, MIN_BOX_VALUE, MIN_CHANGE_VALUE,
    MIN_MINER_FEE, PAIDEIA_EXECUTOR_OUT_VALUE, PAIDEIA_INCENTIVE_ERGO_TREE,
    PAIDEIA_INCENTIVE_VALUE, PAIDEIA_PROXY_ERGO_TREE, PAIDEIA_PROXY_VALUE, PAIDEIA_REFUND_FEE,
};
use crate::state::{RecoverableStake, RecoveryError, StakeStateSnapshot};

/// Total ERG the proxy's unstake branch spends on fixed outputs beyond the (preserved)
/// StakeStateBox value: incentive (0.1) + executor tip (0.002) + miner fee (0.002).
const PAIDEIA_EXECUTOR_FIXED_ERG: u64 =
    PAIDEIA_INCENTIVE_VALUE + PAIDEIA_EXECUTOR_OUT_VALUE + PAIDEIA_EXECUTOR_OUT_VALUE;

/// Build the 3-input unstake tx, mirroring the shape of Ergopad recovery tx
/// `0e1f269f2fe8e75d1c75d6550b4bff13ec43cdde7cb5afc94690e5cc1139e032`. The same
/// tx shape applies to every protocol on the shared template (EGIO verified
/// byte-identical StakeBox/StakeStateBox code bodies).
///
/// Inputs:
///  - \[0\] StakeStateBox
///  - \[1\] matching StakeBox
///  - \[2..\] user P2PK boxes (first one MUST hold the stake key NFT)
///
/// Outputs:
///  - \[0\] new StakeStateBox (R4 updated, stake token amount +1)
///  - \[1\] user reward-token payout (MIN_BOX_VALUE + released reward tokens)
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
    // The owning protocol is carried on the stake (set during discovery), so callers
    // don't pass a separate config: resolve it here and fail loudly if unknown.
    let cfg = protocol_by_name(&stake.protocol).ok_or_else(|| {
        RecoveryError::TxBuildError(format!("Unknown staking protocol '{}'", stake.protocol))
    })?;
    // Safety guard: this builder implements the Ergopad/EGIO *direct* unstake shape
    // (key returned via change). Applying it to a proxy-mechanism pool (Paideia) would
    // produce an invalid tx. Route those to `build_paideia_proxy_tx` instead.
    if cfg.mechanism != RecoveryMechanism::Direct {
        return Err(RecoveryError::TxBuildError(format!(
            "{} uses the {:?} recovery mechanism, not the direct builder — use build_paideia_proxy_tx",
            cfg.name, cfg.mechanism
        )));
    }
    validate_state_box(cfg, state_box)?;
    validate_stake_box(cfg, stake_box)?;

    let state_nft = &state_box.assets[0];
    let state_stake_tok = &state_box.assets[1];

    // --- Build new state box ---
    let new_r4 = encode_long_coll(vec![
        state.total_staked_raw - stake.reward_amount_raw,
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

    // --- User's reward-token payout ---
    let reward_output = Eip12Output::change(
        MIN_BOX_VALUE as i64,
        user_ergo_tree,
        vec![Eip12Asset::new(cfg.reward_token, stake.reward_amount_raw)],
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

    let mut outputs = vec![new_state_output, reward_output];

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

/// Build the Paideia unstake **proxy-creation** tx (step 1 of 2).
///
/// Paideia v1 does not permit a direct unstake. Redemption is mediated by a single-use
/// proxy box at the `101b` contract: the key holder first moves their stake key into a
/// proxy box carrying `R4 = [reward amount]` and `R5 = payout recipient ergotree bytes`,
/// funded with [`PAIDEIA_PROXY_VALUE`]. The proxy is then consumed together with the
/// live StakeStateBox and the matching StakeBox to pay the reward to the recipient and
/// burn the key — a spend that requires **no signature** (verified on-chain: unstake tx
/// `fccb0c49…` spent all three inputs with empty proofs). This function builds only the
/// first tx, the one that spends the user's own wallet and therefore needs their
/// signature; it is the sole key-holder step. Both proxy spend paths pay out to `R5`,
/// including a refund path, so pointing `R5` at the user's own address is safe.
///
/// Inputs:
///  - \[0\] the user box holding the stake key NFT (found by `stake.stake_key_id`)
///  - \[1..\] additional user boxes selected to cover the proxy value + miner fee
///
/// Outputs:
///  - \[0\] proxy box (`101b`): stake key NFT, R4 = \[amount\], R5 = recipient tree
///  - \[1?\] change box (remaining ERG/tokens back to the user)
///  - \[last\] miner fee
///
/// `amount` is the reward the StakeBox currently holds (full unstake). This is exact
/// only when the position is already at the StakeStateBox's current checkpoint (no
/// pending compound); callers should confirm `stake.checkpoint == state.checkpoint`
/// before broadcasting.
pub fn build_paideia_proxy_tx(
    stake: &RecoverableStake,
    user_utxos: &[Eip12InputBox],
    recipient_ergo_tree: &str,
    current_height: i32,
) -> Result<Eip12UnsignedTx, RecoveryError> {
    let cfg = protocol_by_name(&stake.protocol).ok_or_else(|| {
        RecoveryError::TxBuildError(format!("Unknown staking protocol '{}'", stake.protocol))
    })?;
    if cfg.mechanism != RecoveryMechanism::PaideiaProxy {
        return Err(RecoveryError::TxBuildError(format!(
            "{} is not a proxy-mechanism pool — use build_recovery_tx_eip12",
            cfg.name
        )));
    }

    if stake.reward_amount_raw <= 0 {
        return Err(RecoveryError::TxBuildError(
            "StakeBox holds no reward to unstake".into(),
        ));
    }
    if recipient_ergo_tree.trim().is_empty() {
        return Err(RecoveryError::TxBuildError(
            "recipient_ergo_tree must be the payout address' full ErgoTree".into(),
        ));
    }

    let mut regs: HashMap<String, String> = HashMap::new();
    regs.insert(
        "R4".into(),
        encode_long_coll(vec![stake.reward_amount_raw])?,
    );
    regs.insert("R5".into(), encode_bytes_coll(recipient_ergo_tree)?);

    let proxy_out = Eip12Output {
        value: PAIDEIA_PROXY_VALUE.to_string(),
        ergo_tree: PAIDEIA_PROXY_ERGO_TREE.to_string(),
        assets: vec![Eip12Asset::new(&stake.stake_key_id, 1)],
        creation_height: current_height,
        additional_registers: regs,
    };

    // Input selection: the stake-key box first (must carry the key), then top up.
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

    let target = PAIDEIA_PROXY_VALUE + MIN_MINER_FEE;
    let additional_needed = target.saturating_sub(key_erg);

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

    let selected_total: u64 = selected_boxes
        .iter()
        .map(|b| b.value.parse::<u64>().unwrap_or(0))
        .sum();
    let change_erg = selected_total
        .checked_sub(PAIDEIA_PROXY_VALUE + MIN_MINER_FEE)
        .ok_or_else(|| RecoveryError::InsufficientFunds("Not enough ERG for proxy + fee".into()))?;

    // Every input token except the single stake key (which goes to the proxy) flows to
    // the change box back to the signer.
    let mut change_assets_map: HashMap<String, u64> = HashMap::new();
    for b in &selected_boxes {
        for a in &b.assets {
            let amt: u64 = a.amount.parse().unwrap_or(0);
            *change_assets_map.entry(a.token_id.clone()).or_insert(0) += amt;
        }
    }
    let key_left = change_assets_map
        .get(&stake.stake_key_id)
        .copied()
        .unwrap_or(0)
        .saturating_sub(1);
    if key_left == 0 {
        change_assets_map.remove(&stake.stake_key_id);
    } else {
        change_assets_map.insert(stake.stake_key_id.clone(), key_left);
    }
    let change_assets: Vec<Eip12Asset> = change_assets_map
        .into_iter()
        .map(|(k, v)| Eip12Asset {
            token_id: k,
            amount: v.to_string(),
        })
        .collect();

    let mut outputs = vec![proxy_out];
    let needs_change = !change_assets.is_empty() || change_erg >= MIN_CHANGE_VALUE;
    let fee_output = if needs_change {
        if change_erg < MIN_BOX_VALUE {
            return Err(RecoveryError::TxBuildError(format!(
                "Change ERG {} below min box value {} (add funding)",
                change_erg, MIN_BOX_VALUE
            )));
        }
        outputs.push(Eip12Output {
            value: change_erg.to_string(),
            ergo_tree: recipient_ergo_tree.to_string(),
            assets: change_assets,
            creation_height: current_height,
            additional_registers: HashMap::new(),
        });
        Eip12Output::fee(MIN_MINER_FEE as i64, current_height)
    } else {
        // Fold sub-min dust ERG into the fee.
        Eip12Output::fee((MIN_MINER_FEE + change_erg) as i64, current_height)
    };
    outputs.push(fee_output);

    Ok(Eip12UnsignedTx {
        inputs: selected_boxes,
        data_inputs: vec![],
        outputs,
    })
}

/// Build the Paideia **executor** (unstake) tx — step 2 of 2, the permissionless keeper
/// transaction that consumes a step-1 proxy box and pays out the reward.
///
/// This mirrors, byte-for-byte, the shape of the real on-chain unstake tx
/// `fccb0c4979d43295a27af7be0da3aa93db840776979621bee7c13a1b51c3cf97`. It needs **no
/// signature** — on-chain that tx spent all three inputs with empty proofs — so anyone
/// (the key-holder themselves, or any keeper) can assemble and broadcast it. The
/// `executor_ergo_tree` names who collects the 0.002 ERG tip in `OUTPUTS(3)` (its script
/// is unconstrained by the proxy); point it at the key-holder's own address for a
/// self-executed recovery.
///
/// Inputs (**order is load-bearing** — the proxy's unstake branch requires INPUTS(0) to
/// be the StakeStateBox):
///  - \[0\] the live StakeStateBox (`104e`)
///  - \[1\] the matching StakeBox (`101f`), still unspent (step 1 does not consume it)
///  - \[2\] the step-1 proxy box (`101b`) carrying the stake key, R4 = \[amount\],
///    R5 = recipient tree
///
/// Outputs (proxy pins `OUTPUTS.size == 5`):
///  - \[0\] new StakeStateBox: value preserved, stake token +1, R4 transitioned
///  - \[1\] reward payout to `proxy.R5`: reward tokens + the conservation-remainder ERG
///  - \[2\] the fixed `102f` incentive box (0.1 ERG)
///  - \[3\] executor tip (0.002 ERG) to `executor_ergo_tree`
///  - \[4\] miner fee (0.002 ERG)
///
/// `amount` and the recipient are read from the proxy box's own R4/R5, so the caller
/// cannot desynchronise them from what step 1 committed.
pub fn build_paideia_executor_tx(
    state_box: &Eip12InputBox,
    state: &StakeStateSnapshot,
    stake_box: &Eip12InputBox,
    stake: &RecoverableStake,
    proxy_box: &Eip12InputBox,
    executor_ergo_tree: &str,
    current_height: i32,
) -> Result<Eip12UnsignedTx, RecoveryError> {
    let cfg = protocol_by_name(&stake.protocol).ok_or_else(|| {
        RecoveryError::TxBuildError(format!("Unknown staking protocol '{}'", stake.protocol))
    })?;
    if cfg.mechanism != RecoveryMechanism::PaideiaProxy {
        return Err(RecoveryError::TxBuildError(format!(
            "{} is not a proxy-mechanism pool — the executor tx only applies to Paideia",
            cfg.name
        )));
    }

    validate_state_box(cfg, state_box)?;
    validate_stake_box(cfg, stake_box)?;
    if proxy_box.ergo_tree != PAIDEIA_PROXY_ERGO_TREE {
        return Err(RecoveryError::TxBuildError(
            "Proxy input is not at the Paideia unstake-proxy contract".into(),
        ));
    }

    // The proxy carries the authoritative unstake amount (R4[0]) and payout tree (R5).
    let amount = decode_first_long_reg(proxy_box, "R4")?;
    if amount <= 0 {
        return Err(RecoveryError::TxBuildError(
            "Proxy R4 amount must be positive".into(),
        ));
    }
    let recipient_ergo_tree = decode_bytes_coll_reg(proxy_box, "R5")?;

    // The StakeBox must currently hold exactly `amount` reward tokens: the unstake burns
    // the key, returns the stake token to the state box, and pays `amount` reward tokens
    // out. A mismatch (e.g. a compound happened after step 1) means the StakeBox and the
    // proxy have desynced and the executor tx would be rejected — refund instead.
    let stake_reward_amount: u64 = stake_box.assets[1]
        .amount
        .parse()
        .map_err(|_| RecoveryError::TxBuildError("Invalid StakeBox reward amount".into()))?;
    if stake_reward_amount != amount as u64 {
        return Err(RecoveryError::TxBuildError(format!(
            "StakeBox holds {} reward tokens but proxy R4 declares {} — position changed since step 1; refund the proxy instead",
            stake_reward_amount, amount
        )));
    }
    // The unstake spend requires the StakeBox to be at the state's current checkpoint;
    // a stale StakeBox (checkpoint behind) would be rejected by the StakeBox script.
    if stake.checkpoint != state.checkpoint {
        return Err(RecoveryError::TxBuildError(format!(
            "StakeBox checkpoint {} != StakeStateBox checkpoint {} — the position must be compounded to the current checkpoint before it can be unstaked; refund the proxy instead",
            stake.checkpoint, state.checkpoint
        )));
    }

    let state_nft = &state_box.assets[0];
    let state_stake_tok = &state_box.assets[1];

    // --- Output[0]: new StakeStateBox (same transition as the direct builder) ---
    let new_r4 = encode_long_coll(vec![
        state.total_staked_raw - amount,
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
    let proxy_value_nano: u64 = proxy_box
        .value
        .parse()
        .map_err(|_| RecoveryError::TxBuildError("Invalid proxy box value".into()))?;
    let old_stake_token_amount: u64 = state_stake_tok
        .amount
        .parse()
        .map_err(|_| RecoveryError::TxBuildError("Invalid stake token amount".into()))?;

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
                amount: (old_stake_token_amount + 1).to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: new_registers,
    };

    // --- ERG conservation → reward box value is the remainder ---
    // in = state + stake + proxy; fixed outs = state(preserved) + incentive + tip + fee.
    let inputs_total = state_value_nano
        .checked_add(stake_value_nano)
        .and_then(|s| s.checked_add(proxy_value_nano))
        .ok_or_else(|| RecoveryError::TxBuildError("ERG overflow".into()))?;
    let fixed_out = state_value_nano
        .checked_add(PAIDEIA_EXECUTOR_FIXED_ERG)
        .ok_or_else(|| RecoveryError::TxBuildError("ERG overflow".into()))?;
    let reward_value = inputs_total.checked_sub(fixed_out).ok_or_else(|| {
        RecoveryError::TxBuildError("Proxy underfunded for the unstake outputs".into())
    })?;
    if reward_value < MIN_BOX_VALUE {
        return Err(RecoveryError::TxBuildError(format!(
            "Reward box ERG {} below min box value {} — proxy underfunded (need >= {} ERG)",
            reward_value,
            MIN_BOX_VALUE,
            PAIDEIA_EXECUTOR_FIXED_ERG + MIN_BOX_VALUE
        )));
    }

    // --- Output[1]: reward payout to the R5 recipient ---
    let reward_output = Eip12Output {
        value: reward_value.to_string(),
        ergo_tree: recipient_ergo_tree,
        assets: vec![Eip12Asset::new(cfg.reward_token, amount)],
        creation_height: current_height,
        additional_registers: std::collections::HashMap::new(),
    };

    // --- Output[2]: fixed 102f incentive box ---
    let incentive_output = Eip12Output {
        value: PAIDEIA_INCENTIVE_VALUE.to_string(),
        ergo_tree: PAIDEIA_INCENTIVE_ERGO_TREE.to_string(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: std::collections::HashMap::new(),
    };

    // --- Output[3]: executor tip (script free; the keeper's own address) ---
    let tip_output = Eip12Output {
        value: PAIDEIA_EXECUTOR_OUT_VALUE.to_string(),
        ergo_tree: executor_ergo_tree.to_string(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: std::collections::HashMap::new(),
    };

    // --- Output[4]: miner fee (fixed value pinned by the proxy) ---
    let fee_output = Eip12Output::fee(PAIDEIA_EXECUTOR_OUT_VALUE as i64, current_height);

    Ok(Eip12UnsignedTx {
        inputs: vec![state_box.clone(), stake_box.clone(), proxy_box.clone()],
        data_inputs: vec![],
        outputs: vec![
            new_state_output,
            reward_output,
            incentive_output,
            tip_output,
            fee_output,
        ],
    })
}

/// Build the Paideia proxy **refund** tx — the safety net for step 1.
///
/// If step 2 never runs, the proxy box is not a one-way trap: its `101b` contract has a
/// second, equally permissionless spend path (INPUTS(0) is *not* the StakeStateBox) that
/// returns the box's entire contents — including the stake-key NFT — to the box's own R5
/// recipient, minus a 0.001 ERG miner fee. No signature is required. Verified byte-for-
/// byte against real on-chain refunds (e.g.
/// `72e33dd7344b76cd3cd1a3721f126b3e6dc2f24620ed486af9eceabbbe690c7f`).
///
/// Inputs:  \[0\] the proxy box (`101b`).
/// Outputs: \[0\] recipient box (value = proxy.value − 0.001 ERG, carries the stake key
/// NFT, script = proxy.R5);  \[1\] miner fee (0.001 ERG). `OUTPUTS.size` must be 2.
pub fn build_paideia_refund_tx(
    proxy_box: &Eip12InputBox,
    current_height: i32,
) -> Result<Eip12UnsignedTx, RecoveryError> {
    if proxy_box.ergo_tree != PAIDEIA_PROXY_ERGO_TREE {
        return Err(RecoveryError::TxBuildError(
            "Refund input is not at the Paideia unstake-proxy contract".into(),
        ));
    }
    let recipient_ergo_tree = decode_bytes_coll_reg(proxy_box, "R5")?;
    let proxy_value_nano: u64 = proxy_box
        .value
        .parse()
        .map_err(|_| RecoveryError::TxBuildError("Invalid proxy box value".into()))?;
    let refund_value = proxy_value_nano
        .checked_sub(PAIDEIA_REFUND_FEE)
        .ok_or_else(|| RecoveryError::TxBuildError("Proxy value below the refund fee".into()))?;
    if refund_value < MIN_BOX_VALUE {
        return Err(RecoveryError::TxBuildError(format!(
            "Refund box ERG {} below min box value {}",
            refund_value, MIN_BOX_VALUE
        )));
    }

    // The refund returns *all* of the proxy's tokens (the stake key NFT) to the recipient.
    let refund_assets: Vec<Eip12Asset> = proxy_box
        .assets
        .iter()
        .map(|a| Eip12Asset {
            token_id: a.token_id.clone(),
            amount: a.amount.clone(),
        })
        .collect();

    let refund_output = Eip12Output {
        value: refund_value.to_string(),
        ergo_tree: recipient_ergo_tree,
        assets: refund_assets,
        creation_height: current_height,
        additional_registers: std::collections::HashMap::new(),
    };
    let fee_output = Eip12Output::fee(PAIDEIA_REFUND_FEE as i64, current_height);

    Ok(Eip12UnsignedTx {
        inputs: vec![proxy_box.clone()],
        data_inputs: vec![],
        outputs: vec![refund_output, fee_output],
    })
}

/// Decode the first `SLong` of a `Coll[Long]` register (serialized hex) on an input box.
fn decode_first_long_reg(b: &Eip12InputBox, reg: &str) -> Result<i64, RecoveryError> {
    let hex_val = b
        .additional_registers
        .get(reg)
        .ok_or_else(|| RecoveryError::TxBuildError(format!("Proxy box missing register {reg}")))?;
    let raw = hex::decode(hex_val.trim())
        .map_err(|e| RecoveryError::TxBuildError(format!("{reg} not hex: {e}")))?;
    let constant = Constant::sigma_parse_bytes(&raw)
        .map_err(|e| RecoveryError::TxBuildError(format!("{reg} parse: {e}")))?;
    match &constant.v {
        Literal::Coll(CollKind::WrappedColl {
            elem_tpe: SType::SLong,
            items,
        }) => match items.iter().next() {
            Some(Literal::Long(v)) => Ok(*v),
            _ => Err(RecoveryError::TxBuildError(format!(
                "{reg} is an empty Coll[Long]"
            ))),
        },
        other => Err(RecoveryError::TxBuildError(format!(
            "{reg} is not Coll[Long]: {other:?}"
        ))),
    }
}

/// Decode a `Coll[Byte]` register (serialized hex) to the raw bytes hex (e.g. an
/// ErgoTree stored in the proxy R5).
fn decode_bytes_coll_reg(b: &Eip12InputBox, reg: &str) -> Result<String, RecoveryError> {
    let hex_val = b
        .additional_registers
        .get(reg)
        .ok_or_else(|| RecoveryError::TxBuildError(format!("Proxy box missing register {reg}")))?;
    let raw = hex::decode(hex_val.trim())
        .map_err(|e| RecoveryError::TxBuildError(format!("{reg} not hex: {e}")))?;
    let constant = Constant::sigma_parse_bytes(&raw)
        .map_err(|e| RecoveryError::TxBuildError(format!("{reg} parse: {e}")))?;
    match &constant.v {
        Literal::Coll(CollKind::NativeColl(NativeColl::CollByte(bytes))) => Ok(hex::encode(
            bytes.iter().map(|&x| x as u8).collect::<Vec<u8>>(),
        )),
        other => Err(RecoveryError::TxBuildError(format!(
            "{reg} is not Coll[Byte]: {other:?}"
        ))),
    }
}

/// Serialize `bytes` (hex) as a sigma `Coll[Byte]` constant (the encoding used for a
/// StakeBox/proxy R5 that stores raw ErgoTree bytes).
fn encode_bytes_coll(hex_bytes: &str) -> Result<String, RecoveryError> {
    let raw = hex::decode(hex_bytes.trim())
        .map_err(|e| RecoveryError::TxBuildError(format!("recipient tree not hex: {e}")))?;
    let bytes_i8: Arc<[i8]> = raw.iter().map(|b| *b as i8).collect::<Vec<_>>().into();
    let constant = Constant {
        tpe: SType::SColl(Arc::new(SType::SByte)),
        v: Literal::Coll(CollKind::NativeColl(NativeColl::CollByte(bytes_i8))),
    };
    let bytes = constant
        .sigma_serialize_bytes()
        .map_err(|e| RecoveryError::TxBuildError(format!("R5 serialize: {e}")))?;
    Ok(hex::encode(bytes))
}

fn validate_state_box(cfg: &StakeProtocolConfig, b: &Eip12InputBox) -> Result<(), RecoveryError> {
    if b.assets.len() < 2 {
        return Err(RecoveryError::TxBuildError(
            "State box must have 2 tokens".into(),
        ));
    }
    if b.assets[0].token_id != cfg.stake_state_nft {
        return Err(RecoveryError::TxBuildError(
            "State box token[0] is not the StakeStateNFT".into(),
        ));
    }
    if b.assets[1].token_id != cfg.stake_token {
        return Err(RecoveryError::TxBuildError(
            "State box token[1] is not the stake token".into(),
        ));
    }
    Ok(())
}

fn validate_stake_box(cfg: &StakeProtocolConfig, b: &Eip12InputBox) -> Result<(), RecoveryError> {
    if b.assets.len() < 2 {
        return Err(RecoveryError::TxBuildError(
            "Stake box must have 2 tokens".into(),
        ));
    }
    if b.assets[0].token_id != cfg.stake_token {
        return Err(RecoveryError::TxBuildError(
            "Stake box token[0] is not the stake token".into(),
        ));
    }
    if b.assets[1].token_id != cfg.reward_token {
        return Err(RecoveryError::TxBuildError(format!(
            "Stake box token[1] is not {}",
            cfg.reward_token_name
        )));
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
    use std::collections::HashMap;

    // ----- helpers -----

    /// Build an `Eip12InputBox` from the fields the builders read. `regs` are
    /// register-name → serialized-hex pairs, matching the on-chain `serializedValue`.
    fn mk_input(
        value: u64,
        ergo_tree: &str,
        assets: &[(&str, &str)],
        regs: &[(&str, &str)],
    ) -> Eip12InputBox {
        Eip12InputBox {
            box_id: String::new(),
            transaction_id: String::new(),
            index: 0,
            value: value.to_string(),
            ergo_tree: ergo_tree.to_string(),
            assets: assets
                .iter()
                .map(|(id, amt)| Eip12Asset {
                    token_id: (*id).to_string(),
                    amount: (*amt).to_string(),
                })
                .collect(),
            creation_height: 0,
            additional_registers: regs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            extension: HashMap::new(),
        }
    }

    // On-chain ErgoTrees involved in the Paideia unstake (from tx `fccb0c49…`).
    const PAIDEIA_STATE_TREE: &str = "104e0402040004020400040204000400040204020404040404060406040004000408040404080e2046e552a63e456cc20deaa645daea712b56f04c196bb01faeb5343ef4162d8dc90e201fd6e032e8476c4aa54c18c1a308dce83940e8f4a28f576440513ed7326ad4890500040204000406040404020404040005020502040004020580dddb01050205d00f040205020402040204020404040004000502040404000402050201000e2093cda90b4fe24f075d7961fa0d1d662fdc7e1349d313059b9618eecb16c5eade0404050204000e2012bbef36eaa5e61b64d519196a1e8ebea360f18aba9b02d2a21b16f26208960f040205020404050001000402040204000500040404000502050005020500050004000402040004020402040205d00f0100d829d601b2a5730000d602db63087201d603b27202730100d6048c720301d605db6308a7d606b27205730200d6078c720601d6089372047207d609b2a5730300d60adb63087209d60bb2720a730400d60c8c720b02d60d8c720602d60ee4c6a70411d60fb2720e730500d610e4c672090411d611b27210730600d612b27210730700d613b2720e730800d614b2720e730900d615b27210730a00d616b27210730b00d617b2720e730c00d618b2720a730d00d619b27205730e00d61ab2720e730f00d61b9683070193c17209c1a793c27209c2a7938c7218018c721901938c7218028c721902938c720b01720793b1720a731093b27210731100721ad61c7312d61ddb6903db6503fed61e8c720302d61f7313d62086028300027314d621b2a4731500d622db63087221d623b27222731600d6248c722301d62592b1a47317d626e4c672210411d627e4c67221050ed628b2a5731800d629db63087228d1ecec957208958f720c720dd806d62ab27202731900d62b8c722a02d62ce4c672010411d62de4c67201050ed62eb2a5731a00d62fb2db6308722e731b009683030196830601721b9372119a720f722b93721272139372159a7214731c937216721793720c99720d731d9683080193cbc27201721c93b2722c731e00721392b2722c731f0099721d732093722dc5a7720893721e7321938c722a01721f92722b73229683030193c2722ee4c6b2a4732300050e938c722f01722d938c722f027324d807d62ab27202732500d62b8c722a02d62cb2a4732600d62d8cb2db6308722c732701722002d62ee4c672010411d62fe4c67201050ed630b2db6308b2a57328007329009683030196830601721b9372119a720f99722b722d93721272139372157214937216721793720c720d96830a0193c17201c1722c93cbc27201721c93cbc2722c721c93b2722e732a00721393722ee4c6722c041193722fe4c6722c050e720893721e732b938c722a01721f93722b9a722d8cb2db6308b2a4732c01b2a4732d00732e000296830201938c723001722f938c723002732f7330959683020193722473317225d802d62ab2a4733200d62be4c6722a04119683020196830601721b9372129a7213733393721572149372169a7217721a8f7216721d93720c720d96830301938cb2db6308722a73340001733593b2722b733600997213733793b2722b7338007339733a959683030191720f7211722591b17222733bd807d62ab27222733c00d62b99720f7211d62c998c722a02722bd62d8c722a01d62eb27226733d00d62f90722c733ed6309683040196830201937224720793722e7213938cb2db6308b2a4733f0073400001722796830601721b93721199720f722b937212721393721599721495722f73417342937216721793720c9a720d95722f7343734496830201937204722d93721e722b9591722c7345d803d631b272297346017220d632b27229734700d633e4c672280411968303017230968302019683080193c17228c1722193c27228c27221938c7231017224938c7231028c722302938c723201722d938c723202722c93b27233734800722e93b27233734900b27226734a00938cb27202734b01722001722792722c734c7230734d";
    const PAIDEIA_STAKE_TREE: &str = "101f040004000e2012bbef36eaa5e61b64d519196a1e8ebea360f18aba9b02d2a21b16f26208960f040204000400040001000e20b682ad9e8c56c5a0ba7fe2d3d9b2fbd40af989e8870628f4a03ae1022d36f0910402040004000402040204000400050204020402040604000100040404020402010001010100040201000100d807d601b2a4730000d6028cb2db6308720173010001d6039372027302d604e4c6a70411d605e4c6a7050ed60695ef7203ed93c5b2a4730300c5a78fb2e4c6b2a57304000411730500b2e4c6720104117306007307d6079372027308d1ecec957203d80ad608b2a5dc0c1aa402a7730900d609e4c672080411d60adb63087208d60bb2720a730a00d60cdb6308a7d60db2720c730b00d60eb2720a730c00d60fb2720c730d00d6107e8c720f0206d611e4c6720104119683090193c17208c1a793c27208c2a793b27209730e009ab27204730f00731093e4c67208050e720593b27209731100b27204731200938c720b018c720d01938c720b028c720d02938c720e018c720f01937e8c720e02069a72109d9c7eb272117313000672107eb27211731400067315957206d801d608b2a5731600ed72079593c27208c2a7d801d609c67208050e95e67209ed93e472097205938cb2db6308b2a57317007318000172057319731a731b9595efec7206720393c5b2a4731c00c5a7731d7207731e";

    // ----- oracle parity -----
    // Expected bytes below are the *actual* serialized R4 register values pulled from
    // real mainnet boxes (via the Ergo explorer `serializedValue` field), never a
    // self-oracle. If `encode_long_coll` diverges from the reference Scala/sigma
    // serialization these will fail.

    // Ergopad StakeStateBox `1449644d…` R4 before unstake:
    // [24832149129, 1095, 1982, 1740595860900, 86400000]
    #[test]
    fn ergopad_state_r4_long_coll_matches_chain_bytes() {
        let hex = encode_long_coll(vec![24832149129, 1095, 1982, 1740595860900, 86400000]).unwrap();
        assert_eq!(hex, "1105929ae481b9018e11fc1ec8d6c8b9a86580f0b252");
    }

    // Ergopad output StakeStateBox R4 after 614682 ergopad unstaked:
    // [24831534447, 1095, 1981, 1740595860900, 86400000]
    #[test]
    fn ergopad_state_r4_after_unstake_matches_chain_bytes() {
        let hex = encode_long_coll(vec![24831534447, 1095, 1981, 1740595860900, 86400000]).unwrap();
        assert_eq!(hex, "1105de959981b9018e11fa1ec8d6c8b9a86580f0b252");
    }

    // EGIO StakeStateBox `4cdefb4b…` R4 (box created in the stake-key mint tx
    // cc962070…): [192944520258, 0, 132, 1656050400000, 86400000].
    #[test]
    fn egio_state_r4_long_coll_matches_chain_bytes() {
        let hex = encode_long_coll(vec![192944520258, 0, 132, 1656050400000, 86400000]).unwrap();
        assert_eq!(hex, "110584f19dc69d0b00880280ecddc4b26080f0b252");
    }

    // EGIO current live StakeStateBox `fcce80f9…` R4:
    // [57097976471, 1, 10, 1656136800000, 86400000].
    #[test]
    fn egio_live_state_r4_long_coll_matches_chain_bytes() {
        let hex = encode_long_coll(vec![57097976471, 1, 10, 1656136800000, 86400000]).unwrap();
        assert_eq!(hex, "1105aeeaefb4a903021480dc9097b36080f0b252");
    }

    // EGIO StakeBox `670a65ae…` R4 (2-long StakeBox layout [checkpoint, stakeTimeMs]):
    // [0, 1656092631524].
    #[test]
    fn egio_stake_box_r4_long_coll_matches_chain_bytes() {
        let hex = encode_long_coll(vec![0, 1656092631524]).unwrap();
        assert_eq!(hex, "110200c88781edb260");
    }

    // --- Paideia proxy oracle parity ---
    // All expected bytes below are the *actual* serialized registers pulled from the
    // real Paideia unstake tx
    // `fccb0c4979d43295a27af7be0da3aa93db840776979621bee7c13a1b51c3cf97`
    // (input[2] = the `101b` unstake proxy, input[0]/output[0] = StakeStateBox).

    // Proxy R4 = [amount to unstake] = the StakeBox's full reward (850495800 = 85049.58
    // PAIDEIA). On-chain proxy R4 bytes: `1101f0a48cab06`.
    #[test]
    fn paideia_proxy_r4_amount_matches_chain_bytes() {
        let hex = encode_long_coll(vec![850_495_800]).unwrap();
        assert_eq!(hex, "1101f0a48cab06");
    }

    // Proxy R5 = Coll[Byte] of the payout recipient's full P2PK ErgoTree bytes.
    // On-chain: recipient tree `0008cd031980055487caf2695d09a69ca59945e5c376ee3826b61e7526b3ba20678c4ff6`
    // serializes to R5 `0e24` + those 36 bytes.
    #[test]
    fn paideia_proxy_r5_recipient_tree_matches_chain_bytes() {
        let recipient = "0008cd031980055487caf2695d09a69ca59945e5c376ee3826b61e7526b3ba20678c4ff6";
        let hex = encode_bytes_coll(recipient).unwrap();
        assert_eq!(
            hex,
            "0e240008cd031980055487caf2695d09a69ca59945e5c376ee3826b61e7526b3ba20678c4ff6"
        );
    }

    // The unstake state transition the executing tx must produce, verified against the
    // real StakeStateBox R4 before/after. in[0] R4 = [509068736638, 790, 474,
    // 1722513600000, 86400000]; out[0] R4 = [total-reward, checkpoint, stakers-1, ts,
    // cycle] with reward = 850495800.
    #[test]
    fn paideia_state_r4_unstake_transition_matches_chain_bytes() {
        let before =
            encode_long_coll(vec![509068736638, 790, 474, 1722513600000, 86400000]).unwrap();
        assert_eq!(before, "1105fce1e3edd01dac0cb40780b8fddca16480f0b252");
        let after = encode_long_coll(vec![
            509068736638 - 850495800,
            790,
            474 - 1,
            1722513600000,
            86400000,
        ])
        .unwrap();
        assert_eq!(after, "11058cbdd7c2ca1dac0cb20780b8fddca16480f0b252");
    }

    // Whole-tx byte-exact reconstruction of the real Paideia unstake (step 2)
    // `fccb0c4979d43295a27af7be0da3aa93db840776979621bee7c13a1b51c3cf97`. We feed the
    // three real historical input boxes to `build_paideia_executor_tx` and assert every
    // output field (value / script / assets / registers) equals what that accepted
    // on-chain tx produced — a real external oracle, not a self-oracle.
    #[test]
    fn paideia_executor_tx_reconstructs_chain_unstake_byte_exact() {
        let state_nft = "b682ad9e8c56c5a0ba7fe2d3d9b2fbd40af989e8870628f4a03ae1022d36f091";
        let stake_token = "245957934c20285ada547aa8f2c8e6f7637be86a1985b3e4c36e4e1ad8ce97ab";
        let reward_token = "1fd6e032e8476c4aa54c18c1a308dce83940e8f4a28f576440513ed7326ad489";
        let stake_key = "83ecb7dd522bd78a0132dbd0fc329f00aa721283ea15451f31242b95aed91f1e";
        // proxy R5 payout recipient (full P2PK ErgoTree).
        let recipient = "0008cd031980055487caf2695d09a69ca59945e5c376ee3826b61e7526b3ba20678c4ff6";
        let executor_tip_tree =
            "0008cd03553448c194fdd843c87d080f5e8ed983f5bb2807b13b45a9683bba8c7bfb5ae8";
        let height = 1399858;

        let state_box = mk_input(
            1_000_000,
            PAIDEIA_STATE_TREE,
            &[(state_nft, "1"), (stake_token, "999999999526")],
            &[("R4", "1105fce1e3edd01dac0cb40780b8fddca16480f0b252")],
        );
        let stake_box = mk_input(
            1_000_000,
            PAIDEIA_STAKE_TREE,
            &[(stake_token, "1"), (reward_token, "850495800")],
            &[
                ("R4", "1102ac0c98e6b793ac60"),
                (
                    "R5",
                    "0e2083ecb7dd522bd78a0132dbd0fc329f00aa721283ea15451f31242b95aed91f1e",
                ),
            ],
        );
        let proxy_box = mk_input(
            113_000_000,
            PAIDEIA_PROXY_ERGO_TREE,
            &[(stake_key, "1")],
            &[
                ("R4", "1101f0a48cab06"),
                (
                    "R5",
                    "0e240008cd031980055487caf2695d09a69ca59945e5c376ee3826b61e7526b3ba20678c4ff6",
                ),
            ],
        );

        let state = StakeStateSnapshot {
            protocol: "Paideia".into(),
            state_box_id: String::new(),
            state_box_value_nano: 1_000_000,
            total_staked_raw: 509_068_736_638,
            checkpoint: 790,
            num_stakers: 474,
            last_checkpoint_ts: 1_722_513_600_000,
            cycle_duration_ms: 86_400_000,
            stake_token_amount: 999_999_999_526,
        };
        let stake = RecoverableStake {
            protocol: "Paideia".into(),
            reward_token_name: "PAIDEIA".into(),
            stake_key_id: stake_key.into(),
            stake_box_id: String::new(),
            stake_box_value_nano: 1_000_000,
            reward_amount_raw: 850_495_800,
            checkpoint: 790,
            stake_time_ms: 0,
            reward_amount_display: String::new(),
        };

        let tx = build_paideia_executor_tx(
            &state_box,
            &state,
            &stake_box,
            &stake,
            &proxy_box,
            executor_tip_tree,
            height,
        )
        .unwrap();

        // Inputs: exactly [state, stake, proxy] in that order.
        assert_eq!(tx.inputs.len(), 3);
        assert_eq!(tx.inputs[0].ergo_tree, PAIDEIA_STATE_TREE);
        assert_eq!(tx.inputs[1].ergo_tree, PAIDEIA_STAKE_TREE);
        assert_eq!(tx.inputs[2].ergo_tree, PAIDEIA_PROXY_ERGO_TREE);

        // Five outputs, byte-exact to the on-chain tx.
        assert_eq!(tx.outputs.len(), 5);

        // OUT[0]: new StakeStateBox.
        let o0 = &tx.outputs[0];
        assert_eq!(o0.value, "1000000");
        assert_eq!(o0.ergo_tree, PAIDEIA_STATE_TREE);
        assert_eq!(o0.assets.len(), 2);
        assert_eq!(o0.assets[0].token_id, state_nft);
        assert_eq!(o0.assets[0].amount, "1");
        assert_eq!(o0.assets[1].token_id, stake_token);
        assert_eq!(o0.assets[1].amount, "999999999527");
        assert_eq!(
            o0.additional_registers.get("R4").unwrap(),
            "11058cbdd7c2ca1dac0cb20780b8fddca16480f0b252"
        );

        // OUT[1]: reward payout to the recipient (ERG = conservation remainder = 0.01).
        let o1 = &tx.outputs[1];
        assert_eq!(o1.value, "10000000");
        assert_eq!(o1.ergo_tree, recipient);
        assert_eq!(o1.assets.len(), 1);
        assert_eq!(o1.assets[0].token_id, reward_token);
        assert_eq!(o1.assets[0].amount, "850495800");
        assert!(o1.additional_registers.is_empty());

        // OUT[2]: fixed 102f incentive box (0.1 ERG, no tokens/regs).
        let o2 = &tx.outputs[2];
        assert_eq!(o2.value, "100000000");
        assert_eq!(o2.ergo_tree, PAIDEIA_INCENTIVE_ERGO_TREE);
        assert!(o2.assets.is_empty());
        assert!(o2.additional_registers.is_empty());

        // OUT[3]: executor tip (0.002 ERG) to the executor's own address.
        let o3 = &tx.outputs[3];
        assert_eq!(o3.value, "2000000");
        assert_eq!(o3.ergo_tree, executor_tip_tree);
        assert!(o3.assets.is_empty());

        // OUT[4]: miner fee (0.002 ERG).
        let o4 = &tx.outputs[4];
        assert_eq!(o4.value, "2000000");
        assert_eq!(o4.ergo_tree, citadel_core::constants::MINER_FEE_ERGO_TREE);
        assert!(o4.assets.is_empty());
    }

    // Whole-tx byte-exact reconstruction of a real permissionless proxy *refund*
    // `72e33dd7344b76cd3cd1a3721f126b3e6dc2f24620ed486af9eceabbbe690c7f`. Proves the
    // safety net: with no signature, the proxy's contents (including the stake-key NFT)
    // return to its R5 recipient minus a 0.001 ERG fee.
    #[test]
    fn paideia_refund_tx_reconstructs_chain_refund_byte_exact() {
        let key = "7cab4c64ceb9573a8701fefb79c1bace5bb57ad5ce596968278c631abff930b0";
        let recipient = "0008cd02ffa5b67e85e164c76cd8c8fce547bafa6a1019edae968ae988bb4aeca75dbd9b";
        let height = 1526007;

        let proxy_box = mk_input(
            113_000_000,
            PAIDEIA_PROXY_ERGO_TREE,
            &[(key, "1")],
            &[
                ("R4", "1101ceb5c0ba05"),
                (
                    "R5",
                    "0e240008cd02ffa5b67e85e164c76cd8c8fce547bafa6a1019edae968ae988bb4aeca75dbd9b",
                ),
            ],
        );

        let tx = build_paideia_refund_tx(&proxy_box, height).unwrap();

        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.inputs[0].ergo_tree, PAIDEIA_PROXY_ERGO_TREE);
        assert_eq!(tx.outputs.len(), 2);

        // OUT[0]: recipient box — key returned, value = proxy - 0.001 ERG fee.
        let o0 = &tx.outputs[0];
        assert_eq!(o0.value, "112000000");
        assert_eq!(o0.ergo_tree, recipient);
        assert_eq!(o0.assets.len(), 1);
        assert_eq!(o0.assets[0].token_id, key);
        assert_eq!(o0.assets[0].amount, "1");
        assert!(o0.additional_registers.is_empty());

        // OUT[1]: miner fee (0.001 ERG).
        let o1 = &tx.outputs[1];
        assert_eq!(o1.value, "1000000");
        assert_eq!(o1.ergo_tree, citadel_core::constants::MINER_FEE_ERGO_TREE);
        assert!(o1.assets.is_empty());
    }
}
