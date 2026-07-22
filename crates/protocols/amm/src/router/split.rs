//! Split-route optimization.

use super::arb::route_rate_at_input;
use super::search::{find_paths, path_ends_in_erg, quote_allocation, quote_route, route_score};
use super::types::{
    miner_fees_for_hops, PoolEdge, PoolGraph, SplitAllocation, SplitAllocationDetail, SplitRoute,
    SplitRouteDetail,
};

/// Grid-search optimal split across up to 3 routes to maximize fee-aware net.
pub fn optimize_split(paths: &[Vec<PoolEdge>], total_input: u64, max_splits: usize) -> SplitRoute {
    let max_splits = max_splits.min(paths.len()).min(3);

    if max_splits <= 1 || paths.is_empty() {
        let (output, fees, net) = paths
            .first()
            .map(|p| quote_allocation(p, total_input))
            .unwrap_or((0, 0, 0));
        return SplitRoute {
            allocations: vec![SplitAllocation {
                route_index: 0,
                fraction: 1.0,
                input_amount: total_input,
                output_amount: output,
            }],
            total_output: output,
            total_input,
            total_miner_fees: fees,
            net_output: net,
        };
    }

    if max_splits == 2 {
        return optimize_split_two(paths, total_input);
    }

    // For 3 routes: iterative pairwise optimization
    optimize_split_multi(paths, total_input, max_splits)
}

fn finalize_split(
    paths: &[Vec<PoolEdge>],
    total_input: u64,
    allocations: Vec<SplitAllocation>,
) -> SplitRoute {
    let total_output: u64 = allocations.iter().map(|a| a.output_amount).sum();
    let ends_in_erg = paths.first().map(|p| path_ends_in_erg(p)).unwrap_or(false);
    let total_miner_fees: u64 = allocations
        .iter()
        .filter(|a| a.input_amount > 0)
        .map(|a| miner_fees_for_hops(paths[a.route_index].len()))
        .sum();
    let net_output = if ends_in_erg {
        total_output.saturating_sub(total_miner_fees)
    } else {
        total_output
    };

    SplitRoute {
        allocations,
        total_output,
        total_input,
        total_miner_fees,
        net_output,
    }
}

fn optimize_split_two(paths: &[Vec<PoolEdge>], total_input: u64) -> SplitRoute {
    let mut best_score: u64 = 0;
    let mut best_permille: u64 = 1000; // permille for path[0]

    for permille in 0..=1000u64 {
        let input_a = total_input * permille / 1000;
        let input_b = total_input - input_a;

        let (_, _, score_a) = quote_allocation(&paths[0], input_a);
        let (_, _, score_b) = quote_allocation(&paths[1], input_b);
        let score = score_a.saturating_add(score_b);

        if score > best_score {
            best_score = score;
            best_permille = permille;
        }
    }

    let input_a = total_input * best_permille / 1000;
    let input_b = total_input - input_a;
    let (out_a, _, _) = quote_allocation(&paths[0], input_a);
    let (out_b, _, _) = quote_allocation(&paths[1], input_b);

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

    finalize_split(paths, total_input, allocations)
}

fn optimize_split_multi(
    paths: &[Vec<PoolEdge>],
    total_input: u64,
    max_splits: usize,
) -> SplitRoute {
    let n = max_splits.min(paths.len());
    let mut fractions: Vec<u64> = vec![1000 / n as u64; n];
    let remainder = 1000 - fractions.iter().sum::<u64>();
    fractions[0] += remainder;

    for _ in 0..5 {
        for i in 0..n {
            let mut best_score: u64 = 0;
            let mut best_frac: u64 = fractions[i];

            let other_sum: u64 = fractions
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, f)| f)
                .sum();

            let max_for_i = 1000 - other_sum;

            for f in (0..=max_for_i).step_by(10) {
                let mut test_fracs = fractions.clone();
                test_fracs[i] = f;
                let diff = max_for_i - f;
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

                let score: u64 = (0..n)
                    .map(|k| {
                        let inp = total_input * test_fracs[k] / 1000;
                        quote_allocation(&paths[k], inp).2
                    })
                    .sum();

                if score > best_score {
                    best_score = score;
                    best_frac = f;
                }
            }

            fractions[i] = best_frac;
        }
    }

    let sum: u64 = fractions.iter().sum();
    if sum != 1000 && sum > 0 {
        let scale = 1000.0 / sum as f64;
        for f in &mut fractions {
            *f = (*f as f64 * scale).round() as u64;
        }
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
            let (output, _, _) = quote_allocation(&paths[k], input);
            SplitAllocation {
                route_index: k,
                fraction: fractions[k] as f64 / 1000.0,
                input_amount: input,
                output_amount: output,
            }
        })
        .collect();

    finalize_split(paths, total_input, allocations)
}

/// Optimal split with full route details. Only returned if > 0.5% better on net.
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

    let unfiltered_best_net: u64 = all_paths
        .iter()
        .filter_map(|p| quote_route(p, total_input).map(|r| route_score(&r)))
        .max()
        .unwrap_or(0);
    if unfiltered_best_net == 0 {
        return None;
    }

    // Exclude routes trading below price floor (e.g. below oracle price) at small amounts
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

    let mut quoted: Vec<(usize, u64)> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, p)| quote_route(p, total_input).map(|r| (i, route_score(&r))))
        .collect();
    quoted.sort_by_key(|b| std::cmp::Reverse(b.1));

    if quoted.is_empty() {
        return None;
    }

    let max_splits = max_splits.min(quoted.len()).min(3);
    if max_splits < 2 {
        return None;
    }

    quoted.truncate(max_splits);
    let top_paths: Vec<Vec<PoolEdge>> = quoted.iter().map(|(i, _)| paths[*i].clone()).collect();

    let split = optimize_split(&top_paths, total_input, max_splits);

    let improvement =
        (split.net_output as f64 - unfiltered_best_net as f64) / unfiltered_best_net as f64 * 100.0;

    if improvement < 0.5 {
        return None;
    }

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
        total_miner_fees: split.total_miner_fees,
        net_output: split.net_output,
        improvement_pct: improvement,
        allocations,
    })
}
