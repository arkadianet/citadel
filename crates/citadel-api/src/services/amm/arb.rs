//! Pre-built 0-conf arb / swap / split chain execution orchestration.

use crate::services::error::IntoServiceError;
use crate::AppState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbChainLegDto {
    pub pool_id: String,
    pub tx_id: String,
    pub unsigned_tx: serde_json::Value,
    pub summary: amm::DirectSwapSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbChainBuildResponse {
    pub legs: Vec<ArbChainLegDto>,
    pub projected_profit_nano: i64,
}

/// Build a full arb chain over `pool_ids` (hop order). Pools are re-fetched
/// fresh; aborts if the recomputed profit dropped below `min_profit_nano`.
pub async fn build_arb_chain_tx(
    state: &AppState,
    pool_ids: Vec<String>,
    input_nano: u64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
    min_profit_nano: Option<i64>,
) -> Result<ArbChainBuildResponse, String> {
    let client = state.require_node_client().await?;

    let all_pools = amm::discover_pools(&client).await.into_service()?;
    let mut pools: Vec<(amm::AmmPool, ergo_tx::Eip12InputBox)> = Vec::with_capacity(pool_ids.len());
    for pool_id in &pool_ids {
        let pool = all_pools
            .iter()
            .find(|p| &p.pool_id == pool_id)
            .cloned()
            .ok_or_else(|| format!("Pool not found: {}", pool_id))?;
        let pool_box = client
            .get_eip12_box_by_id(&pool.box_id)
            .await
            .map_err(|e| format!("Failed to fetch pool box: {}", e))?;
        pools.push((pool, pool_box));
    }

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let build = amm::build_arb_chain(
        &pools,
        input_nano,
        &user_utxos,
        &user_ergo_tree,
        current_height,
        min_profit_nano.unwrap_or(0),
    )
    .into_service()?;

    let legs = build
        .legs
        .into_iter()
        .map(|leg| {
            Ok(ArbChainLegDto {
                pool_id: leg.pool_id,
                tx_id: leg.tx_id,
                unsigned_tx: serde_json::to_value(&leg.unsigned_tx)
                    .map_err(|e| format!("Failed to serialize leg tx: {}", e))?,
                summary: leg.summary,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(ArbChainBuildResponse {
        legs,
        projected_profit_nano: build.projected_profit_nano,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbLegSignResponse {
    pub request_id: String,
    pub nautilus_url: String,
}

/// Start a sign-only Nautilus request for one arb leg. The signed tx is
/// captured by the local server and broadcast later via `submit_arb_chain`.
pub async fn start_arb_leg_sign(
    state: &AppState,
    unsigned_tx: serde_json::Value,
    message: String,
) -> Result<ArbLegSignResponse, String> {
    let server = state.ergopay_server().await.into_service()?;
    let request_id = server.create_sign_only_request(unsigned_tx, message).await;
    let nautilus_url = server.get_nautilus_url(&request_id);
    Ok(ArbLegSignResponse {
        request_id,
        nautilus_url,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbChainSubmitResponse {
    /// Tx ids of successfully broadcast legs, in order.
    pub tx_ids: Vec<String>,
    /// Index of the first leg that failed to broadcast (if any).
    pub failed_leg: Option<usize>,
    pub error: Option<String>,
}

/// Broadcast the signed legs in order. Stops at the first rejection so the
/// caller can report exactly which legs landed.
pub async fn submit_arb_chain(
    state: &AppState,
    request_ids: Vec<String>,
) -> Result<ArbChainSubmitResponse, String> {
    let client = state.require_node_client().await?;
    let server = state.ergopay_server().await.into_service()?;

    // Collect all signed txs first -- refuse to broadcast a partial chain.
    let mut signed_txs = Vec::with_capacity(request_ids.len());
    for (idx, request_id) in request_ids.iter().enumerate() {
        let signed = server
            .get_signed_tx(request_id)
            .await
            .ok_or_else(|| format!("Leg {} is not signed yet", idx + 1))?;
        signed_txs.push(signed);
    }

    let mut tx_ids = Vec::with_capacity(signed_txs.len());
    for (idx, signed_tx) in signed_txs.iter().enumerate() {
        match client.submit_transaction(signed_tx).await {
            Ok(tx_id) => tx_ids.push(tx_id),
            Err(e) => {
                return Ok(ArbChainSubmitResponse {
                    tx_ids,
                    failed_leg: Some(idx),
                    error: Some(format!("Leg {} rejected: {}", idx + 1, e)),
                });
            }
        }
    }

    Ok(ArbChainSubmitResponse {
        tx_ids,
        failed_leg: None,
        error: None,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapChainBuildResponse {
    pub legs: Vec<ArbChainLegDto>,
    /// Token the chain ends in (null = ERG).
    pub final_token: Option<String>,
    pub final_output: u64,
}

/// Build a multi-hop swap chain over `pool_ids` (hop order) starting from
/// `source_token` (None = ERG). Same 0-conf pre-built chaining as arb
/// execution, but for open routes (ends in the target token).
pub async fn build_swap_chain_tx(
    state: &AppState,
    pool_ids: Vec<String>,
    source_token: Option<String>,
    input_amount: u64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> Result<SwapChainBuildResponse, String> {
    let client = state.require_node_client().await?;

    let all_pools = amm::discover_pools(&client).await.into_service()?;
    let mut pools: Vec<(amm::AmmPool, ergo_tx::Eip12InputBox)> = Vec::with_capacity(pool_ids.len());
    for pool_id in &pool_ids {
        let pool = all_pools
            .iter()
            .find(|p| &p.pool_id == pool_id)
            .cloned()
            .ok_or_else(|| format!("Pool not found: {}", pool_id))?;
        let pool_box = client
            .get_eip12_box_by_id(&pool.box_id)
            .await
            .map_err(|e| format!("Failed to fetch pool box: {}", e))?;
        pools.push((pool, pool_box));
    }

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    // Frontend sends "ERG" for the native side; the builder wants None.
    let source = source_token.filter(|t| t != "ERG");

    let build = amm::build_swap_chain(
        &pools,
        source,
        input_amount,
        &user_utxos,
        &user_ergo_tree,
        current_height,
    )
    .into_service()?;

    let legs = build
        .legs
        .into_iter()
        .map(|leg| {
            Ok(ArbChainLegDto {
                pool_id: leg.pool_id,
                tx_id: leg.tx_id,
                unsigned_tx: serde_json::to_value(&leg.unsigned_tx)
                    .map_err(|e| format!("Failed to serialize leg tx: {}", e))?,
                summary: leg.summary,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(SwapChainBuildResponse {
        legs,
        final_token: build.final_token,
        final_output: build.final_output,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitAllocationInput {
    pub pool_ids: Vec<String>,
    pub source_token: Option<String>,
    pub input_amount: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitAllocationSummaryDto {
    pub input_amount: u64,
    pub output_amount: u64,
    pub final_token: Option<String>,
    pub leg_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitChainBuildResponse {
    pub legs: Vec<ArbChainLegDto>,
    pub allocations: Vec<SplitAllocationSummaryDto>,
    pub total_output: u64,
    pub final_token: Option<String>,
}

/// Pre-build a split as a flat list of 0-conf chained legs across allocations.
/// Allocations must use disjoint pools; UTXOs are threaded between them.
pub async fn build_split_chains_tx(
    state: &AppState,
    allocations: Vec<SplitAllocationInput>,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
    min_total_output: Option<u64>,
) -> Result<SplitChainBuildResponse, String> {
    let client = state.require_node_client().await?;

    let all_pools = amm::discover_pools(&client).await.into_service()?;
    let mut specs: Vec<amm::SplitChainSpec> = Vec::with_capacity(allocations.len());

    for alloc in &allocations {
        let mut pools: Vec<(amm::AmmPool, ergo_tx::Eip12InputBox)> =
            Vec::with_capacity(alloc.pool_ids.len());
        for pool_id in &alloc.pool_ids {
            let pool = all_pools
                .iter()
                .find(|p| &p.pool_id == pool_id)
                .cloned()
                .ok_or_else(|| format!("Pool not found: {}", pool_id))?;
            let pool_box = client
                .get_eip12_box_by_id(&pool.box_id)
                .await
                .map_err(|e| format!("Failed to fetch pool box: {}", e))?;
            pools.push((pool, pool_box));
        }
        let source = alloc.source_token.clone().filter(|t| t != "ERG");
        specs.push(amm::SplitChainSpec {
            pools,
            source_token: source,
            input_amount: alloc.input_amount,
        });
    }

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let build = amm::build_split_chains(
        &specs,
        &user_utxos,
        &user_ergo_tree,
        current_height,
        min_total_output,
    )
    .into_service()?;

    let legs = build
        .legs
        .into_iter()
        .map(|leg| {
            Ok(ArbChainLegDto {
                pool_id: leg.pool_id,
                tx_id: leg.tx_id,
                unsigned_tx: serde_json::to_value(&leg.unsigned_tx)
                    .map_err(|e| format!("Failed to serialize leg tx: {}", e))?,
                summary: leg.summary,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(SplitChainBuildResponse {
        legs,
        allocations: build
            .allocations
            .into_iter()
            .map(|a| SplitAllocationSummaryDto {
                input_amount: a.input_amount,
                output_amount: a.output_amount,
                final_token: a.final_token,
                leg_count: a.leg_count,
            })
            .collect(),
        total_output: build.total_output,
        final_token: build.final_token,
    })
}
