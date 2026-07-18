//! Circular-arb chain builder: N pre-built direct-swap txs, each spending the
//! previous leg's not-yet-broadcast outputs (0-conf chaining).
//!
//! Spectrum CFMM v1 pool contracts hardcode `successor = OUTPUTS(0)`, so one
//! tx can spend at most one pool box — a multi-hop arb MUST be sequential txs.
//! Ergo txIds hash the unsigned tx (proofs excluded), so every leg's output
//! box ids are known before anything is signed.
//!
//! Slippage tolerance is deliberately absent: each leg spends a specific pool
//! box id, so the chain either executes at exactly the computed amounts or a
//! leg fails wholesale (double-spend) when someone else moves the pool first.

use serde::{Deserialize, Serialize};

use crate::direct_swap::{build_direct_swap_eip12, DirectSwapSummary};
use crate::state::{AmmError, AmmPool, PoolType, SwapInput};
use ergo_tx::{derive_output_boxes, Eip12InputBox, Eip12UnsignedTx};

/// One pre-built leg of an arb chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbChainLeg {
    pub pool_id: String,
    /// Deterministic txId of the unsigned leg.
    pub tx_id: String,
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: DirectSwapSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbChainBuild {
    pub legs: Vec<ArbChainLeg>,
    /// Net ERG the user's box set gains across the whole chain (all miner
    /// fees and min-box-value dust already accounted for).
    pub projected_profit_nano: i64,
}

/// A generic multi-hop swap chain (open route, may end in any token).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapChainBuild {
    pub legs: Vec<ArbChainLeg>,
    /// Token the chain ends in (None = ERG).
    pub final_token: Option<String>,
    /// Amount of `final_token` the last leg pays out.
    pub final_output: u64,
}

/// Build a full ERG -> ... -> ERG arb chain over `pools` (in hop order).
///
/// `pools` must be freshly fetched (pool state + current pool box). The
/// chain aborts if the recomputed profit is below `min_profit_nano`.
pub fn build_arb_chain(
    pools: &[(AmmPool, Eip12InputBox)],
    input_nano: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
    min_profit_nano: i64,
) -> Result<ArbChainBuild, AmmError> {
    let initial_erg = sum_erg(user_utxos)?;

    let (legs, final_token, _final_amount, available) = build_chain_core(
        pools,
        None,
        input_nano,
        user_utxos,
        user_ergo_tree,
        current_height,
    )?;

    if final_token.is_some() {
        return Err(AmmError::TxBuildError(
            "Arb route does not end in ERG".to_string(),
        ));
    }

    let final_erg = sum_erg(&available)?;
    let projected_profit_nano = final_erg as i64 - initial_erg as i64;

    if projected_profit_nano < min_profit_nano {
        return Err(AmmError::TxBuildError(format!(
            "Arb no longer profitable: projected {} nanoERG (minimum {})",
            projected_profit_nano, min_profit_nano
        )));
    }

    Ok(ArbChainBuild {
        legs,
        projected_profit_nano,
    })
}

