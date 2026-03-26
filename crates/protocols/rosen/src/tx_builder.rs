use serde::{Deserialize, Serialize};

use ergo_tx::eip12::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};
use ergo_tx::sigma::encode_sigma_coll_coll_byte;
use ergo_tx::{append_change_output, select_inputs_for_spend};

use crate::constants::{LOCK_TX_FEE, MIN_BOX_VALUE};
use crate::validate::validate_target_address;

#[derive(Debug, Clone)]
pub struct LockRequest {
    pub ergo_token_id: String,
    pub amount: i64,
    pub target_chain: String,
    pub target_address: String,
    pub bridge_fee: i64,
    pub network_fee: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: LockSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockSummary {
    pub token_name: String,
    pub amount: i64,
    pub target_chain: String,
    pub target_address: String,
    pub bridge_fee: i64,
    pub network_fee: i64,
    /// For ERG: amount + fees; for tokens: min_box_value + miner_fee
    pub total_cost_erg: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("Invalid target address: {0}")]
    InvalidAddress(String),
    #[error("Amount must be positive")]
    InvalidAmount,
    #[error("Box selection failed: {0}")]
    BoxSelection(String),
    #[error("Failed to convert lock address to ErgoTree: {0}")]
    LockAddressConversion(String),
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
}

impl std::fmt::Display for LockBuildResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Lock {} to {} via {}",
            self.summary.amount, self.summary.target_address, self.summary.target_chain
        )
    }
}

/// R4: Coll[Coll[SByte]] = [target_chain, target_address, network_fee, bridge_fee, from_address]
pub fn build_lock_tx(
    request: &LockRequest,
    lock_address_ergo_tree: &str,
) -> Result<LockBuildResult, LockError> {
    if request.amount <= 0 {
        return Err(LockError::InvalidAmount);
    }
    validate_target_address(&request.target_chain, &request.target_address)
        .map_err(LockError::InvalidAddress)?;

    let is_erg = request.ergo_token_id == "erg";

    let network_fee_str = request.network_fee.to_string();
    let bridge_fee_str = request.bridge_fee.to_string();
    let r4_values: Vec<&[u8]> = vec![
        request.target_chain.as_bytes(),
        request.target_address.as_bytes(),
        network_fee_str.as_bytes(),
        bridge_fee_str.as_bytes(),
        request.user_address.as_bytes(),
    ];
    let r4_hex = encode_sigma_coll_coll_byte(&r4_values);

    let lock_box = if is_erg {
        let registers = ergo_tx::sigma_registers!("R4" => r4_hex);

        Eip12Output {
            value: request.amount.to_string(),
            ergo_tree: lock_address_ergo_tree.to_string(),
            assets: vec![],
            creation_height: request.current_height,
            additional_registers: registers,
        }
    } else {
        let registers = ergo_tx::sigma_registers!("R4" => r4_hex);

        Eip12Output {
            value: MIN_BOX_VALUE.to_string(),
            ergo_tree: lock_address_ergo_tree.to_string(),
            assets: vec![Eip12Asset::new(&request.ergo_token_id, request.amount)],
            creation_height: request.current_height,
            additional_registers: registers,
        }
    };

    let required_erg = if is_erg {
        (request.amount + LOCK_TX_FEE + citadel_core::constants::MIN_BOX_VALUE_NANO) as u64
    } else {
        (MIN_BOX_VALUE + LOCK_TX_FEE + citadel_core::constants::MIN_BOX_VALUE_NANO) as u64
    };

    let token_spend = if is_erg {
        None
    } else {
        Some((request.ergo_token_id.as_str(), request.amount as u64))
    };
    let selected = select_inputs_for_spend(&request.user_inputs, required_erg, token_spend)
        .map_err(|e| LockError::BoxSelection(e.to_string()))?;

    let lock_box_erg = if is_erg {
        request.amount as u64
    } else {
        MIN_BOX_VALUE as u64
    };
    let erg_used = lock_box_erg + LOCK_TX_FEE as u64;

    let spent_tokens: Vec<(&str, u64)> = if is_erg {
        vec![]
    } else {
        vec![(request.ergo_token_id.as_str(), request.amount as u64)]
    };

    let mut outputs = vec![lock_box];
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &spent_tokens,
        &request.user_ergo_tree,
        request.current_height,
        citadel_core::constants::MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| LockError::InsufficientFunds(e.to_string()))?;
    outputs.push(Eip12Output::fee(LOCK_TX_FEE, request.current_height));

    let unsigned_tx = Eip12UnsignedTx {
        inputs: selected.boxes,
        data_inputs: vec![],
        outputs,
    };

    let total_cost_erg = if is_erg {
        request.amount + LOCK_TX_FEE
    } else {
        MIN_BOX_VALUE + LOCK_TX_FEE
    };

    let summary = LockSummary {
        token_name: if is_erg {
            "ERG".to_string()
        } else {
            request.ergo_token_id.clone()
        },
        amount: request.amount,
        target_chain: request.target_chain.clone(),
        target_address: request.target_address.clone(),
        bridge_fee: request.bridge_fee,
        network_fee: request.network_fee,
        total_cost_erg,
    };

    Ok(LockBuildResult {
        unsigned_tx,
        summary,
    })
}

