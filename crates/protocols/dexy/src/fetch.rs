//! Dexy State Fetching from Node
//!
//! Fetches bank, oracle, and LP boxes from node and parses into protocol state.

use citadel_core::{ProtocolError, TokenId};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_node_client::{NodeCapabilities, NodeClient};
use ergo_tx::ergo_box_utils::{extract_int, extract_long, find_token_amount, map_node_error};
use ergo_tx::{Eip12DataInputBox, Eip12InputBox};

use crate::constants::DexyIds;
use crate::state::{
    DexyBankBoxData, DexyFreeMintBoxData, DexyLpBoxData, DexyOracleBoxData, DexyState,
};

/// Fetch Dexy protocol state from node
///
/// Fetches the bank, oracle, LP, and FreeMint boxes and returns the complete Dexy state.
pub async fn fetch_dexy_state(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    ids: &DexyIds,
) -> Result<DexyState, ProtocolError> {
    // Fetch bank box
    let bank_token_id = TokenId::new(&ids.bank_nft);
    let bank_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &bank_token_id,
    )
    .await
    .map_err(|e| map_node_error(e, "Dexy", "Bank box"))?;

    // Fetch oracle box
    let oracle_token_id = TokenId::new(&ids.oracle_pool_nft);
    let oracle_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &oracle_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Oracle box not found: {}", e),
    })?;

    // Fetch LP box
    let lp_token_id = TokenId::new(&ids.lp_nft);
    let lp_box =
        ergo_node_client::queries::get_box_by_token_id(client.inner(), capabilities, &lp_token_id)
            .await
            .map_err(|e| ProtocolError::BoxParseError {
                message: format!("LP box not found: {}", e),
            })?;

    // Fetch FreeMint box
    let free_mint_token_id = TokenId::new(&ids.free_mint_nft);
    let free_mint_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &free_mint_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("FreeMint box not found: {}", e),
    })?;

    // Get current blockchain height
    let current_height = capabilities.chain_height as i32;

    // Get total token supply
    let token_info =
        client
            .get_token_info(&ids.dexy_token)
            .await
            .map_err(|e| ProtocolError::BoxParseError {
                message: format!("Failed to get Dexy token info: {}", e),
            })?;
    let total_supply = token_info.emission_amount.unwrap_or(0);

    // Parse boxes
    let bank_data = parse_bank_box(&bank_box, ids)?;
    let oracle_data = parse_oracle_box(&oracle_box)?;
    let lp_data = parse_lp_box(&lp_box, ids)?;
    let free_mint_data = parse_free_mint_box_data(&free_mint_box)?;

    // Build state
    Ok(DexyState::from_boxes(
        ids.variant,
        &bank_data,
        &oracle_data,
        &lp_data,
        &free_mint_data,
        &ids.dexy_token,
        current_height,
        total_supply,
    ))
}

/// Parse bank box into DexyBankBoxData
///
/// Extracts ERG value, Dexy token count, and ErgoTree from the bank box.
pub fn parse_bank_box(ergo_box: &ErgoBox, ids: &DexyIds) -> Result<DexyBankBoxData, ProtocolError> {
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let box_id = ergo_box.box_id().to_string();
    let erg_value = ergo_box.value.as_i64();

    // Serialize ErgoTree to hex
    let ergo_tree = ergo_box
        .ergo_tree
        .sigma_serialize_bytes()
        .map(|bytes| base16::encode_lower(&bytes))
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Failed to serialize bank ErgoTree: {}", e),
        })?;

    // Find Dexy token amount in bank box
    let dexy_tokens = find_token_amount(ergo_box, &ids.dexy_token)
        .map(|v| v as i64)
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Token {} not found in box", ids.dexy_token),
        })?;

    Ok(DexyBankBoxData {
        box_id,
        erg_value,
        dexy_tokens,
        ergo_tree,
    })
}

