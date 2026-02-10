//! MewLock transaction builders (EIP-12 format)
//!
//! Two transaction types:
//! 1. Lock   — Create a timelock box with ERG/tokens
//! 2. Unlock — Withdraw from a timelock box (with 3% fee)

use std::collections::HashMap;

use ergo_tx::eip12::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};
use ergo_tx::sigma::{
    encode_sigma_coll_byte, encode_sigma_group_element, encode_sigma_int,
    extract_pk_from_p2pk_ergo_tree,
};
use ergo_tx::{collect_change_tokens, select_erg_boxes, select_token_boxes};

use crate::constants::{self, MEWLOCK_ERGO_TREE};

/// Standard miner fee (1.1 mERG)
const MINER_FEE: i64 = 1_100_000;

/// Minimum box value for change outputs
const MIN_CHANGE_VALUE: i64 = 1_000_000;

/// Minimum box value (for dev fee box)
const MIN_BOX_VALUE: i64 = 1_000_000;

#[derive(Debug)]
pub enum MewLockTxError {
    InvalidAddress(String),
    InvalidAmount(String),
    BoxSelection(String),
    InsufficientFunds(String),
}

impl std::fmt::Display for MewLockTxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAddress(msg) => write!(f, "Invalid address: {}", msg),
            Self::InvalidAmount(msg) => write!(f, "Invalid amount: {}", msg),
            Self::BoxSelection(msg) => write!(f, "Box selection failed: {}", msg),
            Self::InsufficientFunds(msg) => write!(f, "Insufficient funds: {}", msg),
        }
    }
}

impl std::error::Error for MewLockTxError {}

// =============================================================================
// Lock Transaction
// =============================================================================

pub struct LockRequest {
    /// User's P2PK ErgoTree hex (0008cd + pubkey)
    pub user_ergo_tree: String,
    /// ERG to lock (nanoERG)
    pub lock_erg: u64,
    /// Tokens to lock: [(token_id, amount)]
    pub lock_tokens: Vec<(String, u64)>,
    /// Block height at which lock becomes withdrawable
    pub unlock_height: i32,
    /// Optional Unix timestamp (seconds)
    pub timestamp: Option<i64>,
    /// Optional lock name
    pub lock_name: Option<String>,
    /// Optional lock description
    pub lock_description: Option<String>,
    /// User's available UTXOs
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current blockchain height
    pub current_height: i32,
}

/// Build a lock transaction that creates a MewLock box.
pub fn build_lock_tx(req: &LockRequest) -> Result<Eip12UnsignedTx, MewLockTxError> {
    if req.lock_erg == 0 && req.lock_tokens.is_empty() {
        return Err(MewLockTxError::InvalidAmount(
            "Must lock at least some ERG or tokens".to_string(),
        ));
    }

    if req.unlock_height <= req.current_height {
        return Err(MewLockTxError::InvalidAmount(
            "Unlock height must be in the future".to_string(),
        ));
    }

    // Extract 33-byte pubkey from P2PK ErgoTree
    let pubkey = extract_pubkey(&req.user_ergo_tree)?;

    // Build registers
    let mut registers = HashMap::new();

    // R4: GroupElement (depositor pubkey)
    registers.insert("R4".to_string(), encode_sigma_group_element(&pubkey));

    // R5: Int (unlock height)
    registers.insert("R5".to_string(), encode_sigma_int(req.unlock_height));

    // R6: Optional Int (timestamp)
    if let Some(ts) = req.timestamp {
        registers.insert("R6".to_string(), encode_sigma_int(ts as i32));
    }

    // R7: Optional Coll[Byte] (name)
    if let Some(ref name) = req.lock_name {
        if !name.is_empty() {
            registers.insert("R7".to_string(), encode_sigma_coll_byte(name.as_bytes()));
        }
    }

    // R8: Optional Coll[Byte] (description)
    if let Some(ref desc) = req.lock_description {
        if !desc.is_empty() {
            registers.insert("R8".to_string(), encode_sigma_coll_byte(desc.as_bytes()));
        }
    }

    // Lock box value
    let lock_value = if req.lock_erg > 0 {
        req.lock_erg as i64
    } else {
        MIN_BOX_VALUE // Minimum box value for token-only locks
    };

    let lock_assets: Vec<Eip12Asset> = req
        .lock_tokens
        .iter()
        .map(|(tid, amt)| Eip12Asset::new(tid, *amt as i64))
        .collect();

    let lock_output = Eip12Output {
        value: lock_value.to_string(),
        ergo_tree: MEWLOCK_ERGO_TREE.to_string(),
        assets: lock_assets,
        creation_height: req.current_height,
        additional_registers: registers,
    };

    // Calculate required ERG
    let required_erg = (lock_value + MINER_FEE + MIN_CHANGE_VALUE) as u64;

    // Select inputs
    let selected = if req.lock_tokens.is_empty() {
        select_erg_boxes(&req.user_inputs, required_erg)
            .map_err(|e| MewLockTxError::BoxSelection(e.to_string()))?
    } else {
        let (ref first_token_id, first_amount) = req.lock_tokens[0];
        select_token_boxes(&req.user_inputs, first_token_id, first_amount, required_erg)
            .map_err(|e| MewLockTxError::BoxSelection(e.to_string()))?
    };

    // Change
    let change_erg = selected.total_erg as i64 - lock_value - MINER_FEE;
    if change_erg < 0 {
        return Err(MewLockTxError::InsufficientFunds(format!(
            "Need {} more nanoERG",
            -change_erg
        )));
    }

    let spent_tokens: Vec<(&str, u64)> = req
        .lock_tokens
        .iter()
        .map(|(tid, amt)| (tid.as_str(), *amt))
        .collect();
    let change_tokens = if spent_tokens.len() == 1 {
        collect_change_tokens(
            &selected.boxes,
            Some((spent_tokens[0].0, spent_tokens[0].1)),
        )
    } else if spent_tokens.is_empty() {
        collect_change_tokens(&selected.boxes, None)
    } else {
        ergo_tx::collect_multi_change_tokens(&selected.boxes, &spent_tokens)
    };

    let change_output = Eip12Output::change(
        change_erg,
        &req.user_ergo_tree,
        change_tokens,
        req.current_height,
    );
    let fee_output = Eip12Output::fee(MINER_FEE, req.current_height);

    Ok(Eip12UnsignedTx {
        inputs: selected.boxes,
        data_inputs: vec![],
        outputs: vec![lock_output, change_output, fee_output],
    })
}

