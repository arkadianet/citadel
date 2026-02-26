//! Dexy Protocol Constants
//!
//! Token IDs and protocol parameters for DexyGold and DexyUSD on mainnet.
//!
//! # Protocol Architecture
//!
//! Dexy is a multi-box protocol. For FreeMint transactions:
//!
//! **Inputs:**
//! - 0: FreeMint box (freeMintNFT) - controls minting rules
//! - 1: Bank box (bankNFT) - holds Dexy tokens
//! - 2: Buyback box (buybackNFT) - receives fees
//!
//! **Data Inputs:**
//! - 0: Oracle box - price feed
//! - 1: LP box - for rate validation
//!
//! **Outputs:**
//! - 0: FreeMint box (updated registers)
//! - 1: Bank box (updated ERG + tokens)
//! - 2: Buyback box (receives fee)
//! - 3+: User outputs

use std::fmt;
use std::str::FromStr;

use citadel_core::Network;
use serde::{Deserialize, Serialize};

/// Dexy protocol variant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DexyVariant {
    Gold,
    Usd,
}

/// Error returned when parsing a `DexyVariant` from a string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DexyParseError;

impl fmt::Display for DexyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Dexy variant (expected 'gold' or 'usd')")
    }
}

impl FromStr for DexyVariant {
    type Err = DexyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gold" => Ok(DexyVariant::Gold),
            "usd" | "use" => Ok(DexyVariant::Usd),
            _ => Err(DexyParseError),
        }
    }
}

impl DexyVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            DexyVariant::Gold => "gold",
            DexyVariant::Usd => "usd",
        }
    }

    pub fn token_name(&self) -> &'static str {
        match self {
            DexyVariant::Gold => "DexyGold",
            DexyVariant::Usd => "DexyUSD",
        }
    }

    pub fn decimals(&self) -> u8 {
        match self {
            DexyVariant::Gold => 0, // DexyGold has 0 decimals
            DexyVariant::Usd => 3,  // DexyUSD (USE) has 3 decimals
        }
    }

    /// Oracle rate divisor to convert raw oracle R4 to nanoERG per token
    ///
    /// - DexyGold: Oracle gives nanoERG per kg, divide by 1,000,000 for nanoERG per mg
    /// - USE: Oracle gives nanoERG per USD, divide by 1,000 for nanoERG per 0.001 USE
    pub fn oracle_divisor(&self) -> i64 {
        match self {
            DexyVariant::Gold => 1_000_000, // kg → mg
            DexyVariant::Usd => 1_000,      // USD → 0.001 USE (3 decimals)
        }
    }

    /// Initial LP token supply (compile-time constant in pool contracts)
    pub fn initial_lp(&self) -> i64 {
        match self {
            DexyVariant::Gold => 100_000_000_000,
            DexyVariant::Usd => 9_223_372_036_854_775_000,
        }
    }

    /// Human-readable peg description
    pub fn peg_description(&self) -> &'static str {
        match self {
            DexyVariant::Gold => "1 DexyGold = 1 milligram of gold",
            DexyVariant::Usd => "1 USE = 0.001 USD",
        }
    }
}

/// Fee parameters for Dexy FreeMint
/// Bank fee: 0.3% (3/1000)
/// Buyback fee: 0.2% (2/1000)
/// Total fee: 0.5%
pub const BANK_FEE_NUM: i64 = 3;
pub const BUYBACK_FEE_NUM: i64 = 2;
pub const FEE_DENOM: i64 = 1000;

/// Fee parameters for Dexy LP Swap
/// LP swap fee: 0.3% (3/1000)
/// From contracts/lp/pool/swap.es: feeNum = 3, feeDenom = 1000
pub const LP_SWAP_FEE_NUM: i64 = 3;
pub const LP_SWAP_FEE_DENOM: i64 = 1000;

/// DexyGold mainnet constants
pub mod gold_mainnet {
    /// DexyGold token ID
    pub const DEXY_TOKEN_ID: &str =
        "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";

    /// Bank NFT - identifies the bank box
    pub const BANK_NFT_ID: &str =
        "75d7bfbfa6d165bfda1bad3e3fda891e67ccdcfc7b4410c1790923de2ccc9f7f";

