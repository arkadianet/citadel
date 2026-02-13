//! Duckpools Lending Transaction Builder
//!
//! Builds unsigned transactions for Duckpools lending operations.
//! Uses proxy box architecture - creates proxy boxes that Duckpools bots process.
//!
//! This module provides:
//! - Constants for transaction building
//! - Request/response structures
//! - UTXO selection utilities
//! - Sigma encoding helpers
//!
//! The actual transaction building functions (build_lend_tx, build_withdraw_tx, etc.)
//! will be added in subsequent tasks.

use std::collections::HashMap;

use crate::constants::PoolConfig;
use ergo_tx::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};

// =============================================================================
// Constants
// =============================================================================

/// Miner fee for the user's proxy-creating transaction (0.001 ERG).
/// Matches the Duckpools bot's TX_FEE = 1_000_000.
pub const TX_FEE_NANO: i64 = 1_000_000;

/// Proxy execution fee embedded in the proxy box for the bot (0.002 ERG).
/// The bot uses this to pay child transaction fees.
pub const PROXY_EXECUTION_FEE_NANO: i64 = 2_000_000;

/// Minimum box value in nanoERG (0.001 ERG)
pub const MIN_BOX_VALUE_NANO: i64 = citadel_core::constants::MIN_BOX_VALUE_NANO;

/// Bot processing overhead in nanoERG (0.003 ERG)
/// This covers the bot's costs for processing the proxy transaction
pub const BOT_PROCESSING_OVERHEAD: i64 = 3_000_000;

/// Refund height offset in blocks (~24 hours)
/// After this many blocks, users can reclaim stuck proxy boxes
pub const REFUND_HEIGHT_OFFSET: i32 = 720;

/// Miner fee ErgoTree (standard fee box script)
pub const MINER_FEE_ERGO_TREE: &str = citadel_core::constants::MINER_FEE_ERGO_TREE;

// =============================================================================
// Request Structures
// =============================================================================

/// User UTXO from wallet (simplified view of Eip12InputBox)
///
/// This structure represents an unspent box owned by the user that can be
/// used as input for building transactions.
#[derive(Debug, Clone)]
pub struct UserUtxo {
    /// Box ID (32 bytes hex)
    pub box_id: String,
    /// Transaction ID where this box was created
    pub tx_id: String,
    /// Output index in the creating transaction
    pub index: u16,
    /// Value in nanoERG
    pub value: i64,
    /// ErgoTree hex (spending script)
    pub ergo_tree: String,
    /// Assets: (token_id, amount)
    pub assets: Vec<(String, i64)>,
    /// Block height when box was created
    pub creation_height: i32,
    /// Additional registers (R4-R9)
    pub registers: HashMap<String, String>,
}

/// Request to lend (deposit) assets to a pool
#[derive(Debug, Clone)]
pub struct LendRequest {
    /// Pool identifier (e.g., "erg", "sigusd")
    pub pool_id: String,
    /// Amount to lend (in pool's base units)
    pub amount: u64,
    /// User's Ergo address
    pub user_address: String,
    /// User's available UTXOs
    pub user_utxos: Vec<UserUtxo>,
    /// Minimum LP tokens to receive (slippage protection)
    pub min_lp_tokens: Option<u64>,
    /// Slippage tolerance in basis points (0-200 for 0%-2%)
    pub slippage_bps: u16,
}

/// Request to withdraw (redeem LP tokens) from a pool
#[derive(Debug, Clone)]
pub struct WithdrawRequest {
    /// Pool identifier
    pub pool_id: String,
    /// Amount of LP tokens to redeem
    pub lp_amount: u64,
    /// User's Ergo address
    pub user_address: String,
    /// User's available UTXOs
    pub user_utxos: Vec<UserUtxo>,
    /// Minimum underlying to receive (slippage protection)
    pub min_output: Option<u64>,
}

/// Request to borrow assets using collateral
#[derive(Debug, Clone)]
pub struct BorrowRequest {
    /// Pool identifier to borrow from
    pub pool_id: String,
    /// Token ID of collateral (or "native" for ERG)
    pub collateral_token: String,
    /// Amount of collateral to provide
    pub collateral_amount: u64,
    /// Amount to borrow
    pub borrow_amount: u64,
    /// User's Ergo address
    pub user_address: String,
    /// User's available UTXOs
    pub user_utxos: Vec<UserUtxo>,
}

/// Request to repay a loan and reclaim collateral
#[derive(Debug, Clone)]
pub struct RepayRequest {
    /// Pool identifier
    pub pool_id: String,
    /// Box ID of the collateral position
    pub collateral_box_id: String,
    /// Amount to repay
    pub repay_amount: u64,
    /// Total owed including interest. Used to choose full vs partial repay proxy.
    pub total_owed: u64,
    /// User's Ergo address
    pub user_address: String,
    /// User's available UTXOs
    pub user_utxos: Vec<UserUtxo>,
}

/// Request to refund a stuck proxy box
#[derive(Debug, Clone)]
pub struct RefundRequest {
    /// Box ID of the proxy box to refund
    pub proxy_box_id: String,
    /// User's Ergo address (must match proxy's R4)
    pub user_address: String,
    /// User's available UTXOs (for fee if needed)
    pub user_utxos: Vec<UserUtxo>,
}

// =============================================================================
// Response Structures
// =============================================================================

/// Summary of a transaction for UI display
#[derive(Debug, Clone)]
pub struct TxSummary {
    /// Action type (e.g., "lend", "withdraw", "borrow", "repay", "refund")
    pub action: String,
    /// Pool identifier
    pub pool_id: String,
    /// Pool display name
    pub pool_name: String,
    /// Input amount formatted for display
    pub amount_in: String,
    /// Estimated output amount (if applicable)
    pub amount_out_estimate: Option<String>,
    /// Proxy contract address
    pub proxy_address: String,
    /// Block height after which refund is available
    pub refund_height: i32,
    /// Service fee in base units
    pub service_fee_raw: u64,
    /// Service fee formatted for display (e.g. "0.01 SigUSD")
    pub service_fee_display: String,
    /// Total tokens user sends (amount + fee + slippage buffer)
    pub total_to_send_raw: u64,
    /// Total to send formatted for display (e.g. "0.11 SigUSD")
    pub total_to_send_display: String,
}

/// Response from transaction building
#[derive(Debug, Clone)]
pub struct BuildResponse {
    /// JSON serialized EIP-12 unsigned transaction
    pub unsigned_tx: String,
    /// Transaction fee in nanoERG
    pub fee_nano: i64,
    /// Transaction summary for UI
    pub summary: TxSummary,
}

/// Proxy box data needed to build a refund transaction
///
/// This struct contains all the information needed to build a refund transaction
/// for a stuck proxy box. The proxy contract allows refunds when the current
/// blockchain height exceeds the refund height stored in R6.
#[derive(Debug, Clone)]
pub struct ProxyBoxData {
    /// Box ID (32 bytes hex)
    pub box_id: String,
    /// Transaction ID where this box was created
    pub tx_id: String,
    /// Output index in the creating transaction
    pub index: u16,
    /// Value in nanoERG
    pub value: i64,
    /// ErgoTree hex (proxy contract script)
    pub ergo_tree: String,
    /// Assets: (token_id, amount)
    pub assets: Vec<(String, i64)>,
    /// Block height when box was created
    pub creation_height: i32,
    /// User's ErgoTree hex from R4 or R5 (where refund goes)
    pub r4_user_tree: String,
    /// Block height after which refund is allowed (from R6)
    pub r6_refund_height: i64,
    /// All additional registers (R4-R9) as sigma-serialized hex.
    /// Must be included in the input box for correct box ID verification.
    pub additional_registers: HashMap<String, String>,
}

/// Response from refund transaction building
#[derive(Debug, Clone)]
pub struct RefundResponse {
    /// JSON serialized EIP-12 unsigned transaction
    pub unsigned_tx: String,
    /// Transaction fee in nanoERG
    pub fee_nano: i64,
    /// Block height after which refund is available
    pub refundable_after_height: i64,
}

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during transaction building
#[derive(Debug, Clone)]
pub enum BuildError {
    /// Pool not found
    PoolNotFound(String),
    /// Invalid amount
    InvalidAmount(String),
    /// Insufficient ERG balance
    InsufficientBalance { required: i64, available: i64 },
    /// Insufficient token balance
    InsufficientTokens {
        token: String,
        required: i64,
        available: i64,
    },
    /// Invalid address
    InvalidAddress(String),
    /// Transaction building failed
    TxBuildError(String),
    /// Proxy contract not configured
    ProxyContractMissing(String),
    /// Collateral box not found
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
    /// Get error code for API responses
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

