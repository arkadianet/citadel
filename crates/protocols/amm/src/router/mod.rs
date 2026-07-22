//! Smart Router: Multi-Hop DEX Routing & Liquidity Analysis
//!
//! Finds optimal swap paths across all available AMM pools, supporting
//! multi-hop routes through intermediate tokens to minimize price impact.

mod arb;
mod depth;
mod graph;
mod search;
mod split;
mod types;

#[cfg(test)]
mod tests;

pub use arb::{
    calculate_oracle_arb_snapshot, find_circular_arbs, find_cycles, CircularArb,
    CircularArbSnapshot, OracleArbSnapshot, OracleArbWindow,
};
pub use depth::{
    calculate_all_depth_tiers, calculate_depth_tiers, max_executable_swap_to_erg,
    max_swap_hint_if_needed,
};
pub use graph::{build_pool_graph, build_pool_graph_with_limit, ensure_direct_pair_edges};
pub use search::{
    find_best_routes, find_best_routes_by_output, find_paths, make_route_quote, quote_route,
    quote_route_reverse,
};
pub use split::{optimize_split, optimize_split_detailed};
pub use types::{
    miner_fees_for_hops, DepthTiers, MaxSwapHint, PoolEdge, PoolGraph, Route, RouteHop, RouteQuote,
    SplitAllocation, SplitAllocationDetail, SplitRoute, SplitRouteDetail,
    DEFAULT_MAX_POOLS_PER_PAIR, DEFAULT_MIN_LIQUIDITY_NANO, ERG_TOKEN_ID, MINER_FEE_PER_HOP,
};
