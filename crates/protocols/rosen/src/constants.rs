//! Rosen Bridge protocol constants

/// GitHub API URL for rosen-bridge/contract releases
pub const ROSEN_CONTRACT_RELEASE_URL: &str =
    "https://api.github.com/repos/rosen-bridge/contract/releases";

/// Minimum box value for lock boxes (0.002 ERG)
/// Lock boxes need higher min value for Rosen bridge
pub const LOCK_MIN_BOX_VALUE: i64 = 2_000_000;

/// Keep the old name as an alias for backward compatibility
pub const MIN_BOX_VALUE: i64 = LOCK_MIN_BOX_VALUE;

/// Standard miner fee for lock transactions (0.0011 ERG)
pub const LOCK_TX_FEE: i64 = citadel_core::constants::TX_FEE_NANO;

/// Supported target chains for bridging from Ergo
pub const SUPPORTED_CHAINS: &[&str] = &[
    "cardano",
    "bitcoin",
    "ethereum",
    "doge",
    "binance",
    "bitcoin-runes",
];

/// Display name for each chain
pub fn chain_display_name(chain: &str) -> &str {
    match chain {
        "cardano" => "Cardano",
        "bitcoin" => "Bitcoin",
        "ethereum" => "Ethereum",
        "doge" => "Dogecoin",
        "binance" => "Binance",
        "bitcoin-runes" => "Bitcoin Runes",
        _ => chain,
    }
}
