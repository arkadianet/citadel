//! AMM State Types
//!
//! Data structures for pools, swaps, and orders.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Pool type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PoolType {
    /// Native ERG to Token pool
    N2T,
    /// Token to Token pool
    T2T,
}

/// Type of swap order contract
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwapOrderType {
    /// N2T SwapSell: user sends ERG, receives token
    N2tSwapSell,
    /// N2T SwapBuy: user sends token, receives ERG
    N2tSwapBuy,
}

/// Token amount with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAmount {
    pub token_id: String,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// AMM Pool state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmPool {
    /// Pool NFT token ID (unique identifier)
    pub pool_id: String,
    /// Pool type (N2T or T2T)
    pub pool_type: PoolType,
    /// Current UTXO box ID
    pub box_id: String,

    /// ERG reserves (N2T pools only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erg_reserves: Option<u64>,
    /// Token X (T2T pools only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_x: Option<TokenAmount>,
    /// Token Y
    pub token_y: TokenAmount,

    /// LP token ID
    pub lp_token_id: String,
    /// Circulating LP supply
    pub lp_circulating: u64,

    /// Fee numerator (e.g., 997)
    pub fee_num: i32,
    /// Fee denominator (e.g., 1000)
    pub fee_denom: i32,
}

impl fmt::Display for AmmPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pool_type {
            PoolType::N2T => write!(
                f,
                "N2T Pool {} | ERG: {} | {}: {}",
                &self.pool_id[..8],
                self.erg_reserves.unwrap_or(0),
                self.token_y.name.as_deref().unwrap_or("Token"),
                self.token_y.amount
            ),
            PoolType::T2T => write!(
                f,
                "T2T Pool {} | X: {} | Y: {}",
                &self.pool_id[..8],
                self.token_x.as_ref().map(|t| t.amount).unwrap_or(0),
                self.token_y.amount
            ),
        }
    }
}

/// Swap input specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SwapInput {
    /// Swap ERG for token
    Erg { amount: u64 },
    /// Swap token for ERG or another token
    Token { token_id: String, amount: u64 },
}

/// Swap request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapRequest {
    /// Target pool ID
    pub pool_id: String,
    /// Input to swap
    pub input: SwapInput,
    /// Minimum output amount (slippage protection)
    pub min_output: u64,
    /// Address to receive output
    pub redeemer_address: String,
}

/// Swap quote with calculated values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuote {
    /// Input being swapped
    pub input: SwapInput,
    /// Expected output
    pub output: TokenAmount,
    /// Price impact percentage
    pub price_impact: f64,
    /// Fee amount deducted
    pub fee_amount: u64,
    /// Effective rate after fees
    pub effective_rate: f64,
    /// Suggested min output with default slippage
    pub min_output_suggested: u64,
}

/// Pending swap order (user's unexecuted order)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSwapOrder {
    /// Order box ID
    pub box_id: String,
    /// Submission transaction ID
    pub tx_id: String,
    /// Target pool ID
    pub pool_id: String,
    /// Input locked in order
    pub input: SwapInput,
    /// Minimum output required
    pub min_output: u64,
    /// Redeemer address
    pub redeemer_address: String,
    /// Block height when created
    pub created_height: u32,
    /// Value in nanoERG
    pub value_nano_erg: u64,
    /// Type of swap order contract
    pub order_type: SwapOrderType,
}

/// A direct swap transaction found in the mempool (unconfirmed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolSwap {
    /// Transaction ID
    pub tx_id: String,
    /// Pool NFT ID (from outputs[0] first asset)
    pub pool_id: String,
    /// ERG amount in the user's output box (nanoERG)
    pub receiving_erg: u64,
    /// Tokens in the user's output box: (token_id, amount)
    pub receiving_tokens: Vec<(String, u64)>,
}

/// AMM protocol errors
#[derive(Debug, Error)]
pub enum AmmError {
    #[error("Pool not found: {0}")]
    PoolNotFound(String),

    #[error("Insufficient liquidity for swap")]
    InsufficientLiquidity,

    #[error("Output below minimum: got {got}, need {min}")]
    SlippageExceeded { got: u64, min: u64 },

    #[error("Invalid token for pool: {0}")]
    InvalidToken(String),

    #[error("Invalid pool box layout: expected {expected}, found {found}")]
    InvalidLayout {
        expected: &'static str,
        found: &'static str,
    },

    #[error("Node error: {0}")]
    NodeError(String),

    #[error("Transaction build failed: {0}")]
    TxBuildError(String),

    #[error("Refund failed: {0}")]
    RefundError(String),
}
