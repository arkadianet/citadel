//! HodlCoin State Types
//!
//! Data structures for bank state, mint/burn previews, and errors.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Parsed state of a HodlCoin bank box
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HodlBankState {
    // Identity
    pub bank_box_id: String,
    pub singleton_token_id: String,
    pub hodl_token_id: String,
    pub hodl_token_name: Option<String>,

    // Bank parameters (from R4-R8)
    pub total_token_supply: i64,
    pub precision_factor: i64,
    pub min_bank_value: i64,
    pub dev_fee_num: i64,
    pub bank_fee_num: i64,

    // Derived state
    pub reserve_nano_erg: i64,
    pub hodl_tokens_in_bank: i64,
    pub circulating_supply: i64,
    pub price_nano_per_hodl: f64,
    pub tvl_nano_erg: i64,

    // Fee info
    pub total_fee_pct: f64,
    pub bank_fee_pct: f64,
    pub dev_fee_pct: f64,
}

/// Preview for minting hodlTokens
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HodlMintPreview {
    /// nanoERG user deposits into the bank
    pub erg_deposited: i64,
    /// hodlTokens the user will receive
    pub hodl_tokens_received: i64,
    /// Price per token at time of mint
    pub price_per_token: f64,
    /// Miner fee
    pub miner_fee: i64,
    /// Total ERG cost (deposit + miner fee + min box value)
    pub total_erg_cost: i64,
}

/// Preview for burning hodlTokens
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HodlBurnPreview {
    /// hodlTokens the user will burn
    pub hodl_tokens_spent: i64,
    /// ERG received after all fees
    pub erg_received: i64,
    /// Bank fee in nanoERG
    pub bank_fee_nano: i64,
    /// Dev fee in nanoERG
    pub dev_fee_nano: i64,
    /// ERG value before fees
    pub erg_before_fees: i64,
    /// Price per token at time of burn
    pub price_per_token: f64,
    /// Miner fee
    pub miner_fee: i64,
}

/// HodlCoin protocol errors
#[derive(Debug, Error)]
pub enum HodlError {
    #[error("Bank not found: {0}")]
    BankNotFound(String),

    #[error("Invalid bank box layout: {0}")]
    InvalidLayout(String),

    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),

    #[error("Below minimum bank value")]
    BelowMinBankValue,

    #[error("Node error: {0}")]
    NodeError(String),

    #[error("Transaction build failed: {0}")]
    TxBuildError(String),
}