    /// Get HTTP status code for API responses
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

// =============================================================================
// UTXO Selection
// =============================================================================

/// Result of UTXO selection
#[derive(Debug, Clone)]
pub struct SelectedInputs {
    /// Selected boxes
    pub boxes: Vec<UserUtxo>,
    /// Total ERG value
    pub total_erg: i64,
    /// Total amount of selected token (if selecting for token)
    pub token_amount: i64,
}

/// Select UTXOs to cover required ERG amount
///
/// Strategy: Sort by value descending, select until requirement met.
/// This minimizes the number of inputs needed.
pub fn select_erg_inputs(
    utxos: &[UserUtxo],
    required_erg: i64,
) -> Result<SelectedInputs, BuildError> {
    let mut selected = Vec::new();
    let mut total = 0i64;

    // Sort by value descending (largest first)
    let mut sorted_utxos: Vec<_> = utxos.iter().collect();
    sorted_utxos.sort_by(|a, b| b.value.cmp(&a.value));

    for utxo in sorted_utxos {
        if total >= required_erg {
            break;
        }
        selected.push(utxo.clone());
        total += utxo.value;
    }

    if total < required_erg {
        return Err(BuildError::InsufficientBalance {
            required: required_erg,
            available: total,
        });
    }

    Ok(SelectedInputs {
        boxes: selected,
        total_erg: total,
        token_amount: 0,
    })
}

/// Select UTXOs to cover required token amount and minimum ERG
///
/// Strategy:
/// 1. First select boxes containing the required token
/// 2. Then add more boxes if ERG requirement not met
pub fn select_token_inputs(
    utxos: &[UserUtxo],
    token_id: &str,
    required_amount: i64,
    min_erg: i64,
) -> Result<SelectedInputs, BuildError> {
    let mut selected = Vec::new();
    let mut total_erg = 0i64;
    let mut total_tokens = 0i64;

    // First pass: select boxes with the required token
    for utxo in utxos {
        for (tid, amt) in &utxo.assets {
            if tid == token_id {
                selected.push(utxo.clone());
                total_erg += utxo.value;
                total_tokens += amt;
                break;
            }
        }
        if total_tokens >= required_amount && total_erg >= min_erg {
            break;
        }
    }

    // Check if we have enough tokens
    if total_tokens < required_amount {
        return Err(BuildError::InsufficientTokens {
            token: token_id.to_string(),
            required: required_amount,
            available: total_tokens,
        });
    }

    // Second pass: add more boxes if ERG requirement not met
    if total_erg < min_erg {
        for utxo in utxos {
            // Skip already selected boxes
            if selected.iter().any(|u| u.box_id == utxo.box_id) {
                continue;
            }
            selected.push(utxo.clone());
            total_erg += utxo.value;
            if total_erg >= min_erg {
                break;
            }
        }
    }

    if total_erg < min_erg {
        return Err(BuildError::InsufficientBalance {
            required: min_erg,
            available: total_erg,
        });
    }

    Ok(SelectedInputs {
        boxes: selected,
        total_erg,
        token_amount: total_tokens,
    })
}

// =============================================================================
// Sigma Encoding Helpers
// =============================================================================

/// Encode a byte array for Sigma registers
///
/// Format: 0x0e (Coll[Byte] type tag) + VLQ length + data
///
/// This is used for encoding ErgoTrees, token IDs, and box IDs in registers.
pub fn encode_sigma_byte_array(data: &[u8]) -> String {
    let mut bytes = vec![0x0eu8]; // Coll[Byte] type tag

    // VLQ encode the length
    let mut len = data.len();
    loop {
        let mut byte = (len & 0x7F) as u8;
        len >>= 7;
        if len != 0 {
            byte |= 0x80; // Set continuation bit
        }
        bytes.push(byte);
        if len == 0 {
            break;
        }
    }

    // Append the data
    bytes.extend_from_slice(data);

    hex::encode(bytes)
}

/// Convert an Ergo address to its ErgoTree hex representation
///
/// Uses ergo-lib for address parsing and serialization.
pub fn address_to_ergo_tree(address: &str) -> Result<String, BuildError> {
    use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    // Try both mainnet and testnet prefixes
    for prefix in [NetworkPrefix::Mainnet, NetworkPrefix::Testnet] {
        let encoder = AddressEncoder::new(prefix);
        if let Ok(addr) = encoder.parse_address_from_str(address) {
            if let Ok(tree) = addr.script() {
                if let Ok(bytes) = tree.sigma_serialize_bytes() {
                    return Ok(hex::encode(bytes));
                }
            }
        }
    }

    Err(BuildError::InvalidAddress(format!(
        "Failed to parse address: {}",
        address
    )))
}

/// Convert an ErgoTree hex to an Ergo address (mainnet)
pub fn ergo_tree_to_address(ergo_tree_hex: &str) -> Result<String, BuildError> {
    use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let tree_bytes = hex::decode(ergo_tree_hex)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid ErgoTree hex: {}", e)))?;

    let tree = ErgoTree::sigma_parse_bytes(&tree_bytes)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to parse ErgoTree: {}", e)))?;

    let address = Address::recreate_from_ergo_tree(&tree)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to create address: {}", e)))?;

    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&address))
}

/// Encode a Long value for Sigma registers
///
/// Format: 0x05 (SLong type tag) + zigzag-encoded VLQ value
pub fn encode_sigma_long(value: i64) -> String {
    let mut bytes = vec![0x05u8]; // SLong type tag

    // Zigzag encode the value
    let zigzag = if value >= 0 {
        (value as u64) << 1
    } else {
        (((-value - 1) as u64) << 1) | 1
    };

    // VLQ encode the zigzag value
    let mut n = zigzag;
    loop {
        let mut byte = (n & 0x7F) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80; // Set continuation bit
        }
        bytes.push(byte);
        if n == 0 {
            break;
        }
    }

    hex::encode(bytes)
}

/// Convert a UserUtxo to Eip12InputBox for transaction building
fn user_utxo_to_eip12(utxo: &UserUtxo) -> Eip12InputBox {
    Eip12InputBox {
        box_id: utxo.box_id.clone(),
        transaction_id: utxo.tx_id.clone(),
        index: utxo.index,
        value: utxo.value.to_string(),
        ergo_tree: utxo.ergo_tree.clone(),
        assets: utxo
            .assets
            .iter()
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid.clone(),
                amount: amt.to_string(),
            })
            .collect(),
        creation_height: utxo.creation_height,
        additional_registers: utxo.registers.clone(),
        extension: HashMap::new(),
    }
}

// =============================================================================
// Transaction Building Functions
// =============================================================================

/// Build a lend (deposit) proxy transaction
///
/// Creates a proxy box that Duckpools bots will process to complete the lending operation.
/// The bot deducts a service fee from whatever tokens are in the proxy box, so we must
/// include amount + service_fee (+ optional slippage buffer) in the proxy box.
///
/// # Proxy Box Structure
/// - Value: (amount + fee) for ERG pools + BOT_PROCESSING_OVERHEAD, or BOT_PROCESSING_OVERHEAD for token pools
/// - Tokens: (amount + fee + slippage_buffer) currency tokens (token pools only)
/// - R4: user's ErgoTree (for refund)
/// - R5: minimum LP tokens (slippage protection)
/// - R6: refund height
/// - R7: lend token ID (token pools only)
pub fn build_lend_tx(
    req: LendRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    use crate::calculator;

    // Validate amount
    if req.amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Amount must be greater than 0".to_string(),
        ));
    }

    // Validate amount fits in i64 (required for arithmetic)
    if req.amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "Amount exceeds maximum supported value".to_string(),
        ));
    }

    // Clamp slippage to 0-200 bps (0%-2%)
    let slippage_bps = req.slippage_bps.min(200);

    // Calculate service fee on the deposit amount
    let service_fee = calculator::calculate_service_fee(req.amount, config.is_erg_pool);
    // Enforce minimum fee: 1 token unit for token pools, MIN_BOX_VALUE_NANO for ERG pools
    let service_fee = if config.is_erg_pool {
        service_fee.max(MIN_BOX_VALUE_NANO as u64)
    } else {
        service_fee.max(1)
    };

    // Calculate slippage buffer
    let slippage_buffer = req.amount * slippage_bps as u64 / 10000;

    // Total tokens/ERG to put in proxy box: amount + fee + slippage
    let total_to_send = req
        .amount
        .checked_add(service_fee)
        .and_then(|v| v.checked_add(slippage_buffer))
        .ok_or_else(|| {
            BuildError::TxBuildError("Amount overflow in total_to_send calculation".to_string())
        })?;

    // Convert user address to ErgoTree
    let user_ergo_tree = address_to_ergo_tree(&req.user_address)?;
    let user_ergo_tree_bytes = hex::decode(&user_ergo_tree)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid user ErgoTree: {}", e)))?;

    // Get proxy contract address
    let proxy_ergo_tree = address_to_ergo_tree(config.proxy_contracts.lend_address)?;

    // Calculate proxy box value
    let proxy_value = if config.is_erg_pool {
        // ERG pool: proxy holds the total deposit (amount + fee + slippage) + processing overhead
        (total_to_send as i64) + BOT_PROCESSING_OVERHEAD
    } else {
        // Token pool: proxy just holds processing overhead, tokens are separate
        BOT_PROCESSING_OVERHEAD
    };

    // Calculate total ERG required
    let total_required = proxy_value + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

    // Select inputs
    let inputs = if config.is_erg_pool {
        select_erg_inputs(&req.user_utxos, total_required)?
    } else {
        let currency_id = config.currency_id.ok_or_else(|| {
            BuildError::TxBuildError("Token pool missing currency_id".to_string())
        })?;
        select_token_inputs(
            &req.user_utxos,
            currency_id,
            total_to_send as i64,
            total_required,
        )?
    };

    // Convert inputs to EIP-12 format
    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();

    // Calculate refund height
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    // Build proxy box registers
    let mut proxy_registers = HashMap::new();

    // R4: user's ErgoTree (for refund)
    proxy_registers.insert(
        "R4".to_string(),
        encode_sigma_byte_array(&user_ergo_tree_bytes),
    );

    // R5: minimum LP tokens (slippage protection)
    proxy_registers.insert(
        "R5".to_string(),
        encode_sigma_long(req.min_lp_tokens.unwrap_or(0) as i64),
    );

    // R6: refund height
    proxy_registers.insert("R6".to_string(), encode_sigma_long(refund_height as i64));

    // R7: lend token ID (token pools only)
    if !config.is_erg_pool {
        let lend_token_bytes = hex::decode(config.lend_token_id)
            .map_err(|e| BuildError::TxBuildError(format!("Invalid lend token ID: {}", e)))?;
        proxy_registers.insert("R7".to_string(), encode_sigma_byte_array(&lend_token_bytes));
    }

    // Build proxy box assets — include total_to_send (amount + fee + slippage)
    let mut proxy_assets = Vec::new();
    if !config.is_erg_pool {
        let currency_id = config.currency_id.ok_or_else(|| {
            BuildError::TxBuildError(
                "Pool marked as non-ERG but currency_id is missing".to_string(),
            )
        })?;
        proxy_assets.push(Eip12Asset {
            token_id: currency_id.to_string(),
            amount: total_to_send.to_string(),
        });
    }

    // Build proxy output
    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: proxy_assets,
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    // Build fee output
    let fee_output = Eip12Output {
        value: TX_FEE_NANO.to_string(),
        ergo_tree: MINER_FEE_ERGO_TREE.to_string(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // Calculate change
    let change_erg = inputs.total_erg - proxy_value - TX_FEE_NANO;

    // Collect all tokens from inputs for change calculation
    let mut change_tokens: HashMap<String, i64> = HashMap::new();
    for utxo in &inputs.boxes {
        for (tid, amt) in &utxo.assets {
            *change_tokens.entry(tid.clone()).or_insert(0) += amt;
        }
    }

    // Subtract tokens going to proxy (token pools only)
    if !config.is_erg_pool {
        if let Some(currency_id) = config.currency_id {
            if let Some(amt) = change_tokens.get_mut(currency_id) {
                *amt -= total_to_send as i64;
                if *amt <= 0 {
                    change_tokens.remove(currency_id);
                }
            }
        }
    }

    // Build outputs
    let mut outputs = vec![proxy_output, fee_output];

    // Add change output if needed
    if change_erg >= MIN_BOX_VALUE_NANO || !change_tokens.is_empty() {
        let change_value = change_erg.max(MIN_BOX_VALUE_NANO);
        let change_assets: Vec<Eip12Asset> = change_tokens
            .into_iter()
            .filter(|(_, amt)| *amt > 0)
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid,
                amount: amt.to_string(),
            })
            .collect();

        outputs.push(Eip12Output::change(
            change_value,
            &user_ergo_tree,
            change_assets,
            current_height,
        ));
    }

    // Build unsigned transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs: eip12_inputs,
        data_inputs: vec![],
        outputs,
    };

    // Serialize to JSON
    let unsigned_tx_json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;

    // Get proxy address for display
    let proxy_address = ergo_tree_to_address(&proxy_ergo_tree)?;

    // Format amounts for display
    let divisor = 10f64.powi(config.decimals as i32);
    let amount_display = (req.amount as f64) / divisor;
    let fee_display = (service_fee as f64) / divisor;
    let total_display = (total_to_send as f64) / divisor;

    Ok(BuildResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "lend".to_string(),
            pool_id: config.id.to_string(),
            pool_name: config.name.to_string(),
            amount_in: format!("{:.6} {}", amount_display, config.symbol),
            amount_out_estimate: None,
            proxy_address,
            refund_height,
            service_fee_raw: service_fee,
            service_fee_display: format!("{:.6} {}", fee_display, config.symbol),
            total_to_send_raw: total_to_send,
            total_to_send_display: format!("{:.6} {}", total_display, config.symbol),
        },
    })
}

