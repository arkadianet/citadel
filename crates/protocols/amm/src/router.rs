//! Smart Router: Multi-Hop DEX Routing & Liquidity Analysis
//!
//! Finds optimal swap paths across all available AMM pools, supporting
//! multi-hop routes through intermediate tokens to minimize price impact.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::calculator::{
    apply_slippage, calculate_input, calculate_output, calculate_price_impact,
};
use crate::state::{AmmPool, PoolType};

/// Sentinel token ID for ERG in the pool graph.
pub const ERG_TOKEN_ID: &str = "ERG";

/// Minimum ERG reserves (nanoERG) for a pool to be included in routing.
/// 10 ERG = 10_000_000_000 nanoERG.
pub const DEFAULT_MIN_LIQUIDITY_NANO: u64 = 10_000_000_000;

/// Default maximum pools retained per directed token pair to bound path search.
pub const DEFAULT_MAX_POOLS_PER_PAIR: usize = 3;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// An edge in the pool graph connecting two tokens via a specific pool.
#[derive(Debug, Clone)]
pub struct PoolEdge {
    pub pool: AmmPool,
    pub token_in: String,
    pub token_out: String,
    pub reserves_in: u64,
    pub reserves_out: u64,
}

/// Adjacency-list pool graph.
#[derive(Debug, Clone)]
pub struct PoolGraph {
    pub adjacency: HashMap<String, Vec<PoolEdge>>,
    pub pool_count: usize,
}

/// A single hop in a swap route.
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

/// A complete route from source token to target token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub hops: Vec<RouteHop>,
    pub total_input: u64,
    pub total_output: u64,
    pub total_price_impact: f64,
    pub total_fees: u64,
    pub effective_rate: f64,
}

/// Quoted route with slippage-adjusted minimum output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteQuote {
    pub route: Route,
    pub min_output: u64,
    pub slippage_percent: f64,
}

/// Split allocation across multiple routes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitAllocation {
    pub route_index: usize,
    pub fraction: f64,
    pub input_amount: u64,
    pub output_amount: u64,
}

/// Optimal split across parallel routes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitRoute {
    pub allocations: Vec<SplitAllocation>,
    pub total_output: u64,
    pub total_input: u64,
}

/// Split allocation with full route details for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitAllocationDetail {
    pub route: Route,
    pub fraction: f64,
    pub input_amount: u64,
    pub output_amount: u64,
}

/// Optimal split with full route details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitRouteDetail {
    pub allocations: Vec<SplitAllocationDetail>,
    pub total_output: u64,
    pub total_input: u64,
    /// How much more output the split gives vs best single route
    pub improvement_pct: f64,
}

/// Liquidity depth tiers for a pool edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthTiers {
    pub pool_id: String,
    pub token_in: String,
    pub token_out: String,
    /// (impact_percent, max_input_amount)
    pub tiers: Vec<(f64, u64)>,
}

// ---------------------------------------------------------------------------
// Step 1: Pool Graph & Path Finding
// ---------------------------------------------------------------------------

/// Build a pool graph from discovered pools.
///
/// N2T pools add edges ERG ↔ token_y. T2T pools add edges token_x ↔ token_y.
/// Pools below `min_liquidity_nano` are excluded. Per directed pair, only the
/// top pools by reserves are retained to bound the search space.
pub fn build_pool_graph(pools: &[AmmPool], min_liquidity_nano: u64) -> PoolGraph {
    build_pool_graph_with_limit(pools, min_liquidity_nano, DEFAULT_MAX_POOLS_PER_PAIR)
}

