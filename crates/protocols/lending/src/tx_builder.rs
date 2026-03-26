use std::collections::HashMap;

use crate::constants::PoolConfig;
use ergo_tx::{
    append_change_output,
    sigma::{encode_sigma_coll_byte, encode_sigma_long},
    Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

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
    InsufficientBalance { required: i64, available: i64 },
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

#[derive(Debug, Clone)]
pub struct SelectedInputs {
    pub boxes: Vec<UserUtxo>,
    pub total_erg: i64,
    pub token_amount: i64,
}

/// Selects largest-first until ERG requirement is met.
pub fn select_erg_inputs(
    utxos: &[UserUtxo],
    required_erg: i64,
) -> Result<SelectedInputs, BuildError> {
    let mut selected = Vec::new();
    let mut total = 0i64;

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

/// Selects boxes with the required token first, then adds more for ERG if needed.
pub fn select_token_inputs(
    utxos: &[UserUtxo],
    token_id: &str,
    required_amount: i64,
    min_erg: i64,
) -> Result<SelectedInputs, BuildError> {
    let mut selected = Vec::new();
    let mut total_erg = 0i64;
    let mut total_tokens = 0i64;

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

    if total_tokens < required_amount {
        return Err(BuildError::InsufficientTokens {
            token: token_id.to_string(),
            required: required_amount,
            available: total_tokens,
        });
    }

    if total_erg < min_erg {
        for utxo in utxos {
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

fn to_ergo_tx_selected(
    local: &SelectedInputs,
    eip12_boxes: Vec<Eip12InputBox>,
) -> ergo_tx::SelectedInputs {
    ergo_tx::SelectedInputs {
        boxes: eip12_boxes,
        total_erg: local.total_erg as u64,
        token_amount: local.token_amount as u64,
    }
}

fn resolve_user_ergo_tree(address: &str) -> Result<(String, Vec<u8>), BuildError> {
    let tree = ergo_tx::address::address_to_ergo_tree(address)
        .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;
    let bytes = hex::decode(&tree)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid user ErgoTree: {}", e)))?;
    Ok((tree, bytes))
}

fn miner_fee_output(current_height: i32) -> Eip12Output {
    Eip12Output {
        value: TX_FEE_NANO.to_string(),
        ergo_tree: MINER_FEE_ERGO_TREE.to_string(),
        assets: vec![],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    }
}

fn finalize_proxy_tx(
    eip12_inputs: Vec<Eip12InputBox>,
    outputs: Vec<Eip12Output>,
    proxy_ergo_tree: &str,
) -> Result<(String, String), BuildError> {
    let unsigned_tx = Eip12UnsignedTx {
        inputs: eip12_inputs,
        data_inputs: vec![],
        outputs,
    };
    let json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;
    let addr = ergo_tx::address::ergo_tree_to_address(proxy_ergo_tree)
        .map_err(|e| BuildError::TxBuildError(e.to_string()))?;
    Ok((json, addr))
}

/// The bot deducts a service fee from whatever tokens are in the proxy box, so we must
/// include amount + service_fee (+ optional slippage buffer) in the proxy box.
///
/// Proxy registers: R4=user ErgoTree, R5=min LP tokens, R6=refund height, R7=lend token ID (token pools)
pub fn build_lend_tx(
    req: LendRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    use crate::calculator;

    if req.amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Amount must be greater than 0".to_string(),
        ));
    }
    if req.amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "Amount exceeds maximum supported value".to_string(),
        ));
    }

    let slippage_bps = req.slippage_bps.min(200);

    let service_fee = calculator::calculate_service_fee(req.amount, config.is_erg_pool);
    // Min fee: 1 token unit for token pools, MIN_BOX_VALUE_NANO for ERG pools
    let service_fee = if config.is_erg_pool {
        service_fee.max(MIN_BOX_VALUE_NANO as u64)
    } else {
        service_fee.max(1)
    };

    let slippage_buffer = req.amount * slippage_bps as u64 / 10000;

    let total_to_send = req
        .amount
        .checked_add(service_fee)
        .and_then(|v| v.checked_add(slippage_buffer))
        .ok_or_else(|| {
            BuildError::TxBuildError("Amount overflow in total_to_send calculation".to_string())
        })?;

    let (user_ergo_tree, user_ergo_tree_bytes) = resolve_user_ergo_tree(&req.user_address)?;

    let proxy_ergo_tree = ergo_tx::address::address_to_ergo_tree(config.proxy_contracts.lend_address)
        .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;

    let proxy_value = if config.is_erg_pool {
        (total_to_send as i64) + BOT_PROCESSING_OVERHEAD
    } else {
        BOT_PROCESSING_OVERHEAD
    };

    let total_required = proxy_value + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

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

    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    let mut proxy_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_coll_byte(&user_ergo_tree_bytes),
        "R5" => encode_sigma_long(req.min_lp_tokens.unwrap_or(0) as i64),
        "R6" => encode_sigma_long(refund_height as i64),
    );

    if !config.is_erg_pool {
        let lend_token_bytes = hex::decode(config.lend_token_id)
            .map_err(|e| BuildError::TxBuildError(format!("Invalid lend token ID: {}", e)))?;
        proxy_registers.insert("R7".to_string(), encode_sigma_coll_byte(&lend_token_bytes));
    }

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

    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: proxy_assets,
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    let mut outputs = vec![proxy_output, miner_fee_output(current_height)];
    let erg_used = (proxy_value + TX_FEE_NANO) as u64;
    let selected = to_ergo_tx_selected(&inputs, eip12_inputs.clone());
    let spent_tokens: Vec<(&str, u64)> = if !config.is_erg_pool {
        if let Some(currency_id) = config.currency_id {
            vec![(currency_id, total_to_send)]
        } else {
            vec![]
        }
    } else {
        vec![]
    };
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &spent_tokens,
        &user_ergo_tree,
        current_height,
        MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| BuildError::TxBuildError(e.to_string()))?;

    let (unsigned_tx_json, proxy_address) = finalize_proxy_tx(eip12_inputs, outputs, &proxy_ergo_tree)?;

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

