//! SigmaFi transaction builders (EIP-12 format)
//!
//! Five transaction types:
//! 1. Open Order - Borrower creates a collateralized loan request
//! 2. Cancel Order - Borrower withdraws an unfilled order
//! 3. Close Order - Lender fills an order, creating a bond
//! 4. Repay - Borrower repays loan before maturity
//! 5. Liquidate - Lender claims collateral after maturity

use std::collections::HashMap;

use ergo_tx::eip12::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};
use ergo_tx::sigma::{encode_sigma_coll_byte, encode_sigma_int, encode_sigma_long};
use ergo_tx::{collect_change_tokens, select_erg_boxes, select_token_boxes};

use crate::calculator;
use crate::constants::{self, OrderType, SAFE_MIN_BOX_VALUE, STORAGE_PERIOD};

/// Standard miner fee (1.1 mERG)
const MINER_FEE: i64 = 1_100_000;

/// Minimum box value for change outputs
const MIN_CHANGE_VALUE: i64 = 1_000_000;

/// Encode a SigmaProp(ProveDlog) register value from a P2PK ErgoTree hex.
///
/// P2PK ErgoTree: "0008cd" + 33-byte-compressed-pubkey
/// SigmaProp register: "08cd" + 33-byte-compressed-pubkey
fn encode_sigma_prop_from_ergo_tree(p2pk_ergo_tree: &str) -> Result<String, SigmaFiTxError> {
    // P2PK ErgoTree starts with "0008cd" followed by 66 hex chars (33 bytes pubkey)
    if p2pk_ergo_tree.len() >= 72 && p2pk_ergo_tree.starts_with("0008cd") {
        // Register encoding: "08cd" + pubkey (skip "00" prefix from ErgoTree)
        Ok(format!("08cd{}", &p2pk_ergo_tree[6..72]))
    } else {
        Err(SigmaFiTxError::InvalidAddress(
            "Expected P2PK ErgoTree (0008cd + pubkey)".to_string(),
        ))
    }
}

#[derive(Debug)]
pub enum SigmaFiTxError {
    InvalidAddress(String),
    InvalidAmount(String),
    InvalidMaturity(String),
    BoxSelection(String),
    InsufficientFunds(String),
}

impl std::fmt::Display for SigmaFiTxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAddress(msg) => write!(f, "Invalid address: {}", msg),
            Self::InvalidAmount(msg) => write!(f, "Invalid amount: {}", msg),
            Self::InvalidMaturity(msg) => write!(f, "Invalid maturity: {}", msg),
            Self::BoxSelection(msg) => write!(f, "Box selection failed: {}", msg),
            Self::InsufficientFunds(msg) => write!(f, "Insufficient funds: {}", msg),
        }
    }
}

impl std::error::Error for SigmaFiTxError {}

// =============================================================================
// Tx 1: Open Order
// =============================================================================

pub struct OpenOrderRequest {
    /// Borrower's P2PK ErgoTree hex
    pub borrower_ergo_tree: String,
    /// Loan token ID ("ERG" for native)
    pub loan_token_id: String,
    /// Principal amount in raw units
    pub principal: u64,
    /// Total repayment amount in raw units
    pub repayment: u64,
    /// Maturity duration in blocks
    pub maturity_blocks: i32,
    /// Collateral ERG in nanoERG (0 if token-only with SAFE_MIN_BOX_VALUE)
    pub collateral_erg: u64,
    /// Collateral tokens: [(token_id, amount)]
    pub collateral_tokens: Vec<(String, u64)>,
    /// User's available UTXOs
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current blockchain height
    pub current_height: i32,
}

