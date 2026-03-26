//! Builds EIP-12 unsigned transactions for AMM swap proxy boxes.
//! Spectrum bots detect proxy boxes and execute swaps against pool boxes.

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
    append_change_output, select_inputs_for_spend, Eip12Asset, Eip12InputBox, Eip12Output,
    Eip12UnsignedTx,
};

const PROXY_BOX_VALUE: u64 = 4_000_000; // 0.004 ERG
pub(crate) const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;
const EXECUTION_FEE: u64 = 2_000_000; // 0.002 ERG
pub(crate) const MIN_CHANGE_VALUE: u64 = 1_000_000;

#[derive(Debug)]
pub struct SwapBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: SwapTxSummary,
}

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
    if request.pool_id != pool.pool_id {
        return Err(AmmError::TxBuildError(format!(
            "Pool ID mismatch: request has {}, pool has {}",
            request.pool_id, pool.pool_id
        )));
    }

    let ex_fee = execution_fee.unwrap_or(EXECUTION_FEE);

    let (input_erg_amount, input_token, is_erg_to_token) = match &request.input {
        SwapInput::Erg { amount } => (*amount, None, true),
        SwapInput::Token { token_id, amount } => (0u64, Some((token_id.clone(), *amount)), false),
    };

    // ERG->Token: input + execution fee + proxy overhead; Token->ERG: just fees
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

    let total_erg_needed = proxy_box_erg_value.checked_add(TX_FEE).ok_or_else(|| {
        AmmError::TxBuildError("Arithmetic overflow calculating total ERG needed".to_string())
    })?;

    let token_requirement = input_token
        .as_ref()
        .map(|(id, amt)| (id.as_str(), *amt));
    let selected = select_inputs_for_spend(user_utxos, total_erg_needed, token_requirement)
        .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let swap_ergo_tree_hex = build_swap_ergo_tree(pool, request, user_pk, recipient_ergo_tree)?;

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

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);
    let mut outputs = vec![proxy_output, fee_output];

    let spent: Vec<(&str, u64)> = input_token
        .as_ref()
        .map(|(id, amt)| vec![(id.as_str(), *amt)])
        .unwrap_or_default();
    append_change_output(
        &mut outputs,
        &selected,
        total_erg_needed,
        &spent,
        user_ergo_tree,
        current_height,
        MIN_CHANGE_VALUE,
    )
    .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let unsigned_tx = Eip12UnsignedTx {
        inputs: selected.boxes,
        data_inputs: vec![],
        outputs,
    };

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
            Err(AmmError::TxBuildError(
                "T2T swaps not yet implemented".to_string(),
            ))
        }
    }
}

/// Constant positions: {1}=ExFeePerTokenDenom, {2}=Delta, {3}=BaseAmount,
/// {4}=FeeNum, {5}=RefundProp, {10}=SpectrumIsQuote, {11}=MaxExFee,
/// {13}=PoolNFT, {14}=RedeemerPropBytes, {15}=QuoteId, {16}=MinQuoteAmount,
/// {23}=SpectrumId, {27}=FeeDenom, {28}=MinerPropBytes, {31}=MaxMinerFee
fn build_n2t_swap_sell_tree(
    pool: &AmmPool,
    base_amount: u64,
    min_quote_amount: u64,
    user_pk: &str,
    recipient_ergo_tree: Option<&str>,
) -> Result<String, AmmError> {
    let template_hex = swap_templates::N2T_SWAP_SELL_TEMPLATE;
    let tree = parse_ergo_tree(template_hex)?;

    let pool_nft_bytes = hex_to_bytes(&pool.pool_id)?;
    let quote_id_bytes = hex_to_bytes(&pool.token_y.token_id)?;
    let spf_bytes = hex_to_bytes(swap_templates::SPF_TOKEN_ID)?;
    let miner_prop_bytes = hex_to_bytes(swap_templates::MINER_FEE_ERGO_TREE)?;
    // RedeemerPropBytes must be full P2PK ErgoTree (0008cd + pubkey), NOT raw pubkey
    let redeemer_prop_bytes = if let Some(recipient) = recipient_ergo_tree {
        hex_to_bytes(recipient)?
    } else {
        hex_to_bytes(&format!("0008cd{}", user_pk))?
    };
    let refund_prop = build_prove_dlog(user_pk)?;

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

/// Constant positions: {1}=BaseAmount, {2}=FeeNum, {3}=RefundProp,
/// {7}=MaxExFee, {8}=ExFeePerTokenDenom, {9}=ExFeePerTokenNum,
/// {11}=PoolNFT, {12}=RedeemerPropBytes, {13}=MinQuoteAmount,
/// {16}=SpectrumId, {20}=FeeDenom, {21}=MinerPropBytes, {24}=MaxMinerFee
fn build_n2t_swap_buy_tree(
    pool: &AmmPool,
    base_amount: u64,
    min_quote_amount: u64,
    user_pk: &str,
    recipient_ergo_tree: Option<&str>,
) -> Result<String, AmmError> {
    let template_hex = swap_templates::N2T_SWAP_BUY_TEMPLATE;
    let tree = parse_ergo_tree(template_hex)?;

    let pool_nft_bytes = hex_to_bytes(&pool.pool_id)?;
    let spf_bytes = hex_to_bytes(swap_templates::SPF_TOKEN_ID)?;
    let miner_prop_bytes = hex_to_bytes(swap_templates::MINER_FEE_ERGO_TREE)?;
    // RedeemerPropBytes must be full P2PK ErgoTree (0008cd + pubkey), NOT raw pubkey
    let redeemer_prop_bytes = if let Some(recipient) = recipient_ergo_tree {
        hex_to_bytes(recipient)?
    } else {
        hex_to_bytes(&format!("0008cd{}", user_pk))?
    };
    let refund_prop = build_prove_dlog(user_pk)?;

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

fn parse_ergo_tree(hex_str: &str) -> Result<ErgoTree, AmmError> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| AmmError::TxBuildError(format!("Invalid ErgoTree hex: {}", e)))?;
    ErgoTree::sigma_parse_bytes(&bytes)
        .map_err(|e| AmmError::TxBuildError(format!("Failed to parse ErgoTree: {}", e)))
}

