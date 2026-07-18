//! Deterministic txId + output-box derivation for 0-conf chained transactions.
//!
//! An Ergo transaction id is the hash of the UNSIGNED transaction (spending
//! proofs are excluded), so a follow-up tx can spend this tx's outputs before
//! anything is signed or broadcast. Output candidates are built from each
//! EIP-12 output's OWN fields (including its creation_height) so the derived
//! ids match what a wallet signing the same EIP-12 JSON will compute.

use std::collections::HashMap;

use ergo_lib::chain::ergo_box::box_builder::ErgoBoxCandidateBuilder;
use ergo_lib::chain::transaction::input::UnsignedInput;
use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::chain::transaction::{DataInput, TxIoVec};
use ergo_lib::ergo_chain_types::Digest32;
use ergo_lib::ergotree_ir::chain::context_extension::ContextExtension;
use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
use ergo_lib::ergotree_ir::chain::ergo_box::{BoxId, ErgoBox, NonMandatoryRegisterId};
use ergo_lib::ergotree_ir::chain::token::{Token, TokenAmount, TokenId};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::mir::constant::Constant;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

use crate::eip12::{Eip12InputBox, Eip12Output, Eip12UnsignedTx};

/// Compute the deterministic txId of an unsigned EIP-12 tx and return its
/// outputs as spendable `Eip12InputBox`es (for building the next chained tx).
pub fn derive_output_boxes(tx: &Eip12UnsignedTx) -> Result<(String, Vec<Eip12InputBox>), String> {
    let unsigned = to_unsigned_transaction(tx)?;
    let tx_id = unsigned.id();
    let tx_id_str = tx_id.to_string();

    let mut boxes = Vec::with_capacity(tx.outputs.len());
    for (idx, candidate) in unsigned.output_candidates.iter().enumerate() {
        let ergo_box = ErgoBox::from_box_candidate(candidate, tx_id, idx as u16)
            .map_err(|e| format!("Failed to derive output box {}: {}", idx, e))?;
        boxes.push(Eip12InputBox::from_ergo_box(
            &ergo_box,
            tx_id_str.clone(),
            idx as u16,
        ));
    }

    Ok((tx_id_str, boxes))
}

fn to_unsigned_transaction(tx: &Eip12UnsignedTx) -> Result<UnsignedTransaction, String> {
    let inputs: Vec<UnsignedInput> = tx
        .inputs
        .iter()
        .map(|input| {
            let box_id = parse_box_id(&input.box_id)?;
            Ok(UnsignedInput::new(
                box_id,
                build_context_extension(&input.extension),
            ))
        })
        .collect::<Result<_, String>>()?;

    let data_inputs: Option<TxIoVec<DataInput>> = if tx.data_inputs.is_empty() {
        None
    } else {
        let dis: Vec<DataInput> = tx
            .data_inputs
            .iter()
            .map(|d| parse_box_id(&d.box_id).map(DataInput::from))
            .collect::<Result<_, String>>()?;
        Some(TxIoVec::from_vec(dis).map_err(|e| format!("Data inputs: {}", e))?)
    };

    let outputs: Vec<_> = tx
        .outputs
        .iter()
        .map(output_to_candidate)
        .collect::<Result<_, String>>()?;

    let inputs = TxIoVec::from_vec(inputs).map_err(|e| format!("Inputs: {}", e))?;
    let outputs = TxIoVec::from_vec(outputs).map_err(|e| format!("Outputs: {}", e))?;

    UnsignedTransaction::new(inputs, data_inputs, outputs).map_err(|e| e.to_string())
}

