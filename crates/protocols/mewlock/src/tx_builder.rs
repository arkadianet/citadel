use std::collections::HashMap;

use ergo_tx::eip12::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};
use ergo_tx::sigma::{
    encode_sigma_coll_byte, encode_sigma_group_element, encode_sigma_int,
    extract_pk_from_p2pk_ergo_tree,
};
use ergo_tx::{
    append_change_output, collect_change_tokens, select_erg_boxes, select_inputs_for_spend,
};

use crate::constants::{self, MEWLOCK_ERGO_TREE};

const MINER_FEE: i64 = 1_100_000;
const MIN_CHANGE_VALUE: i64 = 1_000_000;
const MIN_BOX_VALUE: i64 = 1_000_000;

#[derive(Debug, thiserror::Error)]
pub enum MewLockTxError {
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),
    #[error("Box selection failed: {0}")]
    BoxSelection(String),
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
}

pub struct LockRequest {
    pub user_ergo_tree: String,
    pub lock_erg: u64,
    pub lock_tokens: Vec<(String, u64)>,
    pub unlock_height: i32,
    pub timestamp: Option<i64>,
    pub lock_name: Option<String>,
    pub lock_description: Option<String>,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
}

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

    let pubkey = extract_pubkey(&req.user_ergo_tree)?;

    let mut registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_group_element(&pubkey),
        "R5" => encode_sigma_int(req.unlock_height),
    );

    if let Some(ts) = req.timestamp {
        registers.insert("R6".to_string(), encode_sigma_int(ts as i32));
    }

    if let Some(ref name) = req.lock_name {
        if !name.is_empty() {
            registers.insert("R7".to_string(), encode_sigma_coll_byte(name.as_bytes()));
        }
    }

    if let Some(ref desc) = req.lock_description {
        if !desc.is_empty() {
            registers.insert("R8".to_string(), encode_sigma_coll_byte(desc.as_bytes()));
        }
    }

    let lock_value = if req.lock_erg > 0 {
        req.lock_erg as i64
    } else {
        MIN_BOX_VALUE
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

    let required_erg = (lock_value + MINER_FEE + MIN_CHANGE_VALUE) as u64;

    let first_token = req
        .lock_tokens
        .first()
        .map(|(tid, amt)| (tid.as_str(), *amt));
    let selected = select_inputs_for_spend(&req.user_inputs, required_erg, first_token)
        .map_err(|e| MewLockTxError::BoxSelection(e.to_string()))?;

    let erg_used = (lock_value + MINER_FEE) as u64;
    let spent_tokens: Vec<(&str, u64)> = req
        .lock_tokens
        .iter()
        .map(|(tid, amt)| (tid.as_str(), *amt))
        .collect();

    let mut outputs = vec![lock_output];

    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &spent_tokens,
        &req.user_ergo_tree,
        req.current_height,
        MIN_CHANGE_VALUE as u64,
    )
    .map_err(|e| MewLockTxError::InsufficientFunds(e.to_string()))?;

    outputs.push(Eip12Output::fee(MINER_FEE, req.current_height));

    Ok(Eip12UnsignedTx {
        inputs: selected.boxes,
        data_inputs: vec![],
        outputs,
    })
}

pub struct UnlockRequest {
    pub lock_box: Eip12InputBox,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
}