/// Proxy registers: R4=user ErgoTree, R5=min output, R6=refund height, R7=currency ID (token pools)
pub fn build_withdraw_tx(
    req: WithdrawRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    if req.lp_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "LP amount must be greater than 0".to_string(),
        ));
    }
    if req.lp_amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "LP amount exceeds maximum supported value".to_string(),
        ));
    }

    let (user_ergo_tree, user_ergo_tree_bytes) = resolve_user_ergo_tree(&req.user_address)?;
    let proxy_ergo_tree = ergo_tx::address::address_to_ergo_tree(config.proxy_contracts.withdraw_address)
        .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;

    let proxy_value = MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO;
    let total_required = proxy_value + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

    let inputs = select_token_inputs(
        &req.user_utxos,
        config.lend_token_id,
        req.lp_amount as i64,
        total_required,
    )?;

    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    let mut proxy_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_coll_byte(&user_ergo_tree_bytes),
        "R5" => encode_sigma_long(req.min_output.unwrap_or(0) as i64),
        "R6" => encode_sigma_long(refund_height as i64),
    );

    if !config.is_erg_pool {
        if let Some(currency_id) = config.currency_id {
            let currency_bytes = hex::decode(currency_id)
                .map_err(|e| BuildError::TxBuildError(format!("Invalid currency ID: {}", e)))?;
            proxy_registers.insert("R7".to_string(), encode_sigma_coll_byte(&currency_bytes));
        }
    }

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

    let mut outputs = vec![proxy_output, miner_fee_output(current_height)];
    let erg_used = (proxy_value + TX_FEE_NANO) as u64;
    let selected = to_ergo_tx_selected(&inputs, eip12_inputs.clone());
    let spent_tokens: Vec<(&str, u64)> = vec![(config.lend_token_id, req.lp_amount)];
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &spent_tokens,
        &user_ergo_tree,
        current_height,
        MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| BuildError::TxBuildError(e.to_string()))?;

    let (unsigned_tx_json, proxy_address) = finalize_proxy_tx(eip12_inputs, outputs, &proxy_ergo_tree)?;

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

