//! SigmaUSD Protocol Constants
//!
//! Token IDs and protocol parameters for mainnet and testnet.

use citadel_core::Network;

/// Mainnet protocol constants
pub mod mainnet {
    /// Bank NFT - identifies the unique bank box
    pub const BANK_NFT_ID: &str =
        "7d672d1def471720ca5782fd6473e47e796d9ac0c138d9911346f118b2f6d9d9";

    /// SigUSD token ID
    pub const SIGUSD_TOKEN_ID: &str =
        "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04";

    /// SigRSV token ID
    pub const SIGRSV_TOKEN_ID: &str =
        "003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0";

    /// Oracle Pool NFT - ERG/USD price feed (ERGUSD-NFT)
    pub const ORACLE_POOL_NFT_ID: &str =
        "011d3364de07e5a26f0c4eef0852cddb387039a921b7154ef3cab22c6eda887f";
}

/// Testnet protocol constants (not yet deployed)
pub mod testnet {
    pub const BANK_NFT_ID: Option<&str> = None;
    pub const SIGUSD_TOKEN_ID: Option<&str> = None;
    pub const SIGRSV_TOKEN_ID: Option<&str> = None;
    pub const ORACLE_POOL_NFT_ID: Option<&str> = None;
}

/// Protocol parameters
pub mod params {
    /// Minimum reserve ratio (400% = 4.0x collateralization)
    /// Below this, SigUSD minting is disabled
    pub const MIN_RESERVE_RATIO_PCT: i32 = 400;

    /// Maximum reserve ratio (800% = 8.0x collateralization)
    /// Above this, SigRSV minting is disabled
    pub const MAX_RESERVE_RATIO_PCT: i32 = 800;

    /// Protocol fee in basis points (200 = 2%)
    pub const FEE_BPS: i32 = 200;

    /// SigUSD has 2 decimal places (100 units = 1.00 SigUSD)
    pub const SIGUSD_DECIMALS: u8 = 2;

    /// SigRSV has 0 decimal places
    pub const SIGRSV_DECIMALS: u8 = 0;
}

/// NFT IDs for a specific network
#[derive(Debug, Clone)]
pub struct NftIds {
    pub bank_nft: String,
    pub sigusd_token: String,
    pub sigrsv_token: String,
    pub oracle_pool_nft: String,
}

impl NftIds {
    /// Get NFT IDs for a network
    pub fn for_network(network: Network) -> Option<Self> {
        match network {
            Network::Mainnet => Some(Self {
                bank_nft: mainnet::BANK_NFT_ID.to_string(),
                sigusd_token: mainnet::SIGUSD_TOKEN_ID.to_string(),
                sigrsv_token: mainnet::SIGRSV_TOKEN_ID.to_string(),
                oracle_pool_nft: mainnet::ORACLE_POOL_NFT_ID.to_string(),
            }),
            Network::Testnet => {
                // Testnet not yet supported
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mainnet_constants() {
        let ids = NftIds::for_network(Network::Mainnet).unwrap();
        assert_eq!(ids.bank_nft.len(), 64); // 32 bytes hex
        assert_eq!(ids.sigusd_token.len(), 64);
        assert_eq!(ids.sigrsv_token.len(), 64);
        assert_eq!(ids.oracle_pool_nft.len(), 64);
    }

    #[test]
    fn test_testnet_not_supported() {
        assert!(NftIds::for_network(Network::Testnet).is_none());
    }

    #[test]
    fn test_protocol_params() {
        assert_eq!(params::MIN_RESERVE_RATIO_PCT, 400);
        assert_eq!(params::MAX_RESERVE_RATIO_PCT, 800);
        assert_eq!(params::FEE_BPS, 200);
    }
}
