pub mod capabilities;

use std::sync::Arc;

use citadel_core::{BlockHeight, NodeConfig, NodeError};
use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_node_interface::NodeInterface;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const NODE_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub use capabilities::{CapabilityTier, NodeCapabilities};

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub name: Option<String>,
    pub decimals: Option<u32>,
    pub emission_amount: Option<i64>,
}

pub type Result<T> = std::result::Result<T, NodeError>;

#[derive(Clone)]
pub struct NodeClient {
    inner: Arc<NodeInterface>,
    capabilities: Arc<RwLock<Option<NodeCapabilities>>>,
    config: NodeConfig,
}

impl NodeClient {
    pub async fn new(config: NodeConfig) -> Result<Self> {
        let node = NodeInterface::from_url_str(&config.api_key, &config.url)
            .await
            .map_err(|e| NodeError::Unreachable {
                url: format!("{}: {}", config.url, e),
            })?;

        let client = Self {
            inner: Arc::new(node),
            capabilities: Arc::new(RwLock::new(None)),
            config,
        };

        client.refresh_capabilities().await;

        Ok(client)
    }

    pub fn inner(&self) -> &NodeInterface {
        &self.inner
    }

    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    pub async fn refresh_capabilities(&self) {
        let caps = capabilities::detect_capabilities(&self.inner).await;
        let mut lock = self.capabilities.write().await;
        *lock = Some(caps);
    }

    pub async fn capabilities(&self) -> Option<NodeCapabilities> {
        let lock = self.capabilities.read().await;
        lock.clone()
    }

    pub async fn require_capabilities(&self) -> std::result::Result<NodeCapabilities, String> {
        self.capabilities()
            .await
            .ok_or_else(|| "Node capabilities not available".to_string())
    }

    pub async fn current_height(&self) -> Result<BlockHeight> {
        timed_request(self.inner.current_block_height()).await
    }

    pub async fn is_online(&self) -> bool {
        timed_request(self.inner.current_block_height())
            .await
            .is_ok()
    }

    pub async fn node_name(&self) -> Option<String> {
        timed_request(self.inner.node_info())
            .await
            .ok()
            .and_then(|info| info["name"].as_str().map(|s| s.to_string()))
    }

    /// Returns (nanoErgs, Vec<(token_id, amount)>). Requires extraIndex.
    pub async fn get_address_balances(&self, address: &str) -> Result<(u64, Vec<(String, u64)>)> {
        let boxes = timed_request(self.inner.unspent_boxes_by_address(
            &address.to_string(),
            0,
            500,
        ))
        .await?;

        let erg_balance: u64 = boxes.iter().map(|b| *b.value.as_u64()).sum();

        use std::collections::HashMap;
        let mut token_balances: HashMap<String, u64> = HashMap::new();

        for ergo_box in &boxes {
            if let Some(tokens) = ergo_box.tokens.as_ref() {
                for token in tokens.iter() {
                    let token_id: String = token.token_id.into();
                    let amount = *token.amount.as_u64();
                    *token_balances.entry(token_id).or_insert(0) += amount;
                }
            }
        }

        let tokens: Vec<(String, u64)> = token_balances.into_iter().collect();
        Ok((erg_balance, tokens))
    }

    /// Get unspent boxes for an address in EIP-12 format.
    /// Makes an additional API call per box to fetch tx context (transactionId + index).
    pub async fn get_address_utxos(&self, address: &str) -> Result<Vec<ergo_tx::Eip12InputBox>> {
        let boxes = timed_request(self.inner.unspent_boxes_by_address(
            &address.to_string(),
            0,
            500,
        ))
        .await?;

        let mut eip12_boxes = Vec::new();
        for ergo_box in boxes {
            let box_id = ergo_box.box_id().to_string();
            let (tx_id, index) = self.get_box_context(&box_id).await?;
            eip12_boxes.push(ergo_tx::Eip12InputBox::from_ergo_box(
                &ergo_box, tx_id, index,
            ));
        }

        Ok(eip12_boxes)
    }