/// Proxy registers: R4=neededAmount(0), R5=borrower ErgoTree, R6=refundHeight(Int), R7=collateralBoxId
pub fn build_repay_tx(
    req: RepayRequest,
    config: &PoolConfig,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
    if req.repay_amount == 0 {
        return Err(BuildError::InvalidAmount(
            "Repay amount must be greater than 0".to_string(),
        ));
    }
    if req.repay_amount > i64::MAX as u64 {
        return Err(BuildError::InvalidAmount(
            "Repay amount exceeds maximum supported value".to_string(),
        ));
    }
    if req.collateral_box_id.len() != 64 {
        return Err(BuildError::InvalidAmount(
            "Invalid collateral box ID: must be 64 hex characters".to_string(),
        ));
    }

    let collateral_box_bytes = hex::decode(&req.collateral_box_id)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid collateral box ID hex: {}", e)))?;

    let (user_ergo_tree, user_ergo_tree_bytes) = resolve_user_ergo_tree(&req.user_address)?;

    let is_full_repay = req.repay_amount >= req.total_owed || req.total_owed == 0;
    let proxy_address = if is_full_repay {
        config.proxy_contracts.repay_address
    } else {
        if config.proxy_contracts.partial_repay_address.is_empty() {
            return Err(BuildError::ProxyContractMissing(format!(
                "Partial repay proxy not configured for pool: {}. \
                 Please repay the full amount ({}) or wait for partial repay support.",
                config.id, req.total_owed
            )));
        }
        config.proxy_contracts.partial_repay_address
    };
    let proxy_ergo_tree = ergo_tx::address::address_to_ergo_tree(proxy_address)
        .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;

    let proxy_value = if config.is_erg_pool {
        (req.repay_amount as i64) + BOT_PROCESSING_OVERHEAD
    } else {
        BOT_PROCESSING_OVERHEAD
    };

    let total_required = proxy_value + TX_FEE_NANO + MIN_BOX_VALUE_NANO;

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

    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    let mut proxy_registers = ergo_tx::sigma_registers!("R4" => encode_sigma_long(0));
    proxy_registers.insert(
        "R5".to_string(),
        encode_sigma_coll_byte(&user_ergo_tree_bytes),
    );
    // R6 must be SInt not SLong — contract reads SELF.R6[Int].get
    proxy_registers.insert(
        "R6".to_string(),
        ergo_tx::sigma::encode_sigma_int(refund_height),
    );
    proxy_registers.insert(
        "R7".to_string(),
        encode_sigma_coll_byte(&collateral_box_bytes),
    );

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

    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: proxy_assets,
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    let mut outputs = vec![proxy_output, miner_fee_output(current_height)];
    let erg_used = (proxy_value + TX_FEE_NANO) as u64;
    let selected = to_ergo_tx_selected(&inputs, eip12_inputs.clone());
    let spent_tokens: Vec<(&str, u64)> = if !config.is_erg_pool {
        if let Some(currency_id) = config.currency_id {
            vec![(currency_id, req.repay_amount)]
        } else {
            vec![]
        }
    } else {
        vec![]
    };
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &spent_tokens,
        &user_ergo_tree,
        current_height,
        MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| BuildError::TxBuildError(e.to_string()))?;

    let (unsigned_tx_json, proxy_address) = finalize_proxy_tx(eip12_inputs, outputs, &proxy_ergo_tree)?;
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

/// ERG pool: token collateral in proxy box, borrows ERG.
/// Token pool: ERG collateral in proxy value, borrows tokens.
///
/// Proxy registers: R4=user ErgoTree, R5=requestAmount, R6=refundHeight(Int),
/// R7=(threshold,penalty), R8=dexNft, R9=userPk(GroupElement)
pub fn build_borrow_tx(
    req: BorrowRequest,
    config: &PoolConfig,
    collateral_config: &crate::state::CollateralOption,
    current_height: i32,
) -> Result<BuildResponse, BuildError> {
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

    if config.proxy_contracts.borrow_address.is_empty() {
        return Err(BuildError::ProxyContractMissing(config.id.to_string()));
    }

    let (user_ergo_tree, user_ergo_tree_bytes) = resolve_user_ergo_tree(&req.user_address)?;

    let user_pk = ergo_tx::sigma::extract_pk_from_p2pk_ergo_tree(&user_ergo_tree)
        .map_err(|e| {
            BuildError::InvalidAddress(format!(
                "Address must be a P2PK address (not a script): {}",
                e
            ))
        })?;

    let proxy_ergo_tree = ergo_tx::address::address_to_ergo_tree(config.proxy_contracts.borrow_address)
        .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;

    let (proxy_value, inputs) = if config.is_erg_pool {
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
        let proxy_val = (req.collateral_amount as i64) + MIN_BOX_VALUE_NANO + PROXY_EXECUTION_FEE_NANO;
        let total_required = proxy_val + TX_FEE_NANO + MIN_BOX_VALUE_NANO;
        let selected = select_erg_inputs(&req.user_utxos, total_required)?;
        (proxy_val, selected)
    };

    let eip12_inputs: Vec<Eip12InputBox> = inputs.boxes.iter().map(user_utxo_to_eip12).collect();
    let refund_height = current_height + REFUND_HEIGHT_OFFSET;

    let mut proxy_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_coll_byte(&user_ergo_tree_bytes),
        "R5" => encode_sigma_long(req.borrow_amount as i64),
        "R6" => ergo_tx::sigma::encode_sigma_int(refund_height),
        "R7" => ergo_tx::sigma::encode_sigma_long_pair(
            collateral_config.liquidation_threshold as i64,
            collateral_config.liquidation_penalty as i64,
        ),
    );

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
        encode_sigma_coll_byte(&dex_nft_bytes),
    );

    proxy_registers.insert(
        "R9".to_string(),
        ergo_tx::sigma::encode_sigma_group_element(&user_pk),
    );

    let mut proxy_assets = Vec::new();
    if config.is_erg_pool {
        proxy_assets.push(Eip12Asset {
            token_id: req.collateral_token.clone(),
            amount: req.collateral_amount.to_string(),
        });
    }

    let proxy_output = Eip12Output {
        value: proxy_value.to_string(),
        ergo_tree: proxy_ergo_tree.clone(),
        assets: proxy_assets,
        creation_height: current_height,
        additional_registers: proxy_registers,
    };

    let mut outputs = vec![proxy_output, miner_fee_output(current_height)];
    let erg_used = (proxy_value + TX_FEE_NANO) as u64;
    let selected = to_ergo_tx_selected(&inputs, eip12_inputs.clone());
    let spent_tokens: Vec<(&str, u64)> = if config.is_erg_pool {
        vec![(&req.collateral_token, req.collateral_amount)]
    } else {
        vec![]
    };
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &spent_tokens,
        &user_ergo_tree,
        current_height,
        MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| BuildError::TxBuildError(e.to_string()))?;

    let (unsigned_tx_json, proxy_address) = finalize_proxy_tx(eip12_inputs, outputs, &proxy_ergo_tree)?;
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

