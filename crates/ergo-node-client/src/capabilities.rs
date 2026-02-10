//! Node capability detection
//!
//! Detects whether the node has extraIndex enabled and its sync status.

use ergo_node_interface::NodeInterface;
use serde::{Deserialize, Serialize};

/// Capability tier based on node features
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CapabilityTier {
    /// extraIndex enabled and synced (within 10 blocks) - all features available
    Full,
    /// extraIndex enabled but lagging - use with caution
    IndexLagging,
    /// No extraIndex - limited features
    Basic,
}

impl CapabilityTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::IndexLagging => "IndexLagging",
            Self::Basic => "Basic",
        }
    }
}

/// Node capabilities detected through probing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Node is reachable and responding
    pub is_online: bool,

    /// Node has extraIndex enabled
    pub has_extra_index: Option<bool>,

    /// Current indexed height (if extraIndex available)
    pub indexed_height: Option<u64>,

    /// Current chain height
    pub chain_height: u64,

    /// Capability tier
    pub capability_tier: CapabilityTier,
}

impl NodeCapabilities {
    /// Calculate index lag (chain_height - indexed_height)
    pub fn index_lag(&self) -> Option<u64> {
        self.indexed_height
            .map(|ih| self.chain_height.saturating_sub(ih))
    }

    /// Check if index is considered synced (within 10 blocks)
    pub fn is_index_synced(&self) -> bool {
        self.index_lag().is_some_and(|lag| lag <= 10)
    }
}

/// Maximum lag (in blocks) before considering index as "lagging"
const MAX_SYNC_LAG: u64 = 10;

/// Detect node capabilities by probing endpoints
pub async fn detect_capabilities(node: &NodeInterface) -> NodeCapabilities {
    // Check if node is online and get chain height
    let chain_height = match node.current_block_height().await {
        Ok(h) => h,
        Err(_) => {
            return NodeCapabilities {
                is_online: false,
                has_extra_index: None,
                indexed_height: None,
                chain_height: 0,
                capability_tier: CapabilityTier::Basic,
            };
        }
    };

    // Check extraIndex availability
    let extra_index_info = node.has_extra_index();

    let (has_extra_index, indexed_height, tier) = match extra_index_info {
        Some(true) => {
            // ExtraIndex is enabled, check sync status
            let ih = node
                .get_indexed_height()
                .await
                .ok()
                .map(|h| h.indexed_height as u64);

            let lag = ih
                .map(|h| chain_height.saturating_sub(h))
                .unwrap_or(u64::MAX);

            let tier = if lag <= MAX_SYNC_LAG {
                CapabilityTier::Full
            } else {
                CapabilityTier::IndexLagging
            };

            (Some(true), ih, tier)
        }
        Some(false) => (Some(false), None, CapabilityTier::Basic),
        None => {
            // Unknown - try to detect by querying indexed height
            match node.get_indexed_height().await {
                Ok(ih) => {
                    let indexed = ih.indexed_height as u64;
                    let lag = chain_height.saturating_sub(indexed);
                    let tier = if lag <= MAX_SYNC_LAG {
                        CapabilityTier::Full
                    } else {
                        CapabilityTier::IndexLagging
                    };
                    (Some(true), Some(indexed), tier)
                }
                Err(_) => (Some(false), None, CapabilityTier::Basic),
            }
        }
    };

    NodeCapabilities {
        is_online: true,
        has_extra_index,
        indexed_height,
        chain_height,
        capability_tier: tier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_tier_serialization() {
        assert_eq!(CapabilityTier::Full.as_str(), "Full");
        assert_eq!(CapabilityTier::IndexLagging.as_str(), "IndexLagging");
        assert_eq!(CapabilityTier::Basic.as_str(), "Basic");
    }

    #[test]
    fn test_index_lag_calculation() {
        let caps = NodeCapabilities {
            is_online: true,
            has_extra_index: Some(true),
            indexed_height: Some(100),
            chain_height: 110,
            capability_tier: CapabilityTier::Full,
        };

        assert_eq!(caps.index_lag(), Some(10));
        assert!(caps.is_index_synced());

        let caps_lagging = NodeCapabilities {
            indexed_height: Some(100),
            chain_height: 120,
            ..caps
        };

        assert_eq!(caps_lagging.index_lag(), Some(20));
        assert!(!caps_lagging.is_index_synced());
    }

    #[test]
    fn test_no_index() {
        let caps = NodeCapabilities {
            is_online: true,
            has_extra_index: Some(false),
            indexed_height: None,
            chain_height: 100,
            capability_tier: CapabilityTier::Basic,
        };

        assert_eq!(caps.index_lag(), None);
        assert!(!caps.is_index_synced());
    }
}
