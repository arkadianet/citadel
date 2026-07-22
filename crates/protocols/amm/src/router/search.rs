//! Path finding and route quoting.

use std::collections::{HashSet, VecDeque};

use super::types::{
    miner_fees_for_hops, PoolEdge, PoolGraph, Route, RouteHop, RouteQuote, ERG_TOKEN_ID,
};
use crate::calculator::{
    apply_slippage, calculate_input, calculate_output, calculate_price_impact,
    calculate_token_to_erg_output, would_breach_pool_min_erg,
};
use crate::state::AmmPool;

fn finish_route(
    hops: Vec<RouteHop>,
    total_input: u64,
    total_output: u64,
    total_fees: u64,
    total_price_impact: f64,
    effective_rate: f64,
) -> Route {
    let total_miner_fees = miner_fees_for_hops(hops.len());
    let ends_in_erg = hops
        .last()
        .map(|h| h.token_out == ERG_TOKEN_ID)
        .unwrap_or(false);
    let net_output = if ends_in_erg {
        total_output.saturating_sub(total_miner_fees)
    } else {
        total_output
    };
    Route {
        hops,
        total_input,
        total_output,
        total_price_impact,
        total_fees,
        total_miner_fees,
        net_output,
        effective_rate,
    }
}

pub(crate) fn route_score(route: &Route) -> u64 {
    route.net_output
}

pub(crate) fn path_ends_in_erg(path: &[PoolEdge]) -> bool {
    path.last()
        .map(|e| e.token_out == ERG_TOKEN_ID)
        .unwrap_or(false)
}

pub(crate) fn quote_allocation(path: &[PoolEdge], input: u64) -> (u64, u64, u64) {
    if input == 0 {
        return (0, 0, 0);
    }
    match quote_route(path, input) {
        Some(r) => (r.total_output, r.total_miner_fees, route_score(&r)),
        None => (0, 0, 0),
    }
}

/// BFS for all acyclic paths up to `max_hops`. No token or pool_id reused within a path.
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
                if used_pools.contains(&edge.pool.pool_id) {
                    continue;
                }

                if edge.token_out == target_token {
                    let mut complete_path = path.clone();
                    complete_path.push(edge.clone());
                    results.push(complete_path);
                } else if path.len() + 1 < max_hops && !visited.contains(&edge.token_out) {
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

/// Chain `calculate_output` through each hop. Returns `None` if any hop yields zero.
pub fn quote_route(path: &[PoolEdge], input_amount: u64) -> Option<Route> {
    if path.is_empty() || input_amount == 0 {
        return None;
    }

    let mut current_amount = input_amount;
    let mut hops = Vec::with_capacity(path.len());
    let mut total_fees: u64 = 0;

    for edge in path {
        let output = if edge.token_out == ERG_TOKEN_ID {
            calculate_token_to_erg_output(
                edge.reserves_in,
                edge.reserves_out,
                current_amount,
                edge.pool.fee_num,
                edge.pool.fee_denom,
            )
        } else {
            calculate_output(
                edge.reserves_in,
                edge.reserves_out,
                current_amount,
                edge.pool.fee_num,
                edge.pool.fee_denom,
            )
        };

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

    // Compare actual rate to product of spot prices for end-to-end impact
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

    Some(finish_route(
        hops,
        input_amount,
        total_output,
        total_fees,
        total_price_impact,
        effective_rate,
    ))
}

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

    // Prefer fee-aware net when ending in ERG; otherwise gross (== net).
    routes.sort_by_key(|b| std::cmp::Reverse(route_score(b)));
    routes.truncate(max_routes);
    routes
}

pub fn make_route_quote(route: Route, slippage_percent: f64) -> RouteQuote {
    let min_output = apply_slippage(route.total_output, slippage_percent);
    RouteQuote {
        route,
        min_output,
        slippage_percent,
    }
}

/// Reverse quote: given desired output, walk hops backwards via `calculate_input`.
/// Returns `None` if any hop is infeasible (output exceeds reserves).
pub fn quote_route_reverse(path: &[PoolEdge], desired_output: u64) -> Option<Route> {
    if path.is_empty() || desired_output == 0 {
        return None;
    }

    let mut required_amounts: Vec<(u64, u64)> = Vec::with_capacity(path.len());
    let mut needed = desired_output;

    for edge in path.iter().rev() {
        if edge.token_out == ERG_TOKEN_ID && would_breach_pool_min_erg(edge.reserves_out, needed) {
            return None;
        }
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

    let total_input = required_amounts[0].0;
    let mut hops = Vec::with_capacity(path.len());
    let mut total_fees: u64 = 0;

    for (i, edge) in path.iter().enumerate() {
        let (input_amount, _) = required_amounts[i];

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

    Some(finish_route(
        hops,
        total_input,
        total_output,
        total_fees,
        total_price_impact,
        actual_rate,
    ))
}

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

    routes.sort_by_key(|a| a.total_input);
    routes.truncate(max_routes);
    routes
}

pub(crate) fn resolve_token_name(pool: &AmmPool, token_id: &str) -> Option<String> {
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

pub(crate) fn resolve_token_decimals(pool: &AmmPool, token_id: &str) -> u8 {
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

pub(crate) fn make_pool_display_name(
    pool: &AmmPool,
    token_in: &str,
    token_out: &str,
) -> Option<String> {
    let in_name = resolve_token_name(pool, token_in)
        .unwrap_or_else(|| token_in[..8.min(token_in.len())].to_string());
    let out_name = resolve_token_name(pool, token_out)
        .unwrap_or_else(|| token_out[..8.min(token_out.len())].to_string());
    Some(format!("{}/{}", in_name, out_name))
}
