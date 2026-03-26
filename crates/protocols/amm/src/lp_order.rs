//! LP deposit/redeem proxy order builder for Spectrum bots.

use std::collections::HashMap;

use ergo_lib::ergo_chain_types::EcPoint;
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::mir::constant::Constant;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
use serde::{Deserialize, Serialize};

use crate::constants::lp_templates;
use crate::state::{AmmError, AmmPool, PoolType};
use ergo_tx::{
    append_change_output, select_token_boxes, Eip12Asset, Eip12InputBox, Eip12Output,
    Eip12UnsignedTx,
};

const PROXY_BOX_VALUE: u64 = 4_000_000;
const TX_FEE: u64 = citadel_core::constants::TX_FEE_NANO as u64;
const EXECUTION_FEE: u64 = lp_templates::EXECUTION_FEE;
const MIN_CHANGE_VALUE: u64 = 1_000_000;

#[derive(Debug)]
pub struct LpOrderBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: LpOrderSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpOrderSummary {
    pub operation: String,
    pub erg_amount: u64,
    pub token_amount: u64,
    pub token_name: String,
    pub lp_amount: u64,
    pub execution_fee: u64,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn build_lp_deposit_order_eip12(
    pool: &AmmPool,
    erg_amount: u64,
    token_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    user_pk: &str,
    current_height: i32,
    execution_fee: Option<u64>,
) -> Result<LpOrderBuildResult, AmmError> {
    match pool.pool_type {
        PoolType::N2T => {}
        PoolType::T2T => {
            return Err(AmmError::TxBuildError(
                "LP deposit proxy not yet supported for T2T pools".to_string(),
            ));
        }
    }

    let ex_fee = execution_fee.unwrap_or(EXECUTION_FEE);

    let proxy_box_value = erg_amount
        .checked_add(ex_fee)
        .and_then(|v| v.checked_add(PROXY_BOX_VALUE))
        .ok_or_else(|| {
            AmmError::TxBuildError(
                "Arithmetic overflow calculating proxy box value".to_string(),
            )
        })?;

    let total_erg_needed = proxy_box_value.checked_add(TX_FEE).ok_or_else(|| {
        AmmError::TxBuildError("Arithmetic overflow calculating total ERG needed".to_string())
    })?;

    let selected =
        select_token_boxes(user_utxos, &pool.token_y.token_id, token_amount, total_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let deposit_ergo_tree_hex = build_deposit_ergo_tree(pool, erg_amount, user_pk, ex_fee)?;

    let proxy_output = Eip12Output {
        value: proxy_box_value.to_string(),
        ergo_tree: deposit_ergo_tree_hex,
        assets: vec![Eip12Asset {
            token_id: pool.token_y.token_id.clone(),
            amount: token_amount.to_string(),
        }],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    let mut outputs = vec![proxy_output, fee_output];
    let spent = [(pool.token_y.token_id.as_str(), token_amount)];
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

    let lp_reward = if let Some(erg_reserves) = pool.erg_reserves {
        crate::calculator::calculate_lp_reward(
            erg_reserves,
            pool.token_y.amount,
            pool.lp_circulating,
            erg_amount,
            token_amount,
        )
    } else {
        0
    };

    let token_name = pool
        .token_y
        .name
        .clone()
        .unwrap_or_else(|| pool.token_y.token_id[..8].to_string());

    let summary = LpOrderSummary {
        operation: "Deposit".to_string(),
        erg_amount,
        token_amount,
        token_name,
        lp_amount: lp_reward,
        execution_fee: ex_fee,
        miner_fee: TX_FEE,
        total_erg_cost: total_erg_needed,
    };

    Ok(LpOrderBuildResult {
        unsigned_tx,
        summary,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_lp_redeem_order_eip12(
    pool: &AmmPool,
    lp_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    user_pk: &str,
    current_height: i32,
    execution_fee: Option<u64>,
) -> Result<LpOrderBuildResult, AmmError> {
    match pool.pool_type {
        PoolType::N2T => {}
        PoolType::T2T => {
            return Err(AmmError::TxBuildError(
                "LP redeem proxy not yet supported for T2T pools".to_string(),
            ));
        }
    }

    let ex_fee = execution_fee.unwrap_or(EXECUTION_FEE);

    // No ERG deposit for redeem -- user just sends LP tokens
    let proxy_box_value = ex_fee.checked_add(PROXY_BOX_VALUE).ok_or_else(|| {
        AmmError::TxBuildError("Arithmetic overflow calculating proxy box value".to_string())
    })?;

    let total_erg_needed = proxy_box_value.checked_add(TX_FEE).ok_or_else(|| {
        AmmError::TxBuildError("Arithmetic overflow calculating total ERG needed".to_string())
    })?;

    let selected =
        select_token_boxes(user_utxos, &pool.lp_token_id, lp_amount, total_erg_needed)
            .map_err(|e| AmmError::TxBuildError(e.to_string()))?;

    let redeem_ergo_tree_hex = build_redeem_ergo_tree(pool, user_pk, ex_fee)?;

    let proxy_output = Eip12Output {
        value: proxy_box_value.to_string(),
        ergo_tree: redeem_ergo_tree_hex,
        assets: vec![Eip12Asset {
            token_id: pool.lp_token_id.clone(),
            amount: lp_amount.to_string(),
        }],
        creation_height: current_height,
        additional_registers: HashMap::new(),
    };

    let fee_output = Eip12Output::fee(TX_FEE as i64, current_height);

    let mut outputs = vec![proxy_output, fee_output];
    let spent = [(pool.lp_token_id.as_str(), lp_amount)];
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

    let (erg_share, token_share) = if let Some(erg_reserves) = pool.erg_reserves {
        crate::calculator::calculate_redeem_shares(
            erg_reserves,
            pool.token_y.amount,
            pool.lp_circulating,
            lp_amount,
        )
    } else {
        (0, 0)
    };

    let token_name = pool
        .token_y
        .name
        .clone()
        .unwrap_or_else(|| pool.token_y.token_id[..8].to_string());

    let summary = LpOrderSummary {
        operation: "Redeem".to_string(),
        erg_amount: erg_share,
        token_amount: token_share,
        token_name,
        lp_amount,
        execution_fee: ex_fee,
        miner_fee: TX_FEE,
        total_erg_cost: total_erg_needed,
    };

    Ok(LpOrderBuildResult {
        unsigned_tx,
        summary,
    })
}

fn build_deposit_ergo_tree(
    pool: &AmmPool,
    erg_amount: u64,
    user_pk: &str,
    ex_fee: u64,
) -> Result<String, AmmError> {
    let tree = parse_ergo_tree(lp_templates::N2T_DEPOSIT_TEMPLATE)?;
    let pool_nft_bytes = hex_to_bytes(&pool.pool_id)?;
    let refund_prop = build_prove_dlog(user_pk)?;

    let tree = tree
        .with_constant(0, Constant::from(refund_prop))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set RefundProp: {}", e)))?
        .with_constant(2, Constant::from(erg_amount as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set SelfX: {}", e)))?
        .with_constant(12, Constant::from(pool_nft_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set PoolNFT: {}", e)))?
        .with_constant(15, Constant::from(ex_fee as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set ExFee: {}", e)))?
        .with_constant(16, Constant::from(erg_amount as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set SelfX repeat: {}", e)))?
        .with_constant(17, Constant::from(ex_fee as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set ExFee repeat: {}", e)))?
        .with_constant(
            22,
            Constant::from(lp_templates::DEFAULT_MAX_MINER_FEE as i64),
        )
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set MaxMinerFee: {}", e)))?;

    serialize_ergo_tree(&tree)
}

fn build_redeem_ergo_tree(
    pool: &AmmPool,
    user_pk: &str,
    ex_fee: u64,
) -> Result<String, AmmError> {
    let tree = parse_ergo_tree(lp_templates::N2T_REDEEM_TEMPLATE)?;
    let pool_nft_bytes = hex_to_bytes(&pool.pool_id)?;
    let refund_prop = build_prove_dlog(user_pk)?;

    let tree = tree
        .with_constant(0, Constant::from(refund_prop))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set RefundProp: {}", e)))?
        .with_constant(11, Constant::from(pool_nft_bytes))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set PoolNFT: {}", e)))?
        .with_constant(12, Constant::from(ex_fee as i64))
        .map_err(|e| AmmError::TxBuildError(format!("Failed to set ExFee: {}", e)))?
        .with_constant(
            16,
            Constant::from(lp_templates::DEFAULT_MAX_MINER_FEE as i64),
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
    use crate::state::{AmmPool, PoolType, TokenAmount};

    fn test_n2t_pool() -> AmmPool {
        AmmPool {
            pool_id: "0000000000000000000000000000000000000000000000000000000000000001"
                .to_string(),
            pool_type: PoolType::N2T,
            box_id: "box1".to_string(),
            erg_reserves: Some(100_000_000_000),
            token_x: None,
            token_y: TokenAmount {
                token_id:
                    "0000000000000000000000000000000000000000000000000000000000000002"
                        .to_string(),
                amount: 1_000_000,
                decimals: Some(6),
                name: Some("TestToken".to_string()),
            },
            lp_token_id:
                "0000000000000000000000000000000000000000000000000000000000000003"
                    .to_string(),
            lp_circulating: 1000,
            fee_num: 997,
            fee_denom: 1000,
        }
    }

    fn test_user_utxo_with_token_y() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "utxo2".to_string(),
            transaction_id: "tx2".to_string(),
            index: 0,
            value: "10000000000".to_string(), // 10 ERG
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![Eip12Asset {
                token_id:
                    "0000000000000000000000000000000000000000000000000000000000000002"
                        .to_string(),
                amount: "500000".to_string(), // 500k token_y
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    fn test_user_utxo_with_lp() -> Eip12InputBox {
        Eip12InputBox {
            box_id: "utxo3".to_string(),
            transaction_id: "tx3".to_string(),
            index: 0,
            value: "10000000000".to_string(), // 10 ERG
            ergo_tree:
                "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
                    .to_string(),
            assets: vec![Eip12Asset {
                token_id:
                    "0000000000000000000000000000000000000000000000000000000000000003"
                        .to_string(),
                amount: "500".to_string(), // 500 LP tokens
            }],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn test_parse_deposit_template() {
        let result = parse_ergo_tree(lp_templates::N2T_DEPOSIT_TEMPLATE);
        assert!(
            result.is_ok(),
            "Should parse N2T deposit template: {:?}",
            result.err()
        );

        let tree = result.unwrap();
        let num_constants = tree.constants_len().unwrap();
        assert!(
            num_constants >= 23,
            "N2T deposit template should have at least 23 constants (for position 22), got {}",
            num_constants
        );
    }

    #[test]
    fn test_parse_redeem_template() {
        let result = parse_ergo_tree(lp_templates::N2T_REDEEM_TEMPLATE);
        assert!(
            result.is_ok(),
            "Should parse N2T redeem template: {:?}",
            result.err()
        );

        let tree = result.unwrap();
        let num_constants = tree.constants_len().unwrap();
        assert!(
            num_constants >= 17,
            "N2T redeem template should have at least 17 constants (for position 16), got {}",
            num_constants
        );
    }

    #[test]
    fn test_build_deposit_order() {
        let pool = test_n2t_pool();
        let user_utxo = test_user_utxo_with_token_y();
        let user_pk = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

        let result = build_lp_deposit_order_eip12(
            &pool,
            1_000_000_000, // 1 ERG deposit
            100_000,       // 100k tokens
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            user_pk,
            1_000_000,
            None,
        );

        assert!(result.is_ok(), "Should build deposit order: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        let proxy = &build.unsigned_tx.outputs[0];
        let proxy_value: u64 = proxy.value.parse().unwrap();
        assert_eq!(
            proxy_value,
            1_000_000_000 + EXECUTION_FEE + PROXY_BOX_VALUE
        );
        assert_eq!(proxy.assets.len(), 1);
        assert_eq!(
            proxy.assets[0].token_id,
            "0000000000000000000000000000000000000000000000000000000000000002"
        );
        assert_eq!(proxy.assets[0].amount, "100000");
        assert_ne!(
            proxy.ergo_tree,
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );

        let fee_output = &build.unsigned_tx.outputs[1];
        assert_eq!(fee_output.value, TX_FEE.to_string());

        assert_eq!(build.summary.operation, "Deposit");
        assert_eq!(build.summary.erg_amount, 1_000_000_000);
        assert_eq!(build.summary.token_amount, 100_000);
        assert_eq!(build.summary.execution_fee, EXECUTION_FEE);
        assert_eq!(build.summary.miner_fee, TX_FEE);
    }

    #[test]
    fn test_build_redeem_order() {
        let pool = test_n2t_pool();
        let user_utxo = test_user_utxo_with_lp();
        let user_pk = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

        let result = build_lp_redeem_order_eip12(
            &pool,
            100, // 100 LP tokens
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            user_pk,
            1_000_000,
            None,
        );

        assert!(result.is_ok(), "Should build redeem order: {:?}", result.err());
        let build = result.unwrap();

        assert_eq!(build.unsigned_tx.outputs.len(), 3);

        let proxy = &build.unsigned_tx.outputs[0];
        let proxy_value: u64 = proxy.value.parse().unwrap();
        assert_eq!(proxy_value, EXECUTION_FEE + PROXY_BOX_VALUE);
        assert_eq!(proxy.assets.len(), 1);
        assert_eq!(
            proxy.assets[0].token_id,
            "0000000000000000000000000000000000000000000000000000000000000003"
        );
        assert_eq!(proxy.assets[0].amount, "100");
        assert_ne!(
            proxy.ergo_tree,
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );

        let fee_output = &build.unsigned_tx.outputs[1];
        assert_eq!(fee_output.value, TX_FEE.to_string());

        assert_eq!(build.summary.operation, "Redeem");
        assert_eq!(build.summary.lp_amount, 100);
        assert_eq!(build.summary.execution_fee, EXECUTION_FEE);
        assert_eq!(build.summary.miner_fee, TX_FEE);
    }

    #[test]
    fn test_deposit_order_insufficient_erg() {
        let pool = test_n2t_pool();
        let user_utxo = Eip12InputBox {
            value: "1000000".to_string(), // 0.001 ERG - not enough
            ..test_user_utxo_with_token_y()
        };

        let result = build_lp_deposit_order_eip12(
            &pool,
            1_000_000_000, // 1 ERG deposit
            100_000,
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
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
    fn test_redeem_order_insufficient_lp() {
        let pool = test_n2t_pool();
        let user_utxo = Eip12InputBox {
            assets: vec![Eip12Asset {
                token_id:
                    "0000000000000000000000000000000000000000000000000000000000000003"
                        .to_string(),
                amount: "10".to_string(), // Only 10 LP tokens
            }],
            ..test_user_utxo_with_lp()
        };

        let result = build_lp_redeem_order_eip12(
            &pool,
            500, // Need 500 LP, only have 10
            &[user_utxo],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Insufficient"),
            "Should report insufficient LP tokens: {}",
            err
        );
    }

    #[test]
    fn test_deposit_order_t2t_rejected() {
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

        let result = build_lp_deposit_order_eip12(
            &pool,
            1_000_000_000,
            100_000,
            &[test_user_utxo_with_token_y()],
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            1_000_000,
            None,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("T2T"));
    }

    #[test]
    fn test_build_deposit_ergo_tree_roundtrip() {
        let pool = test_n2t_pool();
        let pk = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

        let result = build_deposit_ergo_tree(&pool, 1_000_000_000, pk, EXECUTION_FEE);
        assert!(
            result.is_ok(),
            "Should build deposit tree: {:?}",
            result.err()
        );

        let hex = result.unwrap();
        let roundtrip = parse_ergo_tree(&hex);
        assert!(
            roundtrip.is_ok(),
            "Built deposit tree should round-trip: {:?}",
            roundtrip.err()
        );
    }

    #[test]
    fn test_build_redeem_ergo_tree_roundtrip() {
        let pool = test_n2t_pool();
        let pk = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

        let result = build_redeem_ergo_tree(&pool, pk, EXECUTION_FEE);
        assert!(
            result.is_ok(),
            "Should build redeem tree: {:?}",
            result.err()
        );

        let hex = result.unwrap();
        let roundtrip = parse_ergo_tree(&hex);
        assert!(
            roundtrip.is_ok(),
            "Built redeem tree should round-trip: {:?}",
            roundtrip.err()
        );
    }
}
