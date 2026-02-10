//! SigmaFi protocol state types

use serde::{Deserialize, Serialize};

/// A supported loan token with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoanToken {
    pub token_id: String,
    pub name: String,
    pub decimals: u8,
}

/// An open bond order (loan request from a borrower)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOrder {
    /// Box ID of the order UTXO
    pub box_id: String,
    /// ErgoTree hex of the order contract
    pub ergo_tree: String,
    /// Block height at which the box was created
    pub creation_height: i32,
    /// Borrower's P2PK address
    pub borrower_address: String,
    /// Token ID of the loan currency ("ERG" for native)
    pub loan_token_id: String,
    /// Loan token name
    pub loan_token_name: String,
    /// Loan token decimals
    pub loan_token_decimals: u8,
    /// Principal amount in raw units (nanoERG or smallest token unit)
    pub principal: u64,
    /// Total repayment amount in raw units
    pub repayment: u64,
    /// Maturity duration in blocks (on-close) or target height (fixed-height)
    pub maturity_blocks: i32,
    /// Collateral ERG in nanoERGs (box value)
    pub collateral_erg: u64,
    /// Collateral tokens (token_id, amount)
    pub collateral_tokens: Vec<CollateralToken>,
    /// Calculated interest percentage
    pub interest_percent: f64,
    /// Calculated annualized percentage rate
    pub apr: f64,
    /// Collateral-to-principal ratio (percentage, if price data available)
    pub collateral_ratio: Option<f64>,
    /// Whether this order belongs to the connected wallet
    pub is_own: bool,
    /// Transaction ID of the box creation (for EIP-12 input)
    pub transaction_id: String,
    /// Output index in that transaction
    pub output_index: u16,
}

/// A collateral token entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollateralToken {
    pub token_id: String,
    pub amount: u64,
    pub name: Option<String>,
    pub decimals: Option<u8>,
}

/// An active (filled) bond
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBond {
    /// Box ID of the bond UTXO
    pub box_id: String,
    /// ErgoTree hex of the bond contract
    pub ergo_tree: String,
    /// R4: Box ID of the originating order
    pub originating_order_id: String,
    /// R5: Borrower's P2PK address
    pub borrower_address: String,
    /// R8: Lender's P2PK address
    pub lender_address: String,
    /// Loan token ID ("ERG" for native)
    pub loan_token_id: String,
    /// Loan token name
    pub loan_token_name: String,
    /// Loan token decimals
    pub loan_token_decimals: u8,
    /// R6: Total repayment amount in raw units
    pub repayment: u64,
    /// R7: Maturity height (absolute block height)
    pub maturity_height: i32,
    /// Collateral ERG in nanoERGs
    pub collateral_erg: u64,
    /// Collateral tokens
    pub collateral_tokens: Vec<CollateralToken>,
    /// Blocks remaining until maturity (negative = past due)
    pub blocks_remaining: i32,
    /// Whether the lender can liquidate (past maturity + wallet is lender)
    pub is_liquidable: bool,
    /// Whether the borrower can repay (before maturity + wallet is borrower)
    pub is_repayable: bool,
    /// Connected wallet is the lender
    pub is_own_lend: bool,
    /// Connected wallet is the borrower
    pub is_own_borrow: bool,
    /// Transaction ID of the box creation (for EIP-12 input)
    pub transaction_id: String,
    /// Output index in that transaction
    pub output_index: u16,
}

/// Complete bond market state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BondMarket {
    pub orders: Vec<OpenOrder>,
    pub bonds: Vec<ActiveBond>,
    pub block_height: u32,
}
