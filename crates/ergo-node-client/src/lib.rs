//! ergo-node-client: Wrapper around ergo-node-interface-rust with capability detection
//!
//! This crate provides a high-level client for interacting with Ergo nodes,
//! including automatic capability detection (extraIndex) and graceful degradation.

pub mod capabilities;
pub mod queries;

use std::sync::Arc;

use citadel_core::{BlockHeight, NodeConfig, NodeError};
use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_node_interface::NodeInterface;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Default timeout for node API calls (30 seconds).
/// Long enough for slow nodes, short enough to avoid perpetual spinners.
const NODE_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub use capabilities::{CapabilityTier, NodeCapabilities};

/// Token metadata from the node
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub name: Option<String>,
    pub decimals: Option<u32>,
    pub emission_amount: Option<i64>,
}

/// Result type for node client operations
pub type Result<T> = std::result::Result<T, NodeError>;

/// High-level Ergo node client with capability detection
#[derive(Clone)]
pub struct NodeClient {
    inner: Arc<NodeInterface>,
    capabilities: Arc<RwLock<Option<NodeCapabilities>>>,
    config: NodeConfig,
}

impl NodeClient {
    /// Create a new node client with capability probing
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

        // Initial capability detection
        client.refresh_capabilities().await;

        Ok(client)
    }

    /// Create without probing (for testing or when node may be offline)
    pub fn new_without_probe(config: NodeConfig) -> Result<Self> {
        let node = NodeInterface::new_without_probe(&config.api_key, "127.0.0.1", "9053").map_err(
            |e| NodeError::Unreachable {
                url: format!("{}: {}", config.url, e),
            },
        )?;

        Ok(Self {
            inner: Arc::new(node),
            capabilities: Arc::new(RwLock::new(None)),
            config,
        })
    }

    /// Get the underlying node interface (for advanced usage)
    pub fn inner(&self) -> &NodeInterface {
        &self.inner
    }

    /// Get the current node configuration
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    /// Refresh capability detection
    pub async fn refresh_capabilities(&self) {
        let caps = capabilities::detect_capabilities(&self.inner).await;
        let mut lock = self.capabilities.write().await;
        *lock = Some(caps);
    }

    /// Get current capabilities (may be stale if not recently refreshed)
    pub async fn capabilities(&self) -> Option<NodeCapabilities> {
        let lock = self.capabilities.read().await;
        lock.clone()
    }

    /// Get current block height
    pub async fn current_height(&self) -> Result<BlockHeight> {
        timed_request(self.inner.current_block_height()).await
    }

    /// Check if node is online
    pub async fn is_online(&self) -> bool {
        timed_request(self.inner.current_block_height())
            .await
            .is_ok()
    }

    /// Get node name from /info endpoint
    pub async fn node_name(&self) -> Option<String> {
        timed_request(self.inner.node_info())
            .await
            .ok()
            .and_then(|info| info["name"].as_str().map(|s| s.to_string()))
    }

    /// Get ERG and token balances for an address
    /// Returns (nanoErgs, Vec<(token_id, amount)>)
    /// Requires extraIndex capability (Full or IndexLagging tier)
    pub async fn get_address_balances(&self, address: &str) -> Result<(u64, Vec<(String, u64)>)> {
        // Get all unspent boxes for this address
        let boxes = timed_request(self.inner.unspent_boxes_by_address(
            &address.to_string(),
            0,
            500,
        ))
        .await?;

        // Aggregate ERG balance
        let erg_balance: u64 = boxes.iter().map(|b| *b.value.as_u64()).sum();

        // Aggregate token balances
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

    /// Get unspent boxes for an address in EIP12 format (for tx building)
    pub async fn get_address_utxos(&self, address: &str) -> Result<Vec<ergo_tx::Eip12InputBox>> {
        // Use same limit as get_address_balances (500) to ensure we get all UTXOs
        // Note: This makes an additional API call per box to get tx context
        let boxes = timed_request(self.inner.unspent_boxes_by_address(
            &address.to_string(),
            0,
            500,
        ))
        .await?;

        // Convert to EIP12 format - need tx context for each box
        let mut eip12_boxes = Vec::new();
        for ergo_box in boxes {
            let box_id = ergo_box.box_id().to_string();
            // Query for tx context
            let (tx_id, index) = self.get_box_context(&box_id).await?;
            eip12_boxes.push(ergo_tx::Eip12InputBox::from_ergo_box(
                &ergo_box, tx_id, index,
            ));
        }

        Ok(eip12_boxes)
    }

    /// Get token metadata (name, decimals) from the node
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

    /// Get recent transactions for an address (most recent first).
    /// Returns raw JSON values from the node's `/blockchain/transaction/byAddress` endpoint.
    /// Requires extraIndex capability.
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

    // =========================================================================
    // Explorer methods
    // =========================================================================

    /// Get full node info from /info endpoint (version, state, network, etc.)
    pub async fn get_full_node_info(&self) -> Result<serde_json::Value> {
        timed_request(self.inner.node_info()).await
    }

    /// Get a confirmed transaction by ID.
    /// Requires extraIndex capability.
    pub async fn get_transaction_by_id(&self, tx_id: &str) -> Result<serde_json::Value> {
        timed_request(
            self.inner
                .blockchain_transaction_from_id(&tx_id.to_string()),
        )
        .await
    }

    /// Get an unconfirmed transaction from the mempool by ID.
    pub async fn get_unconfirmed_transaction_by_id(
        &self,
        tx_id: &str,
    ) -> Result<serde_json::Value> {
        timed_request(self.inner.unconfirmed_transaction_by_id(tx_id)).await
    }

    /// Get a full block by header ID.
    pub async fn get_block_by_id(&self, header_id: &str) -> Result<serde_json::Value> {
        timed_request(self.inner.get_block(header_id)).await
    }

    /// Get a block header by ID.
    pub async fn get_block_header_by_id(&self, header_id: &str) -> Result<serde_json::Value> {
        timed_request(self.inner.get_block_header(header_id)).await
    }

    /// Get block IDs at a given height (may return multiple due to forks).
    pub async fn get_block_ids_at_height(&self, height: u64) -> Result<Vec<String>> {
        timed_request(self.inner.block_ids_at_height(height)).await
    }

    /// Get the most recent block headers (typed).
    pub async fn get_last_block_headers(
        &self,
        count: u32,
    ) -> Result<Vec<ergo_lib::ergo_chain_types::Header>> {
        timed_request(self.inner.get_last_block_headers(count)).await
    }

    /// Get the transaction count for a block by header ID.
    pub async fn get_block_tx_count(&self, header_id: &str) -> Result<usize> {
        let json = timed_request(self.inner.get_block_transactions(header_id)).await?;
        Ok(json["transactions"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0))
    }

    /// Get the most recent block headers as raw JSON (preserves all node fields).
    pub async fn get_last_block_headers_raw(&self, count: u32) -> Result<Vec<serde_json::Value>> {
        let endpoint = format!("/blocks/lastHeaders/{}", count);
        let response = timed_request(self.inner.send_get_req(&endpoint)).await?;
        let json: Vec<serde_json::Value> =
            response.json().await.map_err(|e| NodeError::ApiError {
                message: format!("Failed to parse block headers: {}", e),
            })?;
        Ok(json)
    }

    /// Get unconfirmed transactions from the mempool.
    pub async fn get_mempool_transactions(&self) -> Result<Vec<serde_json::Value>> {
        timed_request(self.inner.mempool_transactions()).await
    }

    /// Get a raw box by ID from the blockchain (includes spentTransactionId, etc.)
    pub async fn get_blockchain_box_by_id(&self, box_id: &str) -> Result<serde_json::Value> {
        let endpoint = format!("/blockchain/box/byId/{}", box_id);
        let response = timed_request(self.inner.send_get_req(&endpoint)).await?;
        response.json().await.map_err(|e| NodeError::ApiError {
            message: format!("Failed to parse box: {}", e),
        })
    }

    /// Get transactions for an address with pagination.
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

    /// Get unconfirmed transactions by address from the mempool.
    ///
    /// Converts the address to its ErgoTree hex representation and queries the
    /// node's `POST /transactions/unconfirmed/byErgoTree` endpoint.
    pub async fn get_unconfirmed_by_address(
        &self,
        address: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let ergo_tree_hex = address_to_ergo_tree(address).ok_or_else(|| NodeError::ApiError {
            message: format!("Could not derive ergoTree from address: {}", address),
        })?;

        self.get_unconfirmed_by_ergo_tree(&ergo_tree_hex).await
    }

    /// Get unconfirmed transactions by ErgoTree hex from the mempool.
    ///
    /// Calls `POST /transactions/unconfirmed/byErgoTree` with the ergoTree
    /// as a JSON-quoted string body (required by the Ergo node API).
    async fn get_unconfirmed_by_ergo_tree(
        &self,
        ergo_tree_hex: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let endpoint = "/transactions/unconfirmed/byErgoTree?offset=0&limit=100";
        // Ergo node expects a JSON string body: "ergoTreeHex" (with quotes)
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

    // =========================================================================
    // Internal helpers
    // =========================================================================

    /// Get transaction ID and index for a box
    async fn get_box_context(&self, box_id: &str) -> Result<(String, u16)> {
        let endpoint = format!("/blockchain/box/byId/{}", box_id);
        let response = timed_request(self.inner.send_get_req(&endpoint)).await?;

        let json: serde_json::Value = response.json().await.map_err(|e| NodeError::ApiError {
            message: format!("Failed to parse: {}", e),
        })?;

        let tx_id = json["transactionId"]
            .as_str()
            .ok_or_else(|| NodeError::ApiError {
                message: "Missing transactionId".to_string(),
            })?
            .to_string();

        let index = json["index"].as_u64().ok_or_else(|| NodeError::ApiError {
            message: "Missing index".to_string(),
        })? as u16;

        Ok((tx_id, index))
    }

    /// Get effective UTXOs for an address (mempool-aware).
    ///
    /// Merges confirmed UTXOs with unconfirmed mempool state:
    /// 1. Fetches confirmed UTXOs
    /// 2. Derives ergoTree from address (works even when all UTXOs are spent)
    /// 3. Queries mempool via `POST /transactions/unconfirmed/byErgoTree`
    /// 4. Removes confirmed UTXOs that are spent in mempool txs
    /// 5. Adds unconfirmed outputs that belong to this address
    ///
    /// This enables 0-conf chained transactions: after submitting a tx,
    /// the change output is immediately visible for the next tx.
    pub async fn get_effective_utxos(&self, address: &str) -> Result<Vec<ergo_tx::Eip12InputBox>> {
        // 1. Get confirmed UTXOs (the baseline)
        let confirmed = self.get_address_utxos(address).await?;

        // 2. Derive ergoTree from address — needed for mempool query and output matching
        let user_ergo_tree = match address_to_ergo_tree(address) {
            Some(tree) => tree,
            None => {
                tracing::warn!(
                    "Could not derive ergoTree from address, using confirmed UTXOs only"
                );
                return Ok(confirmed);
            }
        };

        // 3. Get unconfirmed txs involving this address via byErgoTree
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

        // 4. Collect all input boxIds from mempool txs (these are being spent)
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

        // 5. Remove confirmed UTXOs that are spent in mempool
        let mut effective: Vec<ergo_tx::Eip12InputBox> = confirmed
            .into_iter()
            .filter(|utxo| !spent_ids.contains(&utxo.box_id))
            .collect();

        // 6. Add unconfirmed outputs belonging to this address
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

                    // Check if this output belongs to the user
                    if ergo_tree != user_ergo_tree {
                        continue;
                    }

                    let box_id = match output["boxId"].as_str() {
                        Some(id) => id.to_string(),
                        None => continue,
                    };

                    // Skip if this output is already spent by another mempool tx
                    if spent_ids.contains(&box_id) {
                        continue;
                    }

                    // Skip if we already have this box
                    if effective.iter().any(|u| u.box_id == box_id) {
                        continue;
                    }

                    // Parse the output into an Eip12InputBox
                    if let Some(eip12) = json_output_to_eip12(output, tx_id, idx as u16) {
                        effective.push(eip12);
                    }
                }
            }
        }

        Ok(effective)
    }

    /// Get a box by ID in EIP-12 format (for tx building).
    /// Fetches the box from the UTXO set and converts to Eip12InputBox.
    pub async fn get_eip12_box_by_id(&self, box_id: &str) -> Result<ergo_tx::Eip12InputBox> {
        let ergo_box = timed_request(self.inner.box_from_id_with_pool(box_id)).await?;
        let (tx_id, index) = self.get_box_context(box_id).await?;
        Ok(ergo_tx::Eip12InputBox::from_ergo_box(
            &ergo_box, tx_id, index,
        ))
    }
}