/// Parse oracle box into DexyOracleBoxData
///
/// Extracts the oracle rate from register R4.
pub fn parse_oracle_box(ergo_box: &ErgoBox) -> Result<DexyOracleBoxData, ProtocolError> {
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;

    let box_id = ergo_box.box_id().to_string();

    // Get R4 register (oracle rate)
    let r4 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Oracle box R4 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "Oracle box missing R4 register".to_string(),
        })?;

    let rate_nano = extract_long(&r4).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse oracle R4 (rate): {}", e),
    })?;

    Ok(DexyOracleBoxData { box_id, rate_nano })
}

/// Parse LP box into DexyLpBoxData
///
/// Extracts ERG reserves and Dexy token reserves from the LP box.
pub fn parse_lp_box(ergo_box: &ErgoBox, ids: &DexyIds) -> Result<DexyLpBoxData, ProtocolError> {
    let box_id = ergo_box.box_id().to_string();

    // ERG reserves = box value
    let erg_reserves = ergo_box.value.as_i64();

    // Dexy reserves = amount of Dexy token in the box
    let dexy_reserves = find_token_amount(ergo_box, &ids.dexy_token)
        .map(|v| v as i64)
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Token {} not found in box", ids.dexy_token),
        })?;

    Ok(DexyLpBoxData {
        box_id,
        erg_reserves,
        dexy_reserves,
    })
}

/// Parse FreeMint box into DexyFreeMintBoxData
///
/// Extracts reset height (R4) and available tokens (R5) from the FreeMint box.
pub fn parse_free_mint_box_data(ergo_box: &ErgoBox) -> Result<DexyFreeMintBoxData, ProtocolError> {
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;

    let box_id = ergo_box.box_id().to_string();

    // Get R4 (Int - height at which counter resets)
    let r4 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("FreeMint R4 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "FreeMint box missing R4 register".to_string(),
        })?;

    let reset_height = extract_int(&r4).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse FreeMint R4 (reset_height): {}", e),
    })?;

    // Get R5 (Long - remaining tokens available)
    let r5 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R5)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("FreeMint R5 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "FreeMint box missing R5 register".to_string(),
        })?;

    let available = extract_long(&r5).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse FreeMint R5 (available): {}", e),
    })?;

    Ok(DexyFreeMintBoxData {
        box_id,
        reset_height,
        available,
    })
}

/// Collect all tokens from an ErgoBox as (token_id_hex, amount) pairs
fn collect_box_tokens(ergo_box: &ErgoBox) -> Vec<(String, u64)> {
    ergo_box
        .tokens
        .as_ref()
        .map(|tokens| {
            tokens
                .iter()
                .map(|t| {
                    let tid: String = t.token_id.into();
                    (tid, *t.amount.as_u64())
                })
                .collect()
        })
        .unwrap_or_default()
}

// =============================================================================
// Transaction Context Fetching
// =============================================================================

/// Context needed for building Dexy FreeMint transactions
///
/// Contains all boxes needed for the FreeMint transaction:
/// - Inputs: FreeMint, Bank, Buyback
/// - Data Inputs: Oracle, LP
#[derive(Debug, Clone)]
pub struct DexyTxContext {
    // FreeMint box (Input 0)
    /// FreeMint box as EIP-12 input
    pub free_mint_input: Eip12InputBox,
    /// FreeMint box ERG value
    pub free_mint_erg_nano: i64,
    /// FreeMint box ErgoTree (hex)
    pub free_mint_ergo_tree: String,
    /// FreeMint R4: height at which counter resets
    pub free_mint_r4_height: i32,
    /// FreeMint R5: remaining tokens available this period
    pub free_mint_r5_available: i64,
    /// Raw FreeMint box
    pub free_mint_box: ErgoBox,

    // Bank box (Input 1)
    /// Bank box as EIP-12 input
    pub bank_input: Eip12InputBox,
    /// Bank ERG value in nanoERG
    pub bank_erg_nano: i64,
    /// Dexy tokens available in bank
    pub dexy_in_bank: i64,
    /// Bank ErgoTree (hex)
    pub bank_ergo_tree: String,
    /// Raw bank box
    pub bank_box: ErgoBox,

