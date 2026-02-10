//! Data Transfer Objects for API requests and responses

use serde::{Deserialize, Serialize};

/// Health check response
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

/// Node status response
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

/// Node configuration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfigRequest {
    pub url: String,
    #[serde(default)]
    pub api_key: String,
}

/// Oracle price response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OraclePriceResponse {
    /// nanoERG per 1 USD (raw oracle value)
    pub nanoerg_per_usd: i64,
    /// ERG per USD (e.g., 0.54 means 1 ERG = $0.54 => 1 USD = 1.85 ERG)
    pub erg_usd: f64,
    /// Oracle box ID
    pub oracle_box_id: String,
}

/// Generic API error response
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

/// Mint preview request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintPreviewRequest {
    /// Amount of SigUSD to mint (raw units, 2 decimals)
    pub amount: i64,
    /// User's address
    pub user_address: String,
}

/// Mint preview response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintPreviewResponse {
    pub erg_cost_nano: String,
    pub protocol_fee_nano: String,
    pub tx_fee_nano: String,
    pub total_cost_nano: String,
    pub can_execute: bool,
    pub error: Option<String>,
}

/// Mint build request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintBuildRequest {
    /// Amount of SigUSD to mint (raw units, 2 decimals)
    pub amount: i64,
    /// User's address
    pub user_address: String,
    /// User's UTXOs from wallet (EIP-12 format)
    pub user_utxos: Vec<serde_json::Value>,
    /// Current block height
    pub current_height: i32,
}

/// Mint build response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintBuildResponse {
    /// Unsigned transaction in EIP-12 JSON format
    pub unsigned_tx: serde_json::Value,
    /// Transaction summary
    pub summary: TxSummaryDto,
}

/// Transaction summary for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSummaryDto {
    pub action: String,
    pub erg_amount_nano: String,
    pub token_amount: String,
    pub token_name: String,
    pub protocol_fee_nano: String,
    pub tx_fee_nano: String,
}

/// Transaction submission request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSubmitRequest {
    /// Signed transaction JSON
    pub signed_tx: serde_json::Value,
}

/// Transaction submission response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSubmitResponse {
    pub tx_id: String,
    pub submitted: bool,
}

// =============================================================================
// Mint Sign Flow DTOs (Phase 3)
// =============================================================================

/// Request to start ErgoPay signing flow for a built transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintSignRequest {
    /// The unsigned transaction (from build step)
    pub unsigned_tx: serde_json::Value,
    /// User-friendly message for wallet display
    pub message: String,
}

/// Response with ErgoPay URL for signing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintSignResponse {
    /// Request ID for polling status
    pub request_id: String,
    /// ErgoPay URL for QR code (ergopay://...)
    pub ergopay_url: String,
    /// Nautilus deep link URL for desktop signing
    pub nautilus_url: String,
}

/// Status response for mint transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintTxStatusResponse {
    /// Status: "pending", "signed", "submitted", "confirmed", "failed"
    pub status: String,
    /// Transaction ID (once submitted)
    pub tx_id: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
}

// =============================================================================
// Unified SigmaUSD Transaction DTOs
// =============================================================================

/// Unified preview request for any SigmaUSD operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdPreviewRequest {
    /// Operation: "mint_sigusd", "redeem_sigusd", "mint_sigrsv", "redeem_sigrsv"
    pub action: String,
    /// Amount in raw token units
    pub amount: i64,
    /// User's wallet address
    pub user_address: String,
}

/// Unified preview response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdPreviewResponse {
    /// For mint: ERG cost. For redeem: ERG received.
    pub erg_amount_nano: String,
    /// Protocol fee (2%)
    pub protocol_fee_nano: String,
    /// Network transaction fee
    pub tx_fee_nano: String,
    /// Total ERG change (positive = user pays, negative = user receives)
    pub total_erg_nano: String,
    /// Token amount (input for redeem, output for mint)
    pub token_amount: String,
    /// Token name for display
    pub token_name: String,
    /// Whether operation can be executed
    pub can_execute: bool,
    /// Error message if cannot execute
    pub error: Option<String>,
}

