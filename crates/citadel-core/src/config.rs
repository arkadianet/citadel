//! Configuration types for Citadel

use serde::{Deserialize, Serialize};

use crate::Network;

/// Node connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node URL (e.g., "http://127.0.0.1:9053")
    pub url: String,

    /// API key for authenticated endpoints (optional)
    #[serde(default)]
    pub api_key: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:9053".to_string(),
            api_key: String::new(),
        }
    }
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Node connection settings
    pub node: NodeConfig,

    /// Network (mainnet or testnet)
    pub network: Network,

    /// API server port
    #[serde(default = "default_api_port")]
    pub api_port: u16,
}

fn default_api_port() -> u16 {
    19053
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            node: NodeConfig::default(),
            network: Network::Mainnet,
            api_port: default_api_port(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.node.url, "http://127.0.0.1:9053");
        assert_eq!(config.network, Network::Mainnet);
        assert_eq!(config.api_port, 19053);
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.node.url, config.node.url);
    }
}