fn output_to_candidate(
    output: &Eip12Output,
) -> Result<ergo_lib::ergotree_ir::chain::ergo_box::ErgoBoxCandidate, String> {
    let value: u64 = output
        .value
        .parse()
        .map_err(|e| format!("Cannot parse output value: {}", e))?;
    let box_value = BoxValue::try_from(value).map_err(|e| format!("Invalid box value: {}", e))?;

    let tree_bytes =
        hex::decode(&output.ergo_tree).map_err(|e| format!("Invalid ergoTree hex: {}", e))?;
    let ergo_tree = ErgoTree::sigma_parse_bytes(&tree_bytes)
        .map_err(|e| format!("ErgoTree parse error: {}", e))?;

    let mut builder =
        ErgoBoxCandidateBuilder::new(box_value, ergo_tree, output.creation_height as u32);

    for asset in &output.assets {
        let token_id = parse_token_id(&asset.token_id)?;
        let amount: u64 = asset
            .amount
            .parse()
            .map_err(|e| format!("Cannot parse token amount: {}", e))?;
        let token_amount =
            TokenAmount::try_from(amount).map_err(|e| format!("Invalid token amount: {}", e))?;
        builder.add_token(Token {
            token_id,
            amount: token_amount,
        });
    }

    let reg_ids = [
        ("R4", NonMandatoryRegisterId::R4),
        ("R5", NonMandatoryRegisterId::R5),
        ("R6", NonMandatoryRegisterId::R6),
        ("R7", NonMandatoryRegisterId::R7),
        ("R8", NonMandatoryRegisterId::R8),
        ("R9", NonMandatoryRegisterId::R9),
    ];
    for (name, reg_id) in reg_ids {
        if let Some(hex_val) = output.additional_registers.get(name) {
            let bytes = hex::decode(hex_val).map_err(|e| format!("Invalid {} hex: {}", name, e))?;
            let constant = Constant::sigma_parse_bytes(&bytes)
                .map_err(|e| format!("Invalid {} constant: {}", name, e))?;
            builder.set_register_value(reg_id, constant);
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build output candidate: {}", e))
}

fn parse_box_id(hex_str: &str) -> Result<BoxId, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Invalid box id hex: {}", e))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "Box id must be 32 bytes".to_string())?;
    Ok(BoxId::from(Digest32::from(arr)))
}

fn parse_token_id(hex_str: &str) -> Result<TokenId, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Invalid token id hex: {}", e))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "Token id must be 32 bytes".to_string())?;
    Ok(TokenId::from(Digest32::from(arr)))
}

fn build_context_extension(extension: &HashMap<String, String>) -> ContextExtension {
    let mut ctx_ext = ContextExtension::empty();
    for (key, value) in extension {
        let key_num: u8 = match key.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let bytes = match hex::decode(value) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if let Ok(constant) = Constant::sigma_parse_bytes(&bytes) {
            ctx_ext.values.insert(key_num, constant);
        }
    }
    ctx_ext
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eip12::Eip12Asset;

    fn dummy_input(box_id: &str) -> Eip12InputBox {
        Eip12InputBox {
            box_id: box_id.to_string(),
            transaction_id: "00".repeat(32),
            index: 0,
            value: "1000000000".to_string(),
            ergo_tree: "0008cd0327e65711a59378c59359c3e1d0f7abe906479eccb76094e50fe79d743ccc15e6"
                .to_string(),
            assets: vec![],
            creation_height: 100,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    fn simple_tx() -> Eip12UnsignedTx {
        Eip12UnsignedTx {
            inputs: vec![dummy_input(&"11".repeat(32))],
            data_inputs: vec![],
            outputs: vec![Eip12Output {
                value: "999000000".to_string(),
                ergo_tree:
                    "0008cd0327e65711a59378c59359c3e1d0f7abe906479eccb76094e50fe79d743ccc15e6"
                        .to_string(),
                assets: vec![Eip12Asset {
                    token_id: "22".repeat(32),
                    amount: "5".to_string(),
                }],
                creation_height: 100,
                additional_registers: HashMap::new(),
            }],
        }
    }

    #[test]
    fn derives_deterministic_ids() {
        let tx = simple_tx();
        let (tx_id_a, boxes_a) = derive_output_boxes(&tx).unwrap();
        let (tx_id_b, boxes_b) = derive_output_boxes(&tx).unwrap();
        assert_eq!(tx_id_a, tx_id_b);
        assert_eq!(boxes_a[0].box_id, boxes_b[0].box_id);
        assert_eq!(boxes_a.len(), 1);
        assert_eq!(boxes_a[0].transaction_id, tx_id_a);
        assert_eq!(boxes_a[0].index, 0);
        assert_eq!(boxes_a[0].value, "999000000");
        assert_eq!(boxes_a[0].assets[0].amount, "5");
    }

    #[test]
    fn different_tx_different_ids() {
        let tx_a = simple_tx();
        let mut tx_b = simple_tx();
        tx_b.outputs[0].value = "998000000".to_string();
        let (id_a, _) = derive_output_boxes(&tx_a).unwrap();
        let (id_b, _) = derive_output_boxes(&tx_b).unwrap();
        assert_ne!(id_a, id_b);
    }
}
