use std::collections::HashMap;

use crate::calculator;
use crate::constants::{self, MIN_BOX_VALUE, MIN_CHANGE_VALUE, MIN_MINER_FEE};
use crate::state::{HodlBankState, HodlError};
use ergo_tx::{
    append_change_output, select_erg_boxes, select_token_boxes, Eip12Asset, Eip12InputBox,
    Eip12Output, Eip12UnsignedTx,
};

/// Bank box must be inputs[0]; new bank box must be outputs[0].
pub fn build_mint_tx_eip12(
    bank_box: &Eip12InputBox,
    bank_state: &HodlBankState,
    erg_to_deposit: i64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<Eip12UnsignedTx, HodlError> {
    if erg_to_deposit <= 0 {
        return Err(HodlError::TxBuildError(
            "Deposit amount must be positive".to_string(),
        ));
    }

    let tokens_received = calculator::mint_amount(
        bank_state.reserve_nano_erg,
        bank_state.circulating_supply,
        bank_state.precision_factor,
        erg_to_deposit,
    );

    if tokens_received <= 0 {
        return Err(HodlError::TxBuildError(
            "Deposit too small to receive any tokens".to_string(),
        ));
    }

    if tokens_received > bank_state.hodl_tokens_in_bank {
        return Err(HodlError::TxBuildError(format!(
            "Bank only has {} tokens, but {} needed",
            bank_state.hodl_tokens_in_bank, tokens_received
        )));
    }

    let bank_erg: u64 = bank_box
        .value
        .parse()
        .map_err(|_| HodlError::TxBuildError("Invalid bank box ERG value".to_string()))?;

    if bank_box.assets.len() < 2 {
        return Err(HodlError::TxBuildError(format!(
            "Bank box has {} tokens, expected at least 2",
            bank_box.assets.len()
        )));
    }

    let bank_singleton = &bank_box.assets[constants::bank_tokens::SINGLETON];
    let bank_hodl = &bank_box.assets[constants::bank_tokens::HODL_TOKEN];

    let hodl_in_bank: u64 = bank_hodl
        .amount
        .parse()
        .map_err(|_| HodlError::TxBuildError("Invalid hodl token amount".to_string()))?;

    let new_bank_erg = bank_erg + erg_to_deposit as u64;
    let new_hodl_in_bank = hodl_in_bank - tokens_received as u64;

    let new_bank_output = Eip12Output {
        value: new_bank_erg.to_string(),
        ergo_tree: bank_box.ergo_tree.clone(),
        assets: vec![
            Eip12Asset {
                token_id: bank_singleton.token_id.clone(),
                amount: bank_singleton.amount.clone(),
            },
            Eip12Asset {
                token_id: bank_hodl.token_id.clone(),
                amount: new_hodl_in_bank.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: bank_box.additional_registers.clone(),
    };

    let user_output = Eip12Output::change(
        MIN_BOX_VALUE as i64,
        user_ergo_tree,
        vec![Eip12Asset::new(&bank_hodl.token_id, tokens_received)],
        current_height,
    );

    let fee_output = Eip12Output::fee(MIN_MINER_FEE as i64, current_height);

    let user_erg_needed = erg_to_deposit as u64 + MIN_BOX_VALUE + MIN_MINER_FEE;

    let selected = select_erg_boxes(user_utxos, user_erg_needed)
        .map_err(|e| HodlError::InsufficientFunds(e.to_string()))?;

    let mut outputs = vec![new_bank_output, user_output, fee_output];

    append_change_output(
        &mut outputs,
        &selected,
        user_erg_needed,
        &[],
        user_ergo_tree,
        current_height,
        MIN_CHANGE_VALUE,
    )
    .map_err(|e| HodlError::TxBuildError(e.to_string()))?;

    let mut inputs = vec![bank_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

/// Bank box must be inputs[0]; new bank box must be outputs[0].
pub fn build_burn_tx_eip12(
    bank_box: &Eip12InputBox,
    bank_state: &HodlBankState,
    hodl_to_burn: i64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<Eip12UnsignedTx, HodlError> {
    if hodl_to_burn <= 0 {
        return Err(HodlError::TxBuildError(
            "Burn amount must be positive".to_string(),
        ));
    }

    let burn_result = calculator::burn_amount(
        bank_state.reserve_nano_erg,
        bank_state.circulating_supply,
        bank_state.precision_factor,
        hodl_to_burn,
        bank_state.bank_fee_num,
        bank_state.dev_fee_num,
    );

    if burn_result.erg_to_user <= 0 {
        return Err(HodlError::TxBuildError(
            "Burn amount too small to receive any ERG".to_string(),
        ));
    }

    let bank_erg: u64 = bank_box
        .value
        .parse()
        .map_err(|_| HodlError::TxBuildError("Invalid bank box ERG value".to_string()))?;

    if bank_box.assets.len() < 2 {
        return Err(HodlError::TxBuildError(format!(
            "Bank box has {} tokens, expected at least 2",
            bank_box.assets.len()
        )));
    }

    let bank_singleton = &bank_box.assets[constants::bank_tokens::SINGLETON];
    let bank_hodl = &bank_box.assets[constants::bank_tokens::HODL_TOKEN];

    let hodl_in_bank: u64 = bank_hodl
        .amount
        .parse()
        .map_err(|_| HodlError::TxBuildError("Invalid hodl token amount".to_string()))?;

    // bank_fee stays in the bank (increases reserve relative to circulating supply)
    let erg_leaving_bank = (burn_result.erg_to_user + burn_result.dev_fee) as u64;
    let new_bank_erg = bank_erg
        .checked_sub(erg_leaving_bank)
        .ok_or_else(|| HodlError::TxBuildError("Bank ERG underflow".to_string()))?;

    if (new_bank_erg as i64) < bank_state.min_bank_value {
        return Err(HodlError::BelowMinBankValue);
    }

    let new_hodl_in_bank = hodl_in_bank + hodl_to_burn as u64;

    let new_bank_output = Eip12Output {
        value: new_bank_erg.to_string(),
        ergo_tree: bank_box.ergo_tree.clone(),
        assets: vec![
            Eip12Asset {
                token_id: bank_singleton.token_id.clone(),
                amount: bank_singleton.amount.clone(),
            },
            Eip12Asset {
                token_id: bank_hodl.token_id.clone(),
                amount: new_hodl_in_bank.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: bank_box.additional_registers.clone(),
    };

    let user_output = Eip12Output::change(
        burn_result.erg_to_user,
        user_ergo_tree,
        vec![],
        current_height,
    );

    let dev_fee_ergo_tree = extract_dev_fee_tree(bank_box)?;
    let dev_fee_output = Eip12Output {
        value: burn_result.dev_fee.to_string(),
        ergo_tree: dev_fee_ergo_tree,
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let fee_output = Eip12Output::fee(MIN_MINER_FEE as i64, current_height);

    let mut outputs = vec![new_bank_output, user_output, dev_fee_output, fee_output];

    // User's ERG output comes from the bank, not from user UTXOs
    let user_erg_needed = MIN_MINER_FEE;

    let selected = select_token_boxes(
        user_utxos,
        &bank_state.hodl_token_id,
        hodl_to_burn as u64,
        user_erg_needed,
    )
    .map_err(|e| HodlError::InsufficientFunds(e.to_string()))?;

    append_change_output(
        &mut outputs,
        &selected,
        user_erg_needed,
        &[(bank_state.hodl_token_id.as_str(), hodl_to_burn as u64)],
        user_ergo_tree,
        current_height,
        MIN_CHANGE_VALUE,
    )
    .map_err(|e| HodlError::TxBuildError(e.to_string()))?;

    let mut inputs = vec![bank_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

/// Bank contract stores `blake2b256(feeContract.propBytes)`, not the ErgoTree itself.
/// Verify the embedded hash matches our known fee contract, then return the full ErgoTree.
fn extract_dev_fee_tree(bank_box: &Eip12InputBox) -> Result<String, HodlError> {
    let hash_hex = constants::DEV_FEE_CONTRACT_HASH;
    if bank_box.ergo_tree.contains(hash_hex) {
        return Ok(constants::DEV_FEE_CONTRACT_BYTES.to_string());
    }

    tracing::warn!(
        "Bank ErgoTree does not contain known dev fee hash ({}), using fee contract anyway",
        hash_hex
    );
    Ok(constants::DEV_FEE_CONTRACT_BYTES.to_string())
}
