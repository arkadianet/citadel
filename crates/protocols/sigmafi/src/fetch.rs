//! SigmaFi bond/order discovery via node API
//!
//! Scans known contract addresses for open orders and active bonds.
//! All ErgoTree-to-address and pubkey-to-address conversions are done locally
//! via ergo-lib to minimize node API calls.

use citadel_core::ProtocolError;
use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::mir::constant::Literal;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_node_client::NodeClient;

use crate::calculator;
use crate::constants::{self, OrderType, SUPPORTED_TOKENS};
use crate::state::{ActiveBond, BondMarket, CollateralToken, OpenOrder};

/// Fetch the full SigmaFi bond market from the node.
///
/// For each supported token, derives the order and bond contract addresses
/// locally, queries the node for unspent boxes, and parses registers.
pub async fn fetch_bond_market(
    client: &NodeClient,
    user_address: Option<&str>,
    block_height: u32,
    oracle_erg_usd: Option<f64>,
) -> Result<BondMarket, ProtocolError> {
    let mut order_queries: Vec<(String, &str, &str, u8)> = Vec::new();
    let mut bond_queries: Vec<(String, &str, &str, u8)> = Vec::new();

    for token in SUPPORTED_TOKENS {
        let order_tree = constants::build_order_contract(token.token_id, OrderType::OnClose);
        order_queries.push((order_tree, token.token_id, token.name, token.decimals));

        let bond_tree = constants::build_bond_contract(token.token_id);
        bond_queries.push((bond_tree, token.token_id, token.name, token.decimals));
    }

    let mut orders: Vec<OpenOrder> = Vec::new();
    let mut bonds: Vec<ActiveBond> = Vec::new();

    // Fetch orders
    for (ergo_tree, token_id, token_name, token_decimals) in &order_queries {
        match fetch_boxes_by_ergo_tree(client, ergo_tree).await {
            Ok(boxes) => {
                for ergo_box in boxes {
                    match parse_open_order(
                        &ergo_box,
                        token_id,
                        token_name,
                        *token_decimals,
                        user_address,
                        oracle_erg_usd,
                    ) {
                        Ok(order) => orders.push(order),
                        Err(e) => {
                            tracing::debug!(
                                box_id = %ergo_box.box_id(),
                                error = %e,
                                "Skipping unparseable order box"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    token = token_name,
                    error = %e,
                    "Failed to fetch order boxes"
                );
            }
        }
    }

    // Fetch bonds
    for (ergo_tree, token_id, token_name, token_decimals) in &bond_queries {
        match fetch_boxes_by_ergo_tree(client, ergo_tree).await {
            Ok(boxes) => {
                for ergo_box in boxes {
                    match parse_active_bond(
                        &ergo_box,
                        token_id,
                        token_name,
                        *token_decimals,
                        block_height,
                        user_address,
                    ) {
                        Ok(bond) => bonds.push(bond),
                        Err(e) => {
                            tracing::debug!(
                                box_id = %ergo_box.box_id(),
                                error = %e,
                                "Skipping unparseable bond box"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    token = token_name,
                    error = %e,
                    "Failed to fetch bond boxes"
                );
            }
        }
    }

    Ok(BondMarket {
        orders,
        bonds,
        block_height,
    })
}

/// Convert an ErgoTree hex to an Ergo address locally (no node API call).
fn ergo_tree_to_address_local(ergo_tree_hex: &str) -> Result<String, ProtocolError> {
    let tree_bytes = hex::decode(ergo_tree_hex).map_err(|e| ProtocolError::StateUnavailable {
        reason: format!("Invalid ErgoTree hex: {}", e),
    })?;

    let tree =
        ErgoTree::sigma_parse_bytes(&tree_bytes).map_err(|e| ProtocolError::StateUnavailable {
            reason: format!("Failed to parse ErgoTree: {}", e),
        })?;

    let address = Address::recreate_from_ergo_tree(&tree).map_err(|e| {
        ProtocolError::StateUnavailable {
            reason: format!("Failed to create address from ErgoTree: {}", e),
        }
    })?;

    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&address))
}

/// Convert a compressed public key hex (33 bytes) to a P2PK address locally.
fn pk_hex_to_address_local(pk_hex: &str) -> Result<String, ProtocolError> {
    // Build P2PK ErgoTree: 0008cd{33-byte-pubkey}
    let ergo_tree_hex = format!("0008cd{}", pk_hex);
    ergo_tree_to_address_local(&ergo_tree_hex)
}

/// Fetch unspent boxes for a given ErgoTree hex.
///
/// Converts ErgoTree -> address locally, then queries node for unspent boxes.
async fn fetch_boxes_by_ergo_tree(
    client: &NodeClient,
    ergo_tree_hex: &str,
) -> Result<Vec<ErgoBox>, ProtocolError> {
    let address = ergo_tree_to_address_local(ergo_tree_hex)?;

    let boxes = client
        .inner()
        .unspent_boxes_by_address(&address, 0, 500)
        .await
        .map_err(|e| ProtocolError::StateUnavailable {
            reason: format!("Failed to fetch boxes for address {}: {}", address, e),
        })?;

    Ok(boxes)
}

/// Parse an ErgoBox into an OpenOrder (all local, no API calls)
fn parse_open_order(
    ergo_box: &ErgoBox,
    loan_token_id: &str,
    loan_token_name: &str,
    loan_token_decimals: u8,
    user_address: Option<&str>,
    oracle_erg_usd: Option<f64>,
) -> Result<OpenOrder, ProtocolError> {
    let box_id = ergo_box.box_id().to_string();

    // R4: Borrower PK (SigmaProp)
    let borrower_pk_hex = extract_sigma_prop_hex(ergo_box, NonMandatoryRegisterId::R4)?;
    let borrower_address = pk_hex_to_address_local(&borrower_pk_hex)?;

    // R5: Principal (Long)
    let principal = extract_long(ergo_box, NonMandatoryRegisterId::R5)? as u64;

    // R6: Total Repayment (Long)
    let repayment = extract_long(ergo_box, NonMandatoryRegisterId::R6)? as u64;

    // R7: Maturity (Int)
    let maturity_blocks = extract_int(ergo_box, NonMandatoryRegisterId::R7)?;

    // Collateral
    let collateral_erg = *ergo_box.value.as_u64();
    let collateral_tokens = extract_tokens(ergo_box);

    // Calculations
    let interest_percent = calculator::calculate_interest_percent(principal, repayment);
    let apr = calculator::calculate_apr(interest_percent, maturity_blocks);

    let is_own = user_address.is_some_and(|ua| ua == borrower_address);

    // Calculate collateral ratio
    let collateral_ratio = if loan_token_name == "ERG" {
        // ERG-to-ERG: ratio is simply collateral / principal (both in nanoERG)
        if principal > 0 {
            let interest_erg = (repayment as f64 - principal as f64) / 1e9;
            Some(calculator::calculate_collateral_ratio(
                collateral_erg as f64 / 1e9,
                principal as f64 / 1e9,
                interest_erg,
            ))
        } else {
            Some(0.0)
        }
    } else if loan_token_name == "SigUSD" {
        // SigUSD: convert ERG collateral to USD via oracle
        oracle_erg_usd.map(|erg_usd| {
            let collateral_usd = (collateral_erg as f64 / 1e9) * erg_usd;
            let principal_usd = principal as f64 / 1e2;
            let interest_usd = (repayment as f64 - principal as f64) / 1e2;
            calculator::calculate_collateral_ratio(collateral_usd, principal_usd, interest_usd)
        })
    } else {
        // Other tokens: no price data, can't calculate
        None
    };

    let ergo_tree_hex = ergo_box
        .ergo_tree
        .sigma_serialize_bytes()
        .map(hex::encode)
        .unwrap_or_default();

    Ok(OpenOrder {
        box_id,
        ergo_tree: ergo_tree_hex,
        creation_height: ergo_box.creation_height as i32,
        borrower_address,
        loan_token_id: loan_token_id.to_string(),
        loan_token_name: loan_token_name.to_string(),
        loan_token_decimals,
        principal,
        repayment,
        maturity_blocks,
        collateral_erg,
        collateral_tokens,
        interest_percent,
        apr,
        collateral_ratio,
        is_own,
        // Tx context fetched lazily when needed for tx building
        transaction_id: String::new(),
        output_index: 0,
    })
}

/// Parse an ErgoBox into an ActiveBond (all local, no API calls)
fn parse_active_bond(
    ergo_box: &ErgoBox,
    loan_token_id: &str,
    loan_token_name: &str,
    loan_token_decimals: u8,
    block_height: u32,
    user_address: Option<&str>,
) -> Result<ActiveBond, ProtocolError> {
    let box_id = ergo_box.box_id().to_string();

    // R4: Originating order box ID (Coll[Byte])
    let originating_order_id = extract_coll_byte_hex(ergo_box, NonMandatoryRegisterId::R4)?;

    // R5: Borrower PK (SigmaProp)
    let borrower_pk_hex = extract_sigma_prop_hex(ergo_box, NonMandatoryRegisterId::R5)?;
    let borrower_address = pk_hex_to_address_local(&borrower_pk_hex)?;

    // R6: Repayment (Long)
    let repayment = extract_long(ergo_box, NonMandatoryRegisterId::R6)? as u64;

    // R7: Maturity Height (Int)
    let maturity_height = extract_int(ergo_box, NonMandatoryRegisterId::R7)?;

    // R8: Lender PK (SigmaProp)
    let lender_pk_hex = extract_sigma_prop_hex(ergo_box, NonMandatoryRegisterId::R8)?;
    let lender_address = pk_hex_to_address_local(&lender_pk_hex)?;

    // Collateral
    let collateral_erg = *ergo_box.value.as_u64();
    let collateral_tokens = extract_tokens(ergo_box);

    let blocks_remaining = maturity_height - block_height as i32;
    let is_own_lend = user_address.is_some_and(|ua| ua == lender_address);
    let is_own_borrow = user_address.is_some_and(|ua| ua == borrower_address);

    let ergo_tree_hex = ergo_box
        .ergo_tree
        .sigma_serialize_bytes()
        .map(hex::encode)
        .unwrap_or_default();

    Ok(ActiveBond {
        box_id,
        ergo_tree: ergo_tree_hex,
        originating_order_id,
        borrower_address,
        lender_address,
        loan_token_id: loan_token_id.to_string(),
        loan_token_name: loan_token_name.to_string(),
        loan_token_decimals,
        repayment,
        maturity_height,
        collateral_erg,
        collateral_tokens,
        blocks_remaining,
        is_liquidable: blocks_remaining <= 0 && is_own_lend,
        is_repayable: blocks_remaining > 0 && is_own_borrow,
        is_own_lend,
        is_own_borrow,
        // Tx context fetched lazily when needed for tx building
        transaction_id: String::new(),
        output_index: 0,
    })
}

// =============================================================================
// Register extraction helpers
// =============================================================================

/// Extract a Long value from a register
fn extract_long(ergo_box: &ErgoBox, reg: NonMandatoryRegisterId) -> Result<i64, ProtocolError> {
    let constant = ergo_box
        .additional_registers
        .get_constant(reg)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Register {:?} error: {}", reg, e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Register {:?} not found", reg),
        })?;

    match &constant.v {
        Literal::Long(val) => Ok(*val),
        other => Err(ProtocolError::BoxParseError {
            message: format!("Expected Long in {:?}, got {:?}", reg, other),
        }),
    }
}

/// Extract an Int value from a register
fn extract_int(ergo_box: &ErgoBox, reg: NonMandatoryRegisterId) -> Result<i32, ProtocolError> {
    let constant = ergo_box
        .additional_registers
        .get_constant(reg)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Register {:?} error: {}", reg, e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Register {:?} not found", reg),
        })?;

    match &constant.v {
        Literal::Int(val) => Ok(*val),
        other => Err(ProtocolError::BoxParseError {
            message: format!("Expected Int in {:?}, got {:?}", reg, other),
        }),
    }
}