    // Buyback box (Input 2)
    /// Buyback box as EIP-12 input
    pub buyback_input: Eip12InputBox,
    /// Buyback box ERG value
    pub buyback_erg_nano: i64,
    /// Buyback box ErgoTree (hex)
    pub buyback_ergo_tree: String,
    /// Raw Buyback box
    pub buyback_box: ErgoBox,

    // Data inputs
    /// Oracle box as EIP-12 data input
    pub oracle_data_input: Eip12DataInputBox,
    /// Oracle rate: nanoERG per USD (raw, before decimal adjustment)
    pub oracle_rate_nano: i64,
    /// Raw oracle box
    pub oracle_box: ErgoBox,

    /// LP box as EIP-12 data input
    pub lp_data_input: Eip12DataInputBox,
    /// LP ERG reserves
    pub lp_erg_reserves: i64,
    /// LP Dexy reserves
    pub lp_dexy_reserves: i64,
    /// Raw LP box
    pub lp_box: ErgoBox,
}

/// Parse FreeMint box data
fn parse_free_mint_box(ergo_box: &ErgoBox) -> Result<(String, i32, i64), ProtocolError> {
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    // Get ErgoTree
    let ergo_tree = ergo_box
        .ergo_tree
        .sigma_serialize_bytes()
        .map(|bytes| base16::encode_lower(&bytes))
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Failed to serialize FreeMint ErgoTree: {}", e),
        })?;

    // Get R4 (Int - height at which counter resets)
    let r4 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("FreeMint R4 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "FreeMint box missing R4 register".to_string(),
        })?;

    let r4_height = extract_int(&r4).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse FreeMint R4: {}", e),
    })?;

    // Get R5 (Long - remaining tokens available)
    let r5 = ergo_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R5)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("FreeMint R5 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "FreeMint box missing R5 register".to_string(),
        })?;

    let r5_available = extract_long(&r5).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse FreeMint R5: {}", e),
    })?;

    Ok((ergo_tree, r4_height, r5_available))
}

/// Parse Buyback box data
fn parse_buyback_box(ergo_box: &ErgoBox) -> Result<String, ProtocolError> {
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let ergo_tree = ergo_box
        .ergo_tree
        .sigma_serialize_bytes()
        .map(|bytes| base16::encode_lower(&bytes))
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Failed to serialize Buyback ErgoTree: {}", e),
        })?;

    Ok(ergo_tree)
}