fn serialize_ergo_tree(tree: &ErgoTree) -> Result<String, AmmError> {
    let bytes = tree
        .sigma_serialize_bytes()
        .map_err(|e| AmmError::TxBuildError(format!("Failed to serialize ErgoTree: {}", e)))?;
    Ok(hex::encode(bytes))
}

fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>, AmmError> {
    hex::decode(hex_str)
        .map_err(|e| AmmError::TxBuildError(format!("Invalid hex string '{}': {}", hex_str, e)))
}

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

        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        let proxy = &build.unsigned_tx.outputs[0];
        assert_ne!(
            proxy.ergo_tree,
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );
        let proxy_value: u64 = proxy.value.parse().unwrap();
        assert_eq!(proxy_value, 1_000_000_000 + EXECUTION_FEE + PROXY_BOX_VALUE);

        let fee_output = &build.unsigned_tx.outputs[1];
        assert_eq!(fee_output.value, TX_FEE.to_string());

        let change_output = &build.unsigned_tx.outputs[2];
        let change_value: u64 = change_output.value.parse().unwrap();
        let expected_change = 10_000_000_000 - proxy_value - TX_FEE;
        assert_eq!(change_value, expected_change);

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

        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        let proxy = &build.unsigned_tx.outputs[0];
        assert_eq!(proxy.assets.len(), 1);
        assert_eq!(proxy.assets[0].token_id, token_id);
        assert_eq!(proxy.assets[0].amount, "10000");

        let proxy_value: u64 = proxy.value.parse().unwrap();
        assert_eq!(proxy_value, EXECUTION_FEE + PROXY_BOX_VALUE);

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

        assert_eq!(build.unsigned_tx.outputs.len(), 2);
    }

    #[test]
    fn test_build_prove_dlog() {
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
        let tree = parse_ergo_tree(swap_templates::N2T_SWAP_SELL_TEMPLATE).unwrap();
        let num_constants = tree.constants_len().unwrap();
        assert!(
            num_constants >= 32,
            "N2T SwapSell should have at least 32 constants, got {}",
            num_constants
        );
    }

    #[test]
    fn test_parse_n2t_swap_buy_template() {
        let tree = parse_ergo_tree(swap_templates::N2T_SWAP_BUY_TEMPLATE).unwrap();
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
        let sell_tree = parse_ergo_tree(swap_templates::N2T_SWAP_SELL_TEMPLATE).unwrap();
        let buy_tree = parse_ergo_tree(swap_templates::N2T_SWAP_BUY_TEMPLATE).unwrap();

        assert_eq!(sell_tree.constants_len().unwrap(), 33);
        let c5 = sell_tree.get_constant(5).unwrap().unwrap(); // RefundProp
        assert_eq!(format!("{:?}", c5.tpe), "SSigmaProp");
        let c13 = sell_tree.get_constant(13).unwrap().unwrap(); // PoolNFT
        assert_eq!(format!("{:?}", c13.tpe), "SColl(SByte)");

        assert_eq!(buy_tree.constants_len().unwrap(), 26);
        let c3 = buy_tree.get_constant(3).unwrap().unwrap(); // RefundProp
        assert_eq!(format!("{:?}", c3.tpe), "SSigmaProp");
        let c11 = buy_tree.get_constant(11).unwrap().unwrap(); // PoolNFT
        assert_eq!(format!("{:?}", c11.tpe), "SColl(SByte)");
    }
}
