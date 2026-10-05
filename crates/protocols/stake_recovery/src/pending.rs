//! Pending spends and terminal outputs shared by discovery and recovery builds.
use std::collections::{HashMap, HashSet};

use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;

use crate::{RecoveryError, PAIDEIA_PROXY_ERGO_TREE, PROTOCOLS};

pub(crate) struct PendingBoxes {
    spent_ids: HashSet<String>,
    outputs: HashMap<String, Vec<ErgoBox>>,
}

impl PendingBoxes {
    pub async fn load(node: &ergo_node_client::NodeClient) -> Result<Self, RecoveryError> {
        let transactions = node.get_mempool_transactions().await.map_err(|e| {
            RecoveryError::NodeError(format!("Cannot check pending stake transactions: {e}. Retry when the node's mempool is available."))
        })?;
        Self::from_transactions(&transactions)
    }

    fn from_transactions(transactions: &[serde_json::Value]) -> Result<Self, RecoveryError> {
        let invalid = |detail: &str| {
            RecoveryError::NodeError(format!("Invalid mempool transaction: {detail}"))
        };
        let mut spent_ids = HashSet::new();
        for tx in transactions {
            for input in tx["inputs"]
                .as_array()
                .ok_or_else(|| invalid("missing inputs"))?
            {
                spent_ids.insert(
                    input["boxId"]
                        .as_str()
                        .ok_or_else(|| invalid("missing input boxId"))?
                        .to_string(),
                );
            }
        }
        let mut outputs: HashMap<String, Vec<ErgoBox>> = HashMap::new();
        let mut seen = HashSet::new();
        for tx in transactions {
            let tx_id = tx["id"].as_str().ok_or_else(|| invalid("missing id"))?;
            for (index, output) in tx["outputs"]
                .as_array()
                .ok_or_else(|| invalid("missing outputs"))?
                .iter()
                .enumerate()
            {
                let tree = output["ergoTree"]
                    .as_str()
                    .ok_or_else(|| invalid("missing ergoTree"))?;
                if tree != PAIDEIA_PROXY_ERGO_TREE
                    && !PROTOCOLS.iter().any(|cfg| {
                        tree == cfg.stake_box_ergo_tree || tree == cfg.stake_state_ergo_tree
                    })
                {
                    continue;
                }
                let mut box_json = output.clone();
                box_json["transactionId"] = tx_id.into();
                box_json["index"] = index.into();
                let b: ErgoBox = serde_json::from_value(box_json).map_err(|e| {
                    RecoveryError::NodeError(format!("Invalid pending stake box: {e}"))
                })?;
                let box_id = b.box_id().to_string();
                if !spent_ids.contains(&box_id) && seen.insert(box_id) {
                    outputs.entry(tree.to_string()).or_default().push(b);
                }
            }
        }
        Ok(Self { spent_ids, outputs })
    }

    pub fn is_unspent(&self, b: &ErgoBox) -> bool {
        !self.spent_ids.contains(&b.box_id().to_string())
    }

    pub fn outputs_for(&self, tree: &str) -> impl Iterator<Item = &ErgoBox> {
        self.outputs.get(tree).into_iter().flatten()
    }
}
