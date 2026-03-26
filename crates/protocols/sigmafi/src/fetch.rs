use citadel_core::ProtocolError;
use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_node_client::NodeClient;
use ergo_tx::ergo_box_utils;

use crate::calculator;
use crate::constants::{self, OrderType, SUPPORTED_TOKENS};
use crate::state::{ActiveBond, BondMarket, CollateralToken, OpenOrder};

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

fn ergo_tree_to_address_local(ergo_tree_hex: &str) -> Result<String, ProtocolError> {
    ergo_tx::address::ergo_tree_to_address(ergo_tree_hex)
        .map_err(|e| ProtocolError::StateUnavailable { reason: e.to_string() })
}

fn pk_hex_to_address_local(pk_hex: &str) -> Result<String, ProtocolError> {
    let ergo_tree_hex = format!("0008cd{}", pk_hex);
    ergo_tree_to_address_local(&ergo_tree_hex)
}

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

fn parse_open_order(
    ergo_box: &ErgoBox,
    loan_token_id: &str,
    loan_token_name: &str,
    loan_token_decimals: u8,
    user_address: Option<&str>,
    oracle_erg_usd: Option<f64>,
) -> Result<OpenOrder, ProtocolError> {
    let box_id = ergo_box.box_id().to_string();

    let borrower_pk_hex = ergo_box_utils::get_register_sigma_prop_hex(ergo_box, NonMandatoryRegisterId::R4)?;
    let borrower_address = pk_hex_to_address_local(&borrower_pk_hex)?;
    let principal = ergo_box_utils::get_register_long(ergo_box, NonMandatoryRegisterId::R5)? as u64;
    let repayment = ergo_box_utils::get_register_long(ergo_box, NonMandatoryRegisterId::R6)? as u64;
    let maturity_blocks = ergo_box_utils::get_register_int(ergo_box, NonMandatoryRegisterId::R7)?;
    let collateral_erg = *ergo_box.value.as_u64();
    let collateral_tokens = extract_tokens(ergo_box);
    let interest_percent = calculator::calculate_interest_percent(principal, repayment);
    let apr = calculator::calculate_apr(interest_percent, maturity_blocks);

    let is_own = user_address.is_some_and(|ua| ua == borrower_address);

    let collateral_ratio = if loan_token_name == "ERG" {
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
        oracle_erg_usd.map(|erg_usd| {
            let collateral_usd = (collateral_erg as f64 / 1e9) * erg_usd;
            let principal_usd = principal as f64 / 1e2;
            let interest_usd = (repayment as f64 - principal as f64) / 1e2;
            calculator::calculate_collateral_ratio(collateral_usd, principal_usd, interest_usd)
        })
    } else {
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
        transaction_id: String::new(),
        output_index: 0,
    })
}

fn parse_active_bond(
    ergo_box: &ErgoBox,
    loan_token_id: &str,
    loan_token_name: &str,
    loan_token_decimals: u8,
    block_height: u32,
    user_address: Option<&str>,
) -> Result<ActiveBond, ProtocolError> {
    let box_id = ergo_box.box_id().to_string();

    let originating_order_id = ergo_box_utils::get_register_coll_byte_hex(ergo_box, NonMandatoryRegisterId::R4)?;
    let borrower_pk_hex = ergo_box_utils::get_register_sigma_prop_hex(ergo_box, NonMandatoryRegisterId::R5)?;
    let borrower_address = pk_hex_to_address_local(&borrower_pk_hex)?;
    let repayment = ergo_box_utils::get_register_long(ergo_box, NonMandatoryRegisterId::R6)? as u64;
    let maturity_height = ergo_box_utils::get_register_int(ergo_box, NonMandatoryRegisterId::R7)?;
    let lender_pk_hex = ergo_box_utils::get_register_sigma_prop_hex(ergo_box, NonMandatoryRegisterId::R8)?;
    let lender_address = pk_hex_to_address_local(&lender_pk_hex)?;
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
        transaction_id: String::new(),
        output_index: 0,
    })
}

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
