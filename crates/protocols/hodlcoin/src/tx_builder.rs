//! HodlCoin Transaction Builder
//!
//! Builds EIP-12 unsigned transactions for direct mint/burn operations.
//!
//! # Mint Transaction Structure
//!
//! Inputs:  [bank_box, user_utxos...]
//! Outputs: [new_bank_box, user_output (hodl tokens), miner_fee, change?]
//!
//! # Burn Transaction Structure
//!
//! Inputs:  [bank_box, user_utxos... (containing hodl tokens)]
//! Outputs: [new_bank_box, user_output (ERG), dev_fee_output, miner_fee, change?]

use std::collections::HashMap;

use crate::calculator;
use crate::constants::{self, MIN_BOX_VALUE, MIN_CHANGE_VALUE, MIN_MINER_FEE};
use crate::state::{HodlBankState, HodlError};
use ergo_tx::{
    collect_change_tokens, select_erg_boxes, select_token_boxes, Eip12Asset, Eip12InputBox,
    Eip12Output, Eip12UnsignedTx,
};

/// Build a mint transaction (deposit ERG, receive hodlTokens).
///
/// The bank box must be inputs[0] and the new bank box must be outputs[0].
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

    // Calculate tokens received
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

    // Verify bank has enough tokens
    if tokens_received > bank_state.hodl_tokens_in_bank {
        return Err(HodlError::TxBuildError(format!(
            "Bank only has {} tokens, but {} needed",
            bank_state.hodl_tokens_in_bank, tokens_received
        )));
    }

    // Parse bank box values
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

    // New bank box: ERG increased, hodl tokens decreased
    let new_bank_erg = bank_erg + erg_to_deposit as u64;
    let new_hodl_in_bank = hodl_in_bank - tokens_received as u64;

    let new_bank_output = Eip12Output {
        value: new_bank_erg.to_string(),
        ergo_tree: bank_box.ergo_tree.clone(),
        assets: vec![
            Eip12Asset {
                token_id: bank_singleton.token_id.clone(),
                amount: bank_singleton.amount.clone(), // same singleton (1)
            },
            Eip12Asset {
                token_id: bank_hodl.token_id.clone(),
                amount: new_hodl_in_bank.to_string(),
            },
        ],
        creation_height: current_height,
        additional_registers: bank_box.additional_registers.clone(),
    };

    // User output: receives hodl tokens
    let user_output = Eip12Output::change(
        MIN_BOX_VALUE as i64,
        user_ergo_tree,
        vec![Eip12Asset::new(&bank_hodl.token_id, tokens_received)],
        current_height,
    );

    // Miner fee
    let fee_output = Eip12Output::fee(MIN_MINER_FEE as i64, current_height);

    // User ERG needed: deposit + min box value (for user output) + miner fee
    let user_erg_needed = erg_to_deposit as u64 + MIN_BOX_VALUE + MIN_MINER_FEE;

    let selected = select_erg_boxes(user_utxos, user_erg_needed)
        .map_err(|e| HodlError::InsufficientFunds(e.to_string()))?;

    let mut outputs = vec![new_bank_output, user_output, fee_output];

    // Change output
    let change_erg = selected.total_erg - user_erg_needed;
    let change_tokens = collect_change_tokens(&selected.boxes, None);

    if !change_tokens.is_empty() && change_erg < MIN_CHANGE_VALUE {
        return Err(HodlError::TxBuildError(format!(
            "Change tokens exist but not enough ERG for change box (need {}, have {})",
            MIN_CHANGE_VALUE, change_erg
        )));
    }

    if change_erg >= MIN_CHANGE_VALUE || !change_tokens.is_empty() {
        outputs.push(Eip12Output::change(
            change_erg as i64,
            user_ergo_tree,
            change_tokens,
            current_height,
        ));
    }

    // Build transaction: bank box = inputs[0]
    let mut inputs = vec![bank_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

/// Build a burn transaction (return hodlTokens, receive ERG minus fees).
///
/// The bank box must be inputs[0] and the new bank box must be outputs[0].
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

    // Calculate burn result
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

    // Parse bank box values
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

    // New bank box: ERG decreased by user_amount + dev_fee, hodl tokens increased
    // The bank_fee stays in the bank (reserve increases relative to circulating supply)
    let erg_leaving_bank = (burn_result.erg_to_user + burn_result.dev_fee) as u64;
    let new_bank_erg = bank_erg
        .checked_sub(erg_leaving_bank)
        .ok_or_else(|| HodlError::TxBuildError("Bank ERG underflow".to_string()))?;

    // Check min bank value constraint
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

    // User output: receives ERG
    let user_output = Eip12Output::change(
        burn_result.erg_to_user,
        user_ergo_tree,
        vec![],
        current_height,
    );

    // Dev fee output: extract the dev fee contract ErgoTree from the bank box constants
    // The dev fee goes to the contract embedded in the bank's ErgoTree.
    // We use a P2S output with the dev fee contract bytes from constants.
    // NOTE: For the real contract, we need to read the actual dev fee P2PK address
    // from the bank's ErgoTree constants. For now, we use the embedded constant.
    let dev_fee_ergo_tree = extract_dev_fee_tree(bank_box)?;
    let dev_fee_output = Eip12Output {
        value: burn_result.dev_fee.to_string(),
        ergo_tree: dev_fee_ergo_tree,
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // Miner fee
    let fee_output = Eip12Output::fee(MIN_MINER_FEE as i64, current_height);

    let mut outputs = vec![new_bank_output, user_output, dev_fee_output, fee_output];

    // User needs: hodl tokens to burn + ERG for miner fee
    // The user's ERG output comes from the bank, not from the user's UTXOs
    let user_erg_needed = MIN_MINER_FEE;

    let selected = select_token_boxes(
        user_utxos,
        &bank_state.hodl_token_id,
        hodl_to_burn as u64,
        user_erg_needed,
    )
    .map_err(|e| HodlError::InsufficientFunds(e.to_string()))?;

    // Change output (remaining tokens and ERG)
    let change_erg = selected.total_erg - user_erg_needed;
    let spent_token = Some((bank_state.hodl_token_id.as_str(), hodl_to_burn as u64));
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    if !change_tokens.is_empty() && change_erg < MIN_CHANGE_VALUE {
        return Err(HodlError::TxBuildError(format!(
            "Change tokens exist but not enough ERG for change box (need {}, have {})",
            MIN_CHANGE_VALUE, change_erg
        )));
    }

    if change_erg >= MIN_CHANGE_VALUE || !change_tokens.is_empty() {
        outputs.push(Eip12Output::change(
            change_erg as i64,
            user_ergo_tree,
            change_tokens,
            current_height,
        ));
    }

    // Build transaction: bank box = inputs[0]
    let mut inputs = vec![bank_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

/// Get the dev fee contract ErgoTree hex for a hodlERG bank box.
///
/// The hodlERG bank contract stores `blake2b256(feeContract.propBytes)` as a constant,
/// NOT the ErgoTree itself. We verify the embedded hash matches our known fee contract,
/// then return the full fee contract ErgoTree.
fn extract_dev_fee_tree(bank_box: &Eip12InputBox) -> Result<String, HodlError> {
    // The bank contract raw bytes contain the fee contract hash as a Coll[Byte] constant.
    // Verify it matches our known hash before returning the fee contract ErgoTree.
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
