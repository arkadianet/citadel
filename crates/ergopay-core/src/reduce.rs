//! Transaction Reduction for ErgoPay
//!
//! Converts EIP-12 format transactions to sigma-serialized ReducedTransaction bytes
//! for use with ErgoPay protocol (mobile wallet signing).

use std::collections::HashMap;

use ergo_lib::chain::ergo_box::box_builder::ErgoBoxCandidateBuilder;
use ergo_lib::chain::ergo_state_context::ErgoStateContext;
use ergo_lib::chain::transaction::input::UnsignedInput;
use ergo_lib::chain::transaction::reduced::reduce_tx;
use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::chain::transaction::DataInput;
use ergo_lib::chain::transaction::TxIoVec;
use ergo_lib::ergotree_ir::chain::context_extension::ContextExtension;
use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::chain::token::{Token, TokenAmount, TokenId};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::mir::constant::Constant;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::wallet::tx_context::TransactionContext;
use ergo_node_client::NodeClient;
use ergo_tx::{Eip12Output, Eip12UnsignedTx};

use crate::error::ReductionError;

/// Reduce an EIP-12 transaction to sigma-serialized bytes for ErgoPay
///
/// This is the main entry point for transaction reduction. It fetches the
/// current state context from the node and performs the reduction.
///
/// # Arguments
/// * `eip12_tx` - The EIP-12 format unsigned transaction
/// * `input_boxes` - The actual ErgoBox instances for all inputs (in same order as tx inputs)
/// * `data_input_boxes` - The actual ErgoBox instances for data inputs
/// * `client` - Node client for fetching state context
///
/// # Returns
/// Sigma-serialized bytes of the ReducedTransaction
pub async fn reduce_transaction(
    eip12_tx: &Eip12UnsignedTx,
    input_boxes: Vec<ErgoBox>,
    data_input_boxes: Vec<ErgoBox>,
    client: &NodeClient,
) -> Result<Vec<u8>, ReductionError> {
    // Fetch current blockchain state context
    let state_context = client
        .inner()
        .get_state_context()
        .await
        .map_err(|e| ReductionError::StateContextError(e.to_string()))?;

    reduce_transaction_with_context(eip12_tx, input_boxes, data_input_boxes, &state_context)
}

/// Reduce transaction with pre-fetched state context
///
/// Use this when you have already fetched the state context (e.g., for batch operations
/// or testing with a mock context).
///
/// # Arguments
/// * `eip12_tx` - The EIP-12 format unsigned transaction
/// * `input_boxes` - The actual ErgoBox instances for all inputs (in same order as tx inputs)
/// * `data_input_boxes` - The actual ErgoBox instances for data inputs
/// * `state_context` - Pre-fetched blockchain state context
///
/// # Returns
/// Sigma-serialized bytes of the ReducedTransaction
pub fn reduce_transaction_with_context(
    eip12_tx: &Eip12UnsignedTx,
    input_boxes: Vec<ErgoBox>,
    data_input_boxes: Vec<ErgoBox>,
    state_context: &ErgoStateContext,
) -> Result<Vec<u8>, ReductionError> {
    // Get current height from state context for output creation
    let current_height = state_context.pre_header.height;

    // Convert outputs to ErgoBoxCandidate using the builder pattern
    let output_candidates: Vec<_> = eip12_tx
        .outputs
        .iter()
        .map(|output| convert_output_to_candidate(output, current_height))
        .collect::<Result<Vec<_>, ReductionError>>()?;

    // Build data inputs from data input boxes
    let data_inputs: Option<TxIoVec<DataInput>> = if data_input_boxes.is_empty() {
        None
    } else {
        let dis: Vec<DataInput> = data_input_boxes
            .iter()
            .map(|b| DataInput::from(b.box_id()))
            .collect();
        Some(
            TxIoVec::from_vec(dis)
                .map_err(|e| ReductionError::TransactionError(format!("Data inputs: {}", e)))?,
        )
    };

    // Build unsigned inputs from input boxes with context extensions
    // UnsignedInput requires BoxId and ContextExtension
    let unsigned_inputs: Vec<UnsignedInput> = input_boxes
        .iter()
        .zip(eip12_tx.inputs.iter())
        .map(|(b, eip12_input)| {
            let ctx_ext = build_context_extension(&eip12_input.extension);
            UnsignedInput::new(b.box_id(), ctx_ext)
        })
        .collect();

    let inputs = TxIoVec::from_vec(unsigned_inputs)
        .map_err(|e| ReductionError::TransactionError(format!("Inputs: {}", e)))?;

    // Build output candidates vector
    let outputs = TxIoVec::from_vec(output_candidates)
        .map_err(|e| ReductionError::TransactionError(format!("Outputs: {}", e)))?;

    // Create UnsignedTransaction
    let unsigned_tx = UnsignedTransaction::new(inputs, data_inputs, outputs)
        .map_err(|e| ReductionError::TransactionError(e.to_string()))?;

    // Create TransactionContext
    let tx_context = TransactionContext::new(unsigned_tx, input_boxes, data_input_boxes)
        .map_err(|e| ReductionError::TransactionError(e.to_string()))?;

    // Reduce the transaction
    let reduced_tx = reduce_tx(tx_context, state_context)
        .map_err(|e| ReductionError::ReductionFailed(e.to_string()))?;

    // Sigma-serialize
    let bytes = reduced_tx
        .sigma_serialize_bytes()
        .map_err(|e| ReductionError::SerializationError(e.to_string()))?;

    Ok(bytes)
}