    /// Oracle Pool NFT - gold price feed
    pub const ORACLE_POOL_NFT_ID: &str =
        "3c45f29a5165b030fdb5eaf5d81f8108f9d8f507b31487dd51f4ae08fe07cf4a";

    /// LP NFT - for reading LP state and rate validation
    pub const LP_NFT_ID: &str = "905ecdef97381b92c2f0ea9b516f312bfb18082c61b24b40affa6a55555c77c7";

    /// FreeMint NFT - controls minting rules
    pub const FREE_MINT_NFT_ID: &str =
        "74f906985e763192fc1d8d461e29406c75b7952da3a89dbc83fe1b889971e455";

    /// Buyback NFT - receives protocol fees
    pub const BUYBACK_NFT_ID: &str =
        "610735cbf197f9de67b3628129feaa5a52403286859d140be719467c0fb94328";

    /// ArbitrageMint NFT
    pub const ARBITRAGE_MINT_NFT_ID: &str =
        "3fefa1e3fef4e7abbdc074a20bdf751675f058e4bcce5cef0b38bb9460be5c6a";

    /// LP Swap NFT - for LP swaps
    pub const LP_SWAP_NFT_ID: &str =
        "ff7b7eff3c818f9dc573ca03a723a7f6ed1615bf27980ebd4a6c91986b26f801";

    /// LP Token ID - bearer token for liquidity providers
    pub const LP_TOKEN_ID: &str =
        "cf74432b2d3ab8a1a934b6326a1004e1a19aec7b357c57209018c4aa35226246";

    /// LP Mint NFT - identifies the LP Mint action box
    pub const LP_MINT_NFT_ID: &str =
        "19b8281b141d19c5b3843a4a77e616d6df05f601e5908159b1eaf3d9da20e664";

    /// LP Redeem NFT - identifies the LP Redeem action box
    pub const LP_REDEEM_NFT_ID: &str =
        "08c47eef5e782f146cae5e8cfb5e9d26b18442f82f3c5808b1563b6e3b23f729";
}

/// DexyUSD (USE) mainnet constants
pub mod usd_mainnet {
    /// USE token ID (DexyUSD)
    pub const DEXY_TOKEN_ID: &str =
        "a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669";

    /// Bank NFT - identifies the bank box
    pub const BANK_NFT_ID: &str =
        "78c24bdf41283f45208664cd8eb78e2ffa7fbb29f26ebb43e6b31a46b3b975ae";

    /// Oracle Pool NFT - USD price feed
    pub const ORACLE_POOL_NFT_ID: &str =
        "6a2b821b5727e85beb5e78b4efb9f0250d59cd48481d2ded2c23e91ba1d07c66";

    /// LP NFT - for reading LP state and rate validation
    pub const LP_NFT_ID: &str = "4ecaa1aac9846b1454563ae51746db95a3a40ee9f8c5f5301afbe348ae803d41";

    /// FreeMint NFT - controls minting rules
    pub const FREE_MINT_NFT_ID: &str =
        "40db16e1ed50b16077b19102390f36b41ca35c64af87426d04af3b9340859051";

    /// Buyback NFT - receives protocol fees
    pub const BUYBACK_NFT_ID: &str =
        "dcce07af04ea4f9b7979336476594dc16321547bcc9c6b95a67cb1a94192da4f";

    /// ArbitrageMint NFT
    pub const ARBITRAGE_MINT_NFT_ID: &str =
        "c79bef6fe21c788546beab08c963999d5ef74151a9b7fd6c1843f626eea0ecf5";

    /// LP Swap NFT - for LP swaps
    pub const LP_SWAP_NFT_ID: &str =
        "ef461517a55b8bfcd30356f112928f3333b5b50faf472e8374081307a09110cf";

    /// LP Token ID - bearer token for liquidity providers
    pub const LP_TOKEN_ID: &str =
        "804a66426283b8281240df8f9de783651986f20ad6391a71b26b9e7d6faad099";

    /// LP Mint NFT - identifies the LP Mint action box
    pub const LP_MINT_NFT_ID: &str =
        "2cf9fb512f487254777ac1d086a55cda9e74a1009fe0d30390a3792f050de58f";

    /// LP Redeem NFT - identifies the LP Redeem action box
    pub const LP_REDEEM_NFT_ID: &str =
        "1bfea21924f670ca5f13dd6819ed3bf833ec5a3113d5b6ae87d806db29b94b9a";
}

