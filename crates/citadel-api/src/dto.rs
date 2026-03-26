use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

impl Default for HealthResponse {
    fn default() -> Self {
        Self {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatusResponse {
    pub connected: bool,
    pub url: String,
    pub node_name: Option<String>,
    pub network: String,
    pub chain_height: u64,
    pub indexed_height: Option<u64>,
    pub capability_tier: String,
    pub index_lag: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfigRequest {
    pub url: String,
    #[serde(default)]
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OraclePriceResponse {
    pub nanoerg_per_usd: i64,
    pub erg_usd: f64,
    pub oracle_box_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal_error", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new("bad_request", message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintPreviewRequest {
    pub amount: i64,
    pub user_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintPreviewResponse {
    pub erg_cost_nano: String,
    pub protocol_fee_nano: String,
    pub tx_fee_nano: String,
    pub total_cost_nano: String,
    pub can_execute: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintBuildRequest {
    pub amount: i64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: TxSummaryDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSummaryDto {
    pub action: String,
    pub erg_amount_nano: String,
    pub token_amount: String,
    pub token_name: String,
    pub protocol_fee_nano: String,
    pub tx_fee_nano: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSubmitRequest {
    pub signed_tx: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSubmitResponse {
    pub tx_id: String,
    pub submitted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintSignRequest {
    pub unsigned_tx: serde_json::Value,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintSignResponse {
    pub request_id: String,
    pub ergopay_url: String,
    pub nautilus_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintTxStatusResponse {
    pub status: String,
    pub tx_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdPreviewRequest {
    pub action: String,
    pub amount: i64,
    pub user_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdPreviewResponse {
    pub erg_amount_nano: String,
    pub protocol_fee_nano: String,
    pub tx_fee_nano: String,
    pub total_erg_nano: String,
    pub token_amount: String,
    pub token_name: String,
    pub can_execute: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdBuildRequest {
    pub action: String,
    pub amount: i64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
    pub recipient_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: TxSummaryDto,
}

pub mod wallet_status {
    pub const PENDING: &str = "pending";
    pub const CONNECTED: &str = "connected";
    pub const EXPIRED: &str = "expired";
    pub const FAILED: &str = "failed";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConnectResponse {
    pub request_id: String,
    pub qr_url: String,
    pub nautilus_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletStatusResponse {
    pub connected: bool,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusResponse {
    pub status: String,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub token_id: String,
    pub amount: u64,
    pub name: Option<String>,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalanceResponse {
    pub address: String,
    pub erg_nano: u64,
    pub erg_formatted: String,
    pub sigusd_amount: u64,
    pub sigusd_formatted: String,
    pub sigrsv_amount: u64,
    pub tokens: Vec<TokenBalance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyStateResponse {
    pub variant: String,
    pub bank_erg_nano: i64,
    pub dexy_in_bank: i64,
    pub bank_box_id: String,
    pub dexy_token_id: String,
    pub free_mint_available: i64,
    pub free_mint_reset_height: i32,
    pub current_height: i32,
    pub oracle_rate_nano: i64,
    pub oracle_box_id: String,
    pub lp_erg_reserves: i64,
    pub lp_dexy_reserves: i64,
    pub lp_box_id: String,
    pub lp_rate_nano: i64,
    pub lp_token_reserves: i64,
    pub lp_circulating: i64,
    pub can_redeem_lp: bool,
    pub can_mint: bool,
    pub rate_difference_pct: f64,
    pub dexy_circulating: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyPreviewRequest {
    pub variant: String,
    pub amount: i64,
    pub user_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyPreviewResponse {
    pub erg_cost_nano: String,
    pub tx_fee_nano: String,
    pub total_cost_nano: String,
    pub token_amount: String,
    pub token_name: String,
    pub can_execute: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyBuildRequest {
    pub variant: String,
    pub amount: i64,
    pub user_address: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
    pub recipient_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: TxSummaryDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexySwapPreviewResponse {
    pub variant: String,
    pub direction: String,
    pub input_amount: i64,
    pub output_amount: i64,
    pub output_token_name: String,
    pub output_decimals: u8,
    pub min_output: i64,
    pub price_impact: f64,
    pub fee_pct: f64,
    pub miner_fee_nano: i64,
    pub lp_erg_reserves: i64,
    pub lp_dexy_reserves: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexySwapBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: dexy::SwapTxSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyLpPreviewResponse {
    pub variant: String,
    pub action: String,
    pub erg_amount: String,
    pub dexy_amount: String,
    pub lp_tokens: String,
    pub redemption_fee_pct: Option<f64>,
    pub can_execute: bool,
    pub error: Option<String>,
    pub miner_fee_nano: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyLpBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: dexy::LpTxSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmTokenDto {
    pub token_id: String,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmPoolDto {
    pub pool_id: String,
    pub pool_type: String,
    pub box_id: String,
    pub erg_reserves: Option<u64>,
    pub token_x: Option<AmmTokenDto>,
    pub token_y: AmmTokenDto,
    pub lp_token_id: String,
    pub lp_circulating: u64,
    pub fee_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmPoolsResponse {
    pub pools: Vec<AmmPoolDto>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "input_type")]
pub enum SwapInputDto {
    #[serde(rename = "erg")]
    Erg { amount: u64 },
    #[serde(rename = "token")]
    Token { token_id: String, amount: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuoteRequest {
    pub pool_id: String,
    #[serde(flatten)]
    pub input: SwapInputDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuoteResponse {
    pub input: SwapInputDto,
    pub output: AmmTokenDto,
    pub price_impact: f64,
    pub fee_amount: u64,
    pub effective_rate: f64,
    pub min_output_suggested: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentTxDto {
    pub tx_id: String,
    pub inclusion_height: u64,
    pub num_confirmations: u64,
    pub timestamp: u64,
    pub erg_change_nano: i64,
    pub token_changes: Vec<TokenChangeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenChangeDto {
    pub token_id: String,
    pub amount: i64,
    pub name: Option<String>,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentTxsResponse {
    pub transactions: Vec<RecentTxDto>,
}

impl From<amm::AmmPool> for AmmPoolDto {
    fn from(pool: amm::AmmPool) -> Self {
        Self {
            pool_id: pool.pool_id,
            pool_type: format!("{:?}", pool.pool_type),
            box_id: pool.box_id,
            erg_reserves: pool.erg_reserves,
            token_x: pool.token_x.map(|t| AmmTokenDto {
                token_id: t.token_id,
                amount: t.amount,
                decimals: t.decimals,
                name: t.name,
            }),
            token_y: AmmTokenDto {
                token_id: pool.token_y.token_id,
                amount: pool.token_y.amount,
                decimals: pool.token_y.decimals,
                name: pool.token_y.name,
            },
            lp_token_id: pool.lp_token_id,
            lp_circulating: pool.lp_circulating,
            fee_percent: ((1.0 - pool.fee_num as f64 / pool.fee_denom as f64) * 10_000.0).round()
                / 100.0,
        }
    }
}

impl From<amm::SwapQuote> for SwapQuoteResponse {
    fn from(quote: amm::SwapQuote) -> Self {
        Self {
            input: match quote.input {
                amm::SwapInput::Erg { amount } => SwapInputDto::Erg { amount },
                amm::SwapInput::Token { token_id, amount } => {
                    SwapInputDto::Token { token_id, amount }
                }
            },
            output: AmmTokenDto {
                token_id: quote.output.token_id,
                amount: quote.output.amount,
                decimals: quote.output.decimals,
                name: quote.output.name,
            },
            price_impact: quote.price_impact,
            fee_amount: quote.fee_amount,
            effective_rate: quote.effective_rate,
            min_output_suggested: quote.min_output_suggested,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SwapBuildApiRequest {
    pub pool_id: String,
    #[serde(flatten)]
    pub input: SwapInputDto,
    pub min_output: u64,
    pub user_address: String,
    pub user_pk: String,
    pub user_ergo_tree: String,
    pub user_utxos: Vec<serde_json::Value>,
    pub current_height: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwapBuildApiResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: SwapSummaryDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwapSummaryDto {
    pub input_amount: u64,
    pub input_token: String,
    pub min_output: u64,
    pub output_token: String,
    pub execution_fee: u64,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}
