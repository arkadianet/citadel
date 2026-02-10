//! Core type definitions for Citadel

use serde::{Deserialize, Serialize};
use std::fmt;

/// Box ID (32 bytes, hex-encoded)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BoxId(pub String);

impl BoxId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BoxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Token ID (32 bytes, hex-encoded)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenId(pub String);

impl TokenId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Transaction ID (32 bytes, hex-encoded)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TxId(pub String);

impl TxId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Ergo address (P2PK or P2S)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(pub String);

impl Address {
    pub fn new(addr: impl Into<String>) -> Self {
        Self(addr.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a mainnet address
    pub fn is_mainnet(&self) -> bool {
        self.0.starts_with('9')
    }

    /// Check if this is a testnet address
    pub fn is_testnet(&self) -> bool {
        self.0.starts_with('3')
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Network type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Block height
pub type BlockHeight = u64;

/// NanoERG amount (1 ERG = 1_000_000_000 nanoERG)
pub type NanoErg = i64;

/// Constants
pub mod constants {
    use super::NanoErg;

    /// 1 ERG in nanoERG
    pub const NANOERG_PER_ERG: NanoErg = 1_000_000_000;

    /// Standard transaction fee (0.0011 ERG)
    pub const TX_FEE_NANO: NanoErg = 1_100_000;

    /// Minimum box value (0.001 ERG)
    pub const MIN_BOX_VALUE_NANO: NanoErg = 1_000_000;

    /// Miner fee ErgoTree (standard P2PK to miner)
    pub const MINER_FEE_ERGO_TREE: &str = "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_network_detection() {
        let mainnet = Address::new("9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisd");
        assert!(mainnet.is_mainnet());
        assert!(!mainnet.is_testnet());

        let testnet = Address::new("3WwbzW6u8hKWBcL1W7kNVMr25s2UHfSBnYtwSHvrRQt7DdPuoXrt");
        assert!(testnet.is_testnet());
        assert!(!testnet.is_mainnet());
    }

    #[test]
    fn test_network_display() {
        assert_eq!(Network::Mainnet.as_str(), "mainnet");
        assert_eq!(Network::Testnet.as_str(), "testnet");
    }
}