/// Parse a hex string to BoxId (used only by tests)
#[cfg(test)]
fn parse_box_id(
    hex_str: &str,
) -> Result<ergo_lib::ergotree_ir::chain::ergo_box::BoxId, ReductionError> {
    use ergo_lib::ergotree_ir::chain::ergo_box::BoxId;

    let bytes = hex::decode(hex_str)
        .map_err(|e| ReductionError::InvalidBoxId(format!("Invalid hex: {}", e)))?;

    let arr: [u8; 32] = bytes.try_into().map_err(|_| {
        ReductionError::InvalidBoxId("Box ID must be 32 bytes (64 hex chars)".to_string())
    })?;

    Ok(BoxId::from(ergo_chain_types::Digest32::from(arr)))
}

/// Convert EIP-12 output to ErgoBoxCandidate using the builder pattern
fn convert_output_to_candidate(
    output: &Eip12Output,
    creation_height: u32,
) -> Result<ergo_lib::ergotree_ir::chain::ergo_box::ErgoBoxCandidate, ReductionError> {
    // Parse value
    let value: u64 = output
        .value
        .parse()
        .map_err(|e| ReductionError::InvalidValue(format!("Cannot parse value: {}", e)))?;
    let box_value = BoxValue::try_from(value)
        .map_err(|e| ReductionError::InvalidValue(format!("Invalid box value: {}", e)))?;

    // Parse ErgoTree
    let ergo_tree_bytes = hex::decode(&output.ergo_tree)
        .map_err(|e| ReductionError::InvalidErgoTree(format!("Invalid hex: {}", e)))?;
    let ergo_tree = ErgoTree::sigma_parse_bytes(&ergo_tree_bytes)
        .map_err(|e| ReductionError::InvalidErgoTree(format!("Parse error: {}", e)))?;

    // Build using ErgoBoxCandidateBuilder
    let mut builder = ErgoBoxCandidateBuilder::new(box_value, ergo_tree, creation_height);

    // Add tokens
    for asset in &output.assets {
        let token_id = parse_token_id(&asset.token_id)?;
        let amount: u64 = asset
            .amount
            .parse()
            .map_err(|e| ReductionError::InvalidToken(format!("Cannot parse amount: {}", e)))?;
        let token_amount = TokenAmount::try_from(amount)
            .map_err(|e| ReductionError::InvalidToken(format!("Invalid amount: {}", e)))?;

        builder.add_token(Token {
            token_id,
            amount: token_amount,
        });
    }

    // Add registers
    add_registers_to_builder(&mut builder, &output.additional_registers)?;

    // Build the candidate
    builder
        .build()
        .map_err(|e| ReductionError::TransactionError(format!("Failed to build candidate: {}", e)))
}