pub fn build_open_order(req: &OpenOrderRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    // Validate
    if req.principal == 0 {
        return Err(SigmaFiTxError::InvalidAmount(
            "Principal must be positive".to_string(),
        ));
    }
    if req.repayment <= req.principal {
        return Err(SigmaFiTxError::InvalidAmount(
            "Repayment must exceed principal".to_string(),
        ));
    }
    if req.maturity_blocks < constants::MIN_MATURITY_BLOCKS {
        return Err(SigmaFiTxError::InvalidMaturity(format!(
            "Maturity must be at least {} blocks",
            constants::MIN_MATURITY_BLOCKS
        )));
    }
    if req.maturity_blocks >= STORAGE_PERIOD {
        return Err(SigmaFiTxError::InvalidMaturity(format!(
            "Maturity must be less than {} blocks (storage rent period)",
            STORAGE_PERIOD
        )));
    }

    let order_ergo_tree = constants::build_order_contract(&req.loan_token_id, OrderType::OnClose);

    // Register values
    let r4 = encode_sigma_prop_from_ergo_tree(&req.borrower_ergo_tree)?;
    let r5 = encode_sigma_long(req.principal as i64);
    let r6 = encode_sigma_long(req.repayment as i64);
    let r7 = encode_sigma_int(req.maturity_blocks);

    let mut registers = HashMap::new();
    registers.insert("R4".to_string(), r4);
    registers.insert("R5".to_string(), r5);
    registers.insert("R6".to_string(), r6);
    registers.insert("R7".to_string(), r7);

    // Order box value: collateral ERG or SAFE_MIN_BOX_VALUE
    let order_value = if req.collateral_erg > 0 {
        req.collateral_erg as i64
    } else {
        SAFE_MIN_BOX_VALUE
    };

    let order_assets: Vec<Eip12Asset> = req
        .collateral_tokens
        .iter()
        .map(|(tid, amt)| Eip12Asset::new(tid, *amt as i64))
        .collect();

    let order_output = Eip12Output {
        value: order_value.to_string(),
        ergo_tree: order_ergo_tree,
        assets: order_assets,
        creation_height: req.current_height,
        additional_registers: registers,
    };

    // Calculate required ERG
    let required_erg = (order_value + MINER_FEE + MIN_CHANGE_VALUE) as u64;

    // Select inputs
    let selected = if req.collateral_tokens.is_empty() {
        select_erg_boxes(&req.user_inputs, required_erg)
            .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?
    } else {
        // For the first collateral token, use select_token_boxes
        let (ref first_token_id, first_amount) = req.collateral_tokens[0];
        select_token_boxes(&req.user_inputs, first_token_id, first_amount, required_erg)
            .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?
    };

    // Change
    let change_erg = selected.total_erg as i64 - order_value - MINER_FEE;
    if change_erg < 0 {
        return Err(SigmaFiTxError::InsufficientFunds(format!(
            "Need {} more nanoERG",
            -change_erg
        )));
    }

    let spent_tokens: Vec<(&str, u64)> = req
        .collateral_tokens
        .iter()
        .map(|(tid, amt)| (tid.as_str(), *amt))
        .collect();
    let change_tokens = if spent_tokens.len() == 1 {
        collect_change_tokens(
            &selected.boxes,
            Some((spent_tokens[0].0, spent_tokens[0].1)),
        )
    } else {
        ergo_tx::collect_multi_change_tokens(&selected.boxes, &spent_tokens)
    };

    let change_output = Eip12Output::change(
        change_erg,
        &req.borrower_ergo_tree,
        change_tokens,
        req.current_height,
    );
    let fee_output = Eip12Output::fee(MINER_FEE, req.current_height);

    Ok(Eip12UnsignedTx {
        inputs: selected.boxes,
        data_inputs: vec![],
        outputs: vec![order_output, change_output, fee_output],
    })
}

// =============================================================================
// Tx 2: Cancel Order
// =============================================================================

pub struct CancelOrderRequest {
    /// The order box to cancel (as EIP-12 input)
    pub order_box: Eip12InputBox,
    /// Borrower's P2PK ErgoTree hex
    pub borrower_ergo_tree: String,
    /// User's available UTXOs (for miner fee)
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current blockchain height
    pub current_height: i32,
}

pub fn build_cancel_order(req: &CancelOrderRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    let order_erg: i64 = req
        .order_box
        .value
        .parse()
        .map_err(|_| SigmaFiTxError::InvalidAmount("Invalid order box value".to_string()))?;

    // Select user inputs for miner fee
    let fee_required = (MINER_FEE + MIN_CHANGE_VALUE) as u64;
    let selected = select_erg_boxes(&req.user_inputs, fee_required)
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?;

    // Return collateral to borrower
    let return_output = Eip12Output {
        value: order_erg.to_string(),
        ergo_tree: req.borrower_ergo_tree.clone(),
        assets: req.order_box.assets.clone(),
        creation_height: req.current_height,
        additional_registers: HashMap::new(),
    };

    // Change from fee inputs
    let change_erg = selected.total_erg as i64 - MINER_FEE;
    let change_tokens = collect_change_tokens(&selected.boxes, None);
    let change_output = Eip12Output::change(
        change_erg,
        &req.borrower_ergo_tree,
        change_tokens,
        req.current_height,
    );
    let fee_output = Eip12Output::fee(MINER_FEE, req.current_height);

    // Order box is first input
    let mut inputs = vec![req.order_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs: vec![return_output, change_output, fee_output],
    })
}

