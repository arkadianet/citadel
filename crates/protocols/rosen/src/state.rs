//! Serializable state types for frontend communication

use serde::{Deserialize, Serialize};

/// Overall bridge state sent to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosenBridgeState {
    pub supported_chains: Vec<String>,
    pub available_tokens: Vec<BridgeTokenInfo>,
}

/// Token info for the frontend (simplified from BridgeToken)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeTokenInfo {
    pub ergo_token_id: String,
    pub name: String,
    pub decimals: u32,
    pub target_chains: Vec<String>,
}

/// Fee breakdown for a specific bridge transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeFeeInfo {
    /// Protocol bridge fee (human-readable amount string)
    pub bridge_fee: String,
    /// Target chain network fee (human-readable amount string)
    pub network_fee: String,
    /// Variable fee ratio in basis points (100 = 1%)
    pub fee_ratio_bps: i64,
    /// Minimum transfer amount (human-readable)
    pub min_transfer: String,
    /// Amount the user will receive after fees (human-readable)
    pub receiving_amount: String,
    /// Raw bridge fee in base units
    pub bridge_fee_raw: i64,
    /// Raw network fee in base units
    pub network_fee_raw: i64,
}