/// Build a multi-hop swap chain over `pools` (in hop order), starting from
/// `source_token` (None = ERG). Open route: may end in any token.
pub fn build_swap_chain(
    pools: &[(AmmPool, Eip12InputBox)],
    source_token: Option<String>,
    input_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<SwapChainBuild, AmmError> {
    let (legs, final_token, final_output, _available) = build_chain_core(
        pools,
        source_token,
        input_amount,
        user_utxos,
        user_ergo_tree,
        current_height,
    )?;

    Ok(SwapChainBuild {
        legs,
        final_token,
        final_output,
    })
}

/// Shared chain-building loop. Returns (legs, final token, final amount,
/// remaining spendable user boxes after all legs).
#[allow(clippy::type_complexity)]
fn build_chain_core(
    pools: &[(AmmPool, Eip12InputBox)],
    start_token: Option<String>,
    input_amount: u64,
    user_utxos: &[Eip12InputBox],
    user_ergo_tree: &str,
    current_height: i32,
) -> Result<(Vec<ArbChainLeg>, Option<String>, u64, Vec<Eip12InputBox>), AmmError> {
    if pools.is_empty() {
        return Err(AmmError::TxBuildError("Empty route".to_string()));
    }
    if input_amount == 0 {
        return Err(AmmError::TxBuildError("Input must be > 0".to_string()));
    }

    // Boxes the user can spend at each step: wallet UTXOs initially, then
    // shrinks by leg inputs and grows by derived leg outputs.
    let mut available: Vec<Eip12InputBox> = user_utxos.to_vec();

    // None = ERG, Some(id) = token
    let mut current_token: Option<String> = start_token;
    let mut current_amount = input_amount;

    let mut legs: Vec<ArbChainLeg> = Vec::with_capacity(pools.len());

    for (pool, pool_box) in pools {
        let input = match &current_token {
            None => SwapInput::Erg {
                amount: current_amount,
            },
            Some(token_id) => SwapInput::Token {
                token_id: token_id.clone(),
                amount: current_amount,
            },
        };

        let output_token = next_token(pool, current_token.as_deref())?;

        let build = build_direct_swap_eip12(
            pool_box,
            pool,
            &input,
            1, // min_output: pre-built legs are all-or-nothing (see module docs)
            &available,
            user_ergo_tree,
            current_height,
            None,
            None,
        )?;

        // Remove the user boxes this leg consumed (inputs minus the pool box).
        let consumed: Vec<String> = build
            .unsigned_tx
            .inputs
            .iter()
            .skip(1)
            .map(|i| i.box_id.clone())
            .collect();
        available.retain(|b| !consumed.contains(&b.box_id));

        // Add this leg's user-owned outputs as spendable boxes for later legs.
        let (tx_id, output_boxes) = derive_output_boxes(&build.unsigned_tx)
            .map_err(|e| AmmError::TxBuildError(format!("Chain derivation failed: {}", e)))?;
        available.extend(
            output_boxes
                .into_iter()
                .filter(|b| b.ergo_tree == user_ergo_tree),
        );

        current_amount = build.summary.output_amount;
        current_token = output_token;

        legs.push(ArbChainLeg {
            pool_id: pool.pool_id.clone(),
            tx_id,
            unsigned_tx: build.unsigned_tx,
            summary: build.summary,
        });
    }

    Ok((legs, current_token, current_amount, available))
}

/// The token this pool outputs given the input token (None = ERG).
fn next_token(pool: &AmmPool, input_token: Option<&str>) -> Result<Option<String>, AmmError> {
    match pool.pool_type {
        PoolType::N2T => match input_token {
            None => Ok(Some(pool.token_y.token_id.clone())),
            Some(tid) if tid == pool.token_y.token_id => Ok(None),
            Some(tid) => Err(AmmError::TxBuildError(format!(
                "Token {} not in N2T pool {}",
                tid, pool.pool_id
            ))),
        },
        PoolType::T2T => {
            let token_x = pool.token_x.as_ref().ok_or_else(|| {
                AmmError::TxBuildError(format!("T2T pool {} missing token X", pool.pool_id))
            })?;
            match input_token {
                Some(tid) if tid == token_x.token_id => Ok(Some(pool.token_y.token_id.clone())),
                Some(tid) if tid == pool.token_y.token_id => Ok(Some(token_x.token_id.clone())),
                _ => Err(AmmError::TxBuildError(format!(
                    "Input token not in T2T pool {}",
                    pool.pool_id
                ))),
            }
        }
    }
}

fn sum_erg(boxes: &[Eip12InputBox]) -> Result<u64, AmmError> {
    boxes.iter().try_fold(0u64, |acc, b| {
        b.value
            .parse::<u64>()
            .map(|v| acc + v)
            .map_err(|_| AmmError::TxBuildError(format!("Invalid box value in {}", b.box_id)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TokenAmount;
    use ergo_tx::Eip12Asset;
    use std::collections::HashMap;

    // Valid P2PK trees so derive_output_boxes can parse every output.
    const USER_TREE: &str =
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const POOL_TREE: &str =
        "0008cd0327e65711a59378c59359c3e1d0f7abe906479eccb76094e50fe79d743ccc15e6";

    fn hex_id(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    fn n2t_pool(pool_byte: u8, token_byte: u8, erg_reserves: u64, token_reserves: u64) -> AmmPool {
        AmmPool {
            pool_id: hex_id(pool_byte),
            pool_type: PoolType::N2T,
            box_id: hex_id(pool_byte.wrapping_add(1)),
            erg_reserves: Some(erg_reserves),
            token_x: None,
            token_y: TokenAmount {
                token_id: hex_id(token_byte),
                amount: token_reserves,
                decimals: Some(0),
                name: None,
            },
            lp_token_id: hex_id(pool_byte.wrapping_add(2)),
            lp_circulating: 1000,
            fee_num: 997,
            fee_denom: 1000,
        }
    }

    fn pool_box(pool: &AmmPool) -> Eip12InputBox {
        Eip12InputBox {
            box_id: pool.box_id.clone(),
            transaction_id: hex_id(0xf0),
            index: 0,
            value: pool.erg_reserves.unwrap().to_string(),
            ergo_tree: POOL_TREE.to_string(),
            assets: vec![
                Eip12Asset {
                    token_id: pool.pool_id.clone(),
                    amount: "1".to_string(),
                },
                Eip12Asset {
                    token_id: pool.lp_token_id.clone(),
                    amount: "9223372036854774807".to_string(),
                },
                Eip12Asset {
                    token_id: pool.token_y.token_id.clone(),
                    amount: pool.token_y.amount.to_string(),
                },
            ],
            creation_height: 999_000,
            additional_registers: {
                let mut m = HashMap::new();
                m.insert("R4".to_string(), "04ca0f".to_string()); // fee_num=997
                m
            },
            extension: HashMap::new(),
        }
    }

    fn user_utxo(erg: u64) -> Eip12InputBox {
        Eip12InputBox {
            box_id: hex_id(0xaa),
            transaction_id: hex_id(0xab),
            index: 0,
            value: erg.to_string(),
            ergo_tree: USER_TREE.to_string(),
            assets: vec![],
            creation_height: 999_000,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    /// Two N2T pools over the same token with skewed prices: buy cheap from
    /// pool A (token-rich), sell dear to pool B (ERG-rich).
    fn arb_pools() -> Vec<(AmmPool, Eip12InputBox)> {
        let pool_a = n2t_pool(0x01, 0x33, 100_000_000_000, 2_000_000); // cheap token
        let pool_b = n2t_pool(0x11, 0x33, 200_000_000_000, 1_000_000); // dear token
        let box_a = pool_box(&pool_a);
        let box_b = pool_box(&pool_b);
        vec![(pool_a, box_a), (pool_b, box_b)]
    }

    #[test]
    fn builds_profitable_two_leg_chain() {
        let pools = arb_pools();
        let utxos = vec![user_utxo(10_000_000_000)];

        let result =
            build_arb_chain(&pools, 1_000_000_000, &utxos, USER_TREE, 1_000_000, 0).unwrap();

        assert_eq!(result.legs.len(), 2);
        assert!(
            result.projected_profit_nano > 0,
            "expected profit, got {}",
            result.projected_profit_nano
        );

        // Leg 2 must spend leg 1's derived outputs (0-conf chain wiring).
        let (leg1_txid, leg1_outputs) = derive_output_boxes(&result.legs[0].unsigned_tx).unwrap();
        assert_eq!(leg1_txid, result.legs[0].tx_id);
        let leg1_user_ids: Vec<&String> = leg1_outputs
            .iter()
            .filter(|b| b.ergo_tree == USER_TREE)
            .map(|b| &b.box_id)
            .collect();
        let leg2_user_inputs: Vec<&String> = result.legs[1]
            .unsigned_tx
            .inputs
            .iter()
            .skip(1)
            .map(|i| &i.box_id)
            .collect();
        assert!(
            leg2_user_inputs.iter().all(|id| leg1_user_ids.contains(id)),
            "leg 2 inputs {:?} not all from leg 1 outputs {:?}",
            leg2_user_inputs,
            leg1_user_ids
        );

        // Leg 1 buys tokens with ERG; leg 2 sells all of them back.
        assert_eq!(
            result.legs[0].summary.output_amount,
            result.legs[1].summary.input_amount
        );
    }

    #[test]
    fn erg_conservation_matches_profit() {
        let pools = arb_pools();
        let utxos = vec![user_utxo(10_000_000_000)];
        let input = 1_000_000_000u64;

        let result = build_arb_chain(&pools, input, &utxos, USER_TREE, 1_000_000, 0).unwrap();

        // Recompute profit independently: track every user box across legs.
        let mut available: Vec<Eip12InputBox> = utxos.clone();
        for leg in &result.legs {
            let consumed: Vec<String> = leg
                .unsigned_tx
                .inputs
                .iter()
                .skip(1)
                .map(|i| i.box_id.clone())
                .collect();
            available.retain(|b| !consumed.contains(&b.box_id));
            let (_, outputs) = derive_output_boxes(&leg.unsigned_tx).unwrap();
            available.extend(outputs.into_iter().filter(|b| b.ergo_tree == USER_TREE));
        }
        let final_erg: u64 = available
            .iter()
            .map(|b| b.value.parse::<u64>().unwrap())
            .sum();
        assert_eq!(
            final_erg as i64 - 10_000_000_000i64,
            result.projected_profit_nano
        );
    }

    #[test]
    fn aborts_when_profit_below_minimum() {
        // Symmetric pools: round trip only loses fees -> profit negative.
        let pool_a = n2t_pool(0x01, 0x33, 100_000_000_000, 1_000_000);
        let pool_b = n2t_pool(0x11, 0x33, 100_000_000_000, 1_000_000);
        let box_a = pool_box(&pool_a);
        let box_b = pool_box(&pool_b);
        let pools = vec![(pool_a, box_a), (pool_b, box_b)];
        let utxos = vec![user_utxo(10_000_000_000)];

        let err =
            build_arb_chain(&pools, 1_000_000_000, &utxos, USER_TREE, 1_000_000, 0).unwrap_err();
        assert!(
            err.to_string().contains("no longer profitable"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn rejects_route_not_ending_in_erg() {
        // Single N2T leg ERG -> token: ends holding a token, not ERG.
        let pool_a = n2t_pool(0x01, 0x33, 100_000_000_000, 1_000_000);
        let box_a = pool_box(&pool_a);
        let pools = vec![(pool_a, box_a)];
        let utxos = vec![user_utxo(10_000_000_000)];

        let err =
            build_arb_chain(&pools, 1_000_000_000, &utxos, USER_TREE, 1_000_000, 0).unwrap_err();
        assert!(err.to_string().contains("does not end in ERG"));
    }

    #[test]
    fn rejects_insufficient_balance() {
        let pools = arb_pools();
        let utxos = vec![user_utxo(50_000_000)]; // 0.05 ERG, far below input
        let result = build_arb_chain(&pools, 1_000_000_000, &utxos, USER_TREE, 1_000_000, 0);
        assert!(result.is_err());
    }

    #[test]
    fn swap_chain_erg_to_token_single_hop() {
        let pool = n2t_pool(0x01, 0x33, 100_000_000_000, 1_000_000);
        let bx = pool_box(&pool);
        let pools = vec![(pool, bx)];
        let utxos = vec![user_utxo(10_000_000_000)];

        let result =
            build_swap_chain(&pools, None, 1_000_000_000, &utxos, USER_TREE, 1_000_000).unwrap();
        assert_eq!(result.legs.len(), 1);
        assert_eq!(result.final_token, Some(hex_id(0x33)));
        assert_eq!(result.final_output, result.legs[0].summary.output_amount);
        assert!(result.final_output > 0);
    }

    #[test]
    fn swap_chain_token_to_token_via_erg() {
        // token A -> ERG (pool A), ERG -> token B (pool B)
        let pool_a = n2t_pool(0x01, 0x33, 100_000_000_000, 1_000_000);
        let pool_b = n2t_pool(0x11, 0x44, 100_000_000_000, 5_000_000);
        let box_a = pool_box(&pool_a);
        let box_b = pool_box(&pool_b);
        let pools = vec![(pool_a, box_a), (pool_b, box_b)];

        // User holds token A plus ERG for fees.
        let mut utxo = user_utxo(5_000_000_000);
        utxo.assets.push(Eip12Asset {
            token_id: hex_id(0x33),
            amount: "10000".to_string(),
        });
        let utxos = vec![utxo];

        let result = build_swap_chain(
            &pools,
            Some(hex_id(0x33)),
            10_000,
            &utxos,
            USER_TREE,
            1_000_000,
        )
        .unwrap();

        assert_eq!(result.legs.len(), 2);
        assert_eq!(result.final_token, Some(hex_id(0x44)));
        // Leg 2 consumes exactly leg 1's ERG output.
        assert_eq!(
            result.legs[0].summary.output_amount,
            result.legs[1].summary.input_amount
        );
        // Chain wiring: leg 2 user inputs come from leg 1 derived outputs.
        let (_, leg1_outputs) = derive_output_boxes(&result.legs[0].unsigned_tx).unwrap();
        let leg1_ids: Vec<&String> = leg1_outputs
            .iter()
            .filter(|b| b.ergo_tree == USER_TREE)
            .map(|b| &b.box_id)
            .collect();
        assert!(result.legs[1]
            .unsigned_tx
            .inputs
            .iter()
            .skip(1)
            .all(|i| leg1_ids.contains(&&i.box_id)));
    }
}