// =============================================================================
// Tx 3: Close Order (Lender fills)
// =============================================================================

pub struct CloseOrderRequest {
    /// The order box to fill (as EIP-12 input)
    pub order_box: Eip12InputBox,
    /// Lender's P2PK ErgoTree hex
    pub lender_ergo_tree: String,
    /// UI implementor's P2PK ErgoTree hex (for UI fee)
    pub ui_fee_ergo_tree: String,
    /// Loan token ID ("ERG" for native)
    pub loan_token_id: String,
    /// Lender's available UTXOs
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current blockchain height
    pub current_height: i32,
}

pub fn build_close_order(req: &CloseOrderRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    let is_erg = req.loan_token_id == "ERG";

    // Parse order registers
    let r5_hex = req
        .order_box
        .additional_registers
        .get("R5")
        .ok_or_else(|| SigmaFiTxError::InvalidAmount("Order missing R5 (principal)".to_string()))?;
    let r6_hex = req
        .order_box
        .additional_registers
        .get("R6")
        .ok_or_else(|| SigmaFiTxError::InvalidAmount("Order missing R6 (repayment)".to_string()))?;
    let r7_hex = req
        .order_box
        .additional_registers
        .get("R7")
        .ok_or_else(|| {
            SigmaFiTxError::InvalidMaturity("Order missing R7 (maturity)".to_string())
        })?;
    let r4_hex = req
        .order_box
        .additional_registers
        .get("R4")
        .ok_or_else(|| {
            SigmaFiTxError::InvalidAddress("Order missing R4 (borrower PK)".to_string())
        })?;

    // Decode principal from R5
    let principal = ergo_tx::sigma::decode_sigma_long(r5_hex)
        .map_err(|e| SigmaFiTxError::InvalidAmount(format!("Cannot decode R5: {}", e)))?
        as u64;

    // Decode maturity from R7 (Int type 0x04)
    let maturity_blocks = decode_sigma_int(r7_hex)?;

    if maturity_blocks >= STORAGE_PERIOD {
        return Err(SigmaFiTxError::InvalidMaturity(format!(
            "Term {} exceeds storage rent period {}",
            maturity_blocks, STORAGE_PERIOD
        )));
    }

    // Fees
    let dev_fee = calculator::calculate_dev_fee(principal);
    let ui_fee = calculator::calculate_ui_fee(principal);

    // Build bond contract
    let bond_ergo_tree = constants::build_bond_contract(&req.loan_token_id);

    // Output 0: Bond box (preserve collateral from order)
    let order_box_id_bytes = hex::decode(&req.order_box.box_id)
        .map_err(|_| SigmaFiTxError::InvalidAmount("Invalid order box ID hex".to_string()))?;

    let mut bond_registers = HashMap::new();
    bond_registers.insert(
        "R4".to_string(),
        encode_sigma_coll_byte(&order_box_id_bytes),
    );
    bond_registers.insert("R5".to_string(), r4_hex.clone()); // Borrower PK from order R4
    bond_registers.insert("R6".to_string(), r6_hex.clone()); // Repayment from order R6
    bond_registers.insert(
        "R7".to_string(),
        encode_sigma_int(req.current_height + maturity_blocks),
    );
    bond_registers.insert(
        "R8".to_string(),
        encode_sigma_prop_from_ergo_tree(&req.lender_ergo_tree)?,
    );

    let bond_output = Eip12Output {
        value: req.order_box.value.clone(),
        ergo_tree: bond_ergo_tree,
        assets: req.order_box.assets.clone(),
        creation_height: req.current_height,
        additional_registers: bond_registers,
    };

    // Output 1: Principal to borrower
    // Borrower ErgoTree from order R4: SigmaProp "08cd{pubkey}" -> P2PK "0008cd{pubkey}"
    let borrower_ergo_tree = sigma_prop_to_ergo_tree(r4_hex)?;

    let loan_output = if is_erg {
        Eip12Output::simple(principal as i64, &borrower_ergo_tree, req.current_height)
    } else {
        Eip12Output {
            value: SAFE_MIN_BOX_VALUE.to_string(),
            ergo_tree: borrower_ergo_tree.clone(),
            assets: vec![Eip12Asset::new(&req.loan_token_id, principal as i64)],
            creation_height: req.current_height,
            additional_registers: HashMap::new(),
        }
    };

    // Output 2: Dev fee
    let dev_fee_output = if is_erg {
        Eip12Output::simple(
            dev_fee as i64,
            constants::DEV_FEE_ERGO_TREE,
            req.current_height,
        )
    } else {
        Eip12Output {
            value: SAFE_MIN_BOX_VALUE.to_string(),
            ergo_tree: constants::DEV_FEE_ERGO_TREE.to_string(),
            assets: if dev_fee > 0 {
                vec![Eip12Asset::new(&req.loan_token_id, dev_fee as i64)]
            } else {
                vec![]
            },
            creation_height: req.current_height,
            additional_registers: HashMap::new(),
        }
    };

    // Output 3: UI fee
    let ui_fee_output = if is_erg {
        Eip12Output::simple(ui_fee as i64, &req.ui_fee_ergo_tree, req.current_height)
    } else {
        Eip12Output {
            value: SAFE_MIN_BOX_VALUE.to_string(),
            ergo_tree: req.ui_fee_ergo_tree.clone(),
            assets: if ui_fee > 0 {
                vec![Eip12Asset::new(&req.loan_token_id, ui_fee as i64)]
            } else {
                vec![]
            },
            creation_height: req.current_height,
            additional_registers: HashMap::new(),
        }
    };

    // Calculate required ERG from lender
    let outputs_erg: i64 = if is_erg {
        // Bond preserves order ERG, loan + dev_fee + ui_fee come from lender
        principal as i64 + dev_fee as i64 + ui_fee as i64 + MINER_FEE
    } else {
        // Bond preserves order ERG; loan/dev/ui need SAFE_MIN_BOX_VALUE each + miner fee
        SAFE_MIN_BOX_VALUE * 3 + MINER_FEE
    };
    let required_erg = (outputs_erg + MIN_CHANGE_VALUE) as u64;

    // Select lender inputs
    let selected = if is_erg {
        select_erg_boxes(&req.user_inputs, required_erg)
            .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?
    } else {
        let total_tokens_needed = principal + dev_fee + ui_fee;
        select_token_boxes(
            &req.user_inputs,
            &req.loan_token_id,
            total_tokens_needed,
            required_erg,
        )
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?
    };

    // Change
    let change_erg = selected.total_erg as i64 - outputs_erg;
    if change_erg < 0 {
        return Err(SigmaFiTxError::InsufficientFunds(format!(
            "Need {} more nanoERG",
            -change_erg
        )));
    }

    let spent_token = if is_erg {
        None
    } else {
        Some((req.loan_token_id.as_str(), principal + dev_fee + ui_fee))
    };
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);
    let change_output = Eip12Output::change(
        change_erg,
        &req.lender_ergo_tree,
        change_tokens,
        req.current_height,
    );
    let fee_output = Eip12Output::fee(MINER_FEE, req.current_height);

    // Set context extension on order input: var 0 = UI fee SigmaProp
    let mut order_input = req.order_box.clone();
    let ui_sigma_prop = encode_sigma_prop_from_ergo_tree(&req.ui_fee_ergo_tree)?;
    order_input.extension.insert("0".to_string(), ui_sigma_prop);

    // Assemble inputs: order first, then lender's boxes
    let mut inputs = vec![order_input];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs: vec![
            bond_output,
            loan_output,
            dev_fee_output,
            ui_fee_output,
            change_output,
            fee_output,
        ],
    })
}