/// Fetch all boxes needed for FreeMint transaction
///
/// Fetches: FreeMint, Bank, Buyback (inputs), Oracle, LP (data inputs)
pub async fn fetch_tx_context(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    ids: &DexyIds,
) -> Result<DexyTxContext, ProtocolError> {
    // Fetch FreeMint box
    let free_mint_token_id = TokenId::new(&ids.free_mint_nft);
    let free_mint_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &free_mint_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("FreeMint box not found: {}", e),
    })?;

    // Fetch bank box
    let bank_token_id = TokenId::new(&ids.bank_nft);
    let bank_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &bank_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Bank box not found: {}", e),
    })?;

    // Fetch Buyback box
    let buyback_token_id = TokenId::new(&ids.buyback_nft);
    let buyback_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &buyback_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Buyback box not found: {}", e),
    })?;

    // Fetch oracle box
    let oracle_token_id = TokenId::new(&ids.oracle_pool_nft);
    let oracle_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &oracle_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Oracle box not found: {}", e),
    })?;

    // Fetch LP box
    let lp_token_id = TokenId::new(&ids.lp_nft);
    let lp_box =
        ergo_node_client::queries::get_box_by_token_id(client.inner(), capabilities, &lp_token_id)
            .await
            .map_err(|e| ProtocolError::BoxParseError {
                message: format!("LP box not found: {}", e),
            })?;

    // Parse box data
    let (free_mint_ergo_tree, free_mint_r4_height, free_mint_r5_available) =
        parse_free_mint_box(&free_mint_box)?;
    let bank_data = parse_bank_box(&bank_box, ids)?;
    let buyback_ergo_tree = parse_buyback_box(&buyback_box)?;
    let oracle_data = parse_oracle_box(&oracle_box)?;
    let lp_data = parse_lp_box(&lp_box, ids)?;

    // Get transaction context for all boxes
    let free_mint_tx_info = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &free_mint_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;
    let bank_tx_info = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &bank_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;
    let buyback_tx_info = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &buyback_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;
    let oracle_tx_info = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &oracle_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;
    let lp_tx_info = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &lp_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;

    // Convert to EIP-12 format
    let free_mint_input =
        Eip12InputBox::from_ergo_box(&free_mint_box, free_mint_tx_info.0, free_mint_tx_info.1);
    let bank_input = Eip12InputBox::from_ergo_box(&bank_box, bank_tx_info.0, bank_tx_info.1);
    let buyback_input =
        Eip12InputBox::from_ergo_box(&buyback_box, buyback_tx_info.0, buyback_tx_info.1);
    let oracle_data_input =
        Eip12DataInputBox::from_ergo_box(&oracle_box, oracle_tx_info.0, oracle_tx_info.1);
    let lp_data_input = Eip12DataInputBox::from_ergo_box(&lp_box, lp_tx_info.0, lp_tx_info.1);

    Ok(DexyTxContext {
        // FreeMint
        free_mint_input,
        free_mint_erg_nano: free_mint_box.value.as_i64(),
        free_mint_ergo_tree,
        free_mint_r4_height,
        free_mint_r5_available,
        free_mint_box,
        // Bank
        bank_input,
        bank_erg_nano: bank_data.erg_value,
        dexy_in_bank: bank_data.dexy_tokens,
        bank_ergo_tree: bank_data.ergo_tree,
        bank_box,
        // Buyback
        buyback_input,
        buyback_erg_nano: buyback_box.value.as_i64(),
        buyback_ergo_tree,
        buyback_box,
        // Oracle
        oracle_data_input,
        oracle_rate_nano: oracle_data.rate_nano,
        oracle_box,
        // LP
        lp_data_input,
        lp_erg_reserves: lp_data.erg_reserves,
        lp_dexy_reserves: lp_data.dexy_reserves,
        lp_box,
    })
}

/// Context needed for building Dexy LP swap transactions
///
/// LP swap transaction structure:
///   INPUTS:  [0] LP box, [1] Swap NFT box, [2+] User UTXOs
///   OUTPUTS: [0] LP box (updated), [1] Swap NFT box (preserved), [2] User output, [3+] Change, Fee
#[derive(Debug, Clone)]
pub struct DexySwapTxContext {
    // LP box (Input 0)
    pub lp_input: Eip12InputBox,
    pub lp_erg_reserves: i64,
    pub lp_dexy_reserves: i64,
    pub lp_ergo_tree: String,
    pub lp_box: ErgoBox,
    pub lp_tokens: Vec<(String, u64)>,

    // Swap NFT box (Input 1)
    pub swap_input: Eip12InputBox,
    pub swap_erg_value: i64,
    pub swap_ergo_tree: String,
    pub swap_box: ErgoBox,
    pub swap_tokens: Vec<(String, u64)>,
}

