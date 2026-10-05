//! Terminal protocol boxes in a single validated mempool snapshot.

use std::collections::HashSet;

use citadel_core::NodeError;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use serde_json::Value;

use crate::Result;

pub(crate) enum BoxFilter {
    ErgoTree(String),
    TokenId(String),
}

impl BoxFilter {
    fn matches(&self, output: &Value) -> bool {
        match self {
            Self::ErgoTree(tree) => output["ergoTree"].as_str() == Some(tree.as_str()),
            Self::TokenId(token_id) => output["assets"].as_array().is_some_and(|assets| {
                assets
                    .iter()
                    .any(|asset| asset["tokenId"].as_str() == Some(token_id.as_str()))
            }),
        }
    }
}

pub(crate) struct PendingBoxes {
    pub spent_ids: HashSet<String>,
    pub outputs: Vec<ErgoBox>,
}

impl PendingBoxes {
    pub fn from_transactions(transactions: &[Value], filter: &BoxFilter) -> Result<Self> {
        let invalid = |detail: String| NodeError::ApiError {
            message: format!("Cannot check pending protocol transactions: {detail}"),
        };
        let mut spent_ids = HashSet::new();
        for tx in transactions {
            let inputs = tx["inputs"]
                .as_array()
                .ok_or_else(|| invalid("missing transaction inputs".into()))?;
            for input in inputs {
                let box_id = input["boxId"]
                    .as_str()
                    .ok_or_else(|| invalid("missing input boxId".into()))?;
                spent_ids.insert(box_id.to_string());
            }
        }
        let mut seen = HashSet::new();
        let mut outputs = Vec::new();
        for tx in transactions {
            let tx_id = tx["id"]
                .as_str()
                .ok_or_else(|| invalid("missing transaction id".into()))?;
            let candidates = tx["outputs"]
                .as_array()
                .ok_or_else(|| invalid("missing transaction outputs".into()))?;
            for (index, output) in candidates.iter().enumerate() {
                if !filter.matches(output) {
                    continue;
                }
                let mut box_json = output.clone();
                box_json["transactionId"] = tx_id.into();
                box_json["index"] = index.into();
                let box_value: ErgoBox = serde_json::from_value(box_json)
                    .map_err(|error| invalid(format!("invalid pending box: {error}")))?;
                let box_id = box_value.box_id().to_string();
                if !spent_ids.contains(&box_id) && seen.insert(box_id) {
                    outputs.push(box_value);
                }
            }
        }
        Ok(Self { spent_ids, outputs })
    }
}