/// Parse a hex string to TokenId
fn parse_token_id(hex_str: &str) -> Result<TokenId, ReductionError> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| ReductionError::InvalidToken(format!("Invalid hex: {}", e)))?;

    let arr: [u8; 32] = bytes.try_into().map_err(|_| {
        ReductionError::InvalidToken("Token ID must be 32 bytes (64 hex chars)".to_string())
    })?;

    // TokenId is created from a Digest32
    Ok(TokenId::from(ergo_chain_types::Digest32::from(arr)))
}

/// Build ContextExtension from EIP-12 extension map
///
/// The extension map has string keys ("0", "1", etc.) mapping to hex-encoded sigma-serialized constants.
fn build_context_extension(extension: &HashMap<String, String>) -> ContextExtension {
    if extension.is_empty() {
        return ContextExtension::empty();
    }

    // Start with an empty context extension and add values
    let mut ctx_ext = ContextExtension::empty();

    for (key, value) in extension {
        // Parse key as u8
        let key_num: u8 = match key.parse() {
            Ok(n) => n,
            Err(_) => continue, // Skip invalid keys
        };

        // Parse hex value as Constant
        let bytes = match hex::decode(value) {
            Ok(b) => b,
            Err(_) => continue, // Skip invalid hex
        };

        let constant = match Constant::sigma_parse_bytes(&bytes) {
            Ok(c) => c,
            Err(_) => continue, // Skip invalid constants
        };

        ctx_ext.values.insert(key_num, constant);
    }

    ctx_ext
}

/// Add registers to the builder from a string map
fn add_registers_to_builder(
    builder: &mut ErgoBoxCandidateBuilder,
    registers: &HashMap<String, String>,
) -> Result<(), ReductionError> {
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;

    // Process registers in order (R4-R9)
    let reg_ids = [
        ("R4", NonMandatoryRegisterId::R4),
        ("R5", NonMandatoryRegisterId::R5),
        ("R6", NonMandatoryRegisterId::R6),
        ("R7", NonMandatoryRegisterId::R7),
        ("R8", NonMandatoryRegisterId::R8),
        ("R9", NonMandatoryRegisterId::R9),
    ];

    for (name, reg_id) in reg_ids {
        if let Some(value) = registers.get(name) {
            let bytes = hex::decode(value).map_err(|e| {
                ReductionError::InvalidRegister(format!("{}: invalid hex: {}", name, e))
            })?;
            let constant = Constant::sigma_parse_bytes(&bytes).map_err(|e| {
                ReductionError::InvalidRegister(format!("{}: parse error: {}", name, e))
            })?;

            builder.set_register_value(reg_id, constant);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_box_id_valid() {
        // Valid 32-byte hex (64 chars)
        let hex = "0".repeat(64);
        assert!(parse_box_id(&hex).is_ok());
    }

    #[test]
    fn test_parse_box_id_invalid_hex() {
        assert!(parse_box_id("invalid").is_err());
    }

    #[test]
    fn test_parse_box_id_wrong_length() {
        // Too short
        assert!(parse_box_id("abc").is_err());
        // Too long
        let hex = "0".repeat(66);
        assert!(parse_box_id(&hex).is_err());
    }

    #[test]
    fn test_parse_token_id_valid() {
        let hex = "a".repeat(64);
        assert!(parse_token_id(&hex).is_ok());
    }

    #[test]
    fn test_parse_token_id_invalid() {
        assert!(parse_token_id("not_valid_hex").is_err());
        assert!(parse_token_id("abc").is_err());
    }
}
