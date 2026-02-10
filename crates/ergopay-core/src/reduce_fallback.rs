//! Fallback Transaction Reduction
//!
//! Constructs EIP-19 ReducedTransaction bytes manually, bypassing sigma-rust's
//! `reduce_tx` and ErgoTree parsing. This is needed when input or output boxes
//! use ErgoTree opcodes that sigma-rust cannot parse (e.g. the Phoenix HodlCoin
//! bank contract which triggers "not implemented op error: 201").
//!
//! The approach:
//! 1. Build `bytes_to_sign` by serializing the transaction wire format directly
//!    from EIP-12 data, writing ErgoTree as raw bytes (never parsed).
//! 2. For each input, determine the reduced SigmaBoolean:
//!    - P2PK inputs (0x0008cd + pubkey): ProveDlog with the public key
//!    - Complex scripts (bank boxes, etc.): TrivialProp(true)
//! 3. Assemble the EIP-19 ReducedTransaction format.

use indexmap::IndexSet;

use crate::error::ReductionError;
use ergo_tx::Eip12UnsignedTx;

// =============================================================================
// Public API
// =============================================================================

/// Build EIP-19 ReducedTransaction bytes from an EIP-12 unsigned transaction,
/// without parsing any ErgoTrees through sigma-rust.
///
/// Complex script inputs (non-P2PK) get `TrivialProp(true)` — their scripts
/// evaluate to true when spending conditions are met by the transaction structure.
/// The network verifies the actual scripts; the prover doesn't need proofs for them.
pub fn reduce_transaction_fallback(eip12_tx: &Eip12UnsignedTx) -> Result<Vec<u8>, ReductionError> {
    let bytes_to_sign = build_bytes_to_sign(eip12_tx)?;

    let mut out = Vec::with_capacity(bytes_to_sign.len() + 256);

    // EIP-19 format:
    // 1. VLQ(u32) length of bytes_to_sign
    vlq_put_u64(&mut out, bytes_to_sign.len() as u64);
    // 2. Raw bytes_to_sign
    out.extend_from_slice(&bytes_to_sign);

    // 3. For each input: SigmaBoolean + VLQ(u64) cost
    for input in &eip12_tx.inputs {
        let tree_bytes = hex::decode(&input.ergo_tree).map_err(|e| {
            ReductionError::InvalidErgoTree(format!("Invalid input ErgoTree hex: {}", e))
        })?;

        write_sigma_boolean_for_tree(&mut out, &tree_bytes);
        vlq_put_u64(&mut out, 0); // cost = 0
    }

    // 4. VLQ(u32) tx_cost = 0
    vlq_put_u64(&mut out, 0);

    Ok(out)
}

// =============================================================================
// bytes_to_sign Construction
// =============================================================================

/// Build the `bytes_to_sign` for an unsigned transaction.
///
/// This is the standard Ergo transaction wire format with empty spending proofs,
/// matching `Transaction::sigma_serialize` in sigma-rust.
fn build_bytes_to_sign(tx: &Eip12UnsignedTx) -> Result<Vec<u8>, ReductionError> {
    let mut w = Vec::with_capacity(4096);

    // -- Inputs --
    vlq_put_u64(&mut w, tx.inputs.len() as u64); // VLQ(u16)
    for input in &tx.inputs {
        // BoxId: 32 raw bytes
        let box_id = decode_hex_32(&input.box_id, "box_id")?;
        w.extend_from_slice(&box_id);

        // SpendingProof: empty (proof_len = 0)
        vlq_put_u64(&mut w, 0); // VLQ(u16) proof_len = 0

        // ContextExtension
        write_context_extension(&mut w, &input.extension)?;
    }

    // -- Data Inputs --
    vlq_put_u64(&mut w, tx.data_inputs.len() as u64); // VLQ(u16)
    for di in &tx.data_inputs {
        let box_id = decode_hex_32(&di.box_id, "data_input box_id")?;
        w.extend_from_slice(&box_id);
    }

    // -- Distinct Token IDs table --
    let distinct_tokens = collect_distinct_token_ids(tx);
    vlq_put_u64(&mut w, distinct_tokens.len() as u64); // VLQ(u32)
    for token_id_hex in &distinct_tokens {
        let token_id = decode_hex_32(token_id_hex, "token_id")?;
        w.extend_from_slice(&token_id);
    }

    // -- Outputs --
    vlq_put_u64(&mut w, tx.outputs.len() as u64); // VLQ(u16)
    for output in &tx.outputs {
        write_output_candidate(&mut w, output, &distinct_tokens)?;
    }

    Ok(w)
}