/// Peer info from /peers/connected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub address: String,
    pub name: Option<String>,
}

/// Result of probing a single node URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeProbeResult {
    pub url: String,
    pub name: Option<String>,
    pub chain_height: u64,
    pub capability_tier: String,
    pub latency_ms: u64,
}

impl NodeClient {
    /// Fetch connected peers from /peers/connected
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

/// Probe a single node URL. Returns None on failure (timeout/unreachable).
/// Uses a 4-second timeout. Creates a temporary NodeInterface internally.
pub async fn probe_node(url: &str) -> Option<NodeProbeResult> {
    let start = std::time::Instant::now();

    // Create a temporary node interface with empty API key
    let node = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        NodeInterface::from_url_str("", url),
    )
    .await
    .ok()?
    .ok()?;

    // Get /info for height + name
    let info = tokio::time::timeout(std::time::Duration::from_secs(4), node.node_info())
        .await
        .ok()?
        .ok()?;

    let latency_ms = start.elapsed().as_millis() as u64;

    let chain_height = info["fullHeight"].as_u64().unwrap_or(0);
    let name = info["name"].as_str().map(|s| s.to_string());

    // Check extraIndex via /blockchain/indexedHeight
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

/// Wrap a node API call with a timeout. Converts both timeout and API errors to NodeError.
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

/// Convert an Ergo address string to its ErgoTree hex representation.
///
/// Uses ergo-lib to properly decode the address and extract the script.
fn address_to_ergo_tree(address: &str) -> Option<String> {
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    let addr = encoder.parse_address_from_str(address).ok()?;
    let tree = addr.script().ok()?;
    let bytes = tree.sigma_serialize_bytes().ok()?;
    Some(bytes.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Parse a JSON transaction output into an Eip12InputBox.
///
/// Used by `get_effective_utxos` to convert mempool outputs into usable input boxes.
fn json_output_to_eip12(
    output: &serde_json::Value,
    tx_id: &str,
    index: u16,
) -> Option<ergo_tx::Eip12InputBox> {
    let box_id = output["boxId"].as_str()?.to_string();
    let value = output["value"].as_u64()?.to_string();
    let ergo_tree = output["ergoTree"].as_str()?.to_string();
    let creation_height = output["creationHeight"].as_i64()? as i32;

    // Parse assets
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

    // Parse additional registers (R4-R9)
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