// =============================================================================
// Tx 4: Repay Bond
// =============================================================================

pub struct RepayRequest {
    /// The bond box to repay
    pub bond_box: Eip12InputBox,
    /// Loan token ID ("ERG" for native)
    pub loan_token_id: String,
    /// Borrower's P2PK ErgoTree hex
    pub borrower_ergo_tree: String,
    /// Borrower's available UTXOs
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current blockchain height
    pub current_height: i32,
}

pub fn build_repay(req: &RepayRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    let is_erg = req.loan_token_id == "ERG";

    // Parse repayment from R6
    let r6_hex =
        req.bond_box.additional_registers.get("R6").ok_or_else(|| {
            SigmaFiTxError::InvalidAmount("Bond missing R6 (repayment)".to_string())
        })?;
    let repayment = ergo_tx::sigma::decode_sigma_long(r6_hex)
        .map_err(|e| SigmaFiTxError::InvalidAmount(format!("Cannot decode R6: {}", e)))?
        as u64;

    // Get lender ErgoTree from R8
    let r8_hex =
        req.bond_box.additional_registers.get("R8").ok_or_else(|| {
            SigmaFiTxError::InvalidAddress("Bond missing R8 (lender PK)".to_string())
        })?;
    let lender_ergo_tree = sigma_prop_to_ergo_tree(r8_hex)?;

    // Bond box ID for R4 of repayment output
    let bond_box_id_bytes = hex::decode(&req.bond_box.box_id)
        .map_err(|_| SigmaFiTxError::InvalidAmount("Invalid bond box ID hex".to_string()))?;
    let r4_coll = encode_sigma_coll_byte(&bond_box_id_bytes);

    // Output 0: Repayment to lender
    let mut repay_registers = HashMap::new();
    repay_registers.insert("R4".to_string(), r4_coll);

    let repay_output = if is_erg {
        Eip12Output {
            value: (repayment as i64).to_string(),
            ergo_tree: lender_ergo_tree,
            assets: vec![],
            creation_height: req.current_height,
            additional_registers: repay_registers,
        }
    } else {
        Eip12Output {
            value: SAFE_MIN_BOX_VALUE.to_string(),
            ergo_tree: lender_ergo_tree,
            assets: vec![Eip12Asset::new(&req.loan_token_id, repayment as i64)],
            creation_height: req.current_height,
            additional_registers: repay_registers,
        }
    };

    // Output 1: Collateral returned to borrower
    let bond_erg: i64 = req.bond_box.value.parse().unwrap_or(0);
    let collateral_output = Eip12Output {
        value: bond_erg.to_string(),
        ergo_tree: req.borrower_ergo_tree.clone(),
        assets: req.bond_box.assets.clone(),
        creation_height: req.current_height,
        additional_registers: HashMap::new(),
    };

    // Calculate required ERG from borrower
    let outputs_erg = if is_erg {
        repayment as i64 + MINER_FEE
    } else {
        SAFE_MIN_BOX_VALUE + MINER_FEE
    };
    // Bond box contributes its ERG to collateral output, so lender doesn't need to cover that
    let required_erg = (outputs_erg + MIN_CHANGE_VALUE) as u64;

    let selected = if is_erg {
        select_erg_boxes(&req.user_inputs, required_erg)
            .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?
    } else {
        select_token_boxes(
            &req.user_inputs,
            &req.loan_token_id,
            repayment,
            required_erg,
        )
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?
    };

    let change_erg = selected.total_erg as i64 - outputs_erg;
    if change_erg < 0 {
        return Err(SigmaFiTxError::InsufficientFunds(format!(
            "Need {} more nanoERG",
            -change_erg
        )));
    }

    let spent_token = if is_erg {
        None
    } else {
        Some((req.loan_token_id.as_str(), repayment))
    };
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);
    let change_output = Eip12Output::change(
        change_erg,
        &req.borrower_ergo_tree,
        change_tokens,
        req.current_height,
    );
    let fee_output = Eip12Output::fee(MINER_FEE, req.current_height);

    // Bond box is first input
    let mut inputs = vec![req.bond_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs: vec![repay_output, collateral_output, change_output, fee_output],
    })
}