/// Unified build request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdBuildRequest {
    /// Operation: "mint_sigusd", "redeem_sigusd", "mint_sigrsv", "redeem_sigrsv"
    pub action: String,
    /// Amount in raw token units
    pub amount: i64,
    /// User's wallet address
    pub user_address: String,
    /// User's UTXOs as JSON
    pub user_utxos: Vec<serde_json::Value>,
    /// Current block height
    pub current_height: i32,
    /// Optional recipient address (base58). If set, primary output goes here.
    pub recipient_address: Option<String>,
}

/// Unified build response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdBuildResponse {
    /// Unsigned transaction in EIP-12 format
    pub unsigned_tx: serde_json::Value,
    /// Transaction summary for display
    pub summary: TxSummaryDto,
}

// =============================================================================
// Wallet Connection DTOs (Task 6)
// =============================================================================

/// Status values for wallet connection requests.
///
/// These are the possible values for `ConnectionStatusResponse::status`:
/// - `PENDING`: Connection request created, waiting for user to scan QR
/// - `CONNECTED`: User successfully connected their wallet
/// - `EXPIRED`: Connection request timed out (default: 5 minutes)
/// - `FAILED`: Connection failed; error details may be appended as "failed: <reason>"
pub mod wallet_status {
    pub const PENDING: &str = "pending";
    pub const CONNECTED: &str = "connected";
    pub const EXPIRED: &str = "expired";
    pub const FAILED: &str = "failed";
}

/// Response returned when initiating a new wallet connection request.
///
/// The client should display the QR code URL to the user and poll
/// the `/wallet/connect/{request_id}/status` endpoint to check connection status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConnectResponse {
    /// Unique identifier for this connection request.
    /// Use this to poll status via `/wallet/connect/{request_id}/status`.
    pub request_id: String,
    /// URL for the QR code image. Display this to the user to scan with their wallet.
    pub qr_url: String,
    /// URL for Nautilus browser extension wallet connection.
    pub nautilus_url: String,
}

/// Response indicating whether a wallet is currently connected to the session.
///
/// This is the response from `/wallet/status` which checks session-level wallet state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletStatusResponse {
    /// True if a wallet is connected to this session.
    pub connected: bool,
    /// The connected wallet's address. Only populated when `connected` is true.
    pub address: Option<String>,
}

/// Response for checking the status of a specific connection request.
///
/// This is the response from `/wallet/connect/{request_id}/status`.
///
/// # Status Values
///
/// See [`wallet_status`] module for constants. Possible values:
/// - `"pending"` - Waiting for user to scan QR and connect
/// - `"connected"` - Successfully connected; `address` will be populated
/// - `"expired"` - Request timed out; client should start a new connection
/// - `"failed"` or `"failed: <reason>"` - Connection failed; may include error details
///
/// # Field Population
///
/// - `address`: Only populated when `status` is `"connected"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusResponse {
    /// Current status of the connection request.
    /// See struct-level docs for possible values.
    pub status: String,
    /// The wallet address. Only populated when `status` is "connected".
    pub address: Option<String>,
}

// =============================================================================
// Wallet Balance DTOs (Phase 2)
// =============================================================================

/// Balance of a single token in a wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    /// Token ID (64 char hex string)
    pub token_id: String,
    /// Raw token amount (no decimal adjustment)
    pub amount: u64,
    /// Optional token name for known tokens
    pub name: Option<String>,
    /// Number of decimal places (for display formatting)
    pub decimals: u8,
}

/// Complete wallet balance response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalanceResponse {
    /// Wallet address
    pub address: String,
    /// ERG balance in nanoErgs
    pub erg_nano: u64,
    /// ERG balance formatted (with 9 decimals)
    pub erg_formatted: String,
    /// SigUSD balance (raw, 2 decimals)
    pub sigusd_amount: u64,
    /// SigUSD formatted (e.g., "123.45")
    pub sigusd_formatted: String,
    /// SigRSV balance (raw, 0 decimals)
    pub sigrsv_amount: u64,
    /// All token balances (including SigUSD/SigRSV)
    pub tokens: Vec<TokenBalance>,
}

// =============================================================================
// Dexy Protocol DTOs
// =============================================================================

