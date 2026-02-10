//! Swap Order Transaction Builder
//!
//! Builds EIP-12 unsigned transactions for swap orders using the proxy box pattern.
//! The proxy box contains the swap contract ErgoTree with substituted constants,
//! and is detected and executed by off-chain Spectrum bots.
//!
//! # Transaction Structure
//!
//! Inputs:  [user UTXOs]
//! Outputs: [swap proxy box, miner fee, change (optional)]
//!
//! The swap proxy box contains:
//! - The swap contract ErgoTree (template with user-specific constants)
//! - Input funds (ERG for ERG->Token, or ERG + tokens for Token->ERG)
//! - Execution fee for the bot

use std::collections::HashMap;

use ergo_lib::ergo_chain_types::EcPoint;
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::mir::constant::Constant;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
use serde::{Deserialize, Serialize};

use crate::constants::swap_templates;
use crate::state::{AmmError, AmmPool, PoolType, SwapInput, SwapRequest};
use ergo_tx::{
    collect_change_tokens, select_erg_boxes, select_token_boxes, Eip12Asset, Eip12InputBox,
    Eip12Output, Eip12UnsignedTx,
};

// =============================================================================
// Constants
// =============================================================================

/// Minimum proxy box value in nanoERG (0.004 ERG)
/// This covers the minimum box value plus overhead for the bot
const PROXY_BOX_VALUE: u64 = 4_000_000;

/// Transaction fee in nanoERG (0.0011 ERG - standard)
pub(crate) const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;

/// Bot execution fee in nanoERG (0.002 ERG)
const EXECUTION_FEE: u64 = 2_000_000;

/// Minimum box value for change output in nanoERG
pub(crate) const MIN_CHANGE_VALUE: u64 = 1_000_000;

// =============================================================================
// Public Types
// =============================================================================

/// Build result containing the unsigned transaction and a summary
#[derive(Debug)]
pub struct SwapBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: SwapTxSummary,
}

/// Summary of the swap transaction for the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapTxSummary {
    pub input_amount: u64,
    pub input_token: String,
    pub min_output: u64,
    pub output_token: String,
    pub execution_fee: u64,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

// =============================================================================
// Main Builder Function
// =============================================================================

