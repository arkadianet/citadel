//! Duckpools lending IPC / façade DTOs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketsResponse {
    pub pools: Vec<PoolInfo>,
    pub block_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolInfo {
    pub pool_id: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub is_erg_pool: bool,
    pub total_supplied: String,
    pub total_borrowed: String,
    pub available_liquidity: String,
    pub utilization_pct: f64,
    pub supply_apy: f64,
    pub borrow_apy: f64,
    pub pool_box_id: String,
    pub collateral_options: Vec<CollateralOptionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollateralOptionInfo {
    pub token_id: String,
    pub token_name: String,
    pub liquidation_threshold: u64,
    pub liquidation_penalty: u64,
    pub dex_nft: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionsResponse {
    pub address: String,
    pub lend_positions: Vec<LendPositionInfo>,
    pub borrow_positions: Vec<BorrowPositionInfo>,
    pub block_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendPositionInfo {
    pub pool_id: String,
    pub pool_name: String,
    pub lp_tokens: String,
    pub underlying_value: String,
    pub unrealized_profit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowPositionInfo {
    pub pool_id: String,
    pub pool_name: String,
    pub collateral_box_id: String,
    pub collateral_token: String,
    pub collateral_name: String,
    pub collateral_amount: String,
    pub borrowed_amount: String,
    pub total_owed: String,
    pub health_factor: f64,
    pub health_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendBuildRequest {
    pub pool_id: String,
    pub amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
    /// Slippage tolerance in basis points (0-200 for 0%-2%), defaults to 0
    #[serde(default)]
    pub slippage_bps: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawBuildRequest {
    pub pool_id: String,
    pub lp_amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowBuildRequest {
    pub pool_id: String,
    pub collateral_token: String,
    pub collateral_amount: u64,
    pub borrow_amount: u64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepayBuildRequest {
    pub pool_id: String,
    pub collateral_box_id: String,
    pub repay_amount: u64,
    /// Total owed with interest. Determines full vs partial repay proxy.
    pub total_owed: u64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundBuildRequest {
    pub proxy_box_id: String,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: LendingTxSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingTxSummary {
    pub action: String,
    pub pool_id: String,
    pub pool_name: String,
    pub amount_in: String,
    pub amount_out_estimate: Option<String>,
    pub tx_fee_nano: String,
    pub refund_height: i32,
    /// Service fee formatted for display (e.g. "0.006250 SigUSD")
    pub service_fee: String,
    /// Service fee in base units as string (e.g. "6250")
    pub service_fee_nano: String,
    /// Total tokens/ERG user sends to proxy (amount + fee + slippage)
    pub total_to_send: String,
}
