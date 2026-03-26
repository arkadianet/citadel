//! SigmaFi transaction builders (EIP-12 format)

use std::collections::HashMap;

use ergo_tx::eip12::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};
use ergo_tx::sigma::{encode_sigma_coll_byte, encode_sigma_int, encode_sigma_long};
use ergo_tx::{append_change_output, select_erg_boxes, select_inputs_for_spend};

use crate::calculator;
use crate::constants::{self, OrderType, SAFE_MIN_BOX_VALUE, STORAGE_PERIOD};

const MINER_FEE: i64 = 1_100_000;
const MIN_CHANGE_VALUE: i64 = 1_000_000;

/// P2PK ErgoTree "0008cd{pubkey}" -> SigmaProp register "08cd{pubkey}"
fn encode_sigma_prop_from_ergo_tree(p2pk_ergo_tree: &str) -> Result<String, SigmaFiTxError> {
    if p2pk_ergo_tree.len() >= 72 && p2pk_ergo_tree.starts_with("0008cd") {
        Ok(format!("08cd{}", &p2pk_ergo_tree[6..72]))
    } else {
        Err(SigmaFiTxError::InvalidAddress(
            "Expected P2PK ErgoTree (0008cd + pubkey)".to_string(),
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SigmaFiTxError {
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),
    #[error("Invalid maturity: {0}")]
    InvalidMaturity(String),
    #[error("Box selection failed: {0}")]
    BoxSelection(String),
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
}

pub struct OpenOrderRequest {
    pub borrower_ergo_tree: String,
    /// "ERG" for native
    pub loan_token_id: String,
    pub principal: u64,
    pub repayment: u64,
    pub maturity_blocks: i32,
    /// nanoERG (0 if token-only with SAFE_MIN_BOX_VALUE)
    pub collateral_erg: u64,
    pub collateral_tokens: Vec<(String, u64)>,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
}

pub fn build_open_order(req: &OpenOrderRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
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

    let r4 = encode_sigma_prop_from_ergo_tree(&req.borrower_ergo_tree)?;
    let r5 = encode_sigma_long(req.principal as i64);
    let r6 = encode_sigma_long(req.repayment as i64);
    let r7 = encode_sigma_int(req.maturity_blocks);

    let registers = ergo_tx::sigma_registers!("R4" => r4, "R5" => r5, "R6" => r6, "R7" => r7);

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

    let required_erg = (order_value + MINER_FEE + MIN_CHANGE_VALUE) as u64;

    let first_token = req
        .collateral_tokens
        .first()
        .map(|(tid, amt)| (tid.as_str(), *amt));
    let selected = select_inputs_for_spend(&req.user_inputs, required_erg, first_token)
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?;

    let erg_used = (order_value + MINER_FEE) as u64;
    let spent_tokens: Vec<(&str, u64)> = req
        .collateral_tokens
        .iter()
        .map(|(tid, amt)| (tid.as_str(), *amt))
        .collect();

    let mut outputs = vec![order_output];

    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &spent_tokens,
        &req.borrower_ergo_tree,
        req.current_height,
        MIN_CHANGE_VALUE as u64,
    )
    .map_err(|e| SigmaFiTxError::InsufficientFunds(e.to_string()))?;

    outputs.push(Eip12Output::fee(MINER_FEE, req.current_height));

    Ok(Eip12UnsignedTx {
        inputs: selected.boxes,
        data_inputs: vec![],
        outputs,
    })
}

pub struct CancelOrderRequest {
    pub order_box: Eip12InputBox,
    pub borrower_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
}

pub fn build_cancel_order(req: &CancelOrderRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    let order_erg: i64 = req
        .order_box
        .value
        .parse()
        .map_err(|_| SigmaFiTxError::InvalidAmount("Invalid order box value".to_string()))?;

    let fee_required = (MINER_FEE + MIN_CHANGE_VALUE) as u64;
    let selected = select_erg_boxes(&req.user_inputs, fee_required)
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?;

    let return_output = Eip12Output {
        value: order_erg.to_string(),
        ergo_tree: req.borrower_ergo_tree.clone(),
        assets: req.order_box.assets.clone(),
        creation_height: req.current_height,
        additional_registers: HashMap::new(),
    };

    let mut outputs = vec![return_output];

    append_change_output(
        &mut outputs,
        &selected,
        MINER_FEE as u64,
        &[],
        &req.borrower_ergo_tree,
        req.current_height,
        MIN_CHANGE_VALUE as u64,
    )
    .map_err(|e| SigmaFiTxError::InsufficientFunds(e.to_string()))?;

    outputs.push(Eip12Output::fee(MINER_FEE, req.current_height));

    let mut inputs = vec![req.order_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

pub struct CloseOrderRequest {
    pub order_box: Eip12InputBox,
    pub lender_ergo_tree: String,
    /// UI implementor's ErgoTree for fee output
    pub ui_fee_ergo_tree: String,
    /// "ERG" for native
    pub loan_token_id: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
}

pub fn build_close_order(req: &CloseOrderRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    let is_erg = req.loan_token_id == "ERG";

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

    let principal = ergo_tx::sigma::decode_sigma_long(r5_hex)
        .map_err(|e| SigmaFiTxError::InvalidAmount(format!("Cannot decode R5: {}", e)))?
        as u64;

    let maturity_blocks = decode_sigma_int(r7_hex)?;

    if maturity_blocks >= STORAGE_PERIOD {
        return Err(SigmaFiTxError::InvalidMaturity(format!(
            "Term {} exceeds storage rent period {}",
            maturity_blocks, STORAGE_PERIOD
        )));
    }

    let dev_fee = calculator::calculate_dev_fee(principal);
    let ui_fee = calculator::calculate_ui_fee(principal);
    let bond_ergo_tree = constants::build_bond_contract(&req.loan_token_id);

    let order_box_id_bytes = hex::decode(&req.order_box.box_id)
        .map_err(|_| SigmaFiTxError::InvalidAmount("Invalid order box ID hex".to_string()))?;

    let bond_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_coll_byte(&order_box_id_bytes),
        "R5" => r4_hex.clone(),
        "R6" => r6_hex.clone(),
        "R7" => encode_sigma_int(req.current_height + maturity_blocks),
        "R8" => encode_sigma_prop_from_ergo_tree(&req.lender_ergo_tree)?,
    );

    let bond_output = Eip12Output {
        value: req.order_box.value.clone(),
        ergo_tree: bond_ergo_tree,
        assets: req.order_box.assets.clone(),
        creation_height: req.current_height,
        additional_registers: bond_registers,
    };

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

    // Bond preserves order ERG; lender provides loan + fees
    let outputs_erg: i64 = if is_erg {
        principal as i64 + dev_fee as i64 + ui_fee as i64 + MINER_FEE
    } else {
        SAFE_MIN_BOX_VALUE * 3 + MINER_FEE
    };
    let required_erg = (outputs_erg + MIN_CHANGE_VALUE) as u64;

    let token_needed = if is_erg {
        None
    } else {
        Some((req.loan_token_id.as_str(), principal + dev_fee + ui_fee))
    };
    let selected = select_inputs_for_spend(&req.user_inputs, required_erg, token_needed)
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?;

    let spent_tokens: Vec<(&str, u64)> = token_needed.into_iter().collect();
    let mut outputs = vec![bond_output, loan_output, dev_fee_output, ui_fee_output];

    append_change_output(
        &mut outputs,
        &selected,
        outputs_erg as u64,
        &spent_tokens,
        &req.lender_ergo_tree,
        req.current_height,
        MIN_CHANGE_VALUE as u64,
    )
    .map_err(|e| SigmaFiTxError::InsufficientFunds(e.to_string()))?;

    outputs.push(Eip12Output::fee(MINER_FEE, req.current_height));

    // Context var 0 = UI fee recipient SigmaProp (required by order contract)
    let mut order_input = req.order_box.clone();
    let ui_sigma_prop = encode_sigma_prop_from_ergo_tree(&req.ui_fee_ergo_tree)?;
    order_input.extension.insert("0".to_string(), ui_sigma_prop);

    let mut inputs = vec![order_input];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

pub struct RepayRequest {
    pub bond_box: Eip12InputBox,
    /// "ERG" for native
    pub loan_token_id: String,
    pub borrower_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
}

pub fn build_repay(req: &RepayRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    let is_erg = req.loan_token_id == "ERG";

    let r6_hex =
        req.bond_box.additional_registers.get("R6").ok_or_else(|| {
            SigmaFiTxError::InvalidAmount("Bond missing R6 (repayment)".to_string())
        })?;
    let repayment = ergo_tx::sigma::decode_sigma_long(r6_hex)
        .map_err(|e| SigmaFiTxError::InvalidAmount(format!("Cannot decode R6: {}", e)))?
        as u64;

    let r8_hex =
        req.bond_box.additional_registers.get("R8").ok_or_else(|| {
            SigmaFiTxError::InvalidAddress("Bond missing R8 (lender PK)".to_string())
        })?;
    let lender_ergo_tree = sigma_prop_to_ergo_tree(r8_hex)?;

    let bond_box_id_bytes = hex::decode(&req.bond_box.box_id)
        .map_err(|_| SigmaFiTxError::InvalidAmount("Invalid bond box ID hex".to_string()))?;
    let r4_coll = encode_sigma_coll_byte(&bond_box_id_bytes);

    let repay_registers = ergo_tx::sigma_registers!("R4" => r4_coll);

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

    let bond_erg: i64 = req.bond_box.value.parse().unwrap_or(0);
    let collateral_output = Eip12Output {
        value: bond_erg.to_string(),
        ergo_tree: req.borrower_ergo_tree.clone(),
        assets: req.bond_box.assets.clone(),
        creation_height: req.current_height,
        additional_registers: HashMap::new(),
    };

    let outputs_erg = if is_erg {
        repayment as i64 + MINER_FEE
    } else {
        SAFE_MIN_BOX_VALUE + MINER_FEE
    };
    // Bond box ERG covers collateral output, borrower only needs repayment + fee
    let required_erg = (outputs_erg + MIN_CHANGE_VALUE) as u64;

    let token_needed = if is_erg {
        None
    } else {
        Some((req.loan_token_id.as_str(), repayment))
    };
    let selected = select_inputs_for_spend(&req.user_inputs, required_erg, token_needed)
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?;

    let spent_tokens: Vec<(&str, u64)> = token_needed.into_iter().collect();
    let mut outputs = vec![repay_output, collateral_output];

    append_change_output(
        &mut outputs,
        &selected,
        outputs_erg as u64,
        &spent_tokens,
        &req.borrower_ergo_tree,
        req.current_height,
        MIN_CHANGE_VALUE as u64,
    )
    .map_err(|e| SigmaFiTxError::InsufficientFunds(e.to_string()))?;

    outputs.push(Eip12Output::fee(MINER_FEE, req.current_height));

    let mut inputs = vec![req.bond_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

pub struct LiquidateRequest {
    pub bond_box: Eip12InputBox,
    pub lender_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
}

pub fn build_liquidate(req: &LiquidateRequest) -> Result<Eip12UnsignedTx, SigmaFiTxError> {
    let bond_erg: i64 = req.bond_box.value.parse().unwrap_or(0);

    let bond_box_id_bytes = hex::decode(&req.bond_box.box_id)
        .map_err(|_| SigmaFiTxError::InvalidAmount("Invalid bond box ID hex".to_string()))?;

    let liquidate_registers = ergo_tx::sigma_registers!("R4" => encode_sigma_coll_byte(&bond_box_id_bytes));

    let liquidate_output = Eip12Output {
        value: bond_erg.to_string(),
        ergo_tree: req.lender_ergo_tree.clone(),
        assets: req.bond_box.assets.clone(),
        creation_height: req.current_height,
        additional_registers: liquidate_registers,
    };

    let fee_required = (MINER_FEE + MIN_CHANGE_VALUE) as u64;
    let selected = select_erg_boxes(&req.user_inputs, fee_required)
        .map_err(|e| SigmaFiTxError::BoxSelection(e.to_string()))?;

    let mut outputs = vec![liquidate_output];

    append_change_output(
        &mut outputs,
        &selected,
        MINER_FEE as u64,
        &[],
        &req.lender_ergo_tree,
        req.current_height,
        MIN_CHANGE_VALUE as u64,
    )
    .map_err(|e| SigmaFiTxError::InsufficientFunds(e.to_string()))?;

    outputs.push(Eip12Output::fee(MINER_FEE, req.current_height));

    let mut inputs = vec![req.bond_box.clone()];
    inputs.extend(selected.boxes);

    Ok(Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    })
}

/// SigmaProp register "08cd{pubkey}" -> P2PK ErgoTree "0008cd{pubkey}"
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

fn decode_sigma_int(hex_str: &str) -> Result<i32, SigmaFiTxError> {
    let bytes =
        hex::decode(hex_str).map_err(|_| SigmaFiTxError::InvalidAmount("Invalid hex".into()))?;
    if bytes.is_empty() || bytes[0] != 0x04 {
        return Err(SigmaFiTxError::InvalidAmount(format!(
            "Expected SInt type tag 0x04, got 0x{:02x}",
            bytes.first().copied().unwrap_or(0)
        )));
    }
    let mut result: u32 = 0;
    let mut shift = 0;
    for &byte in &bytes[1..] {
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
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
        let order_regs = ergo_tx::sigma_registers!(
            "R4" => encode_sigma_prop_from_ergo_tree(TEST_ERGO_TREE).unwrap(),
            "R5" => encode_sigma_long(10_000_000_000),
            "R6" => encode_sigma_long(10_500_000_000),
            "R7" => encode_sigma_int(21600),
        );

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
        let bond_regs = ergo_tx::sigma_registers!(
            "R4" => encode_sigma_coll_byte(&[0u8; 32]),
            "R5" => encode_sigma_prop_from_ergo_tree(TEST_ERGO_TREE).unwrap(),
            "R6" => encode_sigma_long(10_500_000_000),
            "R7" => encode_sigma_int(900),
            "R8" => encode_sigma_prop_from_ergo_tree(TEST_ERGO_TREE).unwrap(),
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
