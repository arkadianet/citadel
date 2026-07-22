//! Duckpools lending transaction builders (proxy box pattern).

use std::collections::HashMap;

mod borrow;
mod common;
mod lend;
mod refund;
mod repay;
mod withdraw;

#[cfg(test)]
mod tests;

pub use borrow::build_borrow_tx;
pub use common::{select_erg_inputs, select_token_inputs, SelectedInputs};
pub use lend::build_lend_tx;
pub use refund::build_refund_tx;
pub use repay::build_repay_tx;
pub use withdraw::build_withdraw_tx;

// Citadel app fee: deferred (phase 2). Proxy funding txs are likely safe, but
// need budget + UI disclosure per lend/borrow/repay path before enabling.

/// Miner fee (0.001 ERG). Matches the Duckpools bot's TX_FEE.
pub const TX_FEE_NANO: i64 = 1_000_000;

/// Proxy execution fee for the bot to pay child tx fees (0.002 ERG).
pub const PROXY_EXECUTION_FEE_NANO: i64 = 2_000_000;

pub const MIN_BOX_VALUE_NANO: i64 = citadel_core::constants::MIN_BOX_VALUE_NANO;

/// Bot processing overhead (0.003 ERG)
pub const BOT_PROCESSING_OVERHEAD: i64 = 3_000_000;

/// Refund height offset (~24 hours / 720 blocks)
pub const REFUND_HEIGHT_OFFSET: i32 = 720;

pub const MINER_FEE_ERGO_TREE: &str = citadel_core::constants::MINER_FEE_ERGO_TREE;

#[derive(Debug, Clone)]
pub struct UserUtxo {
    pub box_id: String,
    pub tx_id: String,
    pub index: u16,
    pub value: i64,
    pub ergo_tree: String,
    pub assets: Vec<(String, i64)>,
    pub creation_height: i32,
    pub registers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct LendRequest {
    pub pool_id: String,
    pub amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<UserUtxo>,
    pub min_lp_tokens: Option<u64>,
    /// Slippage tolerance in basis points (0-200 = 0%-2%)
    pub slippage_bps: u16,
}

#[derive(Debug, Clone)]
pub struct WithdrawRequest {
    pub pool_id: String,
    pub lp_amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<UserUtxo>,
    pub min_output: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct BorrowRequest {
    pub pool_id: String,
    /// Token ID or "native" for ERG
    pub collateral_token: String,
    pub collateral_amount: u64,
    pub borrow_amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<UserUtxo>,
}

#[derive(Debug, Clone)]
pub struct RepayRequest {
    pub pool_id: String,
    pub collateral_box_id: String,
    pub repay_amount: u64,
    /// Used to choose full vs partial repay proxy.
    pub total_owed: u64,
    pub user_address: String,
    pub user_utxos: Vec<UserUtxo>,
}

#[derive(Debug, Clone)]
pub struct RefundRequest {
    pub proxy_box_id: String,
    pub user_address: String,
    pub user_utxos: Vec<UserUtxo>,
}

#[derive(Debug, Clone)]
pub struct TxSummary {
    pub action: String,
    pub pool_id: String,
    pub pool_name: String,
    pub amount_in: String,
    pub amount_out_estimate: Option<String>,
    pub proxy_address: String,
    pub refund_height: i32,
    pub service_fee_raw: u64,
    pub service_fee_display: String,
    /// amount + fee + slippage buffer
    pub total_to_send_raw: u64,
    pub total_to_send_display: String,
}

#[derive(Debug, Clone)]
pub struct BuildResponse {
    pub unsigned_tx: String,
    pub fee_nano: i64,
    pub summary: TxSummary,
}

#[derive(Debug, Clone)]
pub struct ProxyBoxData {
    pub box_id: String,
    pub tx_id: String,
    pub index: u16,
    pub value: i64,
    pub ergo_tree: String,
    pub assets: Vec<(String, i64)>,
    pub creation_height: i32,
    pub user_ergo_tree: String,
    pub r6_refund_height: i64,
    /// Repay proxies need 3 outputs to trigger operation path (avoids R6 type check).
    /// All other proxies use 2 outputs to trigger proveDlog refund path.
    pub is_repay_proxy: bool,
    /// Must be included in the input box for correct box ID verification.
    pub additional_registers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RefundResponse {
    pub unsigned_tx: String,
    pub fee_nano: i64,
    pub refundable_after_height: i64,
}

#[derive(Debug, Clone)]
pub enum BuildError {
    PoolNotFound(String),
    InvalidAmount(String),
    InsufficientBalance {
        required: i64,
        available: i64,
    },
    InsufficientTokens {
        token: String,
        required: i64,
        available: i64,
    },
    InvalidAddress(String),
    TxBuildError(String),
    ProxyContractMissing(String),
    CollateralBoxNotFound(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PoolNotFound(id) => write!(f, "Pool not found: {}", id),
            Self::InvalidAmount(msg) => write!(f, "Invalid amount: {}", msg),
            Self::InsufficientBalance {
                required,
                available,
            } => {
                write!(
                    f,
                    "Insufficient ERG balance: need {} nanoERG, have {}",
                    required, available
                )
            }
            Self::InsufficientTokens {
                token,
                required,
                available,
            } => {
                write!(
                    f,
                    "Insufficient {} tokens: need {}, have {}",
                    token, required, available
                )
            }
            Self::InvalidAddress(msg) => write!(f, "Invalid address: {}", msg),
            Self::TxBuildError(msg) => write!(f, "Transaction build error: {}", msg),
            Self::ProxyContractMissing(pool) => {
                write!(f, "Proxy contract not configured for pool: {}", pool)
            }
            Self::CollateralBoxNotFound(id) => write!(f, "Collateral box not found: {}", id),
        }
    }
}

impl std::error::Error for BuildError {}

impl BuildError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PoolNotFound(_) => "pool_not_found",
            Self::InvalidAmount(_) => "invalid_amount",
            Self::InsufficientBalance { .. } => "insufficient_balance",
            Self::InsufficientTokens { .. } => "insufficient_tokens",
            Self::InvalidAddress(_) => "invalid_address",
            Self::TxBuildError(_) => "tx_build_error",
            Self::ProxyContractMissing(_) => "proxy_contract_missing",
            Self::CollateralBoxNotFound(_) => "collateral_box_not_found",
        }
    }

    pub fn status_code(&self) -> u16 {
        match self {
            Self::InvalidAmount(_) | Self::InvalidAddress(_) => 400,
            Self::InsufficientBalance { .. } | Self::InsufficientTokens { .. } => 422,
            Self::PoolNotFound(_) | Self::CollateralBoxNotFound(_) => 404,
            Self::ProxyContractMissing(_) => 503,
            Self::TxBuildError(_) => 500,
        }
    }
}