/// Lend/Withdraw/Borrow proxies: `proveDlog(userPk)` — 2 outputs, user spends anytime.
///
/// Repay proxies (NO proveDlog!): `operationPath` — 3 outputs triggers operation path
/// which checks OUTPUTS(0).propositionBytes == R5, value >= R4(=0), R4 == SELF.id.
/// This avoids the `refundPath` which requires R6[Int] (fails on old Long-encoded R6).
pub fn build_refund_tx(
    proxy_box: ProxyBoxData,
    current_height: i32,
) -> Result<RefundResponse, BuildError> {
    let use_three_outputs = proxy_box.is_repay_proxy;

    let min_required = if use_three_outputs {
        MIN_BOX_VALUE_NANO * 2 + TX_FEE_NANO
    } else {
        MIN_BOX_VALUE_NANO + TX_FEE_NANO
    };
    if proxy_box.value < min_required {
        return Err(BuildError::InsufficientBalance {
            required: min_required,
            available: proxy_box.value,
        });
    }

    let primary_value = if use_three_outputs {
        proxy_box.value - TX_FEE_NANO - MIN_BOX_VALUE_NANO
    } else {
        proxy_box.value - TX_FEE_NANO
    };

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

    // R4 = proxy box ID (required by operation path contract check)
    let refund_registers = ergo_tx::sigma_registers!("R4" => format!("0e20{}", proxy_box.box_id));

    let primary_output = Eip12Output {
        value: primary_value.to_string(),
        ergo_tree: proxy_box.user_ergo_tree.clone(),
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

    let fee_output = miner_fee_output(current_height);

    let outputs = if use_three_outputs {
        let dummy_output = Eip12Output {
            value: MIN_BOX_VALUE_NANO.to_string(),
            ergo_tree: proxy_box.user_ergo_tree.clone(),
            assets: vec![],
            creation_height: current_height,
            additional_registers: HashMap::new(),
        };
        vec![primary_output, dummy_output, fee_output]
    } else {
        vec![primary_output, fee_output]
    };

    let unsigned_tx = Eip12UnsignedTx {
        inputs: vec![input],
        data_inputs: vec![],
        outputs,
    };

    let unsigned_tx_json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;

    Ok(RefundResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        refundable_after_height: proxy_box.r6_refund_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let amount: u64 = 10_000_000_000;

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

        // 10 ERG / 160 = 62_500_000 nanoERG (above MIN_BOX_VALUE_NANO minimum)
        assert_eq!(response.summary.service_fee_raw, 62_500_000);
        assert_eq!(response.summary.total_to_send_raw, amount + 62_500_000); // no slippage

        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        assert!(tx["inputs"].is_array());
        assert!(tx["outputs"].is_array());

        assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

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

        let lp_token_id = config.lend_token_id.to_string();
        let lp_amount: u64 = 1000;

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

        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        assert!(tx["inputs"].is_array());
        assert!(tx["outputs"].is_array());

        assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

        let proxy_output = &tx["outputs"][0];
        assert!(!proxy_output["assets"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_build_withdraw_tx_insufficient_lp() {
        use crate::constants::get_pool;

        let config = get_pool("erg").unwrap();
        let current_height = 1_000_000;

        let lp_token_id = config.lend_token_id.to_string();

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

        let repay_amount: u64 = 5_000_000_000;
        let collateral_box_id = "a".repeat(64);

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

        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        assert!(tx["inputs"].is_array());
        assert!(tx["outputs"].is_array());

        assert_eq!(tx["outputs"].as_array().unwrap().len(), 3);

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

        let tx: serde_json::Value =
            serde_json::from_str(&response.unsigned_tx).expect("Valid JSON");
        let outputs = tx["outputs"].as_array().unwrap();
        assert!(outputs.len() >= 2);

        let proxy_value: i64 = outputs[0]["value"].as_str().unwrap().parse().unwrap();
        assert!(proxy_value > 10_000_000_000);
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

        let req = BorrowRequest {
            pool_id: "sigusd".to_string(),
            collateral_token: "native".to_string(),
            collateral_amount: 10_000_000_000,
            borrow_amount: 10_000,
            user_address: TEST_ADDRESS.to_string(),
            user_utxos: vec![sample_utxo("h".repeat(64).as_str(), 1_000_000_000, vec![])],
        };

        let result = build_borrow_tx(req, config, &collateral_config, current_height);
        assert!(matches!(result, Err(BuildError::InsufficientBalance { .. })));
    }

    fn sample_proxy_box(
        box_id: &str,
        value: i64,
        assets: Vec<(String, i64)>,
        refund_height: i64,
    ) -> ProxyBoxData {
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
            user_ergo_tree,
            r6_refund_height: refund_height,
            is_repay_proxy: false,
            additional_registers: HashMap::new(),
        }
    }

    #[test]
    fn test_build_refund_tx_success() {
        let current_height = 1_001_000; // Well past refund height
        let refund_height = 1_000_720; // Was set 720 blocks ago

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

        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        assert!(tx["inputs"].is_array());
        assert!(tx["outputs"].is_array());

        let outputs = tx["outputs"].as_array().unwrap();
        assert_eq!(outputs.len(), 2);

        let primary_output = &outputs[0];
        let expected_primary_value = proxy_box.value - TX_FEE_NANO;
        assert_eq!(
            primary_output["value"].as_str().unwrap(),
            expected_primary_value.to_string()
        );

        let r4 = primary_output["additionalRegisters"]["R4"].as_str().unwrap();
        assert!(r4.starts_with("0e20")); // Coll[Byte] prefix for 32 bytes
        assert!(r4.contains(&"a".repeat(64))); // Contains box ID

        let fee_output = &outputs[1];
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

        let proxy_box = sample_proxy_box(
            &"a".repeat(64),
            1_500_000, // Not enough for MIN_BOX_VALUE + TX_FEE = 2_000_000
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
                assert_eq!(required, MIN_BOX_VALUE_NANO + TX_FEE_NANO);
                assert_eq!(available, 1_500_000);
            }
            _ => panic!("Expected InsufficientBalance error"),
        }
    }

    #[test]
    fn test_build_refund_tx_minimum_viable_value() {
        let current_height = 1_001_000;
        let refund_height = 1_000_720;

        let min_required = MIN_BOX_VALUE_NANO + TX_FEE_NANO;
        let proxy_box = sample_proxy_box(&"a".repeat(64), min_required, vec![], refund_height);

        let result = build_refund_tx(proxy_box, current_height);
        assert!(result.is_ok(), "Minimum viable value should succeed");

        let response = result.unwrap();
        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        let outputs = tx["outputs"].as_array().unwrap();
        assert_eq!(outputs.len(), 2);
        // Primary output gets MIN_BOX_VALUE_NANO (2M - 1M fee)
        assert_eq!(
            outputs[0]["value"].as_str().unwrap(),
            MIN_BOX_VALUE_NANO.to_string()
        );
    }

    #[test]
    fn test_build_refund_tx_repay_proxy_three_outputs() {
        let current_height = 1_001_000;
        let refund_height = 1_000_720;

        let mut proxy_box = sample_proxy_box(
            &"a".repeat(64),
            10_000_000_000,
            vec![],
            refund_height,
        );
        proxy_box.is_repay_proxy = true;

        let result = build_refund_tx(proxy_box, current_height);
        assert!(result.is_ok(), "Repay proxy refund should succeed: {:?}", result.err());

        let response = result.unwrap();
        let tx: serde_json::Value = serde_json::from_str(&response.unsigned_tx).unwrap();
        let outputs = tx["outputs"].as_array().unwrap();
        assert_eq!(outputs.len(), 3);
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
            user_ergo_tree: "0008cd...".to_string(),
            r6_refund_height: 1000720,
            is_repay_proxy: false,
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