// =============================================================================
// Tx 5: Liquidate Bond
// =============================================================================

pub struct LiquidateRequest {
    /// The bond box to liquidate
    pub bond_box: Eip12InputBox,
    /// Lender's P2PK ErgoTree hex
    pub lender_ergo_tree: String,
    /// Lender's available UTXOs (for miner fee)
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current blockchain height
    pub current_height: i32,
}

pub fn build_liquidate(req: &LiquidateRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    let bond_erg: i64 = req.bond_box.value.parse().unwrap_or(0);

    // Bond box ID for R4
    let bond_box_id_bytes = hex::decode(&req.bond_box.box_id)
        .map_err(|_| SigmaFiTxError::InvalidAmount("Invalid bond box ID hex".to_string()))?;

    let mut liquidate_registers = HashMap::new();
    liquidate_registers.insert("R4".to_string(), encode_sigma_coll_byte(&bond_box_id_bytes));

    // Output 0: All collateral to lender
    let liquidate_output = Eip12Output {
        value: bond_erg.to_string(),
        ergo_tree: req.lender_ergo_tree.clone(),
        assets: req.bond_box.assets.clone(),
        creation_height: req.current_height,
        additional_registers: liquidate_registers,
    };

    // Select inputs for miner fee
    let fee_required = (MINER_FEE + MIN_CHANGE_VALUE) as u64;
    let selected = select_erg_boxes(&req.user_inputs, fee_required)
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?;

    let change_erg = selected.total_erg as i64 - MINER_FEE;
    let change_tokens = collect_change_tokens(&selected.boxes, None);
    let change_output = Eip12Output::change(
        change_erg,
        &req.lender_ergo_tree,
        change_tokens,
        req.current_height,
    );
    let fee_output = Eip12Output::fee(MINER_FEE, req.current_height);

    // Bond box first, then lender's fee inputs
    let mut inputs = vec![req.bond_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs: vec![liquidate_output, change_output, fee_output],
    })
}