/// Write an output candidate (ErgoBoxCandidate) in the indexed-digest format.
fn write_output_candidate(
    w: &mut Vec<u8>,
    output: &ergo_tx::Eip12Output,
    distinct_tokens: &IndexSet<String>,
) -> Result<(), ReductionError> {
    // Value: VLQ(u64) nanoERGs
    let value: u64 = output
        .value
        .parse()
        .map_err(|e| ReductionError::InvalidValue(format!("output value: {}", e)))?;
    vlq_put_u64(w, value);

    // ErgoTree: raw bytes, NO length prefix (self-describing)
    let tree_bytes = hex::decode(&output.ergo_tree).map_err(|e| {
        ReductionError::InvalidErgoTree(format!("Invalid output ErgoTree hex: {}", e))
    })?;
    w.extend_from_slice(&tree_bytes);

    // CreationHeight: VLQ(u32)
    vlq_put_u64(w, output.creation_height as u64);

    // Tokens: u8 count + (VLQ(u32) index, VLQ(u64) amount) per token
    w.push(output.assets.len() as u8);
    for asset in &output.assets {
        let idx = distinct_tokens
            .get_index_of(&asset.token_id)
            .ok_or_else(|| {
                ReductionError::InvalidToken(format!(
                    "Token {} not in distinct table",
                    asset.token_id
                ))
            })?;
        vlq_put_u64(w, idx as u64); // VLQ(u32) index

        let amount: u64 = asset
            .amount
            .parse()
            .map_err(|e| ReductionError::InvalidToken(format!("token amount: {}", e)))?;
        vlq_put_u64(w, amount); // VLQ(u64) amount
    }

    // Additional registers: u8 count + raw Constant bytes for each
    write_registers(w, &output.additional_registers)?;

    Ok(())
}

/// Write context extension from EIP-12 extension map.
///
/// Format: u8 count + (u8 var_id, Constant bytes)* sorted by key.
fn write_context_extension(
    w: &mut Vec<u8>,
    extension: &std::collections::HashMap<String, String>,
) -> Result<(), ReductionError> {
    // Sort by key to match sigma-rust's BTreeMap ordering
    let mut entries: Vec<(u8, Vec<u8>)> = Vec::new();
    for (key, value) in extension {
        let key_num: u8 = key.parse().map_err(|_| {
            ReductionError::TransactionError(format!("Invalid extension key: {}", key))
        })?;
        let bytes = hex::decode(value).map_err(|e| {
            ReductionError::TransactionError(format!(
                "Invalid extension hex for key {}: {}",
                key, e
            ))
        })?;
        entries.push((key_num, bytes));
    }
    entries.sort_by_key(|(k, _)| *k);

    w.push(entries.len() as u8);
    for (key_num, bytes) in entries {
        w.push(key_num);
        w.extend_from_slice(&bytes); // Constant is self-delimiting
    }

    Ok(())
}

/// Write additional registers (R4-R9) in order.
///
/// Format: u8 count + raw Constant bytes for each register in order.
fn write_registers(
    w: &mut Vec<u8>,
    registers: &std::collections::HashMap<String, String>,
) -> Result<(), ReductionError> {
    // Registers must be densely packed starting from R4
    let reg_names = ["R4", "R5", "R6", "R7", "R8", "R9"];
    let mut count = 0u8;
    let mut reg_bytes: Vec<Vec<u8>> = Vec::new();

    for name in &reg_names {
        if let Some(hex_val) = registers.get(*name) {
            let bytes = hex::decode(hex_val).map_err(|e| {
                ReductionError::InvalidRegister(format!("{}: invalid hex: {}", name, e))
            })?;
            reg_bytes.push(bytes);
            count += 1;
        } else {
            break; // Registers must be dense; stop at first gap
        }
    }

    w.push(count);
    for bytes in reg_bytes {
        w.extend_from_slice(&bytes);
    }

    Ok(())
}

// =============================================================================
// SigmaBoolean for Inputs
// =============================================================================

/// P2PK ErgoTree prefix: header(0x00) + SigmaPropConstant type(0x08) + ProveDlog(0xCD)
const P2PK_PREFIX: [u8; 3] = [0x00, 0x08, 0xCD];
/// P2PK ErgoTree total length: 3 bytes prefix + 33 bytes compressed EC point
const P2PK_TREE_LEN: usize = 36;

/// SigmaBoolean opcode for ProveDlog
const SIGMA_PROVE_DLOG: u8 = 0xCD;
/// SigmaBoolean opcode for TrivialProp(true)
const SIGMA_TRIVIAL_TRUE: u8 = 0xD3;

/// Write the SigmaBoolean for an input's ErgoTree.
fn write_sigma_boolean_for_tree(w: &mut Vec<u8>, ergo_tree_bytes: &[u8]) {
    if ergo_tree_bytes.len() == P2PK_TREE_LEN && ergo_tree_bytes[..3] == P2PK_PREFIX {
        // P2PK: ProveDlog opcode + 33 bytes compressed public key
        w.push(SIGMA_PROVE_DLOG);
        w.extend_from_slice(&ergo_tree_bytes[3..P2PK_TREE_LEN]);
    } else {
        // Complex script: TrivialProp(true)
        w.push(SIGMA_TRIVIAL_TRUE);
    }
}

// =============================================================================
// Distinct Token IDs
// =============================================================================

