//! Spectrum AMM Protocol Implementation
//!
//! This crate implements Spectrum DEX integration for swapping tokens
//! through existing AMM liquidity pools.

pub mod calculator;
pub mod constants;
pub mod direct_swap;
pub mod fetch;
pub mod refund;
pub mod state;
pub mod tx_builder;

// Re-exports
pub use calculator::{calculate_output, calculate_price_impact, quote_swap};
pub use constants::{erg, fees, lp, pool_indices, pool_templates, swap_template_bytes};
pub use direct_swap::{build_direct_swap_eip12, DirectSwapBuildResult, DirectSwapSummary};
pub use fetch::{
    discover_n2t_pools, discover_pools, discover_t2t_pools, find_mempool_swaps,
    find_pending_orders, match_swap_template, parse_n2t_pool, parse_t2t_pool,
};
pub use refund::{build_refund_tx_eip12, RefundBuildResult, RefundSummary};
pub use state::{
    AmmError, AmmPool, MempoolSwap, PendingSwapOrder, PoolType, SwapInput, SwapOrderType,
    SwapQuote, SwapRequest, TokenAmount,
};
pub use tx_builder::{build_swap_order_eip12, SwapBuildResult, SwapTxSummary};
