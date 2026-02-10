//! EIP-12 Transaction Structures
//!
//! Defines the JSON structure expected by Nautilus wallet for signing.
//! Reference: EIP-12 dApp Connector specification

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// EIP-12 token/asset in a box
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip12Asset {
    pub token_id: String,
    pub amount: String,
}

impl Eip12Asset {
    pub fn new(token_id: impl Into<String>, amount: i64) -> Self {
        Self {
            token_id: token_id.into(),
            amount: amount.to_string(),
        }
    }
}

/// EIP-12 input box - FULL box data required for wallet to sign
///
/// Nautilus requires transactionId and index fields to construct proper
/// transaction inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip12InputBox {
    pub box_id: String,
    /// Transaction ID where this box was created
    pub transaction_id: String,
    /// Output index in that transaction
    pub index: u16,
    pub value: String,
    pub ergo_tree: String,
    pub assets: Vec<Eip12Asset>,
    pub creation_height: i32,
    pub additional_registers: HashMap<String, String>,
    /// Context extension - always present for signing, can be empty
    #[serde(default)]
    pub extension: HashMap<String, String>,
}

/// EIP-12 data input box - FULL box data required
///
/// Data inputs are boxes that are read but not spent (e.g., oracle box).
/// Nautilus requires the full box data, NOT just the box ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip12DataInputBox {
    pub box_id: String,
    /// Transaction ID where this box was created
    pub transaction_id: String,
    /// Output index in that transaction
    pub index: u16,
    pub value: String,
    pub ergo_tree: String,
    pub assets: Vec<Eip12Asset>,
    pub creation_height: i32,
    pub additional_registers: HashMap<String, String>,
}

/// EIP-12 output box candidate
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip12Output {
    pub value: String,
    pub ergo_tree: String,
    pub assets: Vec<Eip12Asset>,
    pub creation_height: i32,
    pub additional_registers: HashMap<String, String>,
}

impl Eip12Output {
    /// Create a simple output with no tokens or registers
    pub fn simple(value: i64, ergo_tree: impl Into<String>, height: i32) -> Self {
        Self {
            value: value.to_string(),
            ergo_tree: ergo_tree.into(),
            assets: vec![],
            creation_height: height,
            additional_registers: HashMap::new(),
        }
    }

    /// Create a fee output to the miner
    pub fn fee(value: i64, height: i32) -> Self {
        Self::simple(value, citadel_core::constants::MINER_FEE_ERGO_TREE, height)
    }

    /// Create a change output returning remaining ERG and tokens to the user.
    pub fn change(
        value: i64,
        ergo_tree: impl Into<String>,
        assets: Vec<Eip12Asset>,
        height: i32,
    ) -> Self {
        Self {
            value: value.to_string(),
            ergo_tree: ergo_tree.into(),
            assets,
            creation_height: height,
            additional_registers: HashMap::new(),
        }
    }
}

/// Complete EIP-12 unsigned transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip12UnsignedTx {
    pub inputs: Vec<Eip12InputBox>,
    pub data_inputs: Vec<Eip12DataInputBox>,
    pub outputs: Vec<Eip12Output>,
}