    pub async fn get_token_info(&self, token_id: &str) -> Result<TokenInfo> {
        let endpoint = format!("/blockchain/token/byId/{}", token_id);
        let response = timed_request(self.inner.send_get_req(&endpoint)).await?;

        let json: serde_json::Value = response.json().await.map_err(|e| NodeError::ApiError {
            message: format!("Failed to parse token info: {}", e),
        })?;

        Ok(TokenInfo {
            name: json["name"].as_str().map(|s| s.to_string()),
            decimals: json["decimals"].as_u64().map(|d| d as u32),
            emission_amount: json["emissionAmount"].as_i64(),
        })
    }

    /// Requires extraIndex.
    pub async fn get_recent_transactions(
        &self,
        address: &str,
        limit: u64,
    ) -> Result<Vec<serde_json::Value>> {
        let paged = timed_request(self.inner.transactions_by_address(
            &address.to_string(),
            0,
            limit,
        ))
        .await?;
        Ok(paged.items)
    }

    pub async fn get_full_node_info(&self) -> Result<serde_json::Value> {
        timed_request(self.inner.node_info()).await
    }

    /// Requires extraIndex.
    pub async fn get_transaction_by_id(&self, tx_id: &str) -> Result<serde_json::Value> {
        timed_request(
            self.inner
                .blockchain_transaction_from_id(&tx_id.to_string()),
        )
        .await
    }

    pub async fn get_unconfirmed_transaction_by_id(
        &self,
        tx_id: &str,
    ) -> Result<serde_json::Value> {
        timed_request(self.inner.unconfirmed_transaction_by_id(tx_id)).await
    }

    pub async fn get_block_by_id(&self, header_id: &str) -> Result<serde_json::Value> {
        timed_request(self.inner.get_block(header_id)).await
    }

    pub async fn get_block_header_by_id(&self, header_id: &str) -> Result<serde_json::Value> {
        timed_request(self.inner.get_block_header(header_id)).await
    }

    /// May return multiple IDs due to forks.
    pub async fn get_block_ids_at_height(&self, height: u64) -> Result<Vec<String>> {
        timed_request(self.inner.block_ids_at_height(height)).await
    }

    pub async fn get_last_block_headers(
        &self,
        count: u32,
    ) -> Result<Vec<ergo_lib::ergo_chain_types::Header>> {
        timed_request(self.inner.get_last_block_headers(count)).await
    }

    pub async fn get_block_tx_count(&self, header_id: &str) -> Result<usize> {
        let json = timed_request(self.inner.get_block_transactions(header_id)).await?;
        Ok(json["transactions"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0))
    }

    /// Raw JSON variant that preserves all node fields (unlike the typed version).
    pub async fn get_last_block_headers_raw(&self, count: u32) -> Result<Vec<serde_json::Value>> {
        let endpoint = format!("/blocks/lastHeaders/{}", count);
        let response = timed_request(self.inner.send_get_req(&endpoint)).await?;
        let json: Vec<serde_json::Value> =
            response.json().await.map_err(|e| NodeError::ApiError {
                message: format!("Failed to parse block headers: {}", e),
            })?;
        Ok(json)
    }

    pub async fn get_mempool_transactions(&self) -> Result<Vec<serde_json::Value>> {
        timed_request(self.inner.mempool_transactions()).await
    }

    /// Raw blockchain box (includes spentTransactionId, unlike UTXO-set lookups).
    pub async fn get_blockchain_box_by_id(&self, box_id: &str) -> Result<serde_json::Value> {
        let endpoint = format!("/blockchain/box/byId/{}", box_id);
        let response = timed_request(self.inner.send_get_req(&endpoint)).await?;
        response.json().await.map_err(|e| NodeError::ApiError {
            message: format!("Failed to parse box: {}", e),
        })
    }

    /// Returns (items, total_count). Requires extraIndex.
    pub async fn get_transactions_by_address(
        &self,
        address: &str,
        offset: u64,
        limit: u64,
    ) -> Result<(Vec<serde_json::Value>, u64)> {
        let paged = timed_request(self.inner.transactions_by_address(
            &address.to_string(),
            offset,
            limit,
        ))
        .await?;
        Ok((paged.items, paged.total))
    }

    pub async fn get_unconfirmed_by_address(
        &self,
        address: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let ergo_tree_hex = address_to_ergo_tree(address).ok_or_else(|| NodeError::ApiError {
            message: format!("Could not derive ergoTree from address: {}", address),
        })?;

        self.get_unconfirmed_by_ergo_tree(&ergo_tree_hex).await
    }

