//! Bridge configuration fetched from rosen-bridge/contract GitHub releases

use serde::{Deserialize, Serialize};

use crate::constants::ROSEN_CONTRACT_RELEASE_URL;

/// Bridge configuration loaded from rosen-bridge/contract GitHub releases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosenConfig {
    /// Ergo lock address (pay-to-address for lock boxes)
    pub lock_address: String,
    /// Token ID of the MinimumFeeBox NFT (used to find fee boxes on-chain)
    pub min_fee_nft_id: String,
}

/// Hardcoded fallback config for mainnet (in case GitHub is unreachable)
pub fn fallback_config() -> RosenConfig {
    RosenConfig {
        // Mainnet lock address from rosen-bridge contracts
        lock_address: "nB3L2PD3LG4ydEj62n9aQs7tFnFrQo1Gf7kfXpHsMoqj6fpaSuak95Wv1VDumhGNiJGGQEMajHsbFTAsfkQzJRePu1yWRFLFZ7KPoJQT3sEBSjXoqmkp2PEbMSMa4WZCWqPbajjJLafz7RJXQ4u96gMZGW5yY3Nfi8FBMaGHYTMRFUxwCbC".to_string(),
        min_fee_nft_id: "e2ed4d64393222db666f20e67803e9e6fbe6d64531e14ff52ddd95615b0cbf17".to_string(),
    }
}

/// GitHub release asset info
#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// GitHub release info
#[derive(Debug, Deserialize)]
struct Release {
    #[allow(dead_code)]
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

/// Contract JSON structure from the release asset
#[derive(Debug, Deserialize)]
struct ContractJson {
    ergo: Option<ErgoSection>,
    tokens: Option<TokensSection>,
}

#[derive(Debug, Deserialize)]
struct ErgoSection {
    addresses: Option<AddressesSection>,
}

#[derive(Debug, Deserialize)]
struct AddressesSection {
    lock: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokensSection {
    #[serde(rename = "MinFeeNFT")]
    min_fee_nft: Option<String>,
}

/// Fetch the latest Rosen Bridge config from GitHub releases.
///
/// Falls back to hardcoded config if the network request fails.
pub async fn fetch_config() -> RosenConfig {
    match fetch_config_from_github().await {
        Ok(config) => {
            tracing::info!(
                lock_address = %config.lock_address,
                "Loaded Rosen Bridge config from GitHub"
            );
            config
        }
        Err(e) => {
            tracing::warn!(
                "Failed to fetch Rosen config from GitHub: {}, using fallback",
                e
            );
            fallback_config()
        }
    }
}

async fn fetch_config_from_github() -> Result<RosenConfig, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder().user_agent("citadel").build()?;

    // Fetch releases list
    let releases: Vec<Release> = client
        .get(ROSEN_CONTRACT_RELEASE_URL)
        .send()
        .await?
        .json()
        .await?;

    // Find the latest release (first entry is most recent).
    // Rosen uses "public-launch" for mainnet assets.
    let release = releases.first().ok_or("No releases found")?;

    // Find the ergo contracts JSON asset (public-launch = mainnet)
    let contracts_asset = release
        .assets
        .iter()
        .find(|a| {
            a.name.contains("public-launch")
                && a.name.starts_with("contracts-")
                && a.name.ends_with(".json")
        })
        .or_else(|| {
            // Older releases have per-chain files; look for the ergo one
            release.assets.iter().find(|a| {
                a.name.contains("ergo")
                    && a.name.contains("public-launch")
                    && a.name.ends_with(".json")
            })
        })
        .ok_or("No contracts JSON asset found")?;

    // Download and parse the contracts JSON
    let contract_json: ContractJson = client
        .get(&contracts_asset.browser_download_url)
        .send()
        .await?
        .json()
        .await?;

    let lock_address = contract_json
        .ergo
        .and_then(|e| e.addresses)
        .and_then(|a| a.lock)
        .ok_or("Missing ergo.addresses.lock in contracts JSON")?;

    let min_fee_nft_id = contract_json
        .tokens
        .and_then(|t| t.min_fee_nft)
        .ok_or("Missing tokens.MinFeeNFT in contracts JSON")?;

    // Also try to fetch the token map asset URL (for token_map module)
    Ok(RosenConfig {
        lock_address,
        min_fee_nft_id,
    })
}

/// Fetch the token map JSON URL from the same GitHub release.
///
/// Returns the download URL for the tokensMap JSON file.
pub async fn fetch_token_map_url() -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder().user_agent("citadel").build()?;

    let releases: Vec<Release> = client
        .get(ROSEN_CONTRACT_RELEASE_URL)
        .send()
        .await?
        .json()
        .await?;

    let release = releases.first().ok_or("No releases found")?;

    let token_map_asset = release
        .assets
        .iter()
        .find(|a| {
            a.name.starts_with("tokensMap-")
                && a.name.contains("public-launch")
                && a.name.ends_with(".json")
        })
        .ok_or("No tokensMap JSON asset found")?;

    Ok(token_map_asset.browser_download_url.clone())
}