/// Build a swap order EIP-12 unsigned transaction
///
/// Creates a proxy box containing the swap contract with user-specific constants.
/// Off-chain Spectrum bots will detect this proxy box and execute the swap.
///
/// # Arguments
///
/// * `request` - Swap parameters (pool, input, min output, redeemer address)
/// * `pool` - Current pool state
/// * `user_utxos` - User's available UTXOs for funding
/// * `user_ergo_tree` - User's ErgoTree hex (for change output)
/// * `user_pk` - User's compressed public key hex (33 bytes, for RefundProp)
/// * `current_height` - Current blockchain height
/// * `execution_fee` - Optional execution fee in nanoERG (defaults to EXECUTION_FEE constant)
///
/// # Returns
///
/// `SwapBuildResult` with the unsigned transaction and a summary for UI display
#[allow(clippy::too_many_arguments)]
pub fn build_swap_order_eip12(
    request: &SwapRequest,
    pool: &AmmPool,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    user_pk: &str,
    current_height: i32,
    execution_fee: Option<u64>,
    recipient_ergo_tree: Option<&str>,
) -> Result<SwapBuildResult, AmmError> {
    // 0. Validate pool ID matches request
    if request.pool_id != pool.pool_id {
        return Err(AmmError::TxBuildError(format!(
            "Pool ID mismatch: request has {}, pool has {}",
            request.pool_id, pool.pool_id
        )));
    }

    let ex_fee = execution_fee.unwrap_or(EXECUTION_FEE);

    // 1. Determine swap direction and amounts
    let (input_erg_amount, input_token, is_erg_to_token) = match &request.input {
        SwapInput::Erg { amount } => (*amount, None, true),
        SwapInput::Token { token_id, amount } => (0u64, Some((token_id.clone(), *amount)), false),
    };

    // 2. Calculate the proxy box value
    // For ERG->Token: proxy box holds input ERG + execution fee + proxy overhead
    // For Token->ERG: proxy box holds minimum ERG for box + execution fee
    let proxy_box_erg_value = if is_erg_to_token {
        input_erg_amount
            .checked_add(ex_fee)
            .and_then(|v| v.checked_add(PROXY_BOX_VALUE))
            .ok_or_else(|| {
                AmmError::TxBuildError(
                    "Arithmetic overflow calculating proxy box value".to_string(),
                )
            })?
    } else {
        ex_fee.checked_add(PROXY_BOX_VALUE).ok_or_else(|| {
            AmmError::TxBuildError("Arithmetic overflow calculating proxy box value".to_string())
        })?
    };

    // 3. Calculate total ERG needed from user
    let total_erg_needed = proxy_box_erg_value.checked_add(TX_FEE).ok_or_else(|| {
        AmmError::TxBuildError("Arithmetic overflow calculating total ERG needed".to_string())
    })?;

    // 4. Select minimum UTXOs needed
    let selected = if let Some((ref token_id, token_amount)) = input_token {
        // Token->ERG: need tokens + enough ERG for proxy box + fee
        select_token_boxes(user_utxos, token_id, token_amount, total_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?
    } else {
        // ERG->Token: need ERG only
        select_erg_boxes(user_utxos, total_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?
    };

    // 6. Build the swap ErgoTree with substituted constants
    let swap_ergo_tree_hex = build_swap_ergo_tree(pool, request, user_pk, recipient_ergo_tree)?;

    // 7. Build proxy box output
    let proxy_assets = if let Some((ref token_id, token_amount)) = input_token {
        vec![Eip12Asset {
            token_id: token_id.clone(),
            amount: token_amount.to_string(),
        }]
    } else {
        vec![]
    };

    let proxy_output = Eip12Output {
        value: proxy_box_erg_value.to_string(),
        ergo_tree: swap_ergo_tree_hex,
        assets: proxy_assets,
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    // 8. Build miner fee output
    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    // 9. Build change output
    let mut outputs = vec![proxy_output, fee_output];

    let change_erg = selected.total_erg - total_erg_needed;
    let spent_token = input_token.as_ref().map(|(id, amt)| (id.as_str(), *amt));
    let change_tokens = collect_change_tokens(&selected.boxes, spent_token);

    // Error if we have change tokens but not enough ERG for a box
    if !change_tokens.is_empty() && change_erg < MIN_CHANGE_VALUE {
        return Err(AmmError::TxBuildError(format!(
            "Change tokens exist but not enough ERG for change box (need {}, have {})",
            MIN_CHANGE_VALUE, change_erg
        )));
    }

    // Create change output if we have sufficient ERG or any tokens
    if change_erg >= MIN_CHANGE_VALUE || !change_tokens.is_empty() {
        outputs.push(Eip12Output::change(
            change_erg as i64,
            user_ergo_tree,
            change_tokens,
            current_height,
        ));
    }

    // 10. Build the transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs: selected.boxes,
        data_inputs: vec![],
        outputs,
    };

    // 11. Build the summary
    let (input_token_name, output_token_name) = match &request.input {
        SwapInput::Erg { .. } => (
            "ERG".to_string(),
            pool.token_y
                .name
                .clone()
                .unwrap_or_else(|| pool.token_y.token_id[..8].to_string()),
        ),
        SwapInput::Token { .. } => (
            pool.token_y
                .name
                .clone()
                .unwrap_or_else(|| pool.token_y.token_id[..8].to_string()),
            "ERG".to_string(),
        ),
    };

    let summary = SwapTxSummary {
        input_amount: match &request.input {
            SwapInput::Erg { amount } => *amount,
            SwapInput::Token { amount, .. } => *amount,
        },
        input_token: input_token_name,
        min_output: request.min_output,
        output_token: output_token_name,
        execution_fee: ex_fee,
        miner_fee: TX_FEE,
        total_erg_cost: total_erg_needed,
    };

    Ok(SwapBuildResult {
        unsigned_tx,
        summary,
    })
}

// =============================================================================
// ErgoTree Construction
// =============================================================================

/// Build the swap ErgoTree by substituting constants into the template
///
/// This is the critical function that creates the swap contract for a specific order.
/// It selects the correct template based on pool type and swap direction, then
/// substitutes user-specific constants at the correct positions.
fn build_swap_ergo_tree(
    pool: &AmmPool,
    request: &SwapRequest,
    user_pk: &str,
    recipient_ergo_tree: Option<&str>,
) -> Result<String, AmmError> {
    match pool.pool_type {
        PoolType::N2T => match &request.input {
            SwapInput::Erg { amount } => build_n2t_swap_sell_tree(
                pool,
                *amount,
                request.min_output,
                user_pk,
                recipient_ergo_tree,
            ),
            SwapInput::Token { amount, .. } => build_n2t_swap_buy_tree(
                pool,
                *amount,
                request.min_output,
                user_pk,
                recipient_ergo_tree,
            ),
        },
        PoolType::T2T => {
            // T2T swap support can be added later when needed
            Err(AmmError::TxBuildError(
                "T2T swaps not yet implemented".to_string(),
            ))
        }
    }
}

/// Build N2T SwapSell ErgoTree (ERG -> Token)
///
/// Constant positions for N2T SwapSell:
/// {1}=ExFeePerTokenDenom[Long], {2}=Delta[Long], {3}=BaseAmount[Long],
/// {4}=FeeNum[Int], {5}=RefundProp[ProveDlog], {10}=SpectrumIsQuote[Boolean],
/// {11}=MaxExFee[Long], {13}=PoolNFT[Coll[Byte]], {14}=RedeemerPropBytes[Coll[Byte]],
/// {15}=QuoteId[Coll[Byte]], {16}=MinQuoteAmount[Long],
/// {23}=SpectrumId[Coll[Byte]], {27}=FeeDenom[Int],
/// {28}=MinerPropBytes[Coll[Byte]], {31}=MaxMinerFee[Long]
fn build_n2t_swap_sell_tree(
    pool: &AmmPool,
    base_amount: u64,
    min_quote_amount: u64,
    user_pk: &str,
    recipient_ergo_tree: Option<&str>,
) -> Result<String, AmmError> {
    let template_hex = swap_templates::N2T_SWAP_SELL_TEMPLATE;
    let tree = parse_ergo_tree(template_hex)?;

    // Prepare constant values
    let pool_nft_bytes = hex_to_bytes(&pool.pool_id)?;
    let quote_id_bytes = hex_to_bytes(&pool.token_y.token_id)?;
    let spf_bytes = hex_to_bytes(swap_templates::SPF_TOKEN_ID)?;
    let miner_prop_bytes = hex_to_bytes(swap_templates::MINER_FEE_ERGO_TREE)?;
    // RedeemerPropBytes: use recipient's full ErgoTree if set, otherwise user's P2PK tree
    let redeemer_prop_bytes = if let Some(recipient) = recipient_ergo_tree {
        hex_to_bytes(recipient)?
    } else {
        hex_to_bytes(&format!("0008cd{}", user_pk))?
    };
    let refund_prop = build_prove_dlog(user_pk)?;

    // Substitute constants using chain pattern (with_constant consumes self)
    let tree = tree
        .with_constant(
            1,
            Constant::from(swap_templates::DEFAULT_EX_FEE_PER_TOKEN_DENOM as i64),
        )
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set ExFeePerTokenDenom: {}", e)))?
        .with_constant(2, Constant::from(0i64)) // Delta
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set Delta: {}", e)))?
        .with_constant(3, Constant::from(base_amount as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set BaseAmount: {}", e)))?
        .with_constant(4, Constant::from(pool.fee_num))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set FeeNum: {}", e)))?
        .with_constant(5, Constant::from(refund_prop))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set RefundProp: {}", e)))?
        .with_constant(10, Constant::from(false)) // SpectrumIsQuote
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set SpectrumIsQuote: {}", e)))?
        .with_constant(
            11,
            Constant::from(swap_templates::DEFAULT_MAX_EX_FEE as i64),
        )
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MaxExFee: {}", e)))?
        .with_constant(13, Constant::from(pool_nft_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set PoolNFT: {}", e)))?
        .with_constant(14, Constant::from(redeemer_prop_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set RedeemerPropBytes: {}", e)))?
        .with_constant(15, Constant::from(quote_id_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set QuoteId: {}", e)))?
        .with_constant(16, Constant::from(min_quote_amount as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MinQuoteAmount: {}", e)))?
        .with_constant(23, Constant::from(spf_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set SpectrumId: {}", e)))?
        .with_constant(27, Constant::from(pool.fee_denom))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set FeeDenom: {}", e)))?
        .with_constant(28, Constant::from(miner_prop_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MinerPropBytes: {}", e)))?
        .with_constant(
            31,
            Constant::from(swap_templates::DEFAULT_MAX_MINER_FEE as i64),
        )
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MaxMinerFee: {}", e)))?;

    serialize_ergo_tree(&tree)
}

/// Build N2T SwapBuy ErgoTree (Token -> ERG)
///
/// Constant positions for N2T SwapBuy:
/// {1}=BaseAmount[Long], {2}=FeeNum[Int], {3}=RefundProp[ProveDlog],
/// {7}=MaxExFee[Long], {8}=ExFeePerTokenDenom[Long], {9}=ExFeePerTokenNum[Long],
/// {11}=PoolNFT[Coll[Byte]], {12}=RedeemerPropBytes[Coll[Byte]],
/// {13}=MinQuoteAmount[Long], {16}=SpectrumId[Coll[Byte]],
/// {20}=FeeDenom[Int], {21}=MinerPropBytes[Coll[Byte]], {24}=MaxMinerFee[Long]
fn build_n2t_swap_buy_tree(
    pool: &AmmPool,
    base_amount: u64,
    min_quote_amount: u64,
    user_pk: &str,
    recipient_ergo_tree: Option<&str>,
) -> Result<String, AmmError> {
    let template_hex = swap_templates::N2T_SWAP_BUY_TEMPLATE;
    let tree = parse_ergo_tree(template_hex)?;

    // Prepare constant values
    let pool_nft_bytes = hex_to_bytes(&pool.pool_id)?;
    let spf_bytes = hex_to_bytes(swap_templates::SPF_TOKEN_ID)?;
    let miner_prop_bytes = hex_to_bytes(swap_templates::MINER_FEE_ERGO_TREE)?;
    // RedeemerPropBytes: use recipient's full ErgoTree if set, otherwise user's P2PK tree
    let redeemer_prop_bytes = if let Some(recipient) = recipient_ergo_tree {
        hex_to_bytes(recipient)?
    } else {
        hex_to_bytes(&format!("0008cd{}", user_pk))?
    };
    let refund_prop = build_prove_dlog(user_pk)?;

    // Substitute constants
    let tree = tree
        .with_constant(1, Constant::from(base_amount as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set BaseAmount: {}", e)))?
        .with_constant(2, Constant::from(pool.fee_num))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set FeeNum: {}", e)))?
        .with_constant(3, Constant::from(refund_prop))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set RefundProp: {}", e)))?
        .with_constant(7, Constant::from(swap_templates::DEFAULT_MAX_EX_FEE as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MaxExFee: {}", e)))?
        .with_constant(
            8,
            Constant::from(swap_templates::DEFAULT_EX_FEE_PER_TOKEN_DENOM as i64),
        )
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set ExFeePerTokenDenom: {}", e)))?
        .with_constant(
            9,
            Constant::from(swap_templates::DEFAULT_EX_FEE_PER_TOKEN_NUM as i64),
        )
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set ExFeePerTokenNum: {}", e)))?
        .with_constant(11, Constant::from(pool_nft_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set PoolNFT: {}", e)))?
        .with_constant(12, Constant::from(redeemer_prop_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set RedeemerPropBytes: {}", e)))?
        .with_constant(13, Constant::from(min_quote_amount as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MinQuoteAmount: {}", e)))?
        .with_constant(16, Constant::from(spf_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set SpectrumId: {}", e)))?
        .with_constant(20, Constant::from(pool.fee_denom))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set FeeDenom: {}", e)))?
        .with_constant(21, Constant::from(miner_prop_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MinerPropBytes: {}", e)))?
        .with_constant(
            24,
            Constant::from(swap_templates::DEFAULT_MAX_MINER_FEE as i64),
        )
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MaxMinerFee: {}", e)))?;

    serialize_ergo_tree(&tree)
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Parse an ErgoTree from hex string
fn parse_ergo_tree(hex_str: &str) -> Result<ErgoTree, AmmError> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| AmmError::TxBuildError(format!("Invalid ErgoTree hex: {}", e)))?;
    ErgoTree::sigma_parse_bytes(&bytes)
        .map_err(|e| AmmError::TxBuildError(format!("Failed to parse ErgoTree: {}", e)))
}

/// Serialize an ErgoTree to hex string
fn serialize_ergo_tree(tree: &ErgoTree) -> Result<String, AmmError> {
    let bytes = tree
        .sigma_serialize_bytes()
        .map_err(|e| AmmError::TxBuildError(format!("Failed to serialize ErgoTree: {}", e)))?;
    Ok(hex::encode(bytes))
}

/// Decode hex string to bytes
fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>, AmmError> {
    hex::decode(hex_str)
        .map_err(|e| AmmError::TxBuildError(format!("Invalid hex string '{}': {}", hex_str, e)))
}

/// Build a ProveDlog constant from a compressed public key hex
fn build_prove_dlog(pk_hex: &str) -> Result<ProveDlog, AmmError> {
    let pk_bytes = hex_to_bytes(pk_hex)?;
    if pk_bytes.len() != 33 {
        return Err(AmmError::TxBuildError(format!(
            "Invalid public key length: expected 33 bytes, got {}",
            pk_bytes.len()
        )));
    }
    let ec_point = EcPoint::from_base16_str(pk_hex.to_string()).ok_or_else(|| {
        AmmError::TxBuildError(format!(
            "Failed to parse EC point from public key: {}",
            pk_hex
        ))
    })?;
    Ok(ProveDlog::new(ec_point))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AmmPool, PoolType, SwapInput, SwapRequest, TokenAmount};

    fn test_n2t_pool() -> AmmPool {
        AmmPool {
            pool_id: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            pool_type: PoolType::N2T,
            box_id: "box1".to_string(),
            erg_reserves: Some(100_000_000_000),
            token_x: None,
            token_y: TokenAmount {
                token_id: "0000000000000000000000000000000000000000000000000000000000000002"
                    .to_string(),
                amount: 1_000_000,
                decimals: Some(6),
                name: Some("TestToken".to_string()),
            },
            lp_token_id: "lp".to_string(),
            lp_circulating: 1000,
            fee_num: 997,
            fee_denom: 1000,
        }
    }

    fn test_user_utxo() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "utxo1".to_string(),
            transaction_id: "tx1".to_string(),
            index: 0,
            value: "10000000000".to_string(), // 10 ERG
            ergo_tree: "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                .to_string(),
            assets: vec![],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_build_swap_order_erg_to_token() {
        let pool = test_n2t_pool();
        let user_utxo = test_user_utxo();
        let request = SwapRequest {
            pool_id: pool.pool_id.clone(),
            input: SwapInput::Erg {
                amount: 1_000_000_000,
            },
            min_output: 9000,
            redeemer_address: "9fMPy1XY3GW4T6t3LjYofqmzER6x9cV2ZfBGGfnkA5G7d1mVSaj".to_string(),
        };
        let user_pk = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

        let result = build_swap_order_eip12(
            &request,
            &pool,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            user_pk,
            1_000_000,
            None,
            None,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        // Should have 3 outputs: proxy box, miner fee, change
        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        // Output 0: proxy box with swap ErgoTree (not the user's tree)
        let proxy = &build.unsigned_tx.outputs[0];
        assert_ne!(
            proxy.ergo_tree,
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );
        let proxy_value: u64 = proxy.value.parse().unwrap();
        // proxy_box_value = input_amount + EXECUTION_FEE + PROXY_BOX_VALUE
        assert_eq!(proxy_value, 1_000_000_000 + EXECUTION_FEE + PROXY_BOX_VALUE);

        // Output 1: miner fee
        let fee_output = &build.unsigned_tx.outputs[1];
        assert_eq!(fee_output.value, TX_FEE.to_string());

        // Output 2: change
        let change_output = &build.unsigned_tx.outputs[2];
        let change_value: u64 = change_output.value.parse().unwrap();
        let expected_change = 10_000_000_000 - proxy_value - TX_FEE;
        assert_eq!(change_value, expected_change);

        // Summary
        assert_eq!(build.summary.input_amount, 1_000_000_000);
        assert_eq!(build.summary.input_token, "ERG");
        assert_eq!(build.summary.min_output, 9000);
        assert_eq!(build.summary.output_token, "TestToken");
        assert_eq!(build.summary.miner_fee, TX_FEE);
        assert_eq!(build.summary.execution_fee, EXECUTION_FEE);
    }

    #[test]
    fn test_build_swap_order_token_to_erg() {
        let pool = test_n2t_pool();
        let token_id =
            "0000000000000000000000000000000000000000000000000000000000000002".to_string();
        let user_utxo = Eip12InputBox {
            assets: vec![Eip12Asset {
                token_id: token_id.clone(),
                amount: "50000".to_string(),
            }],
            ..test_user_utxo()
        };
        let request = SwapRequest {
            pool_id: pool.pool_id.clone(),
            input: SwapInput::Token {
                token_id: token_id.clone(),
                amount: 10000,
            },
            min_output: 500_000_000,
            redeemer_address: "addr".to_string(),
        };

        let result = build_swap_order_eip12(
            &request,
            &pool,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
            None,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        // Should have 3 outputs: proxy box, miner fee, change
        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        // Proxy box should contain the token
        let proxy = &build.unsigned_tx.outputs[0];
        assert_eq!(proxy.assets.len(), 1);
        assert_eq!(proxy.assets[0].token_id, token_id);
        assert_eq!(proxy.assets[0].amount, "10000");

        // Proxy ERG value = EXECUTION_FEE + PROXY_BOX_VALUE (no input ERG for token->ERG)
        let proxy_value: u64 = proxy.value.parse().unwrap();
        assert_eq!(proxy_value, EXECUTION_FEE + PROXY_BOX_VALUE);

        // Change should have remaining tokens
        let change = &build.unsigned_tx.outputs[2];
        let change_tokens: Vec<&Eip12Asset> = change
            .assets
            .iter()
            .filter(|a| a.token_id == token_id)
            .collect();
        assert_eq!(change_tokens.len(), 1);
        assert_eq!(change_tokens[0].amount, "40000"); // 50000 - 10000
    }

    #[test]
    fn test_build_swap_insufficient_erg() {
        let pool = test_n2t_pool();
        let user_utxo = Eip12InputBox {
            value: "1000000".to_string(), // Only 0.001 ERG - not enough
            ..test_user_utxo()
        };
        let request = SwapRequest {
            pool_id: pool.pool_id.clone(),
            input: SwapInput::Erg {
                amount: 1_000_000_000,
            },
            min_output: 9000,
            redeemer_address: "addr".to_string(),
        };

        let result = build_swap_order_eip12(
            &request,
            &pool,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Insufficient"),
            "Should report insufficient funds: {}",
            err
        );
    }

    #[test]
    fn test_build_swap_insufficient_tokens() {
        let pool = test_n2t_pool();
        let token_id =
            "0000000000000000000000000000000000000000000000000000000000000002".to_string();
        let user_utxo = Eip12InputBox {
            assets: vec![Eip12Asset {
                token_id: token_id.clone(),
                amount: "100".to_string(), // Only 100, need 10000
            }],
            ..test_user_utxo()
        };
        let request = SwapRequest {
            pool_id: pool.pool_id.clone(),
            input: SwapInput::Token {
                token_id,
                amount: 10000,
            },
            min_output: 500_000_000,
            redeemer_address: "addr".to_string(),
        };

        let result = build_swap_order_eip12(
            &request,
            &pool,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Insufficient"),
            "Should report insufficient tokens: {}",
            err
        );
    }

    #[test]
    fn test_build_swap_no_change_when_exact() {
        let pool = test_n2t_pool();
        // Calculate exact amount needed:
        // proxy_box_value = 1_000_000_000 + EXECUTION_FEE + PROXY_BOX_VALUE
        // total = proxy_box_value + TX_FEE
        let proxy_val = 1_000_000_000u64 + EXECUTION_FEE + PROXY_BOX_VALUE;
        let total = proxy_val + TX_FEE;

        let user_utxo = Eip12InputBox {
            value: total.to_string(),
            ..test_user_utxo()
        };
        let request = SwapRequest {
            pool_id: pool.pool_id.clone(),
            input: SwapInput::Erg {
                amount: 1_000_000_000,
            },
            min_output: 9000,
            redeemer_address: "addr".to_string(),
        };

        let result = build_swap_order_eip12(
            &request,
            &pool,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
            None,
        );

        assert!(result.is_ok(), "Should build: {:?}", result.err());
        let build = result.unwrap();

        // Should only have 2 outputs when no change: proxy + fee
        assert_eq!(build.unsigned_tx.outputs.len(), 2);
    }

    #[test]
    fn test_build_prove_dlog() {
        // Standard generator point public key
        let pk = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let result = build_prove_dlog(pk);
        assert!(result.is_ok(), "Should parse valid PK: {:?}", result.err());
    }

    #[test]
    fn test_build_prove_dlog_invalid() {
        let result = build_prove_dlog("0102");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_n2t_swap_sell_template() {
        let result = parse_ergo_tree(swap_templates::N2T_SWAP_SELL_TEMPLATE);
        assert!(
            result.is_ok(),
            "Should parse N2T SwapSell template: {:?}",
            result.err()
        );

        let tree = result.unwrap();
        let num_constants = tree.constants_len().unwrap();
        assert!(
            num_constants >= 32,
            "N2T SwapSell should have at least 32 constants, got {}",
            num_constants
        );
    }

    #[test]
    fn test_parse_n2t_swap_buy_template() {
        let result = parse_ergo_tree(swap_templates::N2T_SWAP_BUY_TEMPLATE);
        assert!(
            result.is_ok(),
            "Should parse N2T SwapBuy template: {:?}",
            result.err()
        );

        let tree = result.unwrap();
        let num_constants = tree.constants_len().unwrap();
        assert!(
            num_constants >= 25,
            "N2T SwapBuy should have at least 25 constants, got {}",
            num_constants
        );
    }

    #[test]
    fn test_build_n2t_swap_sell_ergo_tree() {
        let pool = test_n2t_pool();
        let pk = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let result = build_n2t_swap_sell_tree(&pool, 1_000_000_000, 9000, pk, None);
        assert!(
            result.is_ok(),
            "Should build SwapSell tree: {:?}",
            result.err()
        );

        // Verify result is valid hex that can be parsed back
        let hex = result.unwrap();
        let roundtrip = parse_ergo_tree(&hex);
        assert!(
            roundtrip.is_ok(),
            "Built tree should round-trip: {:?}",
            roundtrip.err()
        );
    }

    #[test]
    fn test_build_n2t_swap_buy_ergo_tree() {
        let pool = test_n2t_pool();
        let pk = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let result = build_n2t_swap_buy_tree(&pool, 10000, 500_000_000, pk, None);
        assert!(
            result.is_ok(),
            "Should build SwapBuy tree: {:?}",
            result.err()
        );

        let hex = result.unwrap();
        let roundtrip = parse_ergo_tree(&hex);
        assert!(
            roundtrip.is_ok(),
            "Built tree should round-trip: {:?}",
            roundtrip.err()
        );
    }

    #[test]
    fn test_multiple_utxos_selects_minimum() {
        let pool = test_n2t_pool();
        let utxo1 = Eip12InputBox {
            box_id: "utxo1".to_string(),
            value: "5000000000".to_string(), // 5 ERG
            ..test_user_utxo()
        };
        let utxo2 = Eip12InputBox {
            box_id: "utxo2".to_string(),
            value: "5000000000".to_string(), // 5 ERG
            ..test_user_utxo()
        };
        let request = SwapRequest {
            pool_id: pool.pool_id.clone(),
            input: SwapInput::Erg {
                amount: 1_000_000_000,
            },
            min_output: 9000,
            redeemer_address: "addr".to_string(),
        };

        let result = build_swap_order_eip12(
            &request,
            &pool,
            &[utxo1, utxo2],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
            None,
        );

        assert!(
            result.is_ok(),
            "Should build with multiple UTXOs: {:?}",
            result.err()
        );
        let build = result.unwrap();

        // UTXO selection should pick only 1 box (5 ERG covers ~1.007 ERG needed)
        assert_eq!(build.unsigned_tx.inputs.len(), 1);
    }

    #[test]
    fn test_t2t_swap_not_implemented() {
        let pool = AmmPool {
            pool_type: PoolType::T2T,
            token_x: Some(TokenAmount {
                token_id: "token_x".to_string(),
                amount: 1000,
                decimals: None,
                name: None,
            }),
            ..test_n2t_pool()
        };
        let request = SwapRequest {
            pool_id: pool.pool_id.clone(),
            input: SwapInput::Erg {
                amount: 1_000_000_000,
            },
            min_output: 9000,
            redeemer_address: "addr".to_string(),
        };

        let result = build_swap_order_eip12(
            &request,
            &pool,
            &[test_user_utxo()],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
            None,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("T2T"));
    }

    #[test]
    fn test_template_constant_types() {
        // Verify that the template constants have the expected types at key positions
        let sell_tree = parse_ergo_tree(swap_templates::N2T_SWAP_SELL_TEMPLATE).unwrap();
        let buy_tree = parse_ergo_tree(swap_templates::N2T_SWAP_BUY_TEMPLATE).unwrap();

        // SwapSell: verify key constant types
        assert_eq!(sell_tree.constants_len().unwrap(), 33);
        // [5] = SSigmaProp (RefundProp)
        let c5 = sell_tree.get_constant(5).unwrap().unwrap();
        assert_eq!(format!("{:?}", c5.tpe), "SSigmaProp");
        // [13] = SColl(SByte) (PoolNFT)
        let c13 = sell_tree.get_constant(13).unwrap().unwrap();
        assert_eq!(format!("{:?}", c13.tpe), "SColl(SByte)");

        // SwapBuy: verify key constant types
        assert_eq!(buy_tree.constants_len().unwrap(), 26);
        // [3] = SSigmaProp (RefundProp)
        let c3 = buy_tree.get_constant(3).unwrap().unwrap();
        assert_eq!(format!("{:?}", c3.tpe), "SSigmaProp");
        // [11] = SColl(SByte) (PoolNFT)
        let c11 = buy_tree.get_constant(11).unwrap().unwrap();
        assert_eq!(format!("{:?}", c11.tpe), "SColl(SByte)");
    }
}