/// Build a pool graph with custom per-pair limit.
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

                // ERG -> token_y
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

                // token_y -> ERG
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

                // token_x -> token_y
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

                // token_y -> token_x
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

    // Prune: keep top N per (token_in, token_out) pair by reserves
    for edges in adjacency.values_mut() {
        // Group by token_out, sort each group by reserves_in desc, truncate
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

/// Find all acyclic paths from `source_token` to `target_token`, up to `max_hops`.
///
/// Uses BFS with visited-token tracking. No token is revisited and no pool_id
/// is used more than once in a path.
pub fn find_paths(
    graph: &PoolGraph,
    source_token: &str,
    target_token: &str,
    max_hops: usize,
) -> Vec<Vec<PoolEdge>> {
    let mut results: Vec<Vec<PoolEdge>> = Vec::new();

    type SearchState = (String, Vec<PoolEdge>, HashSet<String>, HashSet<String>);
    let mut queue: VecDeque<SearchState> = VecDeque::new();

    let mut initial_visited = HashSet::new();
    initial_visited.insert(source_token.to_string());
    queue.push_back((
        source_token.to_string(),
        Vec::new(),
        initial_visited,
        HashSet::new(),
    ));

    while let Some((current, path, visited, used_pools)) = queue.pop_front() {
        if let Some(edges) = graph.adjacency.get(current.as_str()) {
            for edge in edges {
                // Skip if pool already used in this path
                if used_pools.contains(&edge.pool.pool_id) {
                    continue;
                }

                if edge.token_out == target_token {
                    // Found a complete path
                    let mut complete_path = path.clone();
                    complete_path.push(edge.clone());
                    results.push(complete_path);
                } else if path.len() + 1 < max_hops && !visited.contains(&edge.token_out) {
                    // Continue searching
                    let mut new_visited = visited.clone();
                    new_visited.insert(edge.token_out.clone());
                    let mut new_pools = used_pools.clone();
                    new_pools.insert(edge.pool.pool_id.clone());
                    let mut new_path = path.clone();
                    new_path.push(edge.clone());
                    queue.push_back((edge.token_out.clone(), new_path, new_visited, new_pools));
                }
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Step 2: Multi-Hop Quoting
// ---------------------------------------------------------------------------

/// Quote a route by chaining `calculate_output` through each hop.
///
/// Returns `None` if any hop produces zero output (insufficient liquidity).
pub fn quote_route(path: &[PoolEdge], input_amount: u64) -> Option<Route> {
    if path.is_empty() || input_amount == 0 {
        return None;
    }

    let mut current_amount = input_amount;
    let mut hops = Vec::with_capacity(path.len());
    let mut total_fees: u64 = 0;

    for edge in path {
        let output = calculate_output(
            edge.reserves_in,
            edge.reserves_out,
            current_amount,
            edge.pool.fee_num,
            edge.pool.fee_denom,
        );

        if output == 0 {
            return None;
        }

        let price_impact =
            calculate_price_impact(edge.reserves_in, edge.reserves_out, current_amount, output);

        let fee_amount = (current_amount as f64
            * (1.0 - edge.pool.fee_num as f64 / edge.pool.fee_denom as f64))
            as u64;

        let token_in_name = resolve_token_name(&edge.pool, &edge.token_in);
        let token_out_name = resolve_token_name(&edge.pool, &edge.token_out);
        let token_in_decimals = resolve_token_decimals(&edge.pool, &edge.token_in);
        let token_out_decimals = resolve_token_decimals(&edge.pool, &edge.token_out);
        let pool_display_name = make_pool_display_name(&edge.pool, &edge.token_in, &edge.token_out);

        hops.push(RouteHop {
            pool_id: edge.pool.pool_id.clone(),
            pool_type: format!("{:?}", edge.pool.pool_type),
            token_in: edge.token_in.clone(),
            token_in_name,
            token_in_decimals,
            token_out: edge.token_out.clone(),
            token_out_name,
            token_out_decimals,
            pool_display_name,
            input_amount: current_amount,
            output_amount: output,
            price_impact,
            fee_amount,
            fee_num: edge.pool.fee_num,
            fee_denom: edge.pool.fee_denom,
            reserves_in: edge.reserves_in,
            reserves_out: edge.reserves_out,
        });

        total_fees += fee_amount;
        current_amount = output;
    }

    let total_output = current_amount;

    // End-to-end price impact: compare actual rate to the product of spot prices
    let spot_product: f64 = path
        .iter()
        .map(|e| e.reserves_out as f64 / e.reserves_in as f64)
        .product();
    let actual_rate = total_output as f64 / input_amount as f64;
    let total_price_impact = if spot_product > 0.0 {
        ((spot_product - actual_rate) / spot_product).abs() * 100.0
    } else {
        0.0
    };

    let effective_rate = actual_rate;

    Some(Route {
        hops,
        total_input: input_amount,
        total_output,
        total_price_impact,
        total_fees,
        effective_rate,
    })
}

/// Find and quote all routes, returning the top `max_routes` ranked by output.
pub fn find_best_routes(
    graph: &PoolGraph,
    source_token: &str,
    target_token: &str,
    input_amount: u64,
    max_hops: usize,
    max_routes: usize,
) -> Vec<Route> {
    let paths = find_paths(graph, source_token, target_token, max_hops);

    let mut routes: Vec<Route> = paths
        .iter()
        .filter_map(|path| quote_route(path, input_amount))
        .collect();

    routes.sort_by(|a, b| b.total_output.cmp(&a.total_output));
    routes.truncate(max_routes);
    routes
}

/// Create a `RouteQuote` from a `Route` with slippage applied.
pub fn make_route_quote(route: Route, slippage_percent: f64) -> RouteQuote {
    let min_output = apply_slippage(route.total_output, slippage_percent);
    RouteQuote {
        route,
        min_output,
        slippage_percent,
    }
}

// ---------------------------------------------------------------------------
// Step 2b: Reverse Quoting ("I want X output")
// ---------------------------------------------------------------------------

/// Quote a route in reverse: given the desired output, calculate the required input.
///
/// Works backwards through the hops, using `calculate_input` at each step.
/// Returns `None` if any hop is infeasible (output exceeds reserves).
pub fn quote_route_reverse(path: &[PoolEdge], desired_output: u64) -> Option<Route> {
    if path.is_empty() || desired_output == 0 {
        return None;
    }

    // Work backwards to find required input at each hop
    let mut required_amounts: Vec<(u64, u64)> = Vec::with_capacity(path.len());
    let mut needed = desired_output;

    for edge in path.iter().rev() {
        let input_needed = calculate_input(
            edge.reserves_in,
            edge.reserves_out,
            needed,
            edge.pool.fee_num,
            edge.pool.fee_denom,
        )?;
        required_amounts.push((input_needed, needed));
        needed = input_needed;
    }

    required_amounts.reverse();

    // Now build hops forward with the calculated amounts
    let total_input = required_amounts[0].0;
    let mut hops = Vec::with_capacity(path.len());
    let mut total_fees: u64 = 0;

    for (i, edge) in path.iter().enumerate() {
        let (input_amount, _) = required_amounts[i];

        // Verify forward: calculate the actual output for this input
        let output = calculate_output(
            edge.reserves_in,
            edge.reserves_out,
            input_amount,
            edge.pool.fee_num,
            edge.pool.fee_denom,
        );

        if output == 0 {
            return None;
        }

        let price_impact =
            calculate_price_impact(edge.reserves_in, edge.reserves_out, input_amount, output);

        let fee_amount = (input_amount as f64
            * (1.0 - edge.pool.fee_num as f64 / edge.pool.fee_denom as f64))
            as u64;

        let token_in_name = resolve_token_name(&edge.pool, &edge.token_in);
        let token_out_name = resolve_token_name(&edge.pool, &edge.token_out);
        let token_in_decimals = resolve_token_decimals(&edge.pool, &edge.token_in);
        let token_out_decimals = resolve_token_decimals(&edge.pool, &edge.token_out);
        let pool_display_name = make_pool_display_name(&edge.pool, &edge.token_in, &edge.token_out);

        hops.push(RouteHop {
            pool_id: edge.pool.pool_id.clone(),
            pool_type: format!("{:?}", edge.pool.pool_type),
            token_in: edge.token_in.clone(),
            token_in_name,
            token_in_decimals,
            token_out: edge.token_out.clone(),
            token_out_name,
            token_out_decimals,
            pool_display_name,
            input_amount,
            output_amount: output,
            price_impact,
            fee_amount,
            fee_num: edge.pool.fee_num,
            fee_denom: edge.pool.fee_denom,
            reserves_in: edge.reserves_in,
            reserves_out: edge.reserves_out,
        });

        total_fees += fee_amount;
    }

    let total_output = hops.last().map(|h| h.output_amount).unwrap_or(0);

    let spot_product: f64 = path
        .iter()
        .map(|e| e.reserves_out as f64 / e.reserves_in as f64)
        .product();
    let actual_rate = if total_input > 0 {
        total_output as f64 / total_input as f64
    } else {
        0.0
    };
    let total_price_impact = if spot_product > 0.0 {
        ((spot_product - actual_rate) / spot_product).abs() * 100.0
    } else {
        0.0
    };

    Some(Route {
        hops,
        total_input,
        total_output,
        total_price_impact,
        total_fees,
        effective_rate: actual_rate,
    })
}

/// Find best routes for a desired output amount, ranked by lowest input needed.
pub fn find_best_routes_by_output(
    graph: &PoolGraph,
    source_token: &str,
    target_token: &str,
    desired_output: u64,
    max_hops: usize,
    max_routes: usize,
) -> Vec<Route> {
    let paths = find_paths(graph, source_token, target_token, max_hops);

    let mut routes: Vec<Route> = paths
        .iter()
        .filter_map(|path| quote_route_reverse(path, desired_output))
        .collect();

    // Sort by total_input ascending (cheapest first)
    routes.sort_by(|a, b| a.total_input.cmp(&b.total_input));
    routes.truncate(max_routes);
    routes
}

// ---------------------------------------------------------------------------
// Step 3: Split Optimization
// ---------------------------------------------------------------------------

/// Find optimal split across multiple routes to maximize total output.
///
/// Uses grid search with 1% step size. Only considers the top `max_splits`
/// routes by single-route output.
pub fn optimize_split(
    paths: &[Vec<PoolEdge>],
    total_input: u64,
    max_splits: usize,
) -> SplitRoute {
    let max_splits = max_splits.min(paths.len()).min(3);

    if max_splits <= 1 || paths.is_empty() {
        // No splitting: 100% to the best single route
        let output = paths
            .first()
            .and_then(|p| quote_route(p, total_input))
            .map(|r| r.total_output)
            .unwrap_or(0);
        return SplitRoute {
            allocations: vec![SplitAllocation {
                route_index: 0,
                fraction: 1.0,
                input_amount: total_input,
                output_amount: output,
            }],
            total_output: output,
            total_input,
        };
    }

    if max_splits == 2 {
        return optimize_split_two(paths, total_input);
    }

    // For 3 routes: iterative pairwise optimization
    optimize_split_multi(paths, total_input, max_splits)
}

fn optimize_split_two(paths: &[Vec<PoolEdge>], total_input: u64) -> SplitRoute {
    let mut best_total: u64 = 0;
    let mut best_permille: u64 = 1000; // permille for path[0]

    for permille in 0..=1000u64 {
        let input_a = total_input * permille / 1000;
        let input_b = total_input - input_a;

        let out_a = if input_a > 0 {
            quote_route(&paths[0], input_a)
                .map(|r| r.total_output)
                .unwrap_or(0)
        } else {
            0
        };
        let out_b = if input_b > 0 {
            quote_route(&paths[1], input_b)
                .map(|r| r.total_output)
                .unwrap_or(0)
        } else {
            0
        };

        let total = out_a + out_b;
        if total > best_total {
            best_total = total;
            best_permille = permille;
        }
    }

    let input_a = total_input * best_permille / 1000;
    let input_b = total_input - input_a;
    let out_a = if input_a > 0 {
        quote_route(&paths[0], input_a)
            .map(|r| r.total_output)
            .unwrap_or(0)
    } else {
        0
    };
    let out_b = if input_b > 0 {
        quote_route(&paths[1], input_b)
            .map(|r| r.total_output)
            .unwrap_or(0)
    } else {
        0
    };

    let mut allocations = Vec::new();
    if input_a > 0 {
        allocations.push(SplitAllocation {
            route_index: 0,
            fraction: best_permille as f64 / 1000.0,
            input_amount: input_a,
            output_amount: out_a,
        });
    }
    if input_b > 0 {
        allocations.push(SplitAllocation {
            route_index: 1,
            fraction: (1000 - best_permille) as f64 / 1000.0,
            input_amount: input_b,
            output_amount: out_b,
        });
    }

    SplitRoute {
        allocations,
        total_output: best_total,
        total_input,
    }
}

fn optimize_split_multi(
    paths: &[Vec<PoolEdge>],
    total_input: u64,
    max_splits: usize,
) -> SplitRoute {
    // Start with equal split, then iterate (using permille = 0.1% steps)
    let n = max_splits.min(paths.len());
    let mut fractions: Vec<u64> = vec![1000 / n as u64; n];
    // Distribute remainder to first route
    let remainder = 1000 - fractions.iter().sum::<u64>();
    fractions[0] += remainder;

    // Iterative refinement: for each route, try adjusting its fraction
    // Use 10-step increments (1%) in the multi-route case to keep iteration count bounded
    for _ in 0..5 {
        for i in 0..n {
            let mut best_output: u64 = 0;
            let mut best_frac: u64 = fractions[i];

            // The pool of permille points available for route i
            let other_sum: u64 = fractions.iter().enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, f)| f)
                .sum();

            let max_for_i = 1000 - other_sum;

            for f in (0..=max_for_i).step_by(10) {
                let mut test_fracs = fractions.clone();
                test_fracs[i] = f;
                // Redistribute the difference to maintain sum = 1000
                let diff = max_for_i - f;
                // Spread diff proportionally across others
                if other_sum > 0 {
                    let mut remaining_diff = diff;
                    for j in 0..n {
                        if j != i && remaining_diff > 0 {
                            let add = if j == n - 1 || (j == n - 2 && i == n - 1) {
                                remaining_diff
                            } else {
                                (diff as f64 * fractions[j] as f64 / other_sum as f64).round()
                                    as u64
                            };
                            let add = add.min(remaining_diff);
                            test_fracs[j] = fractions[j] + add;
                            remaining_diff -= add;
                        }
                    }
                }

                let total: u64 = (0..n)
                    .map(|k| {
                        let inp = total_input * test_fracs[k] / 1000;
                        if inp > 0 {
                            quote_route(&paths[k], inp)
                                .map(|r| r.total_output)
                                .unwrap_or(0)
                        } else {
                            0
                        }
                    })
                    .sum();

                if total > best_output {
                    best_output = total;
                    best_frac = f;
                }
            }

            fractions[i] = best_frac;
        }
    }

    // Normalize fractions to sum to 1000
    let sum: u64 = fractions.iter().sum();
    if sum != 1000 && sum > 0 {
        let scale = 1000.0 / sum as f64;
        for f in &mut fractions {
            *f = (*f as f64 * scale).round() as u64;
        }
        // Adjust last to hit exactly 1000
        let new_sum: u64 = fractions.iter().sum();
        if new_sum != 1000 {
            let diff = 1000i64 - new_sum as i64;
            fractions[0] = (fractions[0] as i64 + diff) as u64;
        }
    }

    let allocations: Vec<SplitAllocation> = (0..n)
        .filter(|&k| fractions[k] > 0)
        .map(|k| {
            let input = total_input * fractions[k] / 1000;
            let output = if input > 0 {
                quote_route(&paths[k], input)
                    .map(|r| r.total_output)
                    .unwrap_or(0)
            } else {
                0
            };
            SplitAllocation {
                route_index: k,
                fraction: fractions[k] as f64 / 1000.0,
                input_amount: input,
                output_amount: output,
            }
        })
        .collect();

    let total_output: u64 = allocations.iter().map(|a| a.output_amount).sum();

    SplitRoute {
        allocations,
        total_output,
        total_input,
    }
}

// ---------------------------------------------------------------------------
// Step 3b: Detailed Split with Route Info
// ---------------------------------------------------------------------------

/// Compute the optimal split and return full route details for each allocation.
///
/// Only returns a split if it improves on the best single route by > 0.5%.
pub fn optimize_split_detailed(
    graph: &PoolGraph,
    source_token: &str,
    target_token: &str,
    total_input: u64,
    max_hops: usize,
    max_splits: usize,
    min_rate_filter: Option<(f64, u8)>,
) -> Option<SplitRouteDetail> {
    let all_paths = find_paths(graph, source_token, target_token, max_hops);
    if all_paths.is_empty() {
        return None;
    }

    // Filter by minimum effective rate (spot rate at 0.01 ERG probe).
    // This excludes routes where the token trades below a price floor
    // (e.g. below oracle price) even at small amounts.
    let paths: Vec<Vec<PoolEdge>> = if let Some((min_rate, target_decimals)) = min_rate_filter {
        let probe = 10_000_000u64; // 0.01 ERG
        all_paths
            .into_iter()
            .filter(|p| {
                route_rate_at_input(p, probe, target_decimals)
                    .map(|(rate, _)| rate >= min_rate)
                    .unwrap_or(false)
            })
            .collect()
    } else {
        all_paths
    };

    if paths.is_empty() {
        return None;
    }

    // Quote each path to rank them
    let mut quoted: Vec<(usize, u64)> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, p)| quote_route(p, total_input).map(|r| (i, r.total_output)))
        .collect();
    quoted.sort_by(|a, b| b.1.cmp(&a.1));

    let best_single_output = quoted.first().map(|(_, o)| *o).unwrap_or(0);
    if best_single_output == 0 {
        return None;
    }

    let max_splits = max_splits.min(quoted.len()).min(3);
    if max_splits < 2 {
        return None;
    }

    quoted.truncate(max_splits);
    let top_paths: Vec<Vec<PoolEdge>> = quoted.iter().map(|(i, _)| paths[*i].clone()).collect();

    let split = optimize_split(&top_paths, total_input, max_splits);

    // Check improvement threshold
    let improvement = if best_single_output > 0 {
        (split.total_output as f64 - best_single_output as f64) / best_single_output as f64 * 100.0
    } else {
        0.0
    };

    if improvement < 0.5 {
        return None; // Not worth splitting
    }

    // Build detailed allocations with full route info
    let mut allocations = Vec::new();
    for alloc in &split.allocations {
        if alloc.input_amount == 0 {
            continue;
        }
        if let Some(route) = quote_route(&top_paths[alloc.route_index], alloc.input_amount) {
            allocations.push(SplitAllocationDetail {
                route,
                fraction: alloc.fraction,
                input_amount: alloc.input_amount,
                output_amount: alloc.output_amount,
            });
        }
    }

    if allocations.len() < 2 {
        return None;
    }

    Some(SplitRouteDetail {
        total_output: split.total_output,
        total_input: split.total_input,
        improvement_pct: improvement,
        allocations,
    })
}

// ---------------------------------------------------------------------------
// Step 4: Liquidity Depth Analysis
// ---------------------------------------------------------------------------

/// Standard price impact tiers.
const IMPACT_TIERS: [f64; 5] = [0.005, 0.01, 0.02, 0.05, 0.10];

/// Calculate max input for each impact tier.
///
/// For constant product: `max_input = reserves_in * impact / (1 - impact)`
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

/// Calculate depth tiers for all outgoing edges from a source token.
pub fn calculate_all_depth_tiers(graph: &PoolGraph, source_token: &str) -> Vec<DepthTiers> {
    graph
        .adjacency
        .get(source_token)
        .map(|edges| edges.iter().map(calculate_depth_tiers).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Step 5: Oracle Arb Snapshot
// ---------------------------------------------------------------------------

/// A route's below-oracle opportunity window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleArbWindow {
    /// Route path description, e.g. "ERG → ergopad → SigUSD"
    pub path_label: String,
    /// Number of hops
    pub hops: usize,
    /// Pool IDs along the route
    pub pool_ids: Vec<String>,
    /// Spot rate at small probe (SigUSD per ERG)
    pub spot_rate_usd_per_erg: f64,
    /// Discount vs oracle (positive = route is cheaper than oracle)
    pub discount_pct: f64,
    /// Effective rate at max input (at oracle parity breakeven)
    pub rate_at_max: f64,
    /// Max nanoERG input before effective rate drops to oracle parity
    pub max_erg_input_nano: u64,
    /// Total price impact (%) when swapping max_erg_input_nano
    pub price_impact_at_max: f64,
    /// SigUSD output (raw cents) when swapping max_erg_input_nano
    pub sigusd_output_at_max: u64,
    /// SigUSD output in human-readable USD (e.g. 18.50)
    pub sigusd_output_at_max_usd: f64,
}

/// Snapshot of all below-oracle opportunities across routes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleArbSnapshot {
    pub oracle_rate_usd_per_erg: f64,
    /// Per-route windows (only routes with rate > oracle), sorted by discount descending
    pub windows: Vec<OracleArbWindow>,
    /// Total SigUSD (raw cents) available below oracle across all routes
    pub total_sigusd_below_oracle_raw: u64,
    /// Total ERG (nanoERG) needed to exhaust all arb windows
    pub total_erg_needed_nano: u64,
}

/// Compute the effective SigUSD/ERG rate for a route at a given nanoERG input.
/// Returns None if the route fails to quote at this amount.
fn route_rate_at_input(
    path: &[PoolEdge],
    input_nano: u64,
    target_decimals: u8,
) -> Option<(f64, u64)> {
    let route = quote_route(path, input_nano)?;
    if route.total_input == 0 || route.total_output == 0 {
        return None;
    }
    let divisor = 10f64.powi(target_decimals as i32);
    let rate = (route.total_output as f64 / divisor) / (route.total_input as f64 / 1e9);
    Some((rate, route.total_output))
}

/// Binary search for the max nanoERG input where the effective rate stays >= oracle_rate.
/// Assumes rate is monotonically decreasing with input (true for constant-product AMMs).
fn binary_search_breakeven(
    path: &[PoolEdge],
    oracle_rate: f64,
    target_decimals: u8,
    max_bound: u64,
) -> (u64, u64) {
    let mut lo: u64 = 0;
    let mut hi: u64 = max_bound;
    let mut best_input: u64 = 0;
    let mut best_output: u64 = 0;

    for _ in 0..40 {
        if hi <= lo + 1_000_000 {
            // Within 0.001 ERG precision
            break;
        }
        let mid = lo + (hi - lo) / 2;
        match route_rate_at_input(path, mid, target_decimals) {
            Some((rate, output)) if rate >= oracle_rate => {
                best_input = mid;
                best_output = output;
                lo = mid;
            }
            _ => {
                hi = mid;
            }
        }
    }

    (best_input, best_output)
}

/// Calculate the below-oracle opportunity snapshot for all ERG → target routes.
///
/// Finds all paths (including multi-hop), probes each at 1 ERG, and for routes
/// with rate > oracle, binary-searches for the max ERG input at oracle parity.
pub fn calculate_oracle_arb_snapshot(
    graph: &PoolGraph,
    target_token_id: &str,
    oracle_rate_usd_per_erg: f64,
    target_decimals: u8,
) -> OracleArbSnapshot {
    let empty = OracleArbSnapshot {
        oracle_rate_usd_per_erg,
        windows: Vec::new(),
        total_sigusd_below_oracle_raw: 0,
        total_erg_needed_nano: 0,
    };

    if oracle_rate_usd_per_erg <= 0.0 {
        return empty;
    }

    let paths = find_paths(graph, ERG_TOKEN_ID, target_token_id, 3);
    if paths.is_empty() {
        return empty;
    }

    let mut windows = Vec::new();

    for path in &paths {
        // Binary search for max input at oracle parity.
        // No probe pre-check — the binary search naturally converges from
        // the upper bound down to the sweet spot where the route is quotable
        // AND the rate exceeds oracle. This handles thin multi-hop routes
        // where small probes fail due to integer rounding at intermediate hops.
        let upper = path[0].reserves_in;
        let (max_input, output_at_max) =
            binary_search_breakeven(path, oracle_rate_usd_per_erg, target_decimals, upper);

        if max_input == 0 || output_at_max == 0 {
            continue;
        }

        // Build path label and collect pool IDs
        let mut label_parts: Vec<String> = Vec::new();
        let mut pool_ids: Vec<String> = Vec::new();
        for (i, edge) in path.iter().enumerate() {
            if i == 0 {
                let name = resolve_token_name(&edge.pool, &edge.token_in)
                    .unwrap_or_else(|| edge.token_in[..6.min(edge.token_in.len())].to_string());
                label_parts.push(name);
            }
            let out_name = resolve_token_name(&edge.pool, &edge.token_out)
                .unwrap_or_else(|| edge.token_out[..6.min(edge.token_out.len())].to_string());
            label_parts.push(out_name);
            pool_ids.push(edge.pool.pool_id.clone());
        }
        let path_label = label_parts.join(" \u{2192} "); // →

        // Rate and impact at the breakeven input
        let divisor = 10f64.powi(target_decimals as i32);
        let rate_at_max = (output_at_max as f64 / divisor) / (max_input as f64 / 1e9);
        let impact_at_max = quote_route(path, max_input)
            .map(|r| r.total_price_impact)
            .unwrap_or(0.0);

        // Spot rate: use 1/10th of max_input for a more accurate marginal rate.
        // This gives a rate closer to the "beginning" of the arb window.
        let spot_probe = (max_input / 10).max(1_000_000);
        let spot_rate = route_rate_at_input(path, spot_probe, target_decimals)
            .map(|(r, _)| r)
            .unwrap_or(rate_at_max);

        let discount_pct =
            (spot_rate - oracle_rate_usd_per_erg) / oracle_rate_usd_per_erg * 100.0;

        windows.push(OracleArbWindow {
            path_label,
            hops: path.len(),
            pool_ids,
            spot_rate_usd_per_erg: spot_rate,
            discount_pct,
            rate_at_max,
            max_erg_input_nano: max_input,
            price_impact_at_max: impact_at_max,
            sigusd_output_at_max: output_at_max,
            sigusd_output_at_max_usd: output_at_max as f64 / divisor,
        });
    }

    // Filter out dust opportunities (< $0.10) and sort by SigUSD output descending
    // so the most actionable windows appear first.
    windows.retain(|w| w.sigusd_output_at_max >= 10); // 10 raw cents = $0.10
    windows.sort_by(|a, b| {
        b.sigusd_output_at_max
            .cmp(&a.sigusd_output_at_max)
    });

    let total_sigusd = windows.iter().map(|w| w.sigusd_output_at_max).sum();
    let total_erg = windows.iter().map(|w| w.max_erg_input_nano).sum();

    OracleArbSnapshot {
        oracle_rate_usd_per_erg,
        windows,
        total_sigusd_below_oracle_raw: total_sigusd,
        total_erg_needed_nano: total_erg,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_token_name(pool: &AmmPool, token_id: &str) -> Option<String> {
    if token_id == ERG_TOKEN_ID {
        return Some("ERG".to_string());
    }
    if token_id == pool.token_y.token_id {
        return pool.token_y.name.clone();
    }
    if let Some(ref tx) = pool.token_x {
        if token_id == tx.token_id {
            return tx.name.clone();
        }
    }
    None
}

fn resolve_token_decimals(pool: &AmmPool, token_id: &str) -> u8 {
    if token_id == ERG_TOKEN_ID {
        return 9;
    }
    if token_id == pool.token_y.token_id {
        return pool.token_y.decimals.unwrap_or(0);
    }
    if let Some(ref tx) = pool.token_x {
        if token_id == tx.token_id {
            return tx.decimals.unwrap_or(0);
        }
    }
    0
}

fn make_pool_display_name(pool: &AmmPool, token_in: &str, token_out: &str) -> Option<String> {
    let in_name = resolve_token_name(pool, token_in)
        .unwrap_or_else(|| token_in[..8.min(token_in.len())].to_string());
    let out_name = resolve_token_name(pool, token_out)
        .unwrap_or_else(|| token_out[..8.min(token_out.len())].to_string());
    Some(format!("{}/{}", in_name, out_name))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TokenAmount;

    fn make_n2t_pool(
        pool_id: &str,
        erg_reserves: u64,
        token_id: &str,
        token_name: &str,
        token_reserves: u64,
        fee_num: i32,
    ) -> AmmPool {
        AmmPool {
            pool_id: pool_id.to_string(),
            pool_type: PoolType::N2T,
            box_id: format!("box_{}", pool_id),
            erg_reserves: Some(erg_reserves),
            token_x: None,
            token_y: TokenAmount {
                token_id: token_id.to_string(),
                amount: token_reserves,
                decimals: Some(2),
                name: Some(token_name.to_string()),
            },
            lp_token_id: format!("lp_{}", pool_id),
            lp_circulating: 1000,
            fee_num,
            fee_denom: 1000,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_t2t_pool(
        pool_id: &str,
        x_id: &str,
        x_name: &str,
        x_amount: u64,
        y_id: &str,
        y_name: &str,
        y_amount: u64,
        fee_num: i32,
    ) -> AmmPool {
        AmmPool {
            pool_id: pool_id.to_string(),
            pool_type: PoolType::T2T,
            box_id: format!("box_{}", pool_id),
            erg_reserves: Some(600_000),
            token_x: Some(TokenAmount {
                token_id: x_id.to_string(),
                amount: x_amount,
                decimals: Some(2),
                name: Some(x_name.to_string()),
            }),
            token_y: TokenAmount {
                token_id: y_id.to_string(),
                amount: y_amount,
                decimals: Some(2),
                name: Some(y_name.to_string()),
            },
            lp_token_id: format!("lp_{}", pool_id),
            lp_circulating: 1000,
            fee_num,
            fee_denom: 1000,
        }
    }

    // -- Graph Construction --

    #[test]
    fn test_build_graph_n2t_pools() {
        let pools = vec![
            make_n2t_pool("p1", 100_000_000_000, "sigusd", "SigUSD", 50_000, 997),
            make_n2t_pool("p2", 50_000_000_000, "sigrsv", "SigRSV", 100_000, 997),
        ];
        let graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
        assert_eq!(graph.pool_count, 2);

        // ERG should have 2 outgoing edges (to sigusd and sigrsv)
        let erg_edges = &graph.adjacency[ERG_TOKEN_ID];
        assert_eq!(erg_edges.len(), 2);

        // sigusd should have 1 edge back to ERG
        let sigusd_edges = &graph.adjacency["sigusd"];
        assert_eq!(sigusd_edges.len(), 1);
        assert_eq!(sigusd_edges[0].token_out, ERG_TOKEN_ID);
    }

    #[test]
    fn test_build_graph_t2t_pool() {
        let pools = vec![make_t2t_pool(
            "t2t_1", "gort", "GORT", 10_000, "sigusd", "SigUSD", 5_000, 997,
        )];
        let graph = build_pool_graph(&pools, 0); // no min liquidity for T2T test
        assert_eq!(graph.pool_count, 1);

        let gort_edges = &graph.adjacency["gort"];
        assert_eq!(gort_edges.len(), 1);
        assert_eq!(gort_edges[0].token_out, "sigusd");

        let sigusd_edges = &graph.adjacency["sigusd"];
        assert_eq!(sigusd_edges.len(), 1);
        assert_eq!(sigusd_edges[0].token_out, "gort");
    }

    #[test]
    fn test_liquidity_pruning() {
        let pools = vec![
            make_n2t_pool("deep", 100_000_000_000, "tok", "Token", 50_000, 997),
            make_n2t_pool("shallow", 1_000_000_000, "tok", "Token", 500, 997), // 1 ERG, below threshold
        ];
        let graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
        assert_eq!(graph.pool_count, 1);
        assert_eq!(graph.adjacency[ERG_TOKEN_ID].len(), 1);
        assert_eq!(graph.adjacency[ERG_TOKEN_ID][0].pool.pool_id, "deep");
    }

    #[test]
    fn test_max_pools_per_pair_pruning() {
        let pools: Vec<AmmPool> = (0..5)
            .map(|i| {
                make_n2t_pool(
                    &format!("p{}", i),
                    100_000_000_000 - i as u64 * 10_000_000_000,
                    "tok",
                    "Token",
                    50_000,
                    997,
                )
            })
            .collect();
        let graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
        // Should keep top 3 per pair
        let erg_to_tok: Vec<&PoolEdge> = graph.adjacency[ERG_TOKEN_ID]
            .iter()
            .filter(|e| e.token_out == "tok")
            .collect();
        assert_eq!(erg_to_tok.len(), DEFAULT_MAX_POOLS_PER_PAIR);
    }

    // -- Path Finding --

    #[test]
    fn test_find_direct_path() {
        let pools = vec![make_n2t_pool(
            "p1",
            100_000_000_000,
            "sigusd",
            "SigUSD",
            50_000,
            997,
        )];
        let graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "sigusd", 3);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].len(), 1);
        assert_eq!(paths[0][0].token_out, "sigusd");
    }

    #[test]
    fn test_find_multihop_path() {
        let pools = vec![
            make_n2t_pool("p1", 100_000_000_000, "gort", "GORT", 50_000, 997),
            make_t2t_pool(
                "t2t_1", "gort", "GORT", 10_000, "sigusd", "SigUSD", 5_000, 997,
            ),
        ];
        let graph = build_pool_graph(&pools, 0);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "sigusd", 3);

        // Should find: ERG -> GORT -> SigUSD (2 hops)
        assert!(!paths.is_empty());
        let two_hop = paths.iter().find(|p| p.len() == 2);
        assert!(two_hop.is_some());
        let p = two_hop.unwrap();
        assert_eq!(p[0].token_in, ERG_TOKEN_ID);
        assert_eq!(p[0].token_out, "gort");
        assert_eq!(p[1].token_in, "gort");
        assert_eq!(p[1].token_out, "sigusd");
    }

    #[test]
    fn test_no_cycles() {
        // A -> B -> C -> A loop, searching A -> C
        let pools = vec![
            make_n2t_pool("p1", 50_000_000_000, "b", "B", 10_000, 997),
            make_t2t_pool("p2", "b", "B", 10_000, "c", "C", 10_000, 997),
            make_t2t_pool("p3", "c", "C", 10_000, "d", "D", 10_000, 997), // D connects nowhere useful
        ];
        let graph = build_pool_graph(&pools, 0);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "c", 3);

        // No path should revisit ERG or any token
        for path in &paths {
            let mut seen = HashSet::new();
            seen.insert(ERG_TOKEN_ID.to_string());
            for edge in path {
                assert!(
                    !seen.contains(&edge.token_out) || edge.token_out == "c",
                    "Cycle detected in path"
                );
                seen.insert(edge.token_out.clone());
            }
        }
    }

    #[test]
    fn test_max_hops_limit() {
        // Chain: ERG -> A -> B -> C -> D
        let pools = vec![
            make_n2t_pool("p1", 50_000_000_000, "a", "A", 10_000, 997),
            make_t2t_pool("p2", "a", "A", 10_000, "b", "B", 10_000, 997),
            make_t2t_pool("p3", "b", "B", 10_000, "c", "C", 10_000, 997),
            make_t2t_pool("p4", "c", "C", 10_000, "d", "D", 10_000, 997),
        ];
        let graph = build_pool_graph(&pools, 0);

        let paths_2 = find_paths(&graph, ERG_TOKEN_ID, "d", 2);
        assert!(paths_2.is_empty(), "4-hop path should not fit in max_hops=2");

        let paths_4 = find_paths(&graph, ERG_TOKEN_ID, "d", 4);
        assert!(!paths_4.is_empty(), "4-hop path should fit in max_hops=4");
    }

    // -- Quoting --

    #[test]
    fn test_quote_single_hop() {
        let pool = make_n2t_pool("p1", 1_000_000_000_000, "tok", "Token", 10_000_000, 997);
        let graph = build_pool_graph(&[pool], DEFAULT_MIN_LIQUIDITY_NANO);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 3);
        assert_eq!(paths.len(), 1);

        let route = quote_route(&paths[0], 1_000_000_000).unwrap(); // 1 ERG
        assert_eq!(route.hops.len(), 1);
        assert!(route.total_output > 0);
        assert!(route.total_price_impact > 0.0);
        assert!(route.total_fees > 0);
    }

    #[test]
    fn test_quote_multi_hop() {
        let pools = vec![
            make_n2t_pool("p1", 100_000_000_000, "gort", "GORT", 100_000, 997),
            make_t2t_pool(
                "t2t", "gort", "GORT", 50_000, "sigusd", "SigUSD", 25_000, 997,
            ),
        ];
        let graph = build_pool_graph(&pools, 0);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "sigusd", 3);
        let two_hop_path = paths.iter().find(|p| p.len() == 2).unwrap();

        let route = quote_route(two_hop_path, 1_000_000_000).unwrap();
        assert_eq!(route.hops.len(), 2);
        assert!(route.total_output > 0);
        // Hop 1 output should be hop 2 input
        assert_eq!(route.hops[0].output_amount, route.hops[1].input_amount);
    }

    #[test]
    fn test_quote_zero_input() {
        let pool = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 10_000, 997);
        let graph = build_pool_graph(&[pool], DEFAULT_MIN_LIQUIDITY_NANO);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 3);
        assert!(quote_route(&paths[0], 0).is_none());
    }

    #[test]
    fn test_find_best_routes_ranked() {
        // Two pools for same pair, different depths
        let pools = vec![
            make_n2t_pool("deep", 1_000_000_000_000, "tok", "Token", 1_000_000, 997),
            make_n2t_pool("shallow", 50_000_000_000, "tok", "Token", 50_000, 995),
        ];
        let graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
        let routes = find_best_routes(&graph, ERG_TOKEN_ID, "tok", 10_000_000_000, 3, 5);

        assert!(routes.len() >= 2);
        // Best route should give highest output
        assert!(routes[0].total_output >= routes[1].total_output);
    }

    // -- Split Optimization --

    #[test]
    fn test_split_equal_pools() {
        let p1 = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 100_000, 997);
        let p2 = make_n2t_pool("p2", 100_000_000_000, "tok", "Token", 100_000, 997);
        let graph = build_pool_graph(&[p1, p2], DEFAULT_MIN_LIQUIDITY_NANO);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 3);

        let split = optimize_split(&paths, 50_000_000_000, 2);
        assert_eq!(split.allocations.len(), 2);
        // With equal pools, roughly 50/50 should be optimal
        let frac_0 = split.allocations[0].fraction;
        assert!(
            (frac_0 - 0.5).abs() < 0.15,
            "Expected ~50/50 split, got {:.0}%",
            frac_0 * 100.0
        );
    }

    #[test]
    fn test_split_single_route_better() {
        let p1 = make_n2t_pool("deep", 1_000_000_000_000, "tok", "Token", 1_000_000, 997);
        let graph = build_pool_graph(&[p1], DEFAULT_MIN_LIQUIDITY_NANO);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 3);

        let split = optimize_split(&paths, 1_000_000_000, 2);
        assert_eq!(split.allocations.len(), 1);
        assert_eq!(split.allocations[0].fraction, 1.0);
    }

    // -- Depth Analysis --

    #[test]
    fn test_depth_tiers_formula() {
        let edge = PoolEdge {
            pool: make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 50_000, 997),
            token_in: ERG_TOKEN_ID.to_string(),
            token_out: "tok".to_string(),
            reserves_in: 100_000_000_000, // 100 ERG
            reserves_out: 50_000,
        };

        let tiers = calculate_depth_tiers(&edge);
        assert_eq!(tiers.tiers.len(), 5);

        // 1% impact: max_input ≈ 100 ERG * 0.01 / 0.99 ≈ 1.0101 ERG
        let one_pct = tiers.tiers.iter().find(|(pct, _)| (*pct - 1.0).abs() < 0.01);
        assert!(one_pct.is_some());
        let (_, max_input) = one_pct.unwrap();
        let expected = (100_000_000_000f64 * 0.01 / 0.99) as u64;
        assert!(
            (*max_input as i64 - expected as i64).unsigned_abs() < 2,
            "Expected ~{}, got {}",
            expected,
            max_input
        );
    }

    #[test]
    fn test_depth_tiers_increasing() {
        let edge = PoolEdge {
            pool: make_n2t_pool("p1", 500_000_000_000, "tok", "Token", 100_000, 997),
            token_in: ERG_TOKEN_ID.to_string(),
            token_out: "tok".to_string(),
            reserves_in: 500_000_000_000,
            reserves_out: 100_000,
        };

        let tiers = calculate_depth_tiers(&edge);
        for i in 1..tiers.tiers.len() {
            assert!(
                tiers.tiers[i].1 > tiers.tiers[i - 1].1,
                "Higher impact tier should allow larger input"
            );
        }
    }

    // -- Route Quote --

    #[test]
    fn test_make_route_quote_slippage() {
        let pool = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 100_000, 997);
        let graph = build_pool_graph(&[pool], DEFAULT_MIN_LIQUIDITY_NANO);
        let routes = find_best_routes(&graph, ERG_TOKEN_ID, "tok", 1_000_000_000, 3, 1);
        let quote = make_route_quote(routes[0].clone(), 0.5);

        assert!(quote.min_output < quote.route.total_output);
        let expected_min = apply_slippage(quote.route.total_output, 0.5);
        assert_eq!(quote.min_output, expected_min);
    }

    // -- Reverse Quoting --

    #[test]
    fn test_reverse_quote_single_hop() {
        let pool = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 5_000_000, 997);
        let graph = build_pool_graph(&[pool], DEFAULT_MIN_LIQUIDITY_NANO);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 3);
        assert!(!paths.is_empty());

        // Forward: 1 ERG -> some tokens
        let forward = quote_route(&paths[0], 1_000_000_000).unwrap();

        // Reverse: ask for that many tokens -> should need ~1 ERG
        let reverse = quote_route_reverse(&paths[0], forward.total_output).unwrap();

        // The reverse input should be close to 1 ERG (within 0.01% rounding)
        let diff = (reverse.total_input as i64 - 1_000_000_000i64).unsigned_abs();
        assert!(
            diff < 100_000, // < 0.01% of 1 ERG
            "Reverse input {} should be ~1_000_000_000 (diff={})",
            reverse.total_input,
            diff
        );
    }

    #[test]
    fn test_reverse_quote_exceeds_reserves() {
        let pool = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 100, 997);
        let graph = build_pool_graph(&[pool], DEFAULT_MIN_LIQUIDITY_NANO);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 3);

        // Request more tokens than the pool has
        let result = quote_route_reverse(&paths[0], 200);
        assert!(result.is_none());
    }

    #[test]
    fn test_reverse_routes_ranked_by_input() {
        let pools = vec![
            make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 5_000_000, 997),
            make_n2t_pool("p2", 50_000_000_000, "tok", "Token", 5_000_000, 995),
        ];
        let graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
        let routes =
            find_best_routes_by_output(&graph, ERG_TOKEN_ID, "tok", 1000, 3, 5);

        assert!(routes.len() >= 2);
        // Should be sorted ascending by total_input
        for i in 1..routes.len() {
            assert!(routes[i].total_input >= routes[i - 1].total_input);
        }
    }

    #[test]
    fn test_reverse_multi_hop() {
        let pools = vec![
            make_n2t_pool("p1", 100_000_000_000, "gort", "GORT", 10_000_000, 997),
            make_t2t_pool(
                "p2", "gort", "GORT", 5_000_000,
                "sigusd", "SigUSD", 2_500_000, 997,
            ),
        ];
        let graph = build_pool_graph(&pools, 0);
        let paths = find_paths(&graph, ERG_TOKEN_ID, "sigusd", 3);
        assert!(!paths.is_empty(), "Should find ERG->GORT->SigUSD path");

        // Request 100 SigUSD cents
        let reverse = quote_route_reverse(&paths[0], 100);
        assert!(reverse.is_some());
        let route = reverse.unwrap();
        assert_eq!(route.hops.len(), 2);
        assert!(route.total_input > 0);
        // Verify the last hop outputs close to 100
        assert!(route.hops[1].output_amount >= 99);
    }

    // -----------------------------------------------------------------------
    // Oracle arb snapshot tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_oracle_arb_snapshot_has_window() {
        // Pool: 100 ERG, 50000 SigUSD-cents (= 500 SigUSD), fee 997/1000
        // Spot rate at 1 ERG ≈ 4.96 SigUSD/ERG (very above oracle 4.0)
        let pool = make_n2t_pool("arb1", 100_000_000_000, "sigusd", "SigUSD", 5_000_000, 997);
        let graph = build_pool_graph(&[pool], 0);
        let snap = calculate_oracle_arb_snapshot(&graph, "sigusd", 4.0, 2);
        assert_eq!(snap.windows.len(), 1);
        assert!(snap.windows[0].discount_pct > 0.0);
        assert!(snap.windows[0].max_erg_input_nano > 0);
        assert!(snap.windows[0].sigusd_output_at_max > 0);
        assert!(snap.total_sigusd_below_oracle_raw > 0);
        assert!(snap.total_erg_needed_nano > 0);
    }

    #[test]
    fn test_oracle_arb_snapshot_no_window() {
        // Pool: 100 ERG, 100 SigUSD-cents (= 1 SigUSD), fee 997/1000
        // Spot rate ~ 0.00997 SigUSD/ERG — well below oracle of 2.0
        let pool = make_n2t_pool("noarb", 100_000_000_000, "sigusd", "SigUSD", 100, 997);
        let graph = build_pool_graph(&[pool], 0);
        let snap = calculate_oracle_arb_snapshot(&graph, "sigusd", 2.0, 2);
        assert!(snap.windows.is_empty());
        assert_eq!(snap.total_sigusd_below_oracle_raw, 0);
    }

    #[test]
    fn test_oracle_arb_snapshot_empty_graph() {
        let graph = build_pool_graph(&[], 0);
        let snap = calculate_oracle_arb_snapshot(&graph, "sigusd", 2.0, 2);
        assert!(snap.windows.is_empty());
    }
}