/// Build a withdraw proxy transaction
///
/// Creates a proxy box containing LP tokens that bots will process to redeem for underlying.
///
/// # Proxy Box Structure
/// - Value: MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO (for bot to pay execution fee)
/// - Tokens: LP tokens being redeemed
/// - R4: user's ErgoTree (for refund)
/// - R5: minimum output (slippage protection)
/// - R6: refund height
/// - R7: currency ID (token pools only)
pub fn build_withdraw_tx(
    req: WithdrawRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    // Validate amount
    if req.lp_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "LP amount must be greater than 0".to_string(),
        ));
    }

    // Validate amount fits in i64 (required for arithmetic)
    if req.lp_amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "LP amount exceeds maximum supported value".to_string(),
        ));
    }

    // Convert user address to ErgoTree
    let user_ergo_tree = address_to_ergo_tree(&req.user_address)?;
    let user_ergo_tree_bytes = hex::decode(&user_ergo_tree)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid user ErgoTree: {}", e)))?;

    // Get proxy contract address
    let proxy_ergo_tree = address_to_ergo_tree(config.proxy_contracts.withdraw_address)?;

    // Proxy value includes execution fee for bot to process the transaction
    let proxy_value = MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO;
    let total_required = proxy_value + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

    // Select inputs containing LP tokens + sufficient ERG
    let inputs = select_token_inputs(
        &req.user_utxos,
        config.lend_token_id,
        req.lp_amount as i64,
        total_required,
    )?;

    // Convert inputs to EIP-12 format
    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();

    // Calculate refund height
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    // Build proxy box registers
    let mut proxy_registers = HashMap::new();

    // R4: user's ErgoTree (for refund)
    proxy_registers.insert(
        "R4".to_string(),
        encode_sigma_byte_array(&user_ergo_tree_bytes),
    );

    // R5: minimum output (slippage protection)
    proxy_registers.insert(
        "R5".to_string(),
        encode_sigma_long(req.min_output.unwrap_or(0) as i64),
    );

    // R6: refund height
    proxy_registers.insert("R6".to_string(), encode_sigma_long(refund_height as i64));

    // R7: currency ID (token pools only)
    if !config.is_erg_pool {
        if let Some(currency_id) = config.currency_id {
            let currency_bytes = hex::decode(currency_id)
                .map_err(|e| BuildError::TxBuildError(format!("Invalid currency ID: {}", e)))?;
            proxy_registers.insert("R7".to_string(), encode_sigma_byte_array(&currency_bytes));
        }
    }

    // Build proxy output with LP tokens
    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: vec![Eip12Asset {
            token_id: config.lend_token_id.to_string(),
            amount: req.lp_amount.to_string(),
        }],
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    // Build fee output
    let fee_output = Eip12Output {
        value: TX_FEE_NANO.to_string(),
        ergo_tree: MINER_FEE_ERGO_TREE.to_string(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // Calculate change
    let change_erg = inputs.total_erg - proxy_value - TX_FEE_NANO;

    // Collect all tokens from inputs for change calculation
    let mut change_tokens: HashMap<String, i64> = HashMap::new();
    for utxo in &inputs.boxes {
        for (tid, amt) in &utxo.assets {
            *change_tokens.entry(tid.clone()).or_insert(0) += amt;
        }
    }

    // Subtract LP tokens going to proxy
    if let Some(amt) = change_tokens.get_mut(config.lend_token_id) {
        *amt -= req.lp_amount as i64;
        if *amt <= 0 {
            change_tokens.remove(config.lend_token_id);
        }
    }

    // Build outputs
    let mut outputs = vec![proxy_output, fee_output];

    // Add change output if needed
    if change_erg >= MIN_BOX_VALUE_NANO || !change_tokens.is_empty() {
        let change_value = change_erg.max(MIN_BOX_VALUE_NANO);
        let change_assets: Vec<Eip12Asset> = change_tokens
            .into_iter()
            .filter(|(_, amt)| *amt > 0)
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid,
                amount: amt.to_string(),
            })
            .collect();

        outputs.push(Eip12Output::change(
            change_value,
            &user_ergo_tree,
            change_assets,
            current_height,
        ));
    }

    // Build unsigned transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs: eip12_inputs,
        data_inputs: vec![],
        outputs,
    };

    // Serialize to JSON
    let unsigned_tx_json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;

    // Get proxy address for display
    let proxy_address = ergo_tree_to_address(&proxy_ergo_tree)?;

    Ok(BuildResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "withdraw".to_string(),
            pool_id: config.id.to_string(),
            pool_name: config.name.to_string(),
            amount_in: format!("{} LP", req.lp_amount),
            amount_out_estimate: None,
            proxy_address,
            refund_height,
            service_fee_raw: 0,
            service_fee_display: String::new(),
            total_to_send_raw: 0,
            total_to_send_display: String::new(),
        },
    })
}

