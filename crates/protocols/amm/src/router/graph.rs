//! Pool graph construction.

use std::collections::{HashMap, HashSet};

use super::types::{PoolEdge, PoolGraph, DEFAULT_MAX_POOLS_PER_PAIR, ERG_TOKEN_ID};
use crate::state::{AmmPool, PoolType};

///
/// N2T pools add edges ERG <-> token_y. T2T pools add edges token_x <-> token_y.
/// Pools below `min_liquidity_nano` are excluded. Per directed pair, only the
/// top pools by reserves are retained to bound the search space.
pub fn build_pool_graph(pools: &[AmmPool], min_liquidity_nano: u64) -> PoolGraph {
    build_pool_graph_with_limit(pools, min_liquidity_nano, DEFAULT_MAX_POOLS_PER_PAIR)
}

pub fn build_pool_graph_with_limit(
    pools: &[AmmPool],
    min_liquidity_nano: u64,
    max_pools_per_pair: usize,
) -> PoolGraph {
    let mut adjacency: HashMap<String, Vec<PoolEdge>> = HashMap::new();
    let mut pool_count = 0;

    for pool in pools {
        match pool.pool_type {
            PoolType::N2T => {
                let erg_reserves = match pool.erg_reserves {
                    Some(r) if r >= min_liquidity_nano => r,
                    _ => continue,
                };
                let token_reserves = pool.token_y.amount;
                if token_reserves == 0 {
                    continue;
                }

                adjacency
                    .entry(ERG_TOKEN_ID.to_string())
                    .or_default()
                    .push(PoolEdge {
                        pool: pool.clone(),
                        token_in: ERG_TOKEN_ID.to_string(),
                        token_out: pool.token_y.token_id.clone(),
                        reserves_in: erg_reserves,
                        reserves_out: token_reserves,
                    });

                adjacency
                    .entry(pool.token_y.token_id.clone())
                    .or_default()
                    .push(PoolEdge {
                        pool: pool.clone(),
                        token_in: pool.token_y.token_id.clone(),
                        token_out: ERG_TOKEN_ID.to_string(),
                        reserves_in: token_reserves,
                        reserves_out: erg_reserves,
                    });

                pool_count += 1;
            }
            PoolType::T2T => {
                let token_x = match pool.token_x.as_ref() {
                    Some(t) if t.amount > 0 => t,
                    _ => continue,
                };
                if pool.token_y.amount == 0 {
                    continue;
                }

                adjacency
                    .entry(token_x.token_id.clone())
                    .or_default()
                    .push(PoolEdge {
                        pool: pool.clone(),
                        token_in: token_x.token_id.clone(),
                        token_out: pool.token_y.token_id.clone(),
                        reserves_in: token_x.amount,
                        reserves_out: pool.token_y.amount,
                    });

                adjacency
                    .entry(pool.token_y.token_id.clone())
                    .or_default()
                    .push(PoolEdge {
                        pool: pool.clone(),
                        token_in: pool.token_y.token_id.clone(),
                        token_out: token_x.token_id.clone(),
                        reserves_in: pool.token_y.amount,
                        reserves_out: token_x.amount,
                    });

                pool_count += 1;
            }
        }
    }

    for edges in adjacency.values_mut() {
        let mut by_target: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, edge) in edges.iter().enumerate() {
            by_target.entry(&edge.token_out).or_default().push(i);
        }

        let mut keep: HashSet<usize> = HashSet::new();
        for indices in by_target.values() {
            let mut sorted: Vec<usize> = indices.clone();
            sorted.sort_by(|&a, &b| edges[b].reserves_in.cmp(&edges[a].reserves_in));
            for &idx in sorted.iter().take(max_pools_per_pair) {
                keep.insert(idx);
            }
        }

        let mut i = 0;
        edges.retain(|_| {
            let retained = keep.contains(&i);
            i += 1;
            retained
        });
    }

    PoolGraph {
        adjacency,
        pool_count,
    }
}

/// Ensure the graph contains every pool that directly connects `source` and
/// `target`, regardless of the liquidity floor or per-pair cap.
///
/// The floor exists to bound multi-hop path explosion; a directly-requested
/// pair cannot explode the search, and dropping it makes real pools invisible
/// (e.g. a small N2T pool showing no route at all). Price impact on the quote
/// communicates the thin liquidity.
pub fn ensure_direct_pair_edges(
    graph: &mut PoolGraph,
    pools: &[AmmPool],
    source: &str,
    target: &str,
) {
    for pool in pools {
        let (token_a, token_b, reserves_a, reserves_b) = match pool.pool_type {
            PoolType::N2T => (
                ERG_TOKEN_ID.to_string(),
                pool.token_y.token_id.clone(),
                pool.erg_reserves.unwrap_or(0),
                pool.token_y.amount,
            ),
            PoolType::T2T => match pool.token_x.as_ref() {
                Some(x) => (
                    x.token_id.clone(),
                    pool.token_y.token_id.clone(),
                    x.amount,
                    pool.token_y.amount,
                ),
                None => continue,
            },
        };

        let connects =
            (token_a == source && token_b == target) || (token_a == target && token_b == source);
        if !connects || reserves_a == 0 || reserves_b == 0 {
            continue;
        }

        let already_present = graph
            .adjacency
            .get(token_a.as_str())
            .map(|edges| edges.iter().any(|e| e.pool.pool_id == pool.pool_id))
            .unwrap_or(false);
        if already_present {
            continue;
        }

        graph
            .adjacency
            .entry(token_a.clone())
            .or_default()
            .push(PoolEdge {
                pool: pool.clone(),
                token_in: token_a.clone(),
                token_out: token_b.clone(),
                reserves_in: reserves_a,
                reserves_out: reserves_b,
            });
        graph
            .adjacency
            .entry(token_b.clone())
            .or_default()
            .push(PoolEdge {
                pool: pool.clone(),
                token_in: token_b.clone(),
                token_out: token_a.clone(),
                reserves_in: reserves_b,
                reserves_out: reserves_a,
            });
        graph.pool_count += 1;
    }
}
