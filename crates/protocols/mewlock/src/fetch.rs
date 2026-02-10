//! MewLock box discovery via node API

use citadel_core::ProtocolError;
use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::mir::constant::Literal;
use ergo_lib::ergotree_ir::mir::value::CollKind;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::ergotree_ir::types::stype::SType;
use ergo_node_client::NodeClient;

use crate::constants::MEWLOCK_ADDRESS;
use crate::state::{LockedToken, MewLockBox, MewLockState};

/// Fetch all MewLock boxes from the node.
pub async fn fetch_mewlock_state(
    client: &NodeClient,
    user_address: Option<&str>,
    block_height: u32,
) -> Result<MewLockState, ProtocolError> {
    // Use the known P2S address directly. The constant-segregated ErgoTree
    // does not roundtrip correctly through Address::recreate_from_ergo_tree(),
    // so we must use the canonical contract address.
    let address = MEWLOCK_ADDRESS.to_string();
    let boxes = client
        .inner()
        .unspent_boxes_by_address(&address, 0, 500)
        .await
        .map_err(|e| ProtocolError::StateUnavailable {
            reason: format!("Failed to fetch MewLock boxes: {}", e),
        })?;

    let mut locks = Vec::new();

    for ergo_box in &boxes {
        match parse_mewlock_box(ergo_box, user_address, block_height) {
            Ok(lock) => locks.push(lock),
            Err(e) => {
                tracing::debug!(
                    box_id = %ergo_box.box_id(),
                    error = %e,
                    "Skipping unparseable MewLock box"
                );
            }
        }
    }

    let own_locks = locks.iter().filter(|l| l.is_own).count();

    Ok(MewLockState {
        total_locks: locks.len(),
        own_locks,
        locks,
        current_height: block_height,
    })
}

/// Parse a single MewLock ErgoBox into our domain type
fn parse_mewlock_box(
    ergo_box: &ErgoBox,
    user_address: Option<&str>,
    block_height: u32,
) -> Result<MewLockBox, ProtocolError> {
    let box_id = ergo_box.box_id().to_string();

    // R4: GroupElement (depositor public key)
    let depositor_address = extract_group_element_address(ergo_box)?;

    // R5: Int (unlock height)
    let unlock_height = extract_int(ergo_box, NonMandatoryRegisterId::R5)?;

    // R6: Optional Int (timestamp)
    let timestamp = try_extract_long(ergo_box, NonMandatoryRegisterId::R6);

    // R7: Optional Coll[Byte] (lock name)
    let lock_name = try_extract_coll_byte_utf8(ergo_box, NonMandatoryRegisterId::R7);

    // R8: Optional Coll[Byte] (lock description)
    let lock_description = try_extract_coll_byte_utf8(ergo_box, NonMandatoryRegisterId::R8);

    // Box value and tokens
    let erg_value = *ergo_box.value.as_u64();
    let tokens = extract_tokens(ergo_box);

    let is_own = user_address.is_some_and(|ua| ua == depositor_address);
    let blocks_remaining = unlock_height - block_height as i32;
    let is_unlockable = blocks_remaining <= 0 && is_own;

    Ok(MewLockBox {
        box_id,
        depositor_address,
        unlock_height,
        timestamp,
        lock_name,
        lock_description,
        erg_value,
        tokens,
        // Tx context: filled lazily when needed for building
        transaction_id: String::new(),
        output_index: 0,
        creation_height: ergo_box.creation_height as i32,
        is_own,
        is_unlockable,
        blocks_remaining,
    })
}

/// Extract GroupElement from R4, convert to P2PK address
fn extract_group_element_address(ergo_box: &ErgoBox) -> Result<String, ProtocolError> {
    let constant = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("R4 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "R4 (GroupElement) not found".to_string(),
        })?;

    // Serialize to get sigma bytes, then decode GroupElement
    let bytes = constant
        .sigma_serialize_bytes()
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Failed to serialize R4: {}", e),
        })?;

    let hex_str = hex::encode(&bytes);

    // GroupElement serializes as: 07 + 33-byte compressed point = 34 bytes = 68 hex chars
    if hex_str.len() >= 68 && hex_str.starts_with("07") {
        let pubkey_hex = &hex_str[2..68];
        // Build P2PK ErgoTree: 0008cd + 33-byte pubkey
        let ergo_tree_hex = format!("0008cd{}", pubkey_hex);
        ergo_tree_to_address(&ergo_tree_hex)
    } else {
        Err(ProtocolError::BoxParseError {
            message: format!(
                "Unexpected GroupElement encoding in R4: {}",
                &hex_str[..hex_str.len().min(20)]
            ),
        })
    }
}

/// Extract Int from a register
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

/// Try to extract a Long from a register, returns None on failure
fn try_extract_long(ergo_box: &ErgoBox, reg: NonMandatoryRegisterId) -> Option<i64> {
    ergo_box
        .additional_registers
        .get_constant(reg)
        .ok()
        .flatten()
        .and_then(|c| match &c.v {
            Literal::Long(v) => Some(*v),
            Literal::Int(v) => Some(*v as i64),
            _ => None,
        })
}

/// Try to extract Coll[Byte] from a register and decode as UTF-8
fn try_extract_coll_byte_utf8(
    ergo_box: &ErgoBox,
    reg: NonMandatoryRegisterId,
) -> Option<String> {
    let constant = ergo_box
        .additional_registers
        .get_constant(reg)
        .ok()
        .flatten()?;

    let raw_bytes = match &constant.v {
        Literal::Coll(CollKind::NativeColl(
            ergo_lib::ergotree_ir::mir::value::NativeColl::CollByte(bytes),
        )) => {
            let u8_bytes: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
            Some(u8_bytes)
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
            Some(bytes)
        }
        _ => None,
    }?;

    String::from_utf8(raw_bytes).ok()
}

/// Extract tokens from a box
fn extract_tokens(ergo_box: &ErgoBox) -> Vec<LockedToken> {
    ergo_box
        .tokens
        .as_ref()
        .map(|tokens| {
            tokens
                .iter()
                .map(|t| {
                    let tid: String = t.token_id.into();
                    LockedToken {
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

/// Convert an ErgoTree hex to an Ergo address locally
fn ergo_tree_to_address(ergo_tree_hex: &str) -> Result<String, ProtocolError> {
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