/// Build a repay proxy transaction
///
/// Creates a proxy box to repay a borrowed loan and reclaim collateral.
/// The bot will process this proxy to return collateral tokens to the borrower.
///
/// # Proxy Box Structure
/// - Value: repay_amount + BOT_PROCESSING_OVERHEAD (ERG pools) OR BOT_PROCESSING_OVERHEAD (token pools)
/// - Tokens: repayment currency (token pools only)
/// - R4: neededAmount (Long) - 0 (bot calculates correct collateral return)
/// - R5: borrower (Coll[Byte]) - user's ErgoTree where collateral is returned
/// - R6: refundHeight (Int) - block height after which user can reclaim
/// - R7: collateralBoxId (Coll[Byte]) - identifies which loan position to repay
///
/// # Flow
/// 1. User creates proxy box with repayment ERG/tokens
/// 2. Bot finds the collateral box using R7
/// 3. Bot verifies loan can be repaid
/// 4. Bot returns collateral tokens to user (address from R5)
/// 5. Bot sends repayment to pool
pub fn build_repay_tx(
    req: RepayRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    // Validate repay amount
    if req.repay_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Repay amount must be greater than 0".to_string(),
        ));
    }

    // Validate amount fits in i64
    if req.repay_amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "Repay amount exceeds maximum supported value".to_string(),
        ));
    }

    // Validate collateral box ID (must be 64 hex chars = 32 bytes)
    if req.collateral_box_id.len() != 64 {
        return Err(BuildError::InvalidAmount(
            "Invalid collateral box ID: must be 64 hex characters".to_string(),
        ));
    }

    // Validate collateral box ID is valid hex
    let collateral_box_bytes = hex::decode(&req.collateral_box_id)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid collateral box ID hex: {}", e)))?;

    // Convert user address to ErgoTree
    let user_ergo_tree = address_to_ergo_tree(&req.user_address)?;
    let user_ergo_tree_bytes = hex::decode(&user_ergo_tree)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid user ErgoTree: {}", e)))?;

    // Choose full vs partial repay proxy based on whether repay covers total owed
    let is_full_repay = req.repay_amount >= req.total_owed || req.total_owed == 0;
    let proxy_address = if is_full_repay {
        config.proxy_contracts.repay_address
    } else {
        // Partial repay requires a dedicated proxy contract
        if config.proxy_contracts.partial_repay_address.is_empty() {
            return Err(BuildError::ProxyContractMissing(format!(
                "Partial repay proxy not configured for pool: {}. \
                 Please repay the full amount ({}) or wait for partial repay support.",
                config.id, req.total_owed
            )));
        }
        config.proxy_contracts.partial_repay_address
    };
    let proxy_ergo_tree = address_to_ergo_tree(proxy_address)?;

    // Calculate proxy box value
    // For ERG pool: user repays with ERG, so proxy needs repay_amount + overhead
    // For token pools: user repays with tokens, so proxy just needs overhead for ERG
    let proxy_value = if config.is_erg_pool {
        (req.repay_amount as i64) + BOT_PROCESSING_OVERHEAD
    } else {
        BOT_PROCESSING_OVERHEAD
    };

    // Total ERG required: proxy value + tx fee + change box minimum
    let total_required = proxy_value + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

    // Select inputs
    let inputs = if config.is_erg_pool {
        select_erg_inputs(&req.user_utxos, total_required)?
    } else {
        let currency_id = config.currency_id.ok_or_else(|| {
            BuildError::TxBuildError("Token pool missing currency_id".to_string())
        })?;
        select_token_inputs(
            &req.user_utxos,
            currency_id,
            req.repay_amount as i64,
            total_required,
        )?
    };

    // Convert inputs to EIP-12 format
    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();

    // Calculate refund height
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    // Build proxy box registers
    // Per Duckpools proxyRepay.md and e_repay_proxy.py:
    // R4 = neededAmount (Long) - minimum collateral to receive (0 = bot calculates)
    // R5 = borrower (Coll[Byte]) - user's ErgoTree
    // R6 = refundHeight (Int)
    // R7 = collateralBoxId (Coll[Byte]) - identifies the loan position
    let mut proxy_registers = HashMap::new();

    // R4: neededAmount - set to 0, bot will calculate correct collateral return
    proxy_registers.insert("R4".to_string(), encode_sigma_long(0));

    // R5: borrower - user's ErgoTree (CRITICAL: bot reads this to know where to send collateral)
    proxy_registers.insert(
        "R5".to_string(),
        encode_sigma_byte_array(&user_ergo_tree_bytes),
    );

    // R6: refundHeight (Int) — contract reads SELF.R6[Int].get, must be SInt not SLong
    proxy_registers.insert(
        "R6".to_string(),
        ergo_tx::sigma::encode_sigma_int(refund_height),
    );

    // R7: collateralBoxId - identifies which loan position to repay
    proxy_registers.insert(
        "R7".to_string(),
        encode_sigma_byte_array(&collateral_box_bytes),
    );

    // Build proxy box assets (token pools only)
    let mut proxy_assets = Vec::new();
    if !config.is_erg_pool {
        let currency_id = config.currency_id.ok_or_else(|| {
            BuildError::TxBuildError(
                "Pool marked as non-ERG but currency_id is missing".to_string(),
            )
        })?;
        proxy_assets.push(Eip12Asset {
            token_id: currency_id.to_string(),
            amount: req.repay_amount.to_string(),
        });
    }

    // Build proxy output
    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: proxy_assets,
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    // Build fee output
    let fee_output = Eip12Output {
        value: TX_FEE_NANO.to_string(),
        ergo_tree: MINER_FEE_ERGO_TREE.to_string(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // Calculate change
    let change_erg = inputs.total_erg - proxy_value - TX_FEE_NANO;

    // Collect all tokens from inputs for change calculation
    let mut change_tokens: HashMap<String, i64> = HashMap::new();
    for utxo in &inputs.boxes {
        for (tid, amt) in &utxo.assets {
            *change_tokens.entry(tid.clone()).or_insert(0) += amt;
        }
    }

    // Subtract repayment tokens from change (token pools only)
    if !config.is_erg_pool {
        if let Some(currency_id) = config.currency_id {
            if let Some(amt) = change_tokens.get_mut(currency_id) {
                *amt -= req.repay_amount as i64;
                if *amt <= 0 {
                    change_tokens.remove(currency_id);
                }
            }
        }
    }

    // Build outputs
    let mut outputs = vec![proxy_output, fee_output];

    // Add change output if needed
    if change_erg >= MIN_BOX_VALUE_NANO || !change_tokens.is_empty() {
        let change_value = change_erg.max(MIN_BOX_VALUE_NANO);
        let change_assets: Vec<Eip12Asset> = change_tokens
            .into_iter()
            .filter(|(_, amt)| *amt > 0)
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid,
                amount: amt.to_string(),
            })
            .collect();

        outputs.push(Eip12Output::change(
            change_value,
            &user_ergo_tree,
            change_assets,
            current_height,
        ));
    }

    // Build unsigned transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs: eip12_inputs,
        data_inputs: vec![],
        outputs,
    };

    // Serialize to JSON
    let unsigned_tx_json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;

    // Get proxy address for display
    let proxy_address = ergo_tree_to_address(&proxy_ergo_tree)?;

    // Format amount for display
    let divisor = 10f64.powi(config.decimals as i32);
    let amount_display = (req.repay_amount as f64) / divisor;

    Ok(BuildResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "repay".to_string(),
            pool_id: config.id.to_string(),
            pool_name: config.name.to_string(),
            amount_in: format!("{:.6} {}", amount_display, config.symbol),
            amount_out_estimate: Some("Collateral returned".to_string()),
            proxy_address,
            refund_height,
            service_fee_raw: 0,
            service_fee_display: String::new(),
            total_to_send_raw: 0,
            total_to_send_display: String::new(),
        },
    })
}

/// Build a borrow proxy transaction
///
/// Creates a proxy box that bots will process to execute a borrow.
///
/// **ERG pool** (user posts token collateral, borrows ERG):
/// - Proxy box value: `MIN_BOX_VALUE + PROXY_EXECUTION_FEE` (processing overhead)
/// - Proxy box tokens: `[{collateral_token_id, collateral_amount}]`
///
/// **Token pools** (user posts ERG collateral, borrows tokens):
/// - Proxy box value: `collateral_erg_amount + MIN_BOX_VALUE + PROXY_EXECUTION_FEE`
/// - Proxy box tokens: none
///
/// # Proxy Box Registers (both variants, per proxyBorrow.md)
/// - R4: user's ErgoTree (Coll[Byte]) — for refund
/// - R5: requestAmount (Long) — amount to borrow
/// - R6: refundHeight (Int)
/// - R7: userThresholdPenalty ((Long, Long)) — (threshold, penalty)
/// - R8: userDexNft (Coll[Byte]) — DEX NFT for pricing collateral
/// - R9: userPk (GroupElement) — user's compressed public key
pub fn build_borrow_tx(
    req: BorrowRequest,
    config: &PoolConfig,
    collateral_config: &crate::state::CollateralOption,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    // Validate borrow amount
    if req.borrow_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Borrow amount must be greater than 0".to_string(),
        ));
    }

    if req.collateral_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Collateral amount must be greater than 0".to_string(),
        ));
    }

    // Validate proxy contract is configured
    if config.proxy_contracts.borrow_address.is_empty() {
        return Err(BuildError::ProxyContractMissing(config.id.to_string()));
    }

    // Convert user address to ErgoTree
    let user_ergo_tree = address_to_ergo_tree(&req.user_address)?;
    let user_ergo_tree_bytes = hex::decode(&user_ergo_tree)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid user ErgoTree: {}", e)))?;

    // Extract user's public key from P2PK ErgoTree for R9 (GroupElement)
    let user_pk = ergo_tx::sigma::extract_pk_from_p2pk_ergo_tree(&user_ergo_tree)
        .map_err(|e| {
            BuildError::InvalidAddress(format!(
                "Address must be a P2PK address (not a script): {}",
                e
            ))
        })?;

    // Get proxy contract ErgoTree
    let proxy_ergo_tree = address_to_ergo_tree(config.proxy_contracts.borrow_address)?;

    // Calculate proxy box value and select inputs
    let (proxy_value, inputs) = if config.is_erg_pool {
        // ERG pool: user posts token collateral, borrows ERG
        // Proxy box only needs processing overhead in ERG, collateral is tokens
        let proxy_val = MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO;
        let total_required = proxy_val + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

        let selected = select_token_inputs(
            &req.user_utxos,
            &req.collateral_token,
            req.collateral_amount as i64,
            total_required,
        )?;
        (proxy_val, selected)
    } else {
        // Token pool: user posts ERG as collateral, borrows tokens
        // Proxy box holds the ERG collateral + processing overhead
        let proxy_val = (req.collateral_amount as i64) + MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO;
        let total_required = proxy_val + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

        let selected = select_erg_inputs(&req.user_utxos, total_required)?;
        (proxy_val, selected)
    };

    // Convert inputs to EIP-12 format
    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();

    // Calculate refund height
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    // Build proxy box registers
    let mut proxy_registers = HashMap::new();

    // R4: user's ErgoTree (for refund)
    proxy_registers.insert(
        "R4".to_string(),
        encode_sigma_byte_array(&user_ergo_tree_bytes),
    );

    // R5: requestAmount (Long) - amount to borrow
    proxy_registers.insert(
        "R5".to_string(),
        encode_sigma_long(req.borrow_amount as i64),
    );

    // R6: refundHeight (Int)
    proxy_registers.insert(
        "R6".to_string(),
        ergo_tx::sigma::encode_sigma_int(refund_height),
    );

    // R7: userThresholdPenalty ((Long, Long)) - (threshold, penalty) from CollateralOption
    proxy_registers.insert(
        "R7".to_string(),
        ergo_tx::sigma::encode_sigma_long_pair(
            collateral_config.liquidation_threshold as i64,
            collateral_config.liquidation_penalty as i64,
        ),
    );

    // R8: userDexNft (Coll[Byte]) - DEX NFT for pricing collateral
    let dex_nft_hex = collateral_config
        .dex_nft
        .as_deref()
        .ok_or_else(|| {
            BuildError::TxBuildError("Collateral option missing DEX NFT for pricing".to_string())
        })?;
    let dex_nft_bytes = hex::decode(dex_nft_hex)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid DEX NFT hex: {}", e)))?;
    proxy_registers.insert(
        "R8".to_string(),
        encode_sigma_byte_array(&dex_nft_bytes),
    );

    // R9: userPk (GroupElement) - user's compressed public key
    proxy_registers.insert(
        "R9".to_string(),
        ergo_tx::sigma::encode_sigma_group_element(&user_pk),
    );

    // Build proxy box assets (ERG pool: include collateral tokens; token pool: none)
    let mut proxy_assets = Vec::new();
    if config.is_erg_pool {
        proxy_assets.push(Eip12Asset {
            token_id: req.collateral_token.clone(),
            amount: req.collateral_amount.to_string(),
        });
    }

    // Build proxy output
    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: proxy_assets,
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    // Build fee output
    let fee_output = Eip12Output {
        value: TX_FEE_NANO.to_string(),
        ergo_tree: MINER_FEE_ERGO_TREE.to_string(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // Calculate change
    let change_erg = inputs.total_erg - proxy_value - TX_FEE_NANO;

    // Collect all tokens from inputs for change calculation
    let mut change_tokens: HashMap<String, i64> = HashMap::new();
    for utxo in &inputs.boxes {
        for (tid, amt) in &utxo.assets {
            *change_tokens.entry(tid.clone()).or_insert(0) += amt;
        }
    }

    // Subtract collateral tokens going to proxy (ERG pool only)
    if config.is_erg_pool {
        if let Some(amt) = change_tokens.get_mut(&req.collateral_token) {
            *amt -= req.collateral_amount as i64;
            if *amt <= 0 {
                change_tokens.remove(&req.collateral_token);
            }
        }
    }

    // Build outputs
    let mut outputs = vec![proxy_output, fee_output];

    // Add change output if needed
    if change_erg >= MIN_BOX_VALUE_NANO || !change_tokens.is_empty() {
        let change_value = change_erg.max(MIN_BOX_VALUE_NANO);
        let change_assets: Vec<Eip12Asset> = change_tokens
            .into_iter()
            .filter(|(_, amt)| *amt > 0)
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid,
                amount: amt.to_string(),
            })
            .collect();

        outputs.push(Eip12Output::change(
            change_value,
            &user_ergo_tree,
            change_assets,
            current_height,
        ));
    }

    // Build unsigned transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs: eip12_inputs,
        data_inputs: vec![],
        outputs,
    };

    // Serialize to JSON
    let unsigned_tx_json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;

    // Get proxy address for display
    let proxy_address = ergo_tree_to_address(&proxy_ergo_tree)?;

    // Format amounts for display
    let divisor = 10f64.powi(config.decimals as i32);
    let borrow_display = (req.borrow_amount as f64) / divisor;

    Ok(BuildResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        summary: TxSummary {
            action: "borrow".to_string(),
            pool_id: config.id.to_string(),
            pool_name: config.name.to_string(),
            amount_in: format!(
                "{} collateral",
                req.collateral_amount
            ),
            amount_out_estimate: Some(format!("{:.6} {}", borrow_display, config.symbol)),
            proxy_address,
            refund_height,
            service_fee_raw: 0,
            service_fee_display: String::new(),
            total_to_send_raw: req.collateral_amount,
            total_to_send_display: format!("{} collateral", req.collateral_amount),
        },
    })
}