impl Eip12UnsignedTx {
    /// Create a new empty unsigned transaction
    pub fn new() -> Self {
        Self {
            inputs: vec![],
            data_inputs: vec![],
            outputs: vec![],
        }
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to pretty JSON string
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

impl Default for Eip12UnsignedTx {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ErgoBox Conversion (requires ergo-lib)
// =============================================================================

#[cfg(feature = "ergo-lib")]
mod ergo_lib_conversion {
    use super::*;
    use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    /// Extract assets from ErgoBox as Eip12Asset vector
    fn extract_assets(ergo_box: &ErgoBox) -> Vec<Eip12Asset> {
        ergo_box
            .tokens
            .as_ref()
            .map(|tokens| {
                tokens
                    .iter()
                    .map(|t| Eip12Asset {
                        token_id: t.token_id.into(),
                        amount: t.amount.as_u64().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract additional registers (R4-R9) as hex strings
    fn extract_registers(ergo_box: &ErgoBox) -> HashMap<String, String> {
        use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;

        let mut registers = HashMap::new();

        let reg_ids = [
            (NonMandatoryRegisterId::R4, "R4"),
            (NonMandatoryRegisterId::R5, "R5"),
            (NonMandatoryRegisterId::R6, "R6"),
            (NonMandatoryRegisterId::R7, "R7"),
            (NonMandatoryRegisterId::R8, "R8"),
            (NonMandatoryRegisterId::R9, "R9"),
        ];

        for (reg_id, reg_name) in reg_ids {
            if let Ok(Some(constant)) = ergo_box.additional_registers.get_constant(reg_id) {
                if let Ok(bytes) = constant.sigma_serialize_bytes() {
                    registers.insert(reg_name.to_string(), base16::encode_lower(&bytes));
                }
            }
        }

        registers
    }

    impl Eip12InputBox {
        /// Convert from ergo-lib ErgoBox
        ///
        /// Note: transaction_id and index must be provided separately as
        /// ErgoBox doesn't contain this context information.
        pub fn from_ergo_box(ergo_box: &ErgoBox, transaction_id: String, index: u16) -> Self {
            let assets = extract_assets(ergo_box);
            let additional_registers = extract_registers(ergo_box);

            Self {
                box_id: ergo_box.box_id().to_string(),
                transaction_id,
                index,
                value: ergo_box.value.as_i64().to_string(),
                ergo_tree: ergo_box
                    .ergo_tree
                    .sigma_serialize_bytes()
                    .map(|bytes| base16::encode_lower(&bytes))
                    .unwrap_or_default(),
                assets,
                creation_height: ergo_box.creation_height as i32,
                additional_registers,
                extension: HashMap::new(),
            }
        }
    }

    impl Eip12DataInputBox {
        /// Convert from ergo-lib ErgoBox for use as data input
        pub fn from_ergo_box(ergo_box: &ErgoBox, transaction_id: String, index: u16) -> Self {
            let assets = extract_assets(ergo_box);
            let additional_registers = extract_registers(ergo_box);

            Self {
                box_id: ergo_box.box_id().to_string(),
                transaction_id,
                index,
                value: ergo_box.value.as_i64().to_string(),
                ergo_tree: ergo_box
                    .ergo_tree
                    .sigma_serialize_bytes()
                    .map(|bytes| base16::encode_lower(&bytes))
                    .unwrap_or_default(),
                assets,
                creation_height: ergo_box.creation_height as i32,
                additional_registers,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eip12_serialization() {
        let input = Eip12InputBox {
            box_id: "abc123".to_string(),
            transaction_id: "def456".to_string(),
            index: 0,
            value: "1000000".to_string(),
            ergo_tree: "0008cd...".to_string(),
            assets: vec![],
            creation_height: 12345,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        };

        let json = serde_json::to_string(&input).unwrap();

        // Check camelCase serialization
        assert!(json.contains("boxId"));
        assert!(json.contains("transactionId"));
        assert!(json.contains("ergoTree"));
        assert!(json.contains("creationHeight"));
        assert!(json.contains("additionalRegisters"));

        // Roundtrip
        let parsed: Eip12InputBox = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.box_id, input.box_id);
        assert_eq!(parsed.transaction_id, input.transaction_id);
    }

    #[test]
    fn test_data_input_serialization() {
        let data_input = Eip12DataInputBox {
            box_id: "oracle123".to_string(),
            transaction_id: "tx789".to_string(),
            index: 0,
            value: "1000000".to_string(),
            ergo_tree: "0008cd...".to_string(),
            assets: vec![Eip12Asset::new("token123", 100)],
            creation_height: 12345,
            additional_registers: HashMap::from([("R4".to_string(), "05...".to_string())]),
        };

        let json = serde_json::to_string(&data_input).unwrap();

        // Must have full box data for Nautilus
        assert!(json.contains("transactionId"));
        assert!(json.contains("ergoTree"));
        assert!(json.contains("additionalRegisters"));
    }

    #[test]
    fn test_unsigned_tx_structure() {
        let tx = Eip12UnsignedTx {
            inputs: vec![],
            data_inputs: vec![],
            outputs: vec![Eip12Output::fee(1_100_000, 12345)],
        };

        let json = tx.to_json().unwrap();
        assert!(json.contains("inputs"));
        assert!(json.contains("dataInputs"));
        assert!(json.contains("outputs"));
    }
}
