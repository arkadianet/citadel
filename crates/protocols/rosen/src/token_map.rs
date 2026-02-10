//! Token mapping between Ergo and target chains
//!
//! Loaded from the `tokensMap-mainnet-*.json` GitHub release asset.

use serde::{Deserialize, Serialize};

/// A bridgeable token with its mappings across chains
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeToken {
    pub ergo_token_id: String,
    pub ergo_name: String,
    pub ergo_decimals: u32,
    pub target_chains: Vec<ChainToken>,
}

/// Token info on a specific target chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainToken {
    pub chain: String,
    pub token_id: String,
    pub name: String,
    pub decimals: u32,
}

/// Complete token map for the bridge
#[derive(Debug, Clone, Default)]
pub struct TokenMap {
    pub tokens: Vec<BridgeToken>,
}

impl TokenMap {
    /// Get tokens bridgeable to a specific chain
    pub fn tokens_for_chain(&self, chain: &str) -> Vec<&BridgeToken> {
        self.tokens
            .iter()
            .filter(|t| t.target_chains.iter().any(|c| c.chain == chain))
            .collect()
    }

    /// Get supported target chains for a specific Ergo token
    pub fn chains_for_token(&self, ergo_token_id: &str) -> Vec<&str> {
        self.tokens
            .iter()
            .find(|t| t.ergo_token_id == ergo_token_id)
            .map(|t| t.target_chains.iter().map(|c| c.chain.as_str()).collect())
            .unwrap_or_default()
    }

    /// Look up target chain token info for an Ergo token
    pub fn get_target_token(&self, ergo_token_id: &str, chain: &str) -> Option<&ChainToken> {
        self.tokens
            .iter()
            .find(|t| t.ergo_token_id == ergo_token_id)?
            .target_chains
            .iter()
            .find(|c| c.chain == chain)
    }

    /// Get all unique supported chains
    pub fn supported_chains(&self) -> Vec<String> {
        let mut chains: Vec<String> = self
            .tokens
            .iter()
            .flat_map(|t| t.target_chains.iter().map(|c| c.chain.clone()))
            .collect();
        chains.sort();
        chains.dedup();
        chains
    }
}

// =============================================================================
// JSON parsing from GitHub release asset
// =============================================================================

/// Wrapper for the tokensMap JSON file ({"version": "...", "tokens": [...]})
#[derive(Debug, Deserialize)]
struct RawTokenMapJson {
    tokens: Vec<RawTokenEntry>,
}

/// Raw token entry from the tokensMap JSON file
#[derive(Debug, Deserialize)]
struct RawTokenEntry {
    ergo: Option<RawChainInfo>,
    cardano: Option<RawChainInfo>,
    bitcoin: Option<RawChainInfo>,
    ethereum: Option<RawChainInfo>,
    doge: Option<RawChainInfo>,
    binance: Option<RawChainInfo>,
    #[serde(rename = "bitcoin-runes")]
    bitcoin_runes: Option<RawChainInfo>,
}

#[derive(Debug, Deserialize)]
struct RawChainInfo {
    #[serde(rename = "tokenId", alias = "tokenID")]
    token_id: Option<String>,
    name: Option<String>,
    decimals: Option<u32>,
}

/// Fetch and parse the token map from a URL
pub async fn fetch_token_map(url: &str) -> Result<TokenMap, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder().user_agent("citadel").build()?;

    let wrapper: RawTokenMapJson = client.get(url).send().await?.json().await?;

    Ok(parse_token_entries(&wrapper.tokens))
}

/// Parse raw token entries into our TokenMap
fn parse_token_entries(entries: &[RawTokenEntry]) -> TokenMap {
    let mut tokens = Vec::new();

    for entry in entries {
        let ergo = match &entry.ergo {
            Some(e) => e,
            None => continue,
        };

        let ergo_token_id = match &ergo.token_id {
            Some(id) => id.clone(),
            None => continue,
        };

        let mut target_chains = Vec::new();

        // Check each supported chain
        let chain_pairs: Vec<(&str, &Option<RawChainInfo>)> = vec![
            ("cardano", &entry.cardano),
            ("bitcoin", &entry.bitcoin),
            ("ethereum", &entry.ethereum),
            ("doge", &entry.doge),
            ("binance", &entry.binance),
            ("bitcoin-runes", &entry.bitcoin_runes),
        ];

        for (chain_name, chain_info) in chain_pairs {
            if let Some(info) = chain_info {
                if let Some(token_id) = &info.token_id {
                    target_chains.push(ChainToken {
                        chain: chain_name.to_string(),
                        token_id: token_id.clone(),
                        name: info.name.clone().unwrap_or_default(),
                        decimals: info.decimals.unwrap_or(0),
                    });
                }
            }
        }

        if !target_chains.is_empty() {
            tokens.push(BridgeToken {
                ergo_token_id,
                ergo_name: ergo.name.clone().unwrap_or_default(),
                ergo_decimals: ergo.decimals.unwrap_or(0),
                target_chains,
            });
        }
    }

    TokenMap { tokens }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_entries() {
        let entries = vec![RawTokenEntry {
            ergo: Some(RawChainInfo {
                token_id: Some("erg".to_string()),
                name: Some("ERG".to_string()),
                decimals: Some(9),
            }),
            cardano: Some(RawChainInfo {
                token_id: Some("ada_token_id".to_string()),
                name: Some("rsERG".to_string()),
                decimals: Some(9),
            }),
            bitcoin: None,
            ethereum: Some(RawChainInfo {
                token_id: Some("0xeth".to_string()),
                name: Some("rsERG".to_string()),
                decimals: Some(9),
            }),
            doge: None,
            binance: None,
            bitcoin_runes: None,
        }];

        let map = parse_token_entries(&entries);
        assert_eq!(map.tokens.len(), 1);
        assert_eq!(map.tokens[0].ergo_token_id, "erg");
        assert_eq!(map.tokens[0].target_chains.len(), 2);

        let chains = map.chains_for_token("erg");
        assert!(chains.contains(&"cardano"));
        assert!(chains.contains(&"ethereum"));
        assert!(!chains.contains(&"bitcoin"));

        let cardano_tokens = map.tokens_for_chain("cardano");
        assert_eq!(cardano_tokens.len(), 1);
        assert_eq!(cardano_tokens[0].ergo_name, "ERG");

        let target = map.get_target_token("erg", "cardano").unwrap();
        assert_eq!(target.name, "rsERG");
    }

    #[test]
    fn test_supported_chains() {
        let map = TokenMap {
            tokens: vec![
                BridgeToken {
                    ergo_token_id: "erg".to_string(),
                    ergo_name: "ERG".to_string(),
                    ergo_decimals: 9,
                    target_chains: vec![
                        ChainToken {
                            chain: "cardano".to_string(),
                            token_id: "x".to_string(),
                            name: "rsERG".to_string(),
                            decimals: 9,
                        },
                        ChainToken {
                            chain: "bitcoin".to_string(),
                            token_id: "y".to_string(),
                            name: "rsERG".to_string(),
                            decimals: 8,
                        },
                    ],
                },
                BridgeToken {
                    ergo_token_id: "token1".to_string(),
                    ergo_name: "RSN".to_string(),
                    ergo_decimals: 0,
                    target_chains: vec![ChainToken {
                        chain: "cardano".to_string(),
                        token_id: "z".to_string(),
                        name: "RSN".to_string(),
                        decimals: 0,
                    }],
                },
            ],
        };

        let chains = map.supported_chains();
        assert_eq!(chains, vec!["bitcoin", "cardano"]);
    }
}
