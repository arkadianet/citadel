//! Router types and constants.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::state::AmmPool;

pub const ERG_TOKEN_ID: &str = "ERG";
pub const DEFAULT_MIN_LIQUIDITY_NANO: u64 = 10_000_000_000; // 10 ERG
pub const DEFAULT_MAX_POOLS_PER_PAIR: usize = 3;

/// Network miner fee charged once per hop (each hop is its own tx).
pub const MINER_FEE_PER_HOP: u64 = citadel_core::constants::TX_FEE_NANO as u64;

pub fn miner_fees_for_hops(hops: usize) -> u64 {
    MINER_FEE_PER_HOP.saturating_mul(hops as u64)
}

#[derive(Debug, Clone)]
pub struct PoolEdge {
    pub pool: AmmPool,
    pub token_in: String,
    pub token_out: String,
    pub reserves_in: u64,
    pub reserves_out: u64,
}

#[derive(Debug, Clone)]
pub struct PoolGraph {
    pub adjacency: HashMap<String, Vec<PoolEdge>>,
    pub pool_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteHop {
    pub pool_id: String,
    pub pool_type: String,
    pub token_in: String,
    pub token_in_name: Option<String>,
    pub token_in_decimals: u8,
    pub token_out: String,
    pub token_out_name: Option<String>,
    pub token_out_decimals: u8,
    pub pool_display_name: Option<String>,
    pub input_amount: u64,
    pub output_amount: u64,
    pub price_impact: f64,
    pub fee_amount: u64,
    pub fee_num: i32,
    pub fee_denom: i32,
    pub reserves_in: u64,
    pub reserves_out: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub hops: Vec<RouteHop>,
    pub total_input: u64,
    pub total_output: u64,
    pub total_price_impact: f64,
    /// AMM pool fees (swap fee), in input-token units along the path.
    pub total_fees: u64,
    /// Miner fees for executing this route (hops × network tx fee).
    pub total_miner_fees: u64,
    /// Fee-aware score: `total_output - total_miner_fees` when the route ends
    /// in ERG (miner fees are paid in ERG). Otherwise equals `total_output`.
    pub net_output: u64,
    pub effective_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteQuote {
    pub route: Route,
    pub min_output: u64,
    pub slippage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitAllocation {
    pub route_index: usize,
    pub fraction: f64,
    pub input_amount: u64,
    /// Gross CFMM output for this allocation (before miner fees).
    pub output_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitRoute {
    pub allocations: Vec<SplitAllocation>,
    /// Sum of gross CFMM outputs.
    pub total_output: u64,
    pub total_input: u64,
    pub total_miner_fees: u64,
    /// Fee-aware total (gross − miner fees when target is ERG).
    pub net_output: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitAllocationDetail {
    pub route: Route,
    pub fraction: f64,
    pub input_amount: u64,
    pub output_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitRouteDetail {
    pub allocations: Vec<SplitAllocationDetail>,
    pub total_output: u64,
    pub total_input: u64,
    pub total_miner_fees: u64,
    pub net_output: u64,
    /// Improvement of split net_output vs best single-route net_output.
    pub improvement_pct: f64,
}

/// Largest executable input when `requested_input` cannot be quoted (e.g. would
/// drain an N2T pool below min box ERG). Only set for routes ending in ERG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaxSwapHint {
    pub max_input: u64,
    pub max_output: u64,
    /// Machine-readable cause (`pool_min_erg`).
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthTiers {
    pub pool_id: String,
    pub token_in: String,
    pub token_out: String,
    /// (impact_percent, max_input_amount)
    pub tiers: Vec<(f64, u64)>,
}