// =============================================================================
// Unlock Transaction
// =============================================================================

pub struct UnlockRequest {
    /// The lock box to unlock (as EIP-12 input, with registers)
    pub lock_box: Eip12InputBox,
    /// User's P2PK ErgoTree hex
    pub user_ergo_tree: String,
    /// User's available UTXOs (for miner fee if needed)
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current blockchain height
    pub current_height: i32,
}

/// Build an unlock transaction that spends a MewLock box.
///
/// Calculates 3% fees on ERG and tokens, sends fee to dev treasury.
pub fn build_unlock_tx(req: &UnlockRequest) -> Result<Eip12UnsignedTx, MewLockTxError> {
    let lock_erg: u64 = req
        .lock_box
        .value
        .parse()
        .map_err(|_| MewLockTxError::InvalidAmount("Invalid lock box value".to_string()))?;

    // Derive dev ErgoTree from address
    let dev_ergo_tree =
        ergo_tx::address_to_ergo_tree(constants::DEV_ADDRESS).map_err(|e| {
            MewLockTxError::InvalidAddress(format!("Invalid dev address: {}", e))
        })?;

    // Calculate ERG fee
    let erg_fee = constants::calculate_erg_fee(lock_erg);

    // Calculate token fees
    let mut user_tokens: Vec<Eip12Asset> = Vec::new();
    let mut dev_tokens: Vec<Eip12Asset> = Vec::new();

    for token in &req.lock_box.assets {
        let amount: u64 = token
            .amount
            .parse()
            .unwrap_or(0);
        let token_fee = constants::calculate_token_fee(amount);
        let user_amount = amount - token_fee;

        if user_amount > 0 {
            user_tokens.push(Eip12Asset::new(&token.token_id, user_amount as i64));
        }
        if token_fee > 0 {
            dev_tokens.push(Eip12Asset::new(&token.token_id, token_fee as i64));
        }
    }

    let user_erg = lock_erg - erg_fee;

    // Determine if we need extra UTXOs for miner fee
    // The lock box might have enough ERG to cover the fee + miner fee
    let needs_extra_inputs = (user_erg as i64) < MINER_FEE + MIN_BOX_VALUE;

    let mut inputs = vec![req.lock_box.clone()];
    let mut extra_erg: i64 = 0;

    if needs_extra_inputs && !req.user_inputs.is_empty() {
        let fee_required = (MINER_FEE + MIN_CHANGE_VALUE) as u64;
        let selected = select_erg_boxes(&req.user_inputs, fee_required)
            .map_err(|e| MewLockTxError::BoxSelection(e.to_string()))?;
        extra_erg = selected.total_erg as i64;
        inputs.extend(selected.boxes);
    }

    // Output 0: User receives (erg - erg_fee) + (tokens - token_fees)
    // When extra inputs are used, user gets all remaining ERG minus fees
    let total_available_erg = user_erg as i64 + extra_erg;
    let user_output_erg = total_available_erg - MINER_FEE;

    // If we have dev fees, the dev box needs MIN_BOX_VALUE too
    let has_dev_fees = erg_fee > 0 || !dev_tokens.is_empty();
    let dev_box_erg = if has_dev_fees {
        if erg_fee as i64 >= MIN_BOX_VALUE {
            erg_fee as i64
        } else {
            // Need to take some from user output to ensure dev box meets minimum
            MIN_BOX_VALUE
        }
    } else {
        0
    };

    let final_user_erg = if has_dev_fees {
        user_output_erg - dev_box_erg
    } else {
        user_output_erg
    };

    if final_user_erg < MIN_BOX_VALUE {
        return Err(MewLockTxError::InsufficientFunds(
            "Lock box value too low to cover fees".to_string(),
        ));
    }

    let user_output = Eip12Output {
        value: final_user_erg.to_string(),
        ergo_tree: req.user_ergo_tree.clone(),
        assets: user_tokens,
        creation_height: req.current_height,
        additional_registers: HashMap::new(),
    };

    let mut outputs = vec![user_output];

    // Output 1: Dev fee box (only if there are fees)
    if has_dev_fees {
        let dev_output = Eip12Output {
            value: dev_box_erg.to_string(),
            ergo_tree: dev_ergo_tree,
            assets: dev_tokens,
            creation_height: req.current_height,
            additional_registers: HashMap::new(),
        };
        outputs.push(dev_output);
    }

    // Output 2: Miner fee
    outputs.push(Eip12Output::fee(MINER_FEE, req.current_height));

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

// =============================================================================
// Helpers
// =============================================================================

/// Extract the 33-byte compressed public key from a P2PK ErgoTree hex
fn extract_pubkey(ergo_tree_hex: &str) -> Result<[u8; 33], MewLockTxError> {
    extract_pk_from_p2pk_ergo_tree(ergo_tree_hex)
        .map_err(|e| MewLockTxError::InvalidAddress(format!("Invalid P2PK ErgoTree: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_utxo(value: i64, tokens: Vec<(&str, i64)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: "aaaa".repeat(16),
            transaction_id: "bbbb".repeat(16),
            index: 0,
            value: value.to_string(),
            ergo_tree: TEST_ERGO_TREE.to_string(),
            assets: tokens
                .into_iter()
                .map(|(id, amt)| Eip12Asset::new(id, amt))
                .collect(),
            creation_height: 1000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    const TEST_ERGO_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    const TEST_TOKEN_ID: &str =
        "003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0";

    #[test]
    fn test_extract_pubkey() {
        let pubkey = extract_pubkey(TEST_ERGO_TREE).unwrap();
        assert_eq!(pubkey[0], 0x02); // Compressed point prefix
        assert_eq!(pubkey.len(), 33);
    }

    #[test]
    fn test_extract_pubkey_invalid() {
        assert!(extract_pubkey("deadbeef").is_err());
        assert!(extract_pubkey("").is_err());
    }

    #[test]
    fn test_build_lock_tx_erg_only() {
        let req = LockRequest {
            user_ergo_tree: TEST_ERGO_TREE.to_string(),
            lock_erg: 1_000_000_000, // 1 ERG
            lock_tokens: vec![],
            unlock_height: 2000,
            timestamp: Some(1700000000),
            lock_name: Some("Test Lock".to_string()),
            lock_description: None,
            user_inputs: vec![mock_utxo(5_000_000_000, vec![])],
            current_height: 1000,
        };

        let tx = build_lock_tx(&req).unwrap();
        assert_eq!(tx.outputs.len(), 3); // lock + change + fee
        assert_eq!(tx.outputs[0].value, "1000000000");
        assert_eq!(tx.outputs[0].ergo_tree, MEWLOCK_ERGO_TREE);
        assert!(tx.outputs[0].additional_registers.contains_key("R4"));
        assert!(tx.outputs[0].additional_registers.contains_key("R5"));
        assert!(tx.outputs[0].additional_registers.contains_key("R6"));
        assert!(tx.outputs[0].additional_registers.contains_key("R7"));
        assert!(!tx.outputs[0].additional_registers.contains_key("R8")); // No description
    }

    #[test]
    fn test_build_lock_tx_with_tokens() {
        let req = LockRequest {
            user_ergo_tree: TEST_ERGO_TREE.to_string(),
            lock_erg: 2_000_000_000,
            lock_tokens: vec![(TEST_TOKEN_ID.to_string(), 1000)],
            unlock_height: 2000,
            timestamp: None,
            lock_name: None,
            lock_description: None,
            user_inputs: vec![mock_utxo(5_000_000_000, vec![(TEST_TOKEN_ID, 2000)])],
            current_height: 1000,
        };

        let tx = build_lock_tx(&req).unwrap();
        assert_eq!(tx.outputs.len(), 3);
        assert_eq!(tx.outputs[0].assets.len(), 1);
        assert_eq!(tx.outputs[0].assets[0].token_id, TEST_TOKEN_ID);
    }

    #[test]
    fn test_build_lock_tx_validation() {
        // Zero amount
        let req = LockRequest {
            user_ergo_tree: TEST_ERGO_TREE.to_string(),
            lock_erg: 0,
            lock_tokens: vec![],
            unlock_height: 2000,
            timestamp: None,
            lock_name: None,
            lock_description: None,
            user_inputs: vec![mock_utxo(5_000_000_000, vec![])],
            current_height: 1000,
        };
        assert!(build_lock_tx(&req).is_err());

        // Unlock height in the past
        let req2 = LockRequest {
            user_ergo_tree: TEST_ERGO_TREE.to_string(),
            lock_erg: 1_000_000_000,
            lock_tokens: vec![],
            unlock_height: 500,
            timestamp: None,
            lock_name: None,
            lock_description: None,
            user_inputs: vec![mock_utxo(5_000_000_000, vec![])],
            current_height: 1000,
        };
        assert!(build_lock_tx(&req2).is_err());
    }

    #[test]
    fn test_build_unlock_tx() {
        let lock_box = Eip12InputBox {
            box_id: "cccc".repeat(16),
            transaction_id: "dddd".repeat(16),
            index: 0,
            value: "10000000000".to_string(), // 10 ERG
            ergo_tree: MEWLOCK_ERGO_TREE.to_string(),
            assets: vec![Eip12Asset::new(TEST_TOKEN_ID, 1000)],
            creation_height: 800,
            additional_registers: {
                let mut regs = HashMap::new();
                let mut pubkey = [0u8; 33];
                pubkey[0] = 0x02;
                regs.insert(
                    "R4".to_string(),
                    encode_sigma_group_element(&pubkey),
                );
                regs.insert("R5".to_string(), encode_sigma_int(900));
                regs
            },
            extension: HashMap::new(),
        };

        let req = UnlockRequest {
            lock_box,
            user_ergo_tree: TEST_ERGO_TREE.to_string(),
            user_inputs: vec![],
            current_height: 1100,
        };

        let tx = build_unlock_tx(&req).unwrap();
        // Outputs: user + dev + fee
        assert_eq!(tx.outputs.len(), 3);

        // User gets (10 ERG - 3% fee - miner fee)
        let user_erg: i64 = tx.outputs[0].value.parse().unwrap();
        assert!(user_erg > 0);

        // Dev gets fee
        let dev_erg: i64 = tx.outputs[1].value.parse().unwrap();
        assert!(dev_erg > 0);

        // Token fees: 3% of 1000 = 30
        let user_token_amt: i64 = tx.outputs[0].assets[0].amount.parse().unwrap();
        assert_eq!(user_token_amt, 970);

        let dev_token_amt: i64 = tx.outputs[1].assets[0].amount.parse().unwrap();
        assert_eq!(dev_token_amt, 30);
    }

    #[test]
    fn test_build_unlock_tx_no_token_fee_below_threshold() {
        let lock_box = Eip12InputBox {
            box_id: "cccc".repeat(16),
            transaction_id: "dddd".repeat(16),
            index: 0,
            value: "5000000000".to_string(), // 5 ERG
            ergo_tree: MEWLOCK_ERGO_TREE.to_string(),
            assets: vec![Eip12Asset::new(TEST_TOKEN_ID, 20)], // Below threshold
            creation_height: 800,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        };

        let req = UnlockRequest {
            lock_box,
            user_ergo_tree: TEST_ERGO_TREE.to_string(),
            user_inputs: vec![],
            current_height: 1100,
        };

        let tx = build_unlock_tx(&req).unwrap();
        // User should get all 20 tokens (no fee below threshold)
        let user_token_amt: i64 = tx.outputs[0].assets[0].amount.parse().unwrap();
        assert_eq!(user_token_amt, 20);
    }
}