/// Extract a SigmaProp from a register, returning the raw public key hex (33 bytes).
///
/// SigmaProp in registers is encoded as: 08cd{33-byte-compressed-pubkey}
/// We return just the 33-byte compressed pubkey hex for address conversion.
fn extract_sigma_prop_hex(
    ergo_box: &ErgoBox,
    reg: NonMandatoryRegisterId,
) -> Result<String, ProtocolError> {
    let constant = ergo_box
        .additional_registers
        .get_constant(reg)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Register {:?} error: {}", reg, e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Register {:?} not found", reg),
        })?;

    // Serialize the constant to get the full sigma-encoded bytes
    let bytes = constant
        .sigma_serialize_bytes()
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Failed to serialize {:?}: {}", reg, e),
        })?;

    let hex_str = hex::encode(&bytes);

    // SigmaProp(ProveDlog(ECPoint)) serializes as:
    // 08 cd <33-byte-compressed-pubkey>
    // Total: 2 + 33 = 35 bytes = 70 hex chars
    if hex_str.len() >= 70 && hex_str.starts_with("08cd") {
        Ok(hex_str[4..70].to_string())
    } else {
        Err(ProtocolError::BoxParseError {
            message: format!(
                "Unexpected SigmaProp encoding in {:?}: {}",
                reg,
                &hex_str[..hex_str.len().min(20)]
            ),
        })
    }
}