/// NFT IDs for a specific Dexy variant
#[derive(Debug, Clone)]
pub struct DexyIds {
    pub variant: DexyVariant,
    pub dexy_token: String,
    pub bank_nft: String,
    pub oracle_pool_nft: String,
    pub lp_nft: String,
    pub free_mint_nft: String,
    pub buyback_nft: String,
    pub lp_swap_nft: String,
    pub lp_token_id: String,
    pub lp_mint_nft: String,
    pub lp_redeem_nft: String,
}

impl DexyIds {
    /// Get IDs for DexyGold on mainnet
    pub fn gold_mainnet() -> Self {
        Self {
            variant: DexyVariant::Gold,
            dexy_token: gold_mainnet::DEXY_TOKEN_ID.to_string(),
            bank_nft: gold_mainnet::BANK_NFT_ID.to_string(),
            oracle_pool_nft: gold_mainnet::ORACLE_POOL_NFT_ID.to_string(),
            lp_nft: gold_mainnet::LP_NFT_ID.to_string(),
            free_mint_nft: gold_mainnet::FREE_MINT_NFT_ID.to_string(),
            buyback_nft: gold_mainnet::BUYBACK_NFT_ID.to_string(),
            lp_swap_nft: gold_mainnet::LP_SWAP_NFT_ID.to_string(),
            lp_token_id: gold_mainnet::LP_TOKEN_ID.to_string(),
            lp_mint_nft: gold_mainnet::LP_MINT_NFT_ID.to_string(),
            lp_redeem_nft: gold_mainnet::LP_REDEEM_NFT_ID.to_string(),
        }
    }

    /// Get IDs for DexyUSD on mainnet
    pub fn usd_mainnet() -> Self {
        Self {
            variant: DexyVariant::Usd,
            dexy_token: usd_mainnet::DEXY_TOKEN_ID.to_string(),
            bank_nft: usd_mainnet::BANK_NFT_ID.to_string(),
            oracle_pool_nft: usd_mainnet::ORACLE_POOL_NFT_ID.to_string(),
            lp_nft: usd_mainnet::LP_NFT_ID.to_string(),
            free_mint_nft: usd_mainnet::FREE_MINT_NFT_ID.to_string(),
            buyback_nft: usd_mainnet::BUYBACK_NFT_ID.to_string(),
            lp_swap_nft: usd_mainnet::LP_SWAP_NFT_ID.to_string(),
            lp_token_id: usd_mainnet::LP_TOKEN_ID.to_string(),
            lp_mint_nft: usd_mainnet::LP_MINT_NFT_ID.to_string(),
            lp_redeem_nft: usd_mainnet::LP_REDEEM_NFT_ID.to_string(),
        }
    }

    /// Get IDs for a variant on a network
    pub fn for_variant(variant: DexyVariant, network: Network) -> Option<Self> {
        match (variant, network) {
            (DexyVariant::Gold, Network::Mainnet) => Some(Self::gold_mainnet()),
            (DexyVariant::Usd, Network::Mainnet) => Some(Self::usd_mainnet()),
            _ => None, // Testnet not supported
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_from_str() {
        assert_eq!("gold".parse::<DexyVariant>(), Ok(DexyVariant::Gold));
        assert_eq!("usd".parse::<DexyVariant>(), Ok(DexyVariant::Usd));
        assert_eq!("use".parse::<DexyVariant>(), Ok(DexyVariant::Usd));
        assert!("invalid".parse::<DexyVariant>().is_err());
    }

    #[test]
    fn test_variant_token_name() {
        assert_eq!(DexyVariant::Gold.token_name(), "DexyGold");
        assert_eq!(DexyVariant::Usd.token_name(), "DexyUSD");
    }

    #[test]
    fn test_dexy_ids_mainnet() {
        let gold_ids = DexyIds::gold_mainnet();
        assert_eq!(gold_ids.variant, DexyVariant::Gold);
        assert_eq!(gold_ids.dexy_token.len(), 64);

        let usd_ids = DexyIds::usd_mainnet();
        assert_eq!(usd_ids.variant, DexyVariant::Usd);
        assert_eq!(usd_ids.dexy_token.len(), 64);
    }
}