/// Build a refund transaction to reclaim a stuck proxy box
///
/// Proxy contracts have different spending paths depending on type:
///
/// **Lend/Withdraw/Borrow proxies**: `proveDlog(userPk) || botPath`
///   - User can spend anytime with wallet signature, no output constraints.
///
/// **Repay/PartialRepay proxies**: `operationPath || refundPath` (NO proveDlog!)
///   - `refundPath`: `OUTPUTS.size < 3 && HEIGHT >= SELF.R6[Int].get` — fails if R6 is Long
///   - `operationPath`: `OUTPUTS.size >= 3` + checks on OUTPUTS(0):
///     - `propositionBytes == SELF.R5[Coll[Byte]].get` (user ErgoTree)
///     - `value >= SELF.R4[Long].get` (R4=0 for our proxies, so any value)
///     - `R4[Coll[Byte]].get == SELF.id` (output R4 = proxy box ID)
///
/// To handle both types universally, we always create 3 outputs:
/// - Output 0: user (tokens + R4=proxy_box_id) — triggers operationPath for repay proxies
/// - Output 1: user (dummy min box) — ensures OUTPUTS.size >= 3
/// - Output 2: miner fee
///
/// This is compatible with proveDlog proxies (signature validates regardless of output count)
/// and with repay proxies (triggers the operation path which doesn't check R6 type).
///
/// # Errors
/// - `InsufficientBalance`: If proxy box value too low to cover fee + 2 min outputs
pub fn build_refund_tx(
    proxy_box: ProxyBoxData,
    current_height: i32,
) -> Result<RefundResponse, BuildError> {
    // We need 3 outputs: user (tokens), user (dummy), miner fee.
    // Minimum ERG: MIN_BOX_VALUE_NANO * 2 + TX_FEE_NANO
    let min_required = MIN_BOX_VALUE_NANO * 2 + TX_FEE_NANO;
    if proxy_box.value < min_required {
        return Err(BuildError::InsufficientBalance {
            required: min_required,
            available: proxy_box.value,
        });
    }

    // User receives all ERG minus fee and dummy box
    let primary_value = proxy_box.value - TX_FEE_NANO - MIN_BOX_VALUE_NANO;

    // Build the input (the proxy box being spent)
    let input = Eip12InputBox {
        box_id: proxy_box.box_id.clone(),
        transaction_id: proxy_box.tx_id.clone(),
        index: proxy_box.index,
        value: proxy_box.value.to_string(),
        ergo_tree: proxy_box.ergo_tree.clone(),
        assets: proxy_box
            .assets
            .iter()
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid.clone(),
                amount: amt.to_string(),
            })
            .collect(),
        creation_height: proxy_box.creation_height,
        additional_registers: proxy_box.additional_registers.clone(),
        extension: HashMap::new(),
    };

    // Output 0: Primary refund to user — gets all tokens, R4 = proxy box ID
    let mut refund_registers = HashMap::new();
    refund_registers.insert("R4".to_string(), format!("0e20{}", proxy_box.box_id));

    let primary_output = Eip12Output {
        value: primary_value.to_string(),
        ergo_tree: proxy_box.r4_user_tree.clone(),
        assets: proxy_box
            .assets
            .iter()
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid.clone(),
                amount: amt.to_string(),
            })
            .collect(),
        creation_height: current_height,
        additional_registers: refund_registers,
    };

    // Output 1: Dummy min box to user — ensures OUTPUTS.size >= 3
    let dummy_output = Eip12Output {
        value: MIN_BOX_VALUE_NANO.to_string(),
        ergo_tree: proxy_box.r4_user_tree.clone(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // Output 2: Miner fee
    let fee_output = Eip12Output {
        value: TX_FEE_NANO.to_string(),
        ergo_tree: MINER_FEE_ERGO_TREE.to_string(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // Build unsigned transaction (3 outputs)
    let unsigned_tx = Eip12UnsignedTx {
        inputs: vec![input],
        data_inputs: vec![],
        outputs: vec![primary_output, dummy_output, fee_output],
    };

    // Serialize to JSON
    let unsigned_tx_json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;

    Ok(RefundResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        refundable_after_height: proxy_box.r6_refund_height,
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Valid mainnet P2PK address for testing
    const TEST_ADDRESS: &str = "9hY16vzHmmfyVBwKeFGHvb2bMFsG94A1u7To1QWtUokACyFVENQ";

    fn sample_utxo(box_id: &str, value: i64, assets: Vec<(String, i64)>) -> UserUtxo {
        UserUtxo {
            box_id: box_id.to_string(),
            tx_id: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".to_string(),
            index: 0,
            value,
            ergo_tree: "0008cd0327e65711a59378c59359c3571c6b49a4c25d28e5583b8fa2c99a7b4b5de5a34f"
                .to_string(),
            assets,
            creation_height: 1000000,
            registers: HashMap::new(),
        }
    }

    #[test]
    fn test_encode_sigma_byte_array() {
        // Empty array
        let encoded = encode_sigma_byte_array(&[]);
        assert_eq!(encoded, "0e00"); // Type tag + length 0

        // Single byte
        let encoded = encode_sigma_byte_array(&[0xab]);
        assert_eq!(encoded, "0e01ab"); // Type tag + length 1 + data

        // 32-byte array (typical for box IDs, token IDs)
        let data: Vec<u8> = (0..32).collect();
        let encoded = encode_sigma_byte_array(&data);
        assert!(encoded.starts_with("0e20")); // 0e = type, 20 = 32 in VLQ
        assert_eq!(encoded.len(), 4 + 64); // prefix + 32 bytes as hex
    }

    #[test]
    fn test_encode_sigma_byte_array_long_data() {
        // 128 bytes - requires 2-byte VLQ length
        let data: Vec<u8> = (0..128).map(|i| i as u8).collect();
        let encoded = encode_sigma_byte_array(&data);
        // 128 in VLQ is 0x8001 (continuation bit set on first byte)
        assert!(encoded.starts_with("0e8001"));
    }

    #[test]
    fn test_build_error_display() {
        let err = BuildError::InsufficientBalance {
            required: 100,
            available: 50,
        };
        let msg = err.to_string();
        assert!(msg.contains("100"));
        assert!(msg.contains("50"));

        let err = BuildError::InvalidAmount("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = BuildError::PoolNotFound("unknown".to_string());
        assert!(err.to_string().contains("unknown"));

        let err = BuildError::CollateralBoxNotFound("boxid".to_string());
        assert!(err.to_string().contains("boxid"));
    }

    #[test]
    fn test_build_error_codes() {
        assert_eq!(
            BuildError::PoolNotFound("x".to_string()).code(),
            "pool_not_found"
        );
        assert_eq!(
            BuildError::InvalidAmount("x".to_string()).code(),
            "invalid_amount"
        );
        assert_eq!(
            BuildError::InsufficientBalance {
                required: 1,
                available: 0
            }
            .code(),
            "insufficient_balance"
        );
        assert_eq!(
            BuildError::InsufficientTokens {
                token: "x".to_string(),
                required: 1,
                available: 0
            }
            .code(),
            "insufficient_tokens"
        );
    }

    #[test]
    fn test_build_error_status_codes() {
        assert_eq!(
            BuildError::InvalidAmount("x".to_string()).status_code(),
            400
        );
        assert_eq!(
            BuildError::InvalidAddress("x".to_string()).status_code(),
            400
        );
        assert_eq!(
            BuildError::InsufficientBalance {
                required: 1,
                available: 0
            }
            .status_code(),
            422
        );
        assert_eq!(BuildError::PoolNotFound("x".to_string()).status_code(), 404);
        assert_eq!(
            BuildError::ProxyContractMissing("x".to_string()).status_code(),
            503
        );
        assert_eq!(BuildError::TxBuildError("x".to_string()).status_code(), 500);
    }

    #[test]
    fn test_select_erg_inputs_success() {
        let utxos = vec![
            sample_utxo("box1", 1_000_000_000, vec![]), // 1 ERG
            sample_utxo("box2", 2_000_000_000, vec![]), // 2 ERG
            sample_utxo("box3", 500_000_000, vec![]),   // 0.5 ERG
        ];

        // Need 1.5 ERG - should select box2 (2 ERG) first
        let result = select_erg_inputs(&utxos, 1_500_000_000).unwrap();
        assert_eq!(result.boxes.len(), 1);
        assert_eq!(result.total_erg, 2_000_000_000);
    }

    #[test]
    fn test_select_erg_inputs_multiple_boxes() {
        let utxos = vec![
            sample_utxo("box1", 1_000_000_000, vec![]),
            sample_utxo("box2", 1_000_000_000, vec![]),
            sample_utxo("box3", 1_000_000_000, vec![]),
        ];

        // Need 2.5 ERG - should select 3 boxes
        let result = select_erg_inputs(&utxos, 2_500_000_000).unwrap();
        assert_eq!(result.boxes.len(), 3);
        assert_eq!(result.total_erg, 3_000_000_000);
    }

    #[test]
    fn test_select_erg_inputs_insufficient() {
        let utxos = vec![sample_utxo("box1", 1_000_000_000, vec![])];

        // Need 10 ERG but only have 1
        let result = select_erg_inputs(&utxos, 10_000_000_000);
        assert!(result.is_err());

        match result {
            Err(BuildError::InsufficientBalance {
                required,
                available,
            }) => {
                assert_eq!(required, 10_000_000_000);
                assert_eq!(available, 1_000_000_000);
            }
            _ => panic!("Expected InsufficientBalance error"),
        }
    }

    #[test]
    fn test_select_token_inputs_success() {
        let token_id = "abc123".to_string();
        let utxos = vec![
            sample_utxo("box1", 1_000_000_000, vec![(token_id.clone(), 100)]),
            sample_utxo("box2", 2_000_000_000, vec![]),
        ];

        let result = select_token_inputs(&utxos, &token_id, 50, 500_000_000).unwrap();
        assert_eq!(result.boxes.len(), 1);
        assert_eq!(result.token_amount, 100);
        assert_eq!(result.total_erg, 1_000_000_000);
    }

    #[test]
    fn test_select_token_inputs_needs_more_erg() {
        let token_id = "abc123".to_string();
        let utxos = vec![
            sample_utxo("box1", 100_000_000, vec![(token_id.clone(), 100)]), // Low ERG
            sample_utxo("box2", 2_000_000_000, vec![]),                      // No tokens
        ];

        // Need 50 tokens and 1 ERG
        let result = select_token_inputs(&utxos, &token_id, 50, 1_000_000_000).unwrap();
        assert_eq!(result.boxes.len(), 2); // Need both boxes
        assert_eq!(result.token_amount, 100);
        assert_eq!(result.total_erg, 2_100_000_000);
    }

    #[test]
    fn test_select_token_inputs_insufficient_tokens() {
        let token_id = "abc123".to_string();
        let utxos = vec![sample_utxo(
            "box1",
            1_000_000_000,
            vec![(token_id.clone(), 50)],
        )];

        // Need 100 tokens but only have 50
        let result = select_token_inputs(&utxos, &token_id, 100, 500_000_000);
        assert!(result.is_err());

        match result {
            Err(BuildError::InsufficientTokens {
                token,
                required,
                available,
            }) => {
                assert_eq!(token, token_id);
                assert_eq!(required, 100);
                assert_eq!(available, 50);
            }
            _ => panic!("Expected InsufficientTokens error"),
        }
    }

    #[test]
    fn test_address_to_ergo_tree_valid() {
        // Valid mainnet address
        let result = address_to_ergo_tree(TEST_ADDRESS);
        assert!(result.is_ok());

        let ergo_tree = result.unwrap();
        assert!(ergo_tree.starts_with("0008cd")); // P2PK ErgoTree prefix
    }

    #[test]
    fn test_address_to_ergo_tree_invalid() {
        let result = address_to_ergo_tree("invalid_address");
        assert!(result.is_err());
        assert!(matches!(result, Err(BuildError::InvalidAddress(_))));
    }

    #[test]
    fn test_constants() {
        assert_eq!(TX_FEE_NANO, 1_000_000);
        assert_eq!(PROXY_EXECUTION_FEE_NANO, 2_000_000);
        assert_eq!(MIN_BOX_VALUE_NANO, 1_000_000);
        assert_eq!(BOT_PROCESSING_OVERHEAD, 3_000_000);
        assert_eq!(REFUND_HEIGHT_OFFSET, 720);
    }

    #[test]
    fn test_user_utxo_struct() {
        let utxo = UserUtxo {
            box_id: "a".repeat(64),
            tx_id: "b".repeat(64),
            index: 0,
            value: 1_000_000_000,
            ergo_tree: "0008cd...".to_string(),
            assets: vec![("token1".to_string(), 100)],
            creation_height: 12345,
            registers: HashMap::new(),
        };

        assert_eq!(utxo.value, 1_000_000_000);
        assert_eq!(utxo.assets.len(), 1);
    }

    #[test]
    fn test_lend_request_struct() {
        let req = LendRequest {
            pool_id: "erg".to_string(),
            amount: 1_000_000_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![],
            min_lp_tokens: Some(100),
            slippage_bps: 0,
        };

        assert_eq!(req.pool_id, "erg");
        assert_eq!(req.amount, 1_000_000_000);
        assert!(req.min_lp_tokens.is_some());
    }

    #[test]
    fn test_withdraw_request_struct() {
        let req = WithdrawRequest {
            pool_id: "sigusd".to_string(),
            lp_amount: 1000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![],
            min_output: None,
        };

        assert_eq!(req.pool_id, "sigusd");
        assert_eq!(req.lp_amount, 1000);
        assert!(req.min_output.is_none());
    }

    #[test]
    fn test_borrow_request_struct() {
        let req = BorrowRequest {
            pool_id: "sigusd".to_string(),
            collateral_token: "native".to_string(),
            collateral_amount: 10_000_000_000,
            borrow_amount: 100_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![],
        };

        assert_eq!(req.pool_id, "sigusd");
        assert_eq!(req.collateral_token, "native");
    }

    #[test]
    fn test_repay_request_struct() {
        let req = RepayRequest {
            pool_id: "erg".to_string(),
            collateral_box_id: "a".repeat(64),
            repay_amount: 5_000_000_000,
            total_owed: 5_000_000_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![],
        };

        assert_eq!(req.pool_id, "erg");
        assert_eq!(req.collateral_box_id.len(), 64);
    }

    #[test]
    fn test_refund_request_struct() {
        let req = RefundRequest {
            proxy_box_id: "a".repeat(64),
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![],
        };

        assert_eq!(req.proxy_box_id.len(), 64);
    }

    #[test]
    fn test_tx_summary_struct() {
        let summary = TxSummary {
            action: "lend".to_string(),
            pool_id: "erg".to_string(),
            pool_name: "ERG Pool".to_string(),
            amount_in: "10.0 ERG".to_string(),
            amount_out_estimate: Some("~100 LP".to_string()),
            proxy_address: TEST_ADDRESS.to_string(),
            refund_height: 1000720,
            service_fee_raw: 62500000,
            service_fee_display: "0.062500 ERG".to_string(),
            total_to_send_raw: 10_062_500_000,
            total_to_send_display: "10.062500 ERG".to_string(),
        };

        assert_eq!(summary.action, "lend");
        assert!(summary.amount_out_estimate.is_some());
    }

    #[test]
    fn test_build_response_struct() {
        let response = BuildResponse {
            unsigned_tx: "{}".to_string(),
            fee_nano: TX_FEE_NANO,
            summary: TxSummary {
                action: "withdraw".to_string(),
                pool_id: "erg".to_string(),
                pool_name: "ERG Pool".to_string(),
                amount_in: "100 LP".to_string(),
                amount_out_estimate: None,
                proxy_address: TEST_ADDRESS.to_string(),
                refund_height: 1000720,
                service_fee_raw: 0,
                service_fee_display: String::new(),
                total_to_send_raw: 0,
                total_to_send_display: String::new(),
            },
        };

        assert_eq!(response.fee_nano, TX_FEE_NANO);
        assert!(response.summary.amount_out_estimate.is_none());
    }

    #[test]
    fn test_encode_sigma_long() {
        // Zero
        assert_eq!(encode_sigma_long(0), "0500");

        // Positive: 1
        assert_eq!(encode_sigma_long(1), "0502");

        // Positive: 100
        assert_eq!(encode_sigma_long(100), "05c801");

        // Negative: -1
        assert_eq!(encode_sigma_long(-1), "0501");

        // Large positive value
        let encoded = encode_sigma_long(1_000_000_000);
        assert!(encoded.starts_with("05")); // Type tag
        assert!(encoded.len() > 4); // Has data
    }

    #[test]
    fn test_user_utxo_to_eip12() {
        let utxo = sample_utxo("box123", 1_000_000_000, vec![("token1".to_string(), 100)]);
        let eip12 = user_utxo_to_eip12(&utxo);

        assert_eq!(eip12.box_id, "box123");
        assert_eq!(eip12.value, "1000000000");
        assert_eq!(eip12.assets.len(), 1);
        assert_eq!(eip12.assets[0].token_id, "token1");
        assert_eq!(eip12.assets[0].amount, "100");
    }

    #[test]
    fn test_build_lend_tx_erg_pool() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        // Amount to lend: 10 ERG = 10_000_000_000 nanoERG
        let amount: u64 = 10_000_000_000;

        // Need: amount + BOT_PROCESSING_OVERHEAD + TX_FEE_NANO + MIN_BOX_VALUE_NANO
        // = 10_000_000_000 + 3_000_000 + 1_000_000 + 1_000_000 = 10_005_000_000
        let utxos = vec![sample_utxo(
            "a".repeat(64).as_str(),
            15_000_000_000, // 15 ERG - enough for lend + fees + change
            vec![],
        )];

        let req = LendRequest {
            pool_id: "erg".to_string(),
            amount,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: utxos,
            min_lp_tokens: Some(100),
            slippage_bps: 0,
        };

        let result = build_lend_tx(req, config, current_height);
        assert!(result.is_ok(), "build_lend_tx failed: {:?}", result.err());

        let response = result.unwrap();
        assert_eq!(response.fee_nano, TX_FEE_NANO);
        assert_eq!(response.summary.action, "lend");
        assert_eq!(response.summary.pool_id, "erg");
        assert_eq!(
            response.summary.refund_height,
            current_height + REFUND_HEIGHT_OFFSET
        );

        // Verify service fee is calculated and included
        // 10 ERG / 160 = 62_500_000 nanoERG, but min is MIN_BOX_VALUE_NANO (1_000_000)
        // 62_500_000 > 1_000_000, so fee = 62_500_000
        assert_eq!(response.summary.service_fee_raw, 62_500_000);
        assert_eq!(response.summary.total_to_send_raw, amount + 62_500_000); // no slippage

        // Verify the transaction parses as valid JSON
        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        assert!(tx["inputs"].is_array());
        assert!(tx["outputs"].is_array());

        // Should have 3 outputs: proxy, fee, change
        assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

        // Verify proxy box value includes amount + fee + overhead
        let proxy_value: i64 = tx["outputs"][0]["value"].as_str().unwrap().parse().unwrap();
        let expected_proxy = (amount + 62_500_000) as i64 + BOT_PROCESSING_OVERHEAD;
        assert_eq!(proxy_value, expected_proxy);
    }

    #[test]
    fn test_build_lend_tx_zero_amount() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        let req = LendRequest {
            pool_id: "erg".to_string(),
            amount: 0, // Zero amount
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![],
            min_lp_tokens: None,
            slippage_bps: 0,
        };

        let result = build_lend_tx(req, config, current_height);
        assert!(result.is_err());

        match result {
            Err(BuildError::InvalidAmount(msg)) => {
                assert!(msg.contains("greater than 0"));
            }
            _ => panic!("Expected InvalidAmount error"),
        }
    }

    #[test]
    fn test_build_withdraw_tx_success() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        // LP token ID for ERG pool
        let lp_token_id = config.lend_token_id.to_string();
        let lp_amount: u64 = 1000;

        // Need: MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO (proxy) + TX_FEE_NANO + MIN_BOX_VALUE_NANO
        // = 1_000_000 + 2_000_000 + 1_000_000 + 1_000_000 = 5_000_000
        let utxos = vec![sample_utxo(
            "b".repeat(64).as_str(),
            10_000_000_000,                    // 10 ERG
            vec![(lp_token_id.clone(), 5000)], // 5000 LP tokens
        )];

        let req = WithdrawRequest {
            pool_id: "erg".to_string(),
            lp_amount,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: utxos,
            min_output: Some(9_000_000_000), // Expect at least 9 ERG back
        };

        let result = build_withdraw_tx(req, config, current_height);
        assert!(
            result.is_ok(),
            "build_withdraw_tx failed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert_eq!(response.fee_nano, TX_FEE_NANO);
        assert_eq!(response.summary.action, "withdraw");
        assert_eq!(response.summary.pool_id, "erg");
        assert_eq!(
            response.summary.refund_height,
            current_height + REFUND_HEIGHT_OFFSET
        );

        // Verify the transaction parses as valid JSON
        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        assert!(tx["inputs"].is_array());
        assert!(tx["outputs"].is_array());

        // Should have 3 outputs: proxy, fee, change
        assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

        // Proxy output should have LP tokens
        let proxy_output = &tx["outputs"][0];
        assert!(!proxy_output["assets"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_build_withdraw_tx_insufficient_lp() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        let lp_token_id = config.lend_token_id.to_string();

        // User has only 50 LP tokens but wants to withdraw 1000
        let utxos = vec![sample_utxo(
            "c".repeat(64).as_str(),
            10_000_000_000,                  // 10 ERG
            vec![(lp_token_id.clone(), 50)], // Only 50 LP tokens
        )];

        let req = WithdrawRequest {
            pool_id: "erg".to_string(),
            lp_amount: 1000, // Want 1000 LP
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: utxos,
            min_output: None,
        };

        let result = build_withdraw_tx(req, config, current_height);
        assert!(result.is_err());

        match result {
            Err(BuildError::InsufficientTokens {
                token,
                required,
                available,
            }) => {
                assert_eq!(token, lp_token_id);
                assert_eq!(required, 1000);
                assert_eq!(available, 50);
            }
            _ => panic!("Expected InsufficientTokens error"),
        }
    }

    #[test]
    fn test_miner_fee_ergo_tree_constant() {
        assert!(MINER_FEE_ERGO_TREE.starts_with("1005040004000e36"));
    }

    #[test]
    fn test_build_repay_tx_erg_pool() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        // Amount to repay: 5 ERG = 5_000_000_000 nanoERG
        let repay_amount: u64 = 5_000_000_000;

        // Collateral box ID (32 bytes = 64 hex chars)
        let collateral_box_id = "a".repeat(64);

        // Need: repay_amount + BOT_PROCESSING_OVERHEAD + TX_FEE_NANO + MIN_BOX_VALUE_NANO
        // = 5_000_000_000 + 3_000_000 + 1_000_000 + 1_000_000 = 5_005_000_000
        let utxos = vec![sample_utxo(
            "d".repeat(64).as_str(),
            10_000_000_000, // 10 ERG - enough for repay + fees + change
            vec![],
        )];

        let req = RepayRequest {
            pool_id: "erg".to_string(),
            collateral_box_id: collateral_box_id.clone(),
            repay_amount,
            total_owed: repay_amount,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: utxos,
        };

        let result = build_repay_tx(req, config, current_height);
        assert!(result.is_ok(), "build_repay_tx failed: {:?}", result.err());

        let response = result.unwrap();
        assert_eq!(response.fee_nano, TX_FEE_NANO);
        assert_eq!(response.summary.action, "repay");
        assert_eq!(response.summary.pool_id, "erg");
        assert_eq!(
            response.summary.refund_height,
            current_height + REFUND_HEIGHT_OFFSET
        );
        assert!(response.summary.amount_out_estimate.is_some());

        // Verify the transaction parses as valid JSON
        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        assert!(tx["inputs"].is_array());
        assert!(tx["outputs"].is_array());

        // Should have 3 outputs: proxy, fee, change
        assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

        // Verify proxy output has the correct registers
        let proxy_output = &tx["outputs"][0];
        let registers = &proxy_output["additionalRegisters"];
        assert!(registers["R4"].is_string()); // neededAmount
        assert!(registers["R5"].is_string()); // borrower
        assert!(registers["R6"].is_string()); // refundHeight
        assert!(registers["R7"].is_string()); // collateralBoxId
    }

    #[test]
    fn test_build_repay_tx_zero_amount() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        let req = RepayRequest {
            pool_id: "erg".to_string(),
            collateral_box_id: "a".repeat(64),
            repay_amount: 0, // Zero amount
            total_owed: 1_000_000_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![],
        };

        let result = build_repay_tx(req, config, current_height);
        assert!(result.is_err());

        match result {
            Err(BuildError::InvalidAmount(msg)) => {
                assert!(msg.contains("greater than 0"));
            }
            _ => panic!("Expected InvalidAmount error"),
        }
    }

    #[test]
    fn test_build_repay_tx_invalid_collateral_id_too_short() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        let req = RepayRequest {
            pool_id: "erg".to_string(),
            collateral_box_id: "abc123".to_string(), // Too short - not 64 chars
            repay_amount: 1_000_000_000,
            total_owed: 1_000_000_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![sample_utxo("e".repeat(64).as_str(), 10_000_000_000, vec![])],
        };

        let result = build_repay_tx(req, config, current_height);
        assert!(result.is_err());

        match result {
            Err(BuildError::InvalidAmount(msg)) => {
                assert!(msg.contains("64 hex characters"));
            }
            _ => panic!("Expected InvalidAmount error for invalid collateral box ID"),
        }
    }

    #[test]
    fn test_build_repay_tx_invalid_collateral_id_not_hex() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        // 64 chars but not valid hex (contains 'g' which is invalid)
        let invalid_hex = "g".repeat(64);

        let req = RepayRequest {
            pool_id: "erg".to_string(),
            collateral_box_id: invalid_hex,
            repay_amount: 1_000_000_000,
            total_owed: 1_000_000_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![sample_utxo("f".repeat(64).as_str(), 10_000_000_000, vec![])],
        };

        let result = build_repay_tx(req, config, current_height);
        assert!(result.is_err());

        match result {
            Err(BuildError::TxBuildError(msg)) => {
                assert!(msg.contains("hex"));
            }
            _ => panic!("Expected TxBuildError for invalid hex"),
        }
    }

    #[test]
    fn test_build_repay_tx_insufficient_balance() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        // User wants to repay 10 ERG but only has 1 ERG
        let req = RepayRequest {
            pool_id: "erg".to_string(),
            collateral_box_id: "a".repeat(64),
            repay_amount: 10_000_000_000, // 10 ERG
            total_owed: 10_000_000_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![sample_utxo(
                "g".repeat(64).as_str(),
                1_000_000_000, // Only 1 ERG
                vec![],
            )],
        };

        let result = build_repay_tx(req, config, current_height);
        assert!(result.is_err());

        match result {
            Err(BuildError::InsufficientBalance {
                required,
                available,
            }) => {
                // Required: 10_000_000_000 + 3_000_000 + 1_000_000 + 1_000_000 = 10_005_000_000
                assert!(required > 10_000_000_000);
                assert_eq!(available, 1_000_000_000);
            }
            _ => panic!("Expected InsufficientBalance error"),
        }
    }

    #[test]
    fn test_build_borrow_tx_token_pool_success() {
        use crate::constants::get_pool;
        use crate::state::CollateralOption;

        let config = get_pool("sigusd").unwrap();
        let collateral_config = CollateralOption {
            token_id: "native".to_string(),
            token_name: "ERG".to_string(),
            liquidation_threshold: 1250,
            liquidation_penalty: 500,
            dex_nft: Some(
                "9916d75132593c8b07fe18bd8d583bda1652eed7565cf41a4738ddd90fc992ec".to_string(),
            ),
        };
        let current_height = 1_000_000;

        let req = BorrowRequest {
            pool_id: "sigusd".to_string(),
            collateral_token: "native".to_string(),
            collateral_amount: 10_000_000_000, // 10 ERG as collateral
            borrow_amount: 10_000,             // 100 SigUSD (2 decimals)
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![sample_utxo("h".repeat(64).as_str(), 15_000_000_000, vec![])],
        };

        let result = build_borrow_tx(req, config, &collateral_config, current_height);
        assert!(result.is_ok(), "Expected Ok, got {:?}", result.err());

        let response = result.unwrap();
        assert_eq!(response.summary.action, "borrow");
        assert_eq!(response.summary.pool_id, "sigusd");

        // Verify the unsigned tx parses as valid JSON
        let tx: serde_json::Value =
            serde_json::from_str(&response.unsigned_tx).expect("Valid JSON");
        let outputs = tx["outputs"].as_array().unwrap();
        // Should have proxy box + fee + change = 3 outputs
        assert!(outputs.len() >= 2, "Should have at least proxy + fee");

        // Proxy box value should include collateral + processing overhead
        let proxy_value: i64 = outputs[0]["value"].as_str().unwrap().parse().unwrap();
        assert!(
            proxy_value > 10_000_000_000,
            "Proxy should hold collateral + overhead"
        );
    }

    #[test]
    fn test_build_borrow_tx_missing_dex_nft() {
        use crate::constants::get_pool;
        use crate::state::CollateralOption;

        let config = get_pool("sigusd").unwrap();
        let collateral_config = CollateralOption {
            token_id: "native".to_string(),
            token_name: "ERG".to_string(),
            liquidation_threshold: 1250,
            liquidation_penalty: 500,
            dex_nft: None, // Missing DEX NFT should cause error
        };
        let current_height = 1_000_000;

        let req = BorrowRequest {
            pool_id: "sigusd".to_string(),
            collateral_token: "native".to_string(),
            collateral_amount: 10_000_000_000,
            borrow_amount: 10_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![sample_utxo("h".repeat(64).as_str(), 15_000_000_000, vec![])],
        };

        let result = build_borrow_tx(req, config, &collateral_config, current_height);
        assert!(result.is_err());
        match result {
            Err(BuildError::TxBuildError(msg)) => {
                assert!(msg.contains("DEX NFT"));
            }
            _ => panic!("Expected TxBuildError about missing DEX NFT"),
        }
    }

    #[test]
    fn test_build_borrow_tx_insufficient_collateral() {
        use crate::constants::get_pool;
        use crate::state::CollateralOption;

        let config = get_pool("sigusd").unwrap();
        let collateral_config = CollateralOption {
            token_id: "native".to_string(),
            token_name: "ERG".to_string(),
            liquidation_threshold: 1250,
            liquidation_penalty: 500,
            dex_nft: Some(
                "9916d75132593c8b07fe18bd8d583bda1652eed7565cf41a4738ddd90fc992ec".to_string(),
            ),
        };
        let current_height = 1_000_000;

        // User wants 10 ERG collateral but only has 1 ERG
        let req = BorrowRequest {
            pool_id: "sigusd".to_string(),
            collateral_token: "native".to_string(),
            collateral_amount: 10_000_000_000,
            borrow_amount: 10_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![sample_utxo("h".repeat(64).as_str(), 1_000_000_000, vec![])],
        };

        let result = build_borrow_tx(req, config, &collateral_config, current_height);
        assert!(result.is_err());
        match result {
            Err(BuildError::InsufficientBalance { .. }) => {}
            _ => panic!("Expected InsufficientBalance error"),
        }
    }

    // Helper to create a sample proxy box for refund tests
    fn sample_proxy_box(
        box_id: &str,
        value: i64,
        assets: Vec<(String, i64)>,
        refund_height: i64,
    ) -> ProxyBoxData {
        // Use the ErgoTree for TEST_ADDRESS
        let user_ergo_tree =
            "0008cd0327e65711a59378c59359c3571c6b49a4c25d28e5583b8fa2c99a7b4b5de5a34f".to_string();

        ProxyBoxData {
            box_id: box_id.to_string(),
            tx_id: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
                .to_string(),
            index: 0,
            value,
            ergo_tree: "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304".to_string(), // Dummy proxy contract
            assets,
            creation_height: 1000000,
            r4_user_tree: user_ergo_tree,
            r6_refund_height: refund_height,
            additional_registers: HashMap::new(), // Test helper — real boxes have registers
        }
    }

    #[test]
    fn test_build_refund_tx_success() {
        let current_height = 1_001_000; // Well past refund height
        let refund_height = 1_000_720; // Was set 720 blocks ago

        // Proxy box with 10 ERG (plenty for fee)
        let proxy_box = sample_proxy_box(
            &"a".repeat(64),
            10_000_000_000, // 10 ERG
            vec![],
            refund_height,
        );

        let result = build_refund_tx(proxy_box.clone(), current_height);
        assert!(result.is_ok(), "build_refund_tx failed: {:?}", result.err());

        let response = result.unwrap();
        assert_eq!(response.fee_nano, TX_FEE_NANO);
        assert_eq!(response.refundable_after_height, refund_height);

        // Verify the transaction parses as valid JSON
        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        assert!(tx["inputs"].is_array());
        assert!(tx["outputs"].is_array());

        // Should have 3 outputs: primary refund, dummy min box, fee
        let outputs = tx["outputs"].as_array().unwrap();
        assert_eq!(outputs.len(), 3);

        // Verify primary refund output (output 0)
        let primary_output = &outputs[0];
        let expected_primary_value = proxy_box.value - TX_FEE_NANO - MIN_BOX_VALUE_NANO;
        assert_eq!(
            primary_output["value"].as_str().unwrap(),
            expected_primary_value.to_string()
        );

        // Verify R4 contains proxy box ID
        let r4 = primary_output["additionalRegisters"]["R4"].as_str().unwrap();
        assert!(r4.starts_with("0e20")); // Coll[Byte] prefix for 32 bytes
        assert!(r4.contains(&"a".repeat(64))); // Contains box ID

        // Verify dummy output (output 1) — min box to user
        let dummy_output = &outputs[1];
        assert_eq!(
            dummy_output["value"].as_str().unwrap(),
            MIN_BOX_VALUE_NANO.to_string()
        );

        // Verify fee output (output 2)
        let fee_output = &outputs[2];
        assert_eq!(
            fee_output["value"].as_str().unwrap(),
            TX_FEE_NANO.to_string()
        );
        assert_eq!(
            fee_output["ergoTree"].as_str().unwrap(),
            MINER_FEE_ERGO_TREE
        );
    }

    #[test]
    fn test_build_refund_tx_with_tokens() {
        let current_height = 1_001_000;
        let refund_height = 1_000_720;

        // Proxy box with ERG and tokens
        let token_id = "b".repeat(64);
        let proxy_box = sample_proxy_box(
            &"a".repeat(64),
            5_000_000_000, // 5 ERG
            vec![(token_id.clone(), 1000)],
            refund_height,
        );

        let result = build_refund_tx(proxy_box, current_height);
        assert!(
            result.is_ok(),
            "build_refund_tx with tokens failed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();

        // Verify refund output has the tokens
        let refund_output = &tx["outputs"][0];
        let assets = refund_output["assets"].as_array().unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0]["tokenId"].as_str().unwrap(), token_id);
        assert_eq!(assets[0]["amount"].as_str().unwrap(), "1000");
    }

    #[test]
    fn test_build_refund_tx_before_height_still_works() {
        // proveDlog(userPk) spending path has no height check,
        // so refund should succeed even before refund height
        let current_height = 1_000_000; // Before refund height
        let refund_height = 1_000_720;

        let proxy_box = sample_proxy_box(&"a".repeat(64), 10_000_000_000, vec![], refund_height);

        let result = build_refund_tx(proxy_box, current_height);
        assert!(result.is_ok(), "Refund should work before height via proveDlog: {:?}", result.err());
    }

    #[test]
    fn test_build_refund_tx_exactly_at_refund_height() {
        // Edge case: current_height == refund_height should succeed
        let current_height = 1_000_720;
        let refund_height = 1_000_720;

        let proxy_box = sample_proxy_box(&"a".repeat(64), 10_000_000_000, vec![], refund_height);

        let result = build_refund_tx(proxy_box, current_height);
        assert!(result.is_ok(), "Refund at exact height should succeed");
    }

    #[test]
    fn test_build_refund_tx_insufficient_value() {
        let current_height = 1_001_000;
        let refund_height = 1_000_720;

        // Proxy box with too little ERG (less than MIN_BOX_VALUE * 2 + TX_FEE)
        // Need at least 3_000_000 nanoERG for 3 outputs
        let proxy_box = sample_proxy_box(
            &"a".repeat(64),
            2_500_000, // 0.0025 ERG - not enough for 3 outputs
            vec![],
            refund_height,
        );

        let result = build_refund_tx(proxy_box, current_height);
        assert!(result.is_err());

        match result {
            Err(BuildError::InsufficientBalance {
                required,
                available,
            }) => {
                assert_eq!(required, MIN_BOX_VALUE_NANO * 2 + TX_FEE_NANO);
                assert_eq!(available, 2_500_000);
            }
            _ => panic!("Expected InsufficientBalance error"),
        }
    }

    #[test]
    fn test_build_refund_tx_minimum_viable_value() {
        let current_height = 1_001_000;
        let refund_height = 1_000_720;

        // Proxy box with exactly minimum required value for 3 outputs
        let min_required = MIN_BOX_VALUE_NANO * 2 + TX_FEE_NANO; // 3_000_000
        let proxy_box = sample_proxy_box(&"a".repeat(64), min_required, vec![], refund_height);

        let result = build_refund_tx(proxy_box, current_height);
        assert!(result.is_ok(), "Minimum viable value should succeed");

        let response = result.unwrap();
        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        let outputs = tx["outputs"].as_array().unwrap();
        assert_eq!(outputs.len(), 3);
        // Primary output gets MIN_BOX_VALUE_NANO (3M - 1M fee - 1M dummy)
        assert_eq!(
            outputs[0]["value"].as_str().unwrap(),
            MIN_BOX_VALUE_NANO.to_string()
        );
        // Dummy output gets MIN_BOX_VALUE_NANO
        assert_eq!(
            outputs[1]["value"].as_str().unwrap(),
            MIN_BOX_VALUE_NANO.to_string()
        );
    }

    #[test]
    fn test_proxy_box_data_struct() {
        let proxy_box = ProxyBoxData {
            box_id: "a".repeat(64),
            tx_id: "b".repeat(64),
            index: 0,
            value: 10_000_000_000,
            ergo_tree: "0008cd...".to_string(),
            assets: vec![("token1".to_string(), 100)],
            creation_height: 1000000,
            r4_user_tree: "0008cd...".to_string(),
            r6_refund_height: 1000720,
            additional_registers: HashMap::new(),
        };

        assert_eq!(proxy_box.value, 10_000_000_000);
        assert_eq!(proxy_box.assets.len(), 1);
        assert_eq!(proxy_box.r6_refund_height, 1000720);
    }

    #[test]
    fn test_refund_response_struct() {
        let response = RefundResponse {
            unsigned_tx: "{}".to_string(),
            fee_nano: TX_FEE_NANO,
            refundable_after_height: 1000720,
        };

        assert_eq!(response.fee_nano, TX_FEE_NANO);
        assert_eq!(response.refundable_after_height, 1000720);
    }
}
