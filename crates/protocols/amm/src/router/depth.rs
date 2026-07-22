//! Depth tiers and max executable swap hints.

use super::search::{find_paths, quote_route};
use super::types::{DepthTiers, MaxSwapHint, PoolEdge, PoolGraph, ERG_TOKEN_ID};
use crate::calculator::max_token_in_for_erg_out;

const IMPACT_TIERS: [f64; 5] = [0.005, 0.01, 0.02, 0.05, 0.10];

/// Max executable input for `source → ERG` across all paths (best = largest input).
pub fn max_executable_swap_to_erg(
    graph: &PoolGraph,
    source_token: &str,
    max_hops: usize,
) -> Option<MaxSwapHint> {
    let paths = find_paths(graph, source_token, ERG_TOKEN_ID, max_hops);
    let mut best: Option<(u64, u64)> = None;
    for path in &paths {
        if let Some((inp, out)) = max_executable_input_on_path(path) {
            if best.map(|(i, _)| inp > i).unwrap_or(true) {
                best = Some((inp, out));
            }
        }
    }
    let (max_input, max_output) = best?;
    if max_input == 0 {
        return None;
    }
    Some(MaxSwapHint {
        max_input,
        max_output,
        reason: "pool_min_erg".to_string(),
    })
}

/// When `requested_input` cannot be quoted to ERG but a smaller size can,
/// return that max executable size (pool min-ERG ceiling).
pub fn max_swap_hint_if_needed(
    graph: &PoolGraph,
    source_token: &str,
    target_token: &str,
    requested_input: u64,
    max_hops: usize,
) -> Option<MaxSwapHint> {
    if target_token != ERG_TOKEN_ID || requested_input == 0 {
        return None;
    }
    let has_quote = find_paths(graph, source_token, target_token, max_hops)
        .iter()
        .any(|p| quote_route(p, requested_input).is_some());
    if has_quote {
        return None;
    }
    let hint = max_executable_swap_to_erg(graph, source_token, max_hops)?;
    if hint.max_input == 0 || hint.max_input >= requested_input {
        return None;
    }
    Some(hint)
}

fn max_executable_input_on_path(path: &[PoolEdge]) -> Option<(u64, u64)> {
    if path.is_empty() {
        return None;
    }
    quote_route(path, 1)?;

    // Direct token → ERG: closed-form max input, then walk down if CFMM
    // rounding would still breach the dust ceiling.
    if path.len() == 1 && path[0].token_out == ERG_TOKEN_ID {
        let e = &path[0];
        let mut hi = max_token_in_for_erg_out(
            e.reserves_in,
            e.reserves_out,
            e.pool.fee_num,
            e.pool.fee_denom,
        )?;
        quote_route(path, 1)?;
        let mut lo = 1u64;
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if quote_route(path, mid).is_some() {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let route = quote_route(path, lo)?;
        return Some((lo, route.total_output));
    }

    // Multi-hop: binary search the largest input that still quotes.
    let mut hi = path[0].reserves_in.saturating_mul(100).max(1_000);
    for _ in 0..32 {
        if quote_route(path, hi).is_none() {
            break;
        }
        let next = hi.saturating_mul(2);
        if next == hi {
            let route = quote_route(path, hi)?;
            return Some((hi, route.total_output));
        }
        hi = next;
    }
    if quote_route(path, hi).is_some() {
        let route = quote_route(path, hi)?;
        return Some((hi, route.total_output));
    }

    let mut lo = 1u64;
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if quote_route(path, mid).is_some() {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let route = quote_route(path, lo)?;
    Some((lo, route.total_output))
}

/// Max input per impact tier. Constant product: `max_input = reserves_in * impact / (1 - impact)`
pub fn calculate_depth_tiers(edge: &PoolEdge) -> DepthTiers {
    let tiers: Vec<(f64, u64)> = IMPACT_TIERS
        .iter()
        .map(|&impact| {
            let max_input = (edge.reserves_in as f64 * impact / (1.0 - impact)) as u64;
            (impact * 100.0, max_input)
        })
        .collect();

    DepthTiers {
        pool_id: edge.pool.pool_id.clone(),
        token_in: edge.token_in.clone(),
        token_out: edge.token_out.clone(),
        tiers,
    }
}

pub fn calculate_all_depth_tiers(graph: &PoolGraph, source_token: &str) -> Vec<DepthTiers> {
    graph
        .adjacency
        .get(source_token)
        .map(|edges| edges.iter().map(calculate_depth_tiers).collect())
        .unwrap_or_default()
}