pub fn address_to_ergo_tree(address: &str) -> Result<String, LockError> {
    ergo_tx::address::address_to_ergo_tree(address)
        .map_err(|e| LockError::LockAddressConversion(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn mock_utxo(value: i64, tokens: Vec<(&str, i64)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: format!("box_{}", value),
            transaction_id: "tx_0".to_string(),
            index: 0,
            value: value.to_string(),
            ergo_tree: "0008cd03test".to_string(),
            assets: tokens
                .into_iter()
                .map(|(id, amt)| Eip12Asset::new(id, amt))
                .collect(),
            creation_height: 1000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_build_lock_tx_erg() {
        let request = LockRequest {
            ergo_token_id: "erg".to_string(),
            amount: 1_000_000_000, // 1 ERG
            target_chain: "cardano".to_string(),
            target_address: "addr1qxck39mfuzd4tcamp02gycm7aqnlhkxskvfjxhe0ekmzp8lrstxkxqyer6vk6g3emeqyqsghx09gvpqx9fhsgqx6wlqyu66ts".to_string(),
            bridge_fee: 300_000,
            network_fee: 500_000,
            user_address: "9ftest".to_string(),
            user_ergo_tree: "0008cd03test".to_string(),
            user_inputs: vec![mock_utxo(2_000_000_000, vec![])], // 2 ERG
            current_height: 1000,
        };

        let result = build_lock_tx(&request, "0008cd03lock").unwrap();

        assert_eq!(result.unsigned_tx.outputs.len(), 3);
        assert_eq!(
            result.unsigned_tx.outputs[0].value,
            "1000000000"
        );
        assert_eq!(result.unsigned_tx.outputs[0].ergo_tree, "0008cd03lock");
        assert!(result.unsigned_tx.outputs[0]
            .additional_registers
            .contains_key("R4"));

        let change_val: i64 = result.unsigned_tx.outputs[1].value.parse().unwrap();
        assert!(change_val > 0);

        assert_eq!(result.unsigned_tx.outputs[2].value, LOCK_TX_FEE.to_string());

        assert_eq!(result.summary.token_name, "ERG");
        assert_eq!(result.summary.amount, 1_000_000_000);
        assert_eq!(result.summary.target_chain, "cardano");
    }

    #[test]
    fn test_build_lock_tx_token() {
        let token_id = "abc123def456";
        let request = LockRequest {
            ergo_token_id: token_id.to_string(),
            amount: 1000,
            target_chain: "cardano".to_string(),
            target_address: "addr1qxck39mfuzd4tcamp02gycm7aqnlhkxskvfjxhe0ekmzp8lrstxkxqyer6vk6g3emeqyqsghx09gvpqx9fhsgqx6wlqyu66ts".to_string(),
            bridge_fee: 100,
            network_fee: 50,
            user_address: "9ftest".to_string(),
            user_ergo_tree: "0008cd03test".to_string(),
            user_inputs: vec![mock_utxo(
                100_000_000, // 0.1 ERG
                vec![(token_id, 5000)],
            )],
            current_height: 1000,
        };

        let result = build_lock_tx(&request, "0008cd03lock").unwrap();

        assert_eq!(
            result.unsigned_tx.outputs[0].value,
            MIN_BOX_VALUE.to_string()
        );
        assert_eq!(result.unsigned_tx.outputs[0].assets.len(), 1);
        assert_eq!(result.unsigned_tx.outputs[0].assets[0].token_id, token_id);
        assert_eq!(result.unsigned_tx.outputs[0].assets[0].amount, "1000");

        let change_tokens = &result.unsigned_tx.outputs[1].assets;
        let change_tok = change_tokens
            .iter()
            .find(|a| a.token_id == token_id)
            .unwrap();
        assert_eq!(change_tok.amount, "4000");
    }

    #[test]
    fn test_build_lock_tx_invalid_amount() {
        let request = LockRequest {
            ergo_token_id: "erg".to_string(),
            amount: 0,
            target_chain: "cardano".to_string(),
            target_address: "addr1qtest".to_string(),
            bridge_fee: 0,
            network_fee: 0,
            user_address: "9ftest".to_string(),
            user_ergo_tree: "0008cd03test".to_string(),
            user_inputs: vec![],
            current_height: 1000,
        };

        let result = build_lock_tx(&request, "0008cd03lock");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_lock_tx_r4_content() {
        let request = LockRequest {
            ergo_token_id: "erg".to_string(),
            amount: 1_000_000_000,
            target_chain: "cardano".to_string(),
            target_address: "addr1qxck39mfuzd4tcamp02gycm7aqnlhkxskvfjxhe0ekmzp8lrstxkxqyer6vk6g3emeqyqsghx09gvpqx9fhsgqx6wlqyu66ts".to_string(),
            bridge_fee: 300000,
            network_fee: 500000,
            user_address: "9fRQ5GobV2".to_string(),
            user_ergo_tree: "0008cd03test".to_string(),
            user_inputs: vec![mock_utxo(2_000_000_000, vec![])],
            current_height: 1000,
        };

        let result = build_lock_tx(&request, "0008cd03lock").unwrap();
        let r4 = result.unsigned_tx.outputs[0]
            .additional_registers
            .get("R4")
            .unwrap();

        let bytes = hex::decode(r4).unwrap();
        assert_eq!(bytes[0], 0x0e); // Coll
        assert_eq!(bytes[1], 0x0c); // Coll[SByte]
        assert_eq!(bytes[2], 0x05); // 5 elements
    }
}
