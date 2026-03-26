//! EIP-12 transaction structures for Nautilus wallet signing.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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

/// Nautilus requires full box data including transactionId and index for signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip12InputBox {
    pub box_id: String,
    pub transaction_id: String,
    pub index: u16,
    pub value: String,
    pub ergo_tree: String,
    pub assets: Vec<Eip12Asset>,
    pub creation_height: i32,
    pub additional_registers: HashMap<String, String>,
    #[serde(default)]
    pub extension: HashMap<String, String>,
}

/// Data inputs are read-only boxes (e.g., oracle). Nautilus requires full box data, NOT just the box ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip12DataInputBox {
    pub box_id: String,
    pub transaction_id: String,
    pub index: u16,
    pub value: String,
    pub ergo_tree: String,
    pub assets: Vec<Eip12Asset>,
    pub creation_height: i32,
    pub additional_registers: HashMap<String, String>,
}

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
    pub fn simple(value: i64, ergo_tree: impl Into<String>, height: i32) -> Self {
        Self {
            value: value.to_string(),
            ergo_tree: ergo_tree.into(),
            assets: vec![],
            creation_height: height,
            additional_registers: HashMap::new(),
        }
    }

    pub fn fee(value: i64, height: i32) -> Self {
        Self::simple(value, citadel_core::constants::MINER_FEE_ERGO_TREE, height)
    }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip12UnsignedTx {
    pub inputs: Vec<Eip12InputBox>,
    pub data_inputs: Vec<Eip12DataInputBox>,
    pub outputs: Vec<Eip12Output>,
}


#[cfg(feature = "ergo-lib")]
mod ergo_lib_conversion {
    use super::*;
    use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

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

        assert!(json.contains("boxId"));
        assert!(json.contains("transactionId"));
        assert!(json.contains("ergoTree"));
        assert!(json.contains("creationHeight"));
        assert!(json.contains("additionalRegisters"));

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

        let json = serde_json::to_string(&tx).unwrap();
        assert!(json.contains("inputs"));
        assert!(json.contains("dataInputs"));
        assert!(json.contains("outputs"));
    }
}