/// Contract enforces: user output value >= lock_erg - erg_fee,
/// so the miner fee must come from a separate user UTXO.
pub fn build_unlock_tx(req: &UnlockRequest) -> Result<Eip12UnsignedTx, MewLockTxError> {
    let lock_erg: u64 = req
        .lock_box
        .value
        .parse()
        .map_err(|_| MewLockTxError::InvalidAmount("Invalid lock box value".to_string()))?;

    let dev_ergo_tree =
        ergo_tx::address_to_ergo_tree(constants::DEV_ADDRESS).map_err(|e| {
            MewLockTxError::InvalidAddress(format!("Invalid dev address: {}", e))
        })?;

    let erg_fee = constants::calculate_erg_fee(lock_erg);

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

    let has_dev_fees = erg_fee > 0 || !dev_tokens.is_empty();

    let user_erg = lock_erg as i64 - erg_fee as i64;
    let dev_erg = erg_fee as i64;

    let fee_required = (MINER_FEE + MIN_CHANGE_VALUE) as u64;
    let selected = select_erg_boxes(&req.user_inputs, fee_required)
        .map_err(|e| MewLockTxError::BoxSelection(e.to_string()))?;

    let mut inputs = vec![req.lock_box.clone()];
    let selected_boxes = selected.boxes;
    inputs.extend(selected_boxes.clone());

    let change_erg = selected.total_erg as i64 - MINER_FEE;

    let user_output = Eip12Output {
        value: user_erg.to_string(),
        ergo_tree: req.user_ergo_tree.clone(),
        assets: user_tokens,
        creation_height: req.current_height,
        additional_registers: HashMap::new(),
    };

    let mut outputs = vec![user_output];

    if has_dev_fees {
        let dev_output = Eip12Output {
            value: dev_erg.to_string(),
            ergo_tree: dev_ergo_tree,
            assets: dev_tokens,
            creation_height: req.current_height,
            additional_registers: HashMap::new(),
        };
        outputs.push(dev_output);
    }

    if change_erg > 0 {
        let change_tokens = collect_change_tokens(&selected_boxes, None);
        let change_output = Eip12Output::change(
            change_erg,
            &req.user_ergo_tree,
            change_tokens,
            req.current_height,
        );
        outputs.push(change_output);
    }

    outputs.push(Eip12Output::fee(MINER_FEE, req.current_height));

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

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
        assert_eq!(tx.outputs.len(), 3);
        assert_eq!(tx.outputs[0].value, "1000000000");
        assert_eq!(tx.outputs[0].ergo_tree, MEWLOCK_ERGO_TREE);
        assert!(tx.outputs[0].additional_registers.contains_key("R4"));
        assert!(tx.outputs[0].additional_registers.contains_key("R5"));
        assert!(tx.outputs[0].additional_registers.contains_key("R6"));
        assert!(tx.outputs[0].additional_registers.contains_key("R7"));
        assert!(!tx.outputs[0].additional_registers.contains_key("R8"));
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
                let mut pubkey = [0u8; 33];
                pubkey[0] = 0x02;
                ergo_tx::sigma_registers!(
                    "R4" => encode_sigma_group_element(&pubkey),
                    "R5" => encode_sigma_int(900),
                )
            },
            extension: HashMap::new(),
        };

        let req = UnlockRequest {
            lock_box,
            user_ergo_tree: TEST_ERGO_TREE.to_string(),
            user_inputs: vec![mock_utxo(5_000_000_000, vec![])],
            current_height: 1100,
        };

        let tx = build_unlock_tx(&req).unwrap();
        assert_eq!(tx.inputs.len(), 2);
        assert_eq!(tx.outputs.len(), 4);

        let user_erg: i64 = tx.outputs[0].value.parse().unwrap();
        assert_eq!(user_erg, 9_700_000_000);

        let dev_erg: i64 = tx.outputs[1].value.parse().unwrap();
        assert_eq!(dev_erg, 300_000_000);

        let user_token_amt: i64 = tx.outputs[0].assets[0].amount.parse().unwrap();
        assert_eq!(user_token_amt, 970);

        let dev_token_amt: i64 = tx.outputs[1].assets[0].amount.parse().unwrap();
        assert_eq!(dev_token_amt, 30);

        let change_erg: i64 = tx.outputs[2].value.parse().unwrap();
        assert_eq!(change_erg, 5_000_000_000 - MINER_FEE);

        let total_in = 10_000_000_000i64 + 5_000_000_000;
        let total_out: i64 = tx.outputs.iter().map(|o| o.value.parse::<i64>().unwrap()).sum();
        assert_eq!(total_in, total_out);
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
            user_inputs: vec![mock_utxo(3_000_000_000, vec![])],
            current_height: 1100,
        };

        let tx = build_unlock_tx(&req).unwrap();
        let user_token_amt: i64 = tx.outputs[0].assets[0].amount.parse().unwrap();
        assert_eq!(user_token_amt, 20);
    }

    #[test]
    fn test_build_unlock_tx_small_lock() {
        let lock_box = Eip12InputBox {
            box_id: "cccc".repeat(16),
            transaction_id: "dddd".repeat(16),
            index: 0,
            value: "10000000".to_string(), // 0.01 ERG
            ergo_tree: MEWLOCK_ERGO_TREE.to_string(),
            assets: vec![],
            creation_height: 800,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        };

        let req = UnlockRequest {
            lock_box,
            user_ergo_tree: TEST_ERGO_TREE.to_string(),
            user_inputs: vec![mock_utxo(3_000_000_000, vec![])],
            current_height: 1100,
        };

        let tx = build_unlock_tx(&req).unwrap();

        let user_erg: i64 = tx.outputs[0].value.parse().unwrap();
        assert_eq!(user_erg, 9_700_000);

        let dev_erg: i64 = tx.outputs[1].value.parse().unwrap();
        assert_eq!(dev_erg, 300_000);

        let total_in = 10_000_000i64 + 3_000_000_000;
        let total_out: i64 = tx.outputs.iter().map(|o| o.value.parse::<i64>().unwrap()).sum();
        assert_eq!(total_in, total_out);
    }
}