/// Collect distinct token IDs from all outputs, preserving insertion order.
fn collect_distinct_token_ids(tx: &Eip12UnsignedTx) -> IndexSet<String> {
    let mut set = IndexSet::new();
    for output in &tx.outputs {
        for asset in &output.assets {
            set.insert(asset.token_id.clone());
        }
    }
    set
}

// =============================================================================
// VLQ Encoding (unsigned, protobuf-style little-endian base-128)
// =============================================================================

/// Encode a u64 value as unsigned VLQ and append to the buffer.
fn vlq_put_u64(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        if (value & !0x7F) == 0 {
            buf.push(value as u8);
            break;
        } else {
            buf.push(((value & 0x7F) | 0x80) as u8);
            value >>= 7;
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Decode a 32-byte hex string (box ID, token ID, etc.)
fn decode_hex_32(hex_str: &str, label: &str) -> Result<[u8; 32], ReductionError> {
    let bytes =
        hex::decode(hex_str).map_err(|e| ReductionError::InvalidBoxId(format!("{label}: {e}")))?;
    bytes
        .try_into()
        .map_err(|_| ReductionError::InvalidBoxId(format!("{label}: expected 32 bytes")))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vlq_encoding() {
        let mut buf = Vec::new();

        // 0 -> single byte 0x00
        vlq_put_u64(&mut buf, 0);
        assert_eq!(buf, [0x00]);

        // 1 -> single byte 0x01
        buf.clear();
        vlq_put_u64(&mut buf, 1);
        assert_eq!(buf, [0x01]);

        // 127 -> single byte 0x7F
        buf.clear();
        vlq_put_u64(&mut buf, 127);
        assert_eq!(buf, [0x7F]);

        // 128 -> two bytes: 0x80 0x01
        buf.clear();
        vlq_put_u64(&mut buf, 128);
        assert_eq!(buf, [0x80, 0x01]);

        // 300 -> 0xAC 0x02
        buf.clear();
        vlq_put_u64(&mut buf, 300);
        assert_eq!(buf, [0xAC, 0x02]);
    }

    #[test]
    fn test_p2pk_detection() {
        // Standard P2PK: 0008cd + 33 bytes pubkey
        let mut tree = vec![0x00, 0x08, 0xCD];
        tree.extend_from_slice(&[0x02; 33]); // dummy 33-byte pubkey
        assert_eq!(tree.len(), P2PK_TREE_LEN);
        assert!(tree[..3] == P2PK_PREFIX);

        // Complex script (bank box): starts with 0x10
        let bank_tree = hex::decode("100a0402").unwrap();
        assert!(bank_tree[..3] != P2PK_PREFIX);
    }

    #[test]
    fn test_sigma_boolean_trivial_true() {
        let mut buf = Vec::new();
        let bank_tree = hex::decode("100a0402040004020400040005020500").unwrap();
        write_sigma_boolean_for_tree(&mut buf, &bank_tree);
        assert_eq!(buf, [SIGMA_TRIVIAL_TRUE]); // 0xD3
    }

    #[test]
    fn test_sigma_boolean_prove_dlog() {
        let mut buf = Vec::new();
        // Standard P2PK tree
        let mut tree = vec![0x00, 0x08, 0xCD];
        let pubkey = [0x02u8; 33]; // dummy compressed pubkey
        tree.extend_from_slice(&pubkey);

        write_sigma_boolean_for_tree(&mut buf, &tree);
        assert_eq!(buf.len(), 34); // 1 byte opcode + 33 bytes pubkey
        assert_eq!(buf[0], SIGMA_PROVE_DLOG); // 0xCD
        assert_eq!(&buf[1..], &pubkey);
    }

    #[test]
    fn test_distinct_token_ids() {
        let tx = Eip12UnsignedTx {
            inputs: vec![],
            data_inputs: vec![],
            outputs: vec![
                ergo_tx::Eip12Output {
                    value: "1000000".into(),
                    ergo_tree: "0008cd".into(),
                    assets: vec![
                        ergo_tx::Eip12Asset {
                            token_id: "aaa".into(),
                            amount: "1".into(),
                        },
                        ergo_tx::Eip12Asset {
                            token_id: "bbb".into(),
                            amount: "2".into(),
                        },
                    ],
                    creation_height: 100,
                    additional_registers: Default::default(),
                },
                ergo_tx::Eip12Output {
                    value: "1000000".into(),
                    ergo_tree: "0008cd".into(),
                    assets: vec![ergo_tx::Eip12Asset {
                        token_id: "aaa".into(), // duplicate
                        amount: "3".into(),
                    }],
                    creation_height: 100,
                    additional_registers: Default::default(),
                },
            ],
        };

        let distinct = collect_distinct_token_ids(&tx);
        assert_eq!(distinct.len(), 2);
        assert_eq!(distinct.get_index(0), Some(&"aaa".to_string()));
        assert_eq!(distinct.get_index(1), Some(&"bbb".to_string()));
    }
}