/// Fetch boxes needed for LP swap transactions.
///
/// Fetches the LP pool box and Swap NFT action box, converting both to EIP-12 format.
pub async fn fetch_swap_tx_context(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    ids: &DexyIds,
) -> Result<DexySwapTxContext, ProtocolError> {
    // Fetch LP box by LP NFT
    let lp_token_id = TokenId::new(&ids.lp_nft);
    let lp_box =
        ergo_node_client::queries::get_box_by_token_id(client.inner(), capabilities, &lp_token_id)
            .await
            .map_err(|e| ProtocolError::BoxParseError {
                message: format!("LP box not found: {}", e),
            })?;

    let (lp_tx_id, lp_index) = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &lp_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;
    let lp_input = Eip12InputBox::from_ergo_box(&lp_box, lp_tx_id, lp_index);

    // Parse LP reserves
    let lp_data = parse_lp_box(&lp_box, ids)?;
    let lp_ergo_tree = {
        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
        lp_box
            .ergo_tree
            .sigma_serialize_bytes()
            .map(|bytes| base16::encode_lower(&bytes))
            .map_err(|e| ProtocolError::BoxParseError {
                message: format!("Failed to serialize LP ErgoTree: {}", e),
            })?
    };
    let lp_tokens = collect_box_tokens(&lp_box);

    // Fetch Swap NFT box
    let swap_token_id = TokenId::new(&ids.lp_swap_nft);
    let swap_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &swap_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("LP Swap NFT box not found: {}", e),
    })?;

    let (swap_tx_id, swap_index) = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &swap_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;
    let swap_input = Eip12InputBox::from_ergo_box(&swap_box, swap_tx_id, swap_index);

    let swap_erg_value = swap_box.value.as_i64();
    let swap_ergo_tree = {
        use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
        swap_box
            .ergo_tree
            .sigma_serialize_bytes()
            .map(|bytes| base16::encode_lower(&bytes))
            .map_err(|e| ProtocolError::BoxParseError {
                message: format!("Failed to serialize Swap ErgoTree: {}", e),
            })?
    };
    let swap_tokens = collect_box_tokens(&swap_box);

    Ok(DexySwapTxContext {
        lp_input,
        lp_erg_reserves: lp_data.erg_reserves,
        lp_dexy_reserves: lp_data.dexy_reserves,
        lp_ergo_tree,
        lp_box,
        lp_tokens,
        swap_input,
        swap_erg_value,
        swap_ergo_tree,
        swap_box,
        swap_tokens,
    })
}

// =============================================================================
// Simple Oracle Query
// =============================================================================

/// Simple oracle price data
#[derive(Debug, Clone)]
pub struct DexyOraclePrice {
    /// Oracle rate: nanoERG per unit of underlying
    pub rate_nano: i64,
    /// Human-readable rate (underlying per ERG)
    pub underlying_per_erg: f64,
    /// Oracle box ID
    pub oracle_box_id: String,
}

