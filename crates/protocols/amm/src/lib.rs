//! Spectrum AMM Protocol Implementation
//!
//! This crate implements Spectrum DEX integration for swapping tokens
//! through existing AMM liquidity pools.

pub mod calculator;
pub mod constants;
pub mod cross_protocol;
pub mod direct_swap;
pub mod fetch;
pub mod lp_deposit;
pub mod lp_order;
pub mod lp_redeem;
pub mod pool_setup;
pub mod refund;
pub mod router;
pub mod state;
pub mod tx_builder;

// Re-exports
pub use calculator::{calculate_output, calculate_price_impact, quote_swap};
pub use constants::{erg, fees, lp, pool_indices, pool_templates, swap_template_bytes};
pub use cross_protocol::{
    compare_acquisition, AcquisitionComparison, AcquisitionOption, SigmaUsdParams,
};
pub use direct_swap::{build_direct_swap_eip12, DirectSwapBuildResult, DirectSwapSummary};
pub use lp_deposit::{build_lp_deposit_eip12, LpDepositBuildResult, LpDepositSummary};
pub use lp_order::{
    build_lp_deposit_order_eip12, build_lp_redeem_order_eip12, LpOrderBuildResult, LpOrderSummary,
};
pub use lp_redeem::{build_lp_redeem_eip12, LpRedeemBuildResult, LpRedeemSummary};
pub use pool_setup::{
    build_pool_bootstrap_eip12, build_pool_create_eip12, PoolBootstrapResult,
    PoolBootstrapSummary, PoolCreateResult, PoolCreateSummary, PoolSetupParams,
};
pub use fetch::{
    discover_n2t_pools, discover_pools, discover_t2t_pools, find_mempool_swaps,
    find_pending_orders, match_swap_template, parse_n2t_pool, parse_t2t_pool,
};
pub use refund::{build_refund_tx_eip12, RefundBuildResult, RefundSummary};
pub use state::{
    AmmError, AmmPool, MempoolSwap, PendingSwapOrder, PoolType, SwapInput, SwapOrderType,
    SwapQuote, SwapRequest, TokenAmount,
};
pub use router::{
    build_pool_graph, build_pool_graph_with_limit, calculate_all_depth_tiers,
    calculate_depth_tiers, find_best_routes, find_best_routes_by_output, find_paths,
    make_route_quote, optimize_split, optimize_split_detailed, quote_route, quote_route_reverse,
    DepthTiers, PoolEdge, PoolGraph, Route, RouteHop, RouteQuote, SplitAllocation,
    SplitAllocationDetail, SplitRoute, SplitRouteDetail, DEFAULT_MAX_POOLS_PER_PAIR,
    DEFAULT_MIN_LIQUIDITY_NANO, ERG_TOKEN_ID,
    calculate_oracle_arb_snapshot, OracleArbSnapshot, OracleArbWindow,
};
pub use tx_builder::{build_swap_order_eip12, SwapBuildResult, SwapTxSummary};