// =============================================================================
// Helpers
// =============================================================================

/// Convert a SigmaProp register hex ("08cd{pubkey}") to a P2PK ErgoTree hex ("0008cd{pubkey}")
fn sigma_prop_to_ergo_tree(sigma_prop_hex: &str) -> Result<String, SigmaFiTxError> {
    if sigma_prop_hex.len() >= 70 && sigma_prop_hex.starts_with("08cd") {
        Ok(format!("0008cd{}", &sigma_prop_hex[4..70]))
    } else {
        Err(SigmaFiTxError::InvalidAddress(format!(
            "Expected SigmaProp(ProveDlog) hex starting with '08cd', got: {}",
            &sigma_prop_hex[..sigma_prop_hex.len().min(10)]
        )))
    }
}

/// Decode a Sigma Int (type tag 0x04) from register hex
fn decode_sigma_int(hex_str: &str) -> Result<i32, SigmaFiTxError> {
    let bytes =
        hex::decode(hex_str).map_err(|_| SigmaFiTxError::InvalidAmount("Invalid hex".into()))?;
    if bytes.is_empty() || bytes[0] != 0x04 {
        return Err(SigmaFiTxError::InvalidAmount(format!(
            "Expected SInt type tag 0x04, got 0x{:02x}",
            bytes.first().copied().unwrap_or(0)
        )));
    }
    // VLQ decode
    let mut result: u32 = 0;
    let mut shift = 0;
    for &byte in &bytes[1..] {
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    // Zigzag decode
    let value = if result & 1 == 0 {
        (result >> 1) as i32
    } else {
        -((result >> 1) as i32) - 1
    };
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_utxo(value: i64, tokens: Vec<(&str, i64)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: "aaaa".repeat(16),
            transaction_id: "bbbb".repeat(16),
            index: 0,
            value: value.to_string(),
            ergo_tree: "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                .to_string(),
            assets: tokens
                .into_iter()
                .map(|(id, amt)| Eip12Asset::new(id, amt))
                .collect(),
            creation_height: 1000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    const TEST_ERGO_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    #[test]
    fn test_encode_sigma_prop() {
        let result = encode_sigma_prop_from_ergo_tree(TEST_ERGO_TREE).unwrap();
        assert!(result.starts_with("08cd"));
        assert_eq!(result.len(), 70); // 4 + 66
    }

    #[test]
    fn test_sigma_prop_roundtrip() {
        let sigma_prop = encode_sigma_prop_from_ergo_tree(TEST_ERGO_TREE).unwrap();
        let ergo_tree = sigma_prop_to_ergo_tree(&sigma_prop).unwrap();
        assert_eq!(ergo_tree, TEST_ERGO_TREE);
    }

    #[test]
    fn test_decode_sigma_int_roundtrip() {
        for val in [0, 1, -1, 100, -100, 21600, i32::MAX, i32::MIN] {
            let encoded = encode_sigma_int(val);
            let decoded = decode_sigma_int(&encoded).unwrap();
            assert_eq!(decoded, val, "Failed roundtrip for {}", val);
        }
    }

    #[test]
    fn test_build_open_order_erg() {
        let req = OpenOrderRequest {
            borrower_ergo_tree: TEST_ERGO_TREE.to_string(),
            loan_token_id: "ERG".to_string(),
            principal: 10_000_000_000,      // 10 ERG
            repayment: 10_500_000_000,      // 10.5 ERG
            maturity_blocks: 21600,         // ~30 days
            collateral_erg: 15_000_000_000, // 15 ERG collateral
            collateral_tokens: vec![],
            user_inputs: vec![mock_utxo(20_000_000_000, vec![])],
            current_height: 1000,
        };

        let tx = build_open_order(&req).unwrap();
        assert_eq!(tx.outputs.len(), 3); // order + change + fee
        assert_eq!(tx.outputs[0].value, "15000000000");
        assert!(tx.outputs[0].additional_registers.contains_key("R4"));
        assert!(tx.outputs[0].additional_registers.contains_key("R5"));
        assert!(tx.outputs[0].additional_registers.contains_key("R6"));
        assert!(tx.outputs[0].additional_registers.contains_key("R7"));
    }

    #[test]
    fn test_build_open_order_validation() {
        let base = OpenOrderRequest {
            borrower_ergo_tree: TEST_ERGO_TREE.to_string(),
            loan_token_id: "ERG".to_string(),
            principal: 0,
            repayment: 100,
            maturity_blocks: 21600,
            collateral_erg: 1_000_000_000,
            collateral_tokens: vec![],
            user_inputs: vec![mock_utxo(10_000_000_000, vec![])],
            current_height: 1000,
        };
        assert!(build_open_order(&base).is_err()); // zero principal
    }

    #[test]
    fn test_build_cancel_order() {
        let mut order_regs = HashMap::new();
        order_regs.insert(
            "R4".to_string(),
            encode_sigma_prop_from_ergo_tree(TEST_ERGO_TREE).unwrap(),
        );
        order_regs.insert("R5".to_string(), encode_sigma_long(10_000_000_000));
        order_regs.insert("R6".to_string(), encode_sigma_long(10_500_000_000));
        order_regs.insert("R7".to_string(), encode_sigma_int(21600));

        let order_box = Eip12InputBox {
            box_id: "cccc".repeat(16),
            transaction_id: "dddd".repeat(16),
            index: 0,
            value: "15000000000".to_string(),
            ergo_tree: constants::ORDER_ON_CLOSE_ERG_CONTRACT.to_string(),
            assets: vec![],
            creation_height: 1000,
            additional_registers: order_regs,
            extension: HashMap::new(),
        };

        let req = CancelOrderRequest {
            order_box,
            borrower_ergo_tree: TEST_ERGO_TREE.to_string(),
            user_inputs: vec![mock_utxo(5_000_000_000, vec![])],
            current_height: 1100,
        };

        let tx = build_cancel_order(&req).unwrap();
        assert_eq!(tx.inputs.len(), 2); // order + fee input
        assert_eq!(tx.outputs.len(), 3); // return + change + fee
        assert_eq!(tx.outputs[0].value, "15000000000"); // collateral returned
    }

    #[test]
    fn test_build_liquidate() {
        let mut bond_regs = HashMap::new();
        bond_regs.insert("R4".to_string(), encode_sigma_coll_byte(&[0u8; 32]));
        bond_regs.insert(
            "R5".to_string(),
            encode_sigma_prop_from_ergo_tree(TEST_ERGO_TREE).unwrap(),
        );
        bond_regs.insert("R6".to_string(), encode_sigma_long(10_500_000_000));
        bond_regs.insert("R7".to_string(), encode_sigma_int(900));
        bond_regs.insert(
            "R8".to_string(),
            encode_sigma_prop_from_ergo_tree(TEST_ERGO_TREE).unwrap(),
        );

        let bond_box = Eip12InputBox {
            box_id: "eeee".repeat(16),
            transaction_id: "ffff".repeat(16),
            index: 0,
            value: "15000000000".to_string(),
            ergo_tree: constants::ERG_BOND_CONTRACT.to_string(),
            assets: vec![],
            creation_height: 800,
            additional_registers: bond_regs,
            extension: HashMap::new(),
        };

        let req = LiquidateRequest {
            bond_box,
            lender_ergo_tree: TEST_ERGO_TREE.to_string(),
            user_inputs: vec![mock_utxo(5_000_000_000, vec![])],
            current_height: 1100,
        };

        let tx = build_liquidate(&req).unwrap();
        assert_eq!(tx.inputs.len(), 2); // bond + fee input
        assert_eq!(tx.outputs.len(), 3); // collateral + change + fee
        assert_eq!(tx.outputs[0].value, "15000000000");
        assert!(tx.outputs[0].additional_registers.contains_key("R4"));
    }
}
