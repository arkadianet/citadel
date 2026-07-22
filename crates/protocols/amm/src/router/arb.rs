//! Oracle window and circular arbitrage scans.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::search::{find_paths, quote_route, quote_route_reverse, resolve_token_name};
use super::types::{PoolEdge, PoolGraph, Route, ERG_TOKEN_ID, MINER_FEE_PER_HOP};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleArbWindow {
    pub path_label: String,
    pub hops: usize,
    pub pool_ids: Vec<String>,
    pub spot_rate_usd_per_erg: f64,
    /// Positive = route is cheaper than oracle
    pub discount_pct: f64,
    pub rate_at_max: f64,
    pub max_erg_input_nano: u64,
    pub price_impact_at_max: f64,
    /// Raw cents
    pub sigusd_output_at_max: u64,
    pub sigusd_output_at_max_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleArbSnapshot {
    pub oracle_rate_usd_per_erg: f64,
    pub windows: Vec<OracleArbWindow>,
    pub total_sigusd_below_oracle_raw: u64,
    pub total_erg_needed_nano: u64,
}

pub(crate) fn route_rate_at_input(
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

/// Binary search for max input where rate stays >= oracle_rate (monotone decreasing for CPMM).
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
        // No probe pre-check: binary search converges even for thin multi-hop
        // routes where small probes fail due to integer rounding.
        let upper = path[0].reserves_in;
        let (max_input, output_at_max) =
            binary_search_breakeven(path, oracle_rate_usd_per_erg, target_decimals, upper);

        if max_input == 0 || output_at_max == 0 {
            continue;
        }

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
        let path_label = label_parts.join(" \u{2192} ");

        let divisor = 10f64.powi(target_decimals as i32);
        let rate_at_max = (output_at_max as f64 / divisor) / (max_input as f64 / 1e9);
        let impact_at_max = quote_route(path, max_input)
            .map(|r| r.total_price_impact)
            .unwrap_or(0.0);

        // 1/10th of max_input gives marginal rate near the beginning of the arb window
        let spot_probe = (max_input / 10).max(1_000_000);
        let spot_rate = route_rate_at_input(path, spot_probe, target_decimals)
            .map(|(r, _)| r)
            .unwrap_or(rate_at_max);

        let discount_pct = (spot_rate - oracle_rate_usd_per_erg) / oracle_rate_usd_per_erg * 100.0;

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

    windows.retain(|w| w.sigusd_output_at_max >= 10); // >= $0.10
    windows.sort_by_key(|b| std::cmp::Reverse(b.sigusd_output_at_max));

    let total_sigusd = windows.iter().map(|w| w.sigusd_output_at_max).sum();
    let total_erg = windows.iter().map(|w| w.max_erg_input_nano).sum();

    OracleArbSnapshot {
        oracle_rate_usd_per_erg,
        windows,
        total_sigusd_below_oracle_raw: total_sigusd,
        total_erg_needed_nano: total_erg,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircularArb {
    pub path_label: String,
    pub hops: usize,
    pub pool_ids: Vec<String>,
    pub optimal_input_nano: u64,
    pub output_nano: u64,
    pub gross_profit_nano: i64,
    pub tx_fee_nano: u64,
    pub net_profit_nano: i64,
    pub profit_pct: f64,
    pub price_impact: f64,
    pub route: Route,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircularArbSnapshot {
    pub windows: Vec<CircularArb>,
    pub total_net_profit_nano: i64,
    pub scan_time_ms: u64,
}

/// DFS for all ERG->...->ERG cycles up to max_hops. No token/pool reused within a cycle.
pub fn find_cycles(graph: &PoolGraph, max_hops: usize) -> Vec<Vec<PoolEdge>> {
    let mut results: Vec<Vec<PoolEdge>> = Vec::new();

    type State = (String, Vec<PoolEdge>, HashSet<String>, HashSet<String>);
    let mut stack: Vec<State> = Vec::new();

    let mut initial_visited = HashSet::new();
    initial_visited.insert(ERG_TOKEN_ID.to_string());
    stack.push((
        ERG_TOKEN_ID.to_string(),
        Vec::new(),
        initial_visited,
        HashSet::new(),
    ));

    while let Some((current, path, visited, used_pools)) = stack.pop() {
        if let Some(edges) = graph.adjacency.get(current.as_str()) {
            for edge in edges {
                if used_pools.contains(&edge.pool.pool_id) {
                    continue;
                }

                if edge.token_out == ERG_TOKEN_ID && !path.is_empty() {
                    let mut cycle = path.clone();
                    cycle.push(edge.clone());
                    results.push(cycle);
                } else if path.len() + 1 < max_hops && !visited.contains(&edge.token_out) {
                    let mut new_visited = visited.clone();
                    new_visited.insert(edge.token_out.clone());
                    let mut new_pools = used_pools.clone();
                    new_pools.insert(edge.pool.pool_id.clone());
                    let mut new_path = path.clone();
                    new_path.push(edge.clone());
                    stack.push((edge.token_out.clone(), new_path, new_visited, new_pools));
                }
            }
        }
    }

    results
}

/// Ternary search for profit-maximizing input on each cycle (unimodal: arb then impact).
pub fn find_circular_arbs(
    graph: &PoolGraph,
    max_hops: usize,
    min_profit_nano: i64,
) -> CircularArbSnapshot {
    let start = std::time::Instant::now();
    let cycles = find_cycles(graph, max_hops);

    let tx_fee_per_hop: u64 = MINER_FEE_PER_HOP;

    let mut windows: Vec<CircularArb> = Vec::new();

    for cycle in &cycles {
        if cycle.is_empty() {
            continue;
        }

        let hi_cap = cycle[0].reserves_in.min(1_000_000_000_000);
        let lo: u64 = 10_000_000;

        if hi_cap <= lo {
            continue;
        }

        let mut a = lo;
        let mut b = hi_cap;

        for _ in 0..80 {
            if b - a < 1_000_000 {
                break;
            }
            let m1 = a + (b - a) / 3;
            let m2 = b - (b - a) / 3;

            let p1 = quote_route(cycle, m1)
                .map(|r| r.total_output as i64 - m1 as i64)
                .unwrap_or(i64::MIN);
            let p2 = quote_route(cycle, m2)
                .map(|r| r.total_output as i64 - m2 as i64)
                .unwrap_or(i64::MIN);

            if p1 < p2 {
                a = m1;
            } else {
                b = m2;
            }
        }

        let forward_input = (a + b) / 2;
        let forward_route = match quote_route(cycle, forward_input) {
            Some(r) => r,
            None => continue,
        };

        let forward_output = forward_route.total_output;

        // Reverse-tighten: find exact minimum input, re-quote forward for consistent hop chain
        let (optimal_input, route) = if let Some(tight) = quote_route_reverse(cycle, forward_output)
        {
            if tight.total_input < forward_input {
                match quote_route(cycle, tight.total_input) {
                    Some(r) => (tight.total_input, r),
                    None => (forward_input, forward_route),
                }
            } else {
                (forward_input, forward_route)
            }
        } else {
            (forward_input, forward_route)
        };

        let output = route.total_output;
        let gross_profit = output as i64 - optimal_input as i64;
        let hops = cycle.len();
        let tx_fee = tx_fee_per_hop * hops as u64;
        let net_profit = gross_profit - tx_fee as i64;

        if net_profit < min_profit_nano {
            continue;
        }

        let profit_pct = if optimal_input > 0 {
            net_profit as f64 / optimal_input as f64 * 100.0
        } else {
            0.0
        };

        let mut label_parts: Vec<String> = vec!["ERG".to_string()];
        for edge in cycle {
            let name = resolve_token_name(&edge.pool, &edge.token_out)
                .unwrap_or_else(|| edge.token_out[..6.min(edge.token_out.len())].to_string());
            label_parts.push(name);
        }
        let path_label = label_parts.join(" \u{2192} ");

        let pool_ids: Vec<String> = cycle.iter().map(|e| e.pool.pool_id.clone()).collect();

        windows.push(CircularArb {
            path_label,
            hops,
            pool_ids,
            optimal_input_nano: optimal_input,
            output_nano: output,
            gross_profit_nano: gross_profit,
            tx_fee_nano: tx_fee,
            net_profit_nano: net_profit,
            profit_pct,
            price_impact: route.total_price_impact,
            route,
        });
    }

    windows.sort_by_key(|b| std::cmp::Reverse(b.net_profit_nano));

    let total_net = windows.iter().map(|w| w.net_profit_nano).sum();
    let elapsed = start.elapsed().as_millis() as u64;

    CircularArbSnapshot {
        windows,
        total_net_profit_nano: total_net,
        scan_time_ms: elapsed,
    }
}