    async fn get_unconfirmed_by_ergo_tree(
        &self,
        ergo_tree_hex: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let endpoint = "/transactions/unconfirmed/byErgoTree?offset=0&limit=100";
        // Ergo node expects a JSON-quoted string body: "ergoTreeHex"
        let body = format!("\"{}\"", ergo_tree_hex);

        let response = timed_request(self.inner.send_post_req(endpoint, body)).await?;

        let text = response.text().await.map_err(|e| NodeError::ApiError {
            message: format!("Failed to read mempool response: {}", e),
        })?;

        if text.is_empty() {
            return Ok(Vec::new());
        }

        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| NodeError::ApiError {
                message: format!("Failed to parse mempool response: {}", e),
            })?;

        match value {
            serde_json::Value::Array(arr) => Ok(arr),
            _ => Ok(Vec::new()),
        }
    }

    /// Works regardless of extraIndex availability.
    pub async fn get_box_by_id(&self, box_id: &citadel_core::BoxId) -> Result<ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox> {
        timed_request(self.inner.box_from_id_with_pool(box_id.as_str()))
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("not found") || msg.contains("404") {
                    NodeError::BoxNotFound {
                        box_id: box_id.to_string(),
                    }
                } else {
                    e
                }
            })
    }

    /// Requires extraIndex. Returns error in Basic mode.
    pub async fn get_boxes_by_token_id(
        &self,
        capabilities: &NodeCapabilities,
        token_id: &citadel_core::TokenId,
        limit: u64,
    ) -> Result<Vec<ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox>> {
        match capabilities.capability_tier {
            CapabilityTier::Full | CapabilityTier::IndexLagging => {
                let ergo_token_id: ergo_lib::ergotree_ir::chain::token::TokenId =
                    token_id.as_str().parse().map_err(|e| NodeError::ApiError {
                        message: format!("Invalid token ID format: {}", e),
                    })?;

                let boxes = timed_request(
                    self.inner.unspent_boxes_by_token_id(&ergo_token_id, 0, limit),
                )
                .await?;

                if capabilities.capability_tier == CapabilityTier::IndexLagging {
                    tracing::warn!(
                        token_id = %token_id,
                        index_lag = ?capabilities.index_lag(),
                        "Using potentially stale box data from lagging index"
                    );
                }

                Ok(boxes)
            }
            CapabilityTier::Basic => Err(NodeError::ExtraIndexRequired {
                feature: "token ID lookup",
            }),
        }
    }

    pub async fn get_box_by_token_id(
        &self,
        capabilities: &NodeCapabilities,
        token_id: &citadel_core::TokenId,
    ) -> Result<ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox> {
        let boxes = self.get_boxes_by_token_id(capabilities, token_id, 1).await?;

        boxes
            .into_iter()
            .next()
            .ok_or_else(|| NodeError::BoxNotFound {
                box_id: format!("box with token {}", token_id),
            })
    }

    /// Returns (transactionId, output index) for EIP-12 input construction.
    pub async fn get_box_creation_info(&self, box_id: &str) -> Result<(String, u16)> {
        let endpoint = format!("/blockchain/box/byId/{}", box_id);
        let response = timed_request(self.inner.send_get_req(&endpoint)).await?;

        let json: serde_json::Value = response.json().await.map_err(|e| NodeError::ApiError {
            message: format!("Failed to parse box response: {}", e),
        })?;

        let tx_id = json["transactionId"]
            .as_str()
            .ok_or_else(|| NodeError::ApiError {
                message: format!("Missing transactionId in box {} response", box_id),
            })?
            .to_string();

        let index = json["index"].as_u64().ok_or_else(|| NodeError::ApiError {
            message: format!("Missing index in box {} response", box_id),
        })? as u16;

        Ok((tx_id, index))
    }

    async fn get_box_context(&self, box_id: &str) -> Result<(String, u16)> {
        self.get_box_creation_info(box_id).await
    }

    /// Mempool-aware UTXOs: confirmed minus mempool-spent, plus unconfirmed outputs.
    /// Enables 0-conf chained transactions.
    pub async fn get_effective_utxos(&self, address: &str) -> Result<Vec<ergo_tx::Eip12InputBox>> {
        let confirmed = self.get_address_utxos(address).await?;

        let user_ergo_tree = match address_to_ergo_tree(address) {
            Some(tree) => tree,
            None => {
                tracing::warn!(
                    "Could not derive ergoTree from address, using confirmed UTXOs only"
                );
                return Ok(confirmed);
            }
        };

        let mempool_txs = match self.get_unconfirmed_by_ergo_tree(&user_ergo_tree).await {
            Ok(txs) => txs,
            Err(e) => {
                tracing::warn!("Mempool query failed, using confirmed UTXOs only: {}", e);
                return Ok(confirmed);
            }
        };

        if mempool_txs.is_empty() {
            return Ok(confirmed);
        }

        let mut spent_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for tx in &mempool_txs {
            if let Some(inputs) = tx["inputs"].as_array() {
                for input in inputs {
                    if let Some(box_id) = input["boxId"].as_str() {
                        spent_ids.insert(box_id.to_string());
                    }
                }
            }
        }

        let mut effective: Vec<ergo_tx::Eip12InputBox> = confirmed
            .into_iter()
            .filter(|utxo| !spent_ids.contains(&utxo.box_id))
            .collect();

        for tx in &mempool_txs {
            let tx_id = match tx["id"].as_str() {
                Some(id) => id,
                None => continue,
            };

            if let Some(outputs) = tx["outputs"].as_array() {
                for (idx, output) in outputs.iter().enumerate() {
                    let ergo_tree = match output["ergoTree"].as_str() {
                        Some(et) => et,
                        None => continue,
                    };

                    if ergo_tree != user_ergo_tree {
                        continue;
                    }

                    let box_id = match output["boxId"].as_str() {
                        Some(id) => id.to_string(),
                        None => continue,
                    };

                    if spent_ids.contains(&box_id) {
                        continue;
                    }

                    if effective.iter().any(|u| u.box_id == box_id) {
                        continue;
                    }

                    if let Some(eip12) = json_output_to_eip12(output, tx_id, idx as u16) {
                        effective.push(eip12);
                    }
                }
            }
        }

        Ok(effective)
    }

    pub async fn get_eip12_box_by_id(&self, box_id: &str) -> Result<ergo_tx::Eip12InputBox> {
        let ergo_box = timed_request(self.inner.box_from_id_with_pool(box_id)).await?;
        let (tx_id, index) = self.get_box_context(box_id).await?;
        Ok(ergo_tx::Eip12InputBox::from_ergo_box(
            &ergo_box, tx_id, index,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub address: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeProbeResult {
    pub url: String,
    pub name: Option<String>,
    pub chain_height: u64,
    pub capability_tier: String,
    pub latency_ms: u64,
}

impl NodeClient {
    pub async fn get_connected_peers(&self) -> Result<Vec<PeerInfo>> {
        let response = timed_request(self.inner.send_get_req("/peers/connected")).await?;

        let json: Vec<serde_json::Value> =
            response.json().await.map_err(|e| NodeError::ApiError {
                message: format!("Failed to parse peers response: {}", e),
            })?;

        let peers = json
            .into_iter()
            .filter_map(|p| {
                let address = p["address"].as_str()?.to_string();
                let name = p["name"].as_str().map(|s| s.to_string());
                Some(PeerInfo { address, name })
            })
            .collect();

        Ok(peers)
    }
}

/// Returns None on timeout (4s) or unreachable.
pub async fn probe_node(url: &str) -> Option<NodeProbeResult> {
    let start = std::time::Instant::now();

    let node = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        NodeInterface::from_url_str("", url),
    )
    .await
    .ok()?
    .ok()?;

    let info = tokio::time::timeout(std::time::Duration::from_secs(4), node.node_info())
        .await
        .ok()?
        .ok()?;

    let latency_ms = start.elapsed().as_millis() as u64;

    let chain_height = info["fullHeight"].as_u64().unwrap_or(0);
    let name = info["name"].as_str().map(|s| s.to_string());

    let tier =
        match tokio::time::timeout(std::time::Duration::from_secs(4), node.get_indexed_height())
            .await
        {
            Ok(Ok(ih)) => {
                let indexed = ih.indexed_height as u64;
                let lag = chain_height.saturating_sub(indexed);
                if lag <= 10 {
                    "Full"
                } else {
                    "IndexLagging"
                }
            }
            _ => "Basic",
        };

    Some(NodeProbeResult {
        url: url.to_string(),
        name,
        chain_height,
        capability_tier: tier.to_string(),
        latency_ms,
    })
}

async fn timed_request<T, E: std::fmt::Display>(
    fut: impl std::future::Future<Output = std::result::Result<T, E>>,
) -> Result<T> {
    tokio::time::timeout(NODE_REQUEST_TIMEOUT, fut)
        .await
        .map_err(|_| NodeError::ApiError {
            message: format!(
                "Node request timed out after {}s",
                NODE_REQUEST_TIMEOUT.as_secs()
            ),
        })?
        .map_err(|e| NodeError::ApiError {
            message: e.to_string(),
        })
}

fn address_to_ergo_tree(address: &str) -> Option<String> {
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    let addr = encoder.parse_address_from_str(address).ok()?;
    let tree = addr.script().ok()?;
    let bytes = tree.sigma_serialize_bytes().ok()?;
    Some(bytes.iter().map(|b| format!("{:02x}", b)).collect())
}

fn json_output_to_eip12(
    output: &serde_json::Value,
    tx_id: &str,
    index: u16,
) -> Option<ergo_tx::Eip12InputBox> {
    let box_id = output["boxId"].as_str()?.to_string();
    let value = output["value"].as_u64()?.to_string();
    let ergo_tree = output["ergoTree"].as_str()?.to_string();
    let creation_height = output["creationHeight"].as_i64()? as i32;

    let assets = output["assets"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    Some(ergo_tx::Eip12Asset {
                        token_id: a["tokenId"].as_str()?.to_string(),
                        amount: a["amount"].as_u64()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let additional_registers = output["additionalRegisters"]
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| {
                    let val = v.as_str().unwrap_or_default();
                    // Node may wrap register values in {"serializedValue": "..."} or return plain hex
                    let hex = if val.is_empty() {
                        v["serializedValue"].as_str()?.to_string()
                    } else {
                        val.to_string()
                    };
                    Some((k.clone(), hex))
                })
                .collect()
        })
        .unwrap_or_default();

    Some(ergo_tx::Eip12InputBox {
        box_id,
        transaction_id: tx_id.to_string(),
        index,
        value,
        ergo_tree,
        assets,
        creation_height,
        additional_registers,
        extension: std::collections::HashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NodeConfig::default();
        assert_eq!(config.url, "http://127.0.0.1:9053");
    }

    #[test]
    fn test_json_output_to_eip12() {
        let output = serde_json::json!({
            "boxId": "abc123",
            "value": 1000000000u64,
            "ergoTree": "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "assets": [
                {"tokenId": "token1", "amount": 100u64}
            ],
            "creationHeight": 999000,
            "additionalRegisters": {}
        });

        let result = json_output_to_eip12(&output, "tx_id_123", 0);
        assert!(result.is_some());

        let eip12 = result.unwrap();
        assert_eq!(eip12.box_id, "abc123");
        assert_eq!(eip12.transaction_id, "tx_id_123");
        assert_eq!(eip12.index, 0);
        assert_eq!(eip12.value, "1000000000");
        assert_eq!(eip12.assets.len(), 1);
        assert_eq!(eip12.assets[0].token_id, "token1");
        assert_eq!(eip12.assets[0].amount, "100");
        assert_eq!(eip12.creation_height, 999000);
    }

    #[test]
    fn test_json_output_to_eip12_no_assets() {
        let output = serde_json::json!({
            "boxId": "abc123",
            "value": 1000000u64,
            "ergoTree": "0008cd...",
            "creationHeight": 100,
            "additionalRegisters": {}
        });

        let result = json_output_to_eip12(&output, "tx1", 1);
        assert!(result.is_some());
        assert!(result.unwrap().assets.is_empty());
    }

    #[test]
    fn test_json_output_to_eip12_missing_fields() {
        // Missing boxId
        let output = serde_json::json!({
            "value": 1000000u64,
            "ergoTree": "0008cd..."
        });
        assert!(json_output_to_eip12(&output, "tx1", 0).is_none());
    }
}