/// Fetch just the oracle price (lighter than full Dexy state)
pub async fn fetch_oracle_price(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    ids: &DexyIds,
) -> Result<DexyOraclePrice, ProtocolError> {
    let oracle_token_id = TokenId::new(&ids.oracle_pool_nft);
    let oracle_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &oracle_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Oracle box not found: {}", e),
    })?;

    let oracle_data = parse_oracle_box(&oracle_box)?;

    // Calculate human-readable rate
    // rate_nano = nanoERG per unit of underlying
    // underlying_per_erg = 1 ERG / rate_nano = 1_000_000_000 / rate_nano
    let underlying_per_erg = if oracle_data.rate_nano > 0 {
        1_000_000_000.0 / oracle_data.rate_nano as f64
    } else {
        0.0
    };

    Ok(DexyOraclePrice {
        rate_nano: oracle_data.rate_nano,
        underlying_per_erg,
        oracle_box_id: oracle_data.box_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
    use ergo_lib::ergotree_ir::chain::ergo_box::{BoxTokens, NonMandatoryRegisters};
    use ergo_lib::ergotree_ir::chain::token::{Token, TokenAmount, TokenId as ErgoTokenId};
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::types::stype::SType;
    use std::convert::TryFrom;

    fn test_ergo_tree() -> ErgoTree {
        let hex = "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let bytes = base16::decode(hex).unwrap();
        ErgoTree::sigma_parse_bytes(&bytes).unwrap()
    }

    fn make_token_id(hex_str: &str) -> ErgoTokenId {
        hex_str.parse().unwrap()
    }

    fn long_constant(val: i64) -> Constant {
        Constant {
            tpe: SType::SLong,
            v: Literal::Long(val),
        }
    }

    fn int_constant(val: i32) -> Constant {
        Constant {
            tpe: SType::SInt,
            v: Literal::Int(val),
        }
    }

    fn make_box_with_registers(value_nano: u64, registers: Vec<Constant>) -> ErgoBox {
        let regs = NonMandatoryRegisters::try_from(registers).unwrap();
        ErgoBox::new(
            BoxValue::new(value_nano).unwrap(),
            test_ergo_tree(),
            None,
            regs,
            100_000,
            TxId::zero(),
            0,
        )
        .unwrap()
    }

    fn make_box_with_tokens_and_registers(
        value_nano: u64,
        tokens: Vec<Token>,
        registers: Vec<Constant>,
    ) -> ErgoBox {
        let regs = NonMandatoryRegisters::try_from(registers).unwrap();
        let box_tokens = if tokens.is_empty() {
            None
        } else {
            Some(BoxTokens::from_vec(tokens).unwrap())
        };
        ErgoBox::new(
            BoxValue::new(value_nano).unwrap(),
            test_ergo_tree(),
            box_tokens,
            regs,
            100_000,
            TxId::zero(),
            0,
        )
        .unwrap()
    }

    fn test_dexy_ids() -> DexyIds {
        DexyIds {
            variant: crate::constants::DexyVariant::Gold,
            dexy_token: "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad"
                .to_string(),
            bank_nft: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            oracle_pool_nft: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_string(),
            lp_nft: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
            free_mint_nft: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                .to_string(),
            buyback_nft: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                .to_string(),
            lp_swap_nft: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                .to_string(),
        }
    }

    // ==================== parse_oracle_box tests ====================

    #[test]
    fn parse_oracle_box_happy_path() {
        let rate: i64 = 1_000_000_000_000; // 1000 ERG per kg (gold oracle)
        let regs = vec![long_constant(rate)];
        let ergo_box = make_box_with_registers(1_000_000_000, regs);

        let result = parse_oracle_box(&ergo_box).unwrap();
        assert_eq!(result.rate_nano, rate);
    }

    #[test]
    fn parse_oracle_box_missing_r4() {
        let ergo_box = make_box_with_registers(1_000_000_000, vec![]);
        let result = parse_oracle_box(&ergo_box);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BoxParseError { message } => {
                assert!(
                    message.contains("R4"),
                    "Expected R4 in error, got: {}",
                    message
                );
            }
            other => panic!("Expected BoxParseError, got: {:?}", other),
        }
    }

    #[test]
    fn parse_oracle_box_wrong_type() {
        // R4 is Int instead of Long
        let regs = vec![int_constant(42)];
        let ergo_box = make_box_with_registers(1_000_000_000, regs);
        let result = parse_oracle_box(&ergo_box);
        assert!(result.is_err());
    }

    // ==================== parse_bank_box tests ====================

    #[test]
    fn parse_bank_box_happy_path() {
        let ids = test_dexy_ids();
        let dexy_amount: u64 = 9_999_000_000;
        let bank_value: u64 = 1_000_000_000_000; // 1000 ERG

        let tokens = vec![
            Token {
                token_id: make_token_id(&ids.bank_nft),
                amount: TokenAmount::try_from(1u64).unwrap(),
            },
            Token {
                token_id: make_token_id(&ids.dexy_token),
                amount: TokenAmount::try_from(dexy_amount).unwrap(),
            },
        ];

        let ergo_box = make_box_with_tokens_and_registers(bank_value, tokens, vec![]);
        let result = parse_bank_box(&ergo_box, &ids).unwrap();

        assert_eq!(result.erg_value, bank_value as i64);
        assert_eq!(result.dexy_tokens, dexy_amount as i64);
        assert!(!result.ergo_tree.is_empty());
    }

    #[test]
    fn parse_bank_box_missing_dexy_token() {
        let ids = test_dexy_ids();
        let other_id = "1111111111111111111111111111111111111111111111111111111111111111";

        let tokens = vec![Token {
            token_id: make_token_id(other_id),
            amount: TokenAmount::try_from(1u64).unwrap(),
        }];

        let ergo_box = make_box_with_tokens_and_registers(1_000_000_000, tokens, vec![]);
        let result = parse_bank_box(&ergo_box, &ids);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BoxParseError { message } => {
                assert!(
                    message.contains("not found"),
                    "Expected 'not found' in error, got: {}",
                    message
                );
            }
            other => panic!("Expected BoxParseError, got: {:?}", other),
        }
    }

    // ==================== parse_lp_box tests ====================

    #[test]
    fn parse_lp_box_happy_path() {
        let ids = test_dexy_ids();
        let dexy_reserves: u64 = 1_000_000;
        let erg_reserves: u64 = 1_000_000_000_000; // 1000 ERG

        let tokens = vec![
            Token {
                token_id: make_token_id(&ids.lp_nft),
                amount: TokenAmount::try_from(1u64).unwrap(),
            },
            Token {
                token_id: make_token_id(&ids.dexy_token),
                amount: TokenAmount::try_from(dexy_reserves).unwrap(),
            },
        ];

        let ergo_box = make_box_with_tokens_and_registers(erg_reserves, tokens, vec![]);
        let result = parse_lp_box(&ergo_box, &ids).unwrap();

        assert_eq!(result.erg_reserves, erg_reserves as i64);
        assert_eq!(result.dexy_reserves, dexy_reserves as i64);
    }

    #[test]
    fn parse_lp_box_missing_dexy_token() {
        let ids = test_dexy_ids();
        let other_id = "1111111111111111111111111111111111111111111111111111111111111111";

        let tokens = vec![Token {
            token_id: make_token_id(other_id),
            amount: TokenAmount::try_from(1u64).unwrap(),
        }];

        let ergo_box = make_box_with_tokens_and_registers(1_000_000_000, tokens, vec![]);
        let result = parse_lp_box(&ergo_box, &ids);
        assert!(result.is_err());
    }

    // ==================== parse_free_mint_box_data tests ====================

    #[test]
    fn parse_free_mint_box_data_happy_path() {
        let reset_height: i32 = 1_000_000;
        let available: i64 = 50_000;

        let regs = vec![int_constant(reset_height), long_constant(available)];
        let ergo_box = make_box_with_registers(1_000_000_000, regs);

        let result = parse_free_mint_box_data(&ergo_box).unwrap();
        assert_eq!(result.reset_height, reset_height);
        assert_eq!(result.available, available);
    }

    #[test]
    fn parse_free_mint_box_data_missing_r4() {
        let ergo_box = make_box_with_registers(1_000_000_000, vec![]);
        let result = parse_free_mint_box_data(&ergo_box);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BoxParseError { message } => {
                assert!(
                    message.contains("R4"),
                    "Expected R4 in error, got: {}",
                    message
                );
            }
            other => panic!("Expected BoxParseError, got: {:?}", other),
        }
    }

    #[test]
    fn parse_free_mint_box_data_missing_r5() {
        // Only R4, missing R5
        let regs = vec![int_constant(1_000_000)];
        let ergo_box = make_box_with_registers(1_000_000_000, regs);
        let result = parse_free_mint_box_data(&ergo_box);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BoxParseError { message } => {
                assert!(
                    message.contains("R5"),
                    "Expected R5 in error, got: {}",
                    message
                );
            }
            other => panic!("Expected BoxParseError, got: {:?}", other),
        }
    }

    #[test]
    fn parse_free_mint_box_data_wrong_r4_type() {
        // R4 should be Int but provide Long
        let regs = vec![long_constant(1_000_000), long_constant(50_000)];
        let ergo_box = make_box_with_registers(1_000_000_000, regs);
        let result = parse_free_mint_box_data(&ergo_box);
        assert!(result.is_err());
    }

    // ==================== collect_box_tokens tests ====================

    #[test]
    fn collect_box_tokens_empty() {
        let ergo_box = make_box_with_registers(1_000_000_000, vec![]);
        let tokens = collect_box_tokens(&ergo_box);
        assert!(tokens.is_empty());
    }

    #[test]
    fn collect_box_tokens_with_tokens() {
        let id1 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let id2 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        let tokens = vec![
            Token {
                token_id: make_token_id(id1),
                amount: TokenAmount::try_from(100u64).unwrap(),
            },
            Token {
                token_id: make_token_id(id2),
                amount: TokenAmount::try_from(200u64).unwrap(),
            },
        ];

        let ergo_box = make_box_with_tokens_and_registers(1_000_000_000, tokens, vec![]);
        let result = collect_box_tokens(&ergo_box);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, id1);
        assert_eq!(result[0].1, 100);
        assert_eq!(result[1].0, id2);
        assert_eq!(result[1].1, 200);
    }
}