/// Extract Coll[Byte] from a register, returning as hex string
fn extract_coll_byte_hex(
    ergo_box: &ErgoBox,
    reg: NonMandatoryRegisterId,
) -> Result<String, ProtocolError> {
    let constant = ergo_box
        .additional_registers
        .get_constant(reg)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Register {:?} error: {}", reg, e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Register {:?} not found", reg),
        })?;

    use ergo_lib::ergotree_ir::mir::value::CollKind;
    use ergo_lib::ergotree_ir::types::stype::SType;

    match &constant.v {
        Literal::Coll(CollKind::NativeColl(
            ergo_lib::ergotree_ir::mir::value::NativeColl::CollByte(bytes),
        )) => {
            let u8_bytes: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
            Ok(hex::encode(u8_bytes))
        }
        Literal::Coll(CollKind::WrappedColl {
            elem_tpe: SType::SByte,
            items,
        }) => {
            let bytes: Vec<u8> = items
                .iter()
                .filter_map(|item| match item {
                    Literal::Byte(b) => Some(*b as u8),
                    _ => None,
                })
                .collect();
            Ok(hex::encode(bytes))
        }
        other => Err(ProtocolError::BoxParseError {
            message: format!("Expected Coll[Byte] in {:?}, got {:?}", reg, other),
        }),
    }
}

/// Extract tokens from a box
fn extract_tokens(ergo_box: &ErgoBox) -> Vec<CollateralToken> {
    ergo_box
        .tokens
        .as_ref()
        .map(|tokens| {
            tokens
                .iter()
                .map(|t| {
                    let tid: String = t.token_id.into();
                    CollateralToken {
                        token_id: tid,
                        amount: *t.amount.as_u64(),
                        name: None,
                        decimals: None,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}
