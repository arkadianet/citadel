//! Node query helpers with capability-aware fallbacks

use citadel_core::{BoxId, NodeError, TokenId};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_node_interface::NodeInterface;

use crate::{CapabilityTier, NodeCapabilities, Result};

/// Get an unspent box by its ID
///
/// This works regardless of extraIndex availability.
pub async fn get_box_by_id(node: &NodeInterface, box_id: &BoxId) -> Result<ErgoBox> {
    node.box_from_id_with_pool(box_id.as_str())
        .await
        .map_err(|e| {
            if e.to_string().contains("not found") || e.to_string().contains("404") {
                NodeError::BoxNotFound {
                    box_id: box_id.to_string(),
                }
            } else {
                NodeError::ApiError {
                    message: e.to_string(),
                }
            }
        })
}

/// Get unspent boxes containing a specific token
///
/// Requires extraIndex. Returns error in Basic mode.
pub async fn get_boxes_by_token_id(
    node: &NodeInterface,
    capabilities: &NodeCapabilities,
    token_id: &TokenId,
    limit: u64,
) -> Result<Vec<ErgoBox>> {
    match capabilities.capability_tier {
        CapabilityTier::Full | CapabilityTier::IndexLagging => {
            // Parse the token ID string into ergo-lib's TokenId type
            let ergo_token_id: ergo_lib::ergotree_ir::chain::token::TokenId =
                token_id.as_str().parse().map_err(|e| NodeError::ApiError {
                    message: format!("Invalid token ID format: {}", e),
                })?;

            let boxes = node
                .unspent_boxes_by_token_id(&ergo_token_id, 0, limit)
                .await
                .map_err(|e| NodeError::ApiError {
                    message: e.to_string(),
                })?;

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

/// Get the first unspent box containing a specific token (useful for NFT lookups)
pub async fn get_box_by_token_id(
    node: &NodeInterface,
    capabilities: &NodeCapabilities,
    token_id: &TokenId,
) -> Result<ErgoBox> {
    let boxes = get_boxes_by_token_id(node, capabilities, token_id, 1).await?;

    boxes
        .into_iter()
        .next()
        .ok_or_else(|| NodeError::BoxNotFound {
            box_id: format!("box with token {}", token_id),
        })
}

/// Get the transaction ID and output index where a box was created.
///
/// Queries `/blockchain/box/byId/{box_id}` for the creation context.
/// Used by protocols that need EIP-12 format inputs (which require transactionId + index).
pub async fn get_box_creation_info(node: &NodeInterface, box_id: &str) -> Result<(String, u16)> {
    let endpoint = format!("/blockchain/box/byId/{}", box_id);
    let response = node
        .send_get_req(&endpoint)
        .await
        .map_err(|e| NodeError::ApiError {
            message: format!("Failed to get box details for {}: {}", box_id, e),
        })?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_id_creation() {
        let id = BoxId::new("abc123");
        assert_eq!(id.as_str(), "abc123");
    }

    #[test]
    fn test_token_id_creation() {
        let id = TokenId::new("def456");
        assert_eq!(id.as_str(), "def456");
    }
}