/// Dexy protocol state response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyStateResponse {
    /// Variant: "gold" or "usd"
    pub variant: String,
    /// Bank ERG reserves (nanoERG)
    pub bank_erg_nano: i64,
    /// Dexy tokens available in bank
    pub dexy_in_bank: i64,
    /// Bank box ID
    pub bank_box_id: String,
    /// Dexy token ID (the token users receive)
    pub dexy_token_id: String,
    /// FreeMint remaining this period (actual mintable via FreeMint)
    pub free_mint_available: i64,
    /// Height at which FreeMint period resets
    pub free_mint_reset_height: i32,
    /// Current blockchain height (for period calculation)
    pub current_height: i32,
    /// Oracle rate (nanoERG per unit)
    pub oracle_rate_nano: i64,
    /// Oracle box ID
    pub oracle_box_id: String,
    /// LP ERG reserves
    pub lp_erg_reserves: i64,
    /// LP Dexy reserves
    pub lp_dexy_reserves: i64,
    /// LP box ID
    pub lp_box_id: String,
    /// LP rate (nanoERG per token)
    pub lp_rate_nano: i64,
    /// Whether minting is available
    pub can_mint: bool,
    /// Rate difference percentage (oracle vs LP)
    pub rate_difference_pct: f64,
    /// Circulating supply
    pub dexy_circulating: i64,
}

/// Dexy mint preview request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyPreviewRequest {
    /// Variant: "gold" or "usd"
    pub variant: String,
    /// Amount to mint (raw units)
    pub amount: i64,
    /// User's address
    pub user_address: String,
}

/// Dexy mint preview response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyPreviewResponse {
    /// ERG cost at oracle rate
    pub erg_cost_nano: String,
    /// Network transaction fee
    pub tx_fee_nano: String,
    /// Total ERG required
    pub total_cost_nano: String,
    /// Token amount to receive
    pub token_amount: String,
    /// Token name
    pub token_name: String,
    /// Whether operation can execute
    pub can_execute: bool,
    /// Error message if cannot execute
    pub error: Option<String>,
}

/// Dexy mint build request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyBuildRequest {
    /// Variant: "gold" or "usd"
    pub variant: String,
    /// Amount to mint (raw units)
    pub amount: i64,
    /// User's address
    pub user_address: String,
    /// User's UTXOs
    pub user_utxos: Vec<serde_json::Value>,
    /// Current block height
    pub current_height: i32,
    /// Optional recipient address (base58). If set, primary output goes here.
    pub recipient_address: Option<String>,
}

/// Dexy mint build response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyBuildResponse {
    /// Unsigned transaction
    pub unsigned_tx: serde_json::Value,
    /// Transaction summary
    pub summary: TxSummaryDto,
}

/// Dexy LP swap preview response
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

/// Dexy LP swap build response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexySwapBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: dexy::SwapTxSummary,
}

// =============================================================================
// AMM Protocol DTOs
// =============================================================================

/// AMM pool token amount DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmTokenDto {
    pub token_id: String,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// AMM pool response
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

/// List of AMM pools response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmmPoolsResponse {
    pub pools: Vec<AmmPoolDto>,
    pub count: usize,
}

/// Swap input DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "input_type")]
pub enum SwapInputDto {
    #[serde(rename = "erg")]
    Erg { amount: u64 },
    #[serde(rename = "token")]
    Token { token_id: String, amount: u64 },
}

/// Swap quote request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuoteRequest {
    pub pool_id: String,
    #[serde(flatten)]
    pub input: SwapInputDto,
}

/// Swap quote response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuoteResponse {
    pub input: SwapInputDto,
    pub output: AmmTokenDto,
    pub price_impact: f64,
    pub fee_amount: u64,
    pub effective_rate: f64,
    pub min_output_suggested: u64,
}

// =============================================================================
// Recent Transactions DTOs
// =============================================================================

/// A single transaction summary for dashboard display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentTxDto {
    /// Transaction ID
    pub tx_id: String,
    /// Block height where tx was included
    pub inclusion_height: u64,
    /// Number of confirmations
    pub num_confirmations: u64,
    /// Block timestamp (ms since epoch)
    pub timestamp: u64,
    /// Net ERG change for the wallet address (nanoERG, positive = received)
    pub erg_change_nano: i64,
    /// Token changes (positive = received, negative = sent)
    pub token_changes: Vec<TokenChangeDto>,
}

/// A token balance change in a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenChangeDto {
    pub token_id: String,
    pub amount: i64,
    pub name: Option<String>,
    pub decimals: u8,
}

/// Response for recent transactions
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

/// Swap build API request
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

/// Swap build API response
#[derive(Debug, Clone, Serialize)]
pub struct SwapBuildApiResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: SwapSummaryDto,
}

/// Swap transaction summary
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
