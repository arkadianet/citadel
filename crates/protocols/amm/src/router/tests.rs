use std::collections::HashSet;

use super::*;
use crate::calculator::apply_slippage;
use crate::state::{AmmPool, PoolType, TokenAmount};

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

#[test]
fn test_build_graph_n2t_pools() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "sigusd", "SigUSD", 50_000, 997),
        make_n2t_pool("p2", 50_000_000_000, "sigrsv", "SigRSV", 100_000, 997),
    ];
    let graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
    assert_eq!(graph.pool_count, 2);

    let erg_edges = &graph.adjacency[ERG_TOKEN_ID];
    assert_eq!(erg_edges.len(), 2);

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
        make_n2t_pool("shallow", 1_000_000_000, "tok", "Token", 500, 997),
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
    let erg_to_tok: Vec<&PoolEdge> = graph.adjacency[ERG_TOKEN_ID]
        .iter()
        .filter(|e| e.token_out == "tok")
        .collect();
    assert_eq!(erg_to_tok.len(), DEFAULT_MAX_POOLS_PER_PAIR);
}

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
    let pools = vec![
        make_n2t_pool("p1", 50_000_000_000, "b", "B", 10_000, 997),
        make_t2t_pool("p2", "b", "B", 10_000, "c", "C", 10_000, 997),
        make_t2t_pool("p3", "c", "C", 10_000, "d", "D", 10_000, 997), // D connects nowhere useful
    ];
    let graph = build_pool_graph(&pools, 0);
    let paths = find_paths(&graph, ERG_TOKEN_ID, "c", 3);

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
    let pools = vec![
        make_n2t_pool("p1", 50_000_000_000, "a", "A", 10_000, 997),
        make_t2t_pool("p2", "a", "A", 10_000, "b", "B", 10_000, 997),
        make_t2t_pool("p3", "b", "B", 10_000, "c", "C", 10_000, 997),
        make_t2t_pool("p4", "c", "C", 10_000, "d", "D", 10_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);

    let paths_2 = find_paths(&graph, ERG_TOKEN_ID, "d", 2);
    assert!(
        paths_2.is_empty(),
        "4-hop path should not fit in max_hops=2"
    );

    let paths_4 = find_paths(&graph, ERG_TOKEN_ID, "d", 4);
    assert!(!paths_4.is_empty(), "4-hop path should fit in max_hops=4");
}

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
    let pools = vec![
        make_n2t_pool("deep", 1_000_000_000_000, "tok", "Token", 1_000_000, 997),
        make_n2t_pool("shallow", 50_000_000_000, "tok", "Token", 50_000, 995),
    ];
    let graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
    let routes = find_best_routes(&graph, ERG_TOKEN_ID, "tok", 10_000_000_000, 3, 5);

    assert!(routes.len() >= 2);
    assert!(routes[0].total_output >= routes[1].total_output);
}

#[test]
fn test_split_equal_pools() {
    let p1 = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 100_000, 997);
    let p2 = make_n2t_pool("p2", 100_000_000_000, "tok", "Token", 100_000, 997);
    let graph = build_pool_graph(&[p1, p2], DEFAULT_MIN_LIQUIDITY_NANO);
    let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 3);

    let split = optimize_split(&paths, 50_000_000_000, 2);
    assert_eq!(split.allocations.len(), 2);
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
    let one_pct = tiers
        .tiers
        .iter()
        .find(|(pct, _)| (*pct - 1.0).abs() < 0.01);
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

#[test]
fn test_reverse_quote_single_hop() {
    let pool = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 5_000_000, 997);
    let graph = build_pool_graph(&[pool], DEFAULT_MIN_LIQUIDITY_NANO);
    let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 3);
    assert!(!paths.is_empty());

    let forward = quote_route(&paths[0], 1_000_000_000).unwrap();
    let reverse = quote_route_reverse(&paths[0], forward.total_output).unwrap();
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
    let routes = find_best_routes_by_output(&graph, ERG_TOKEN_ID, "tok", 1000, 3, 5);

    assert!(routes.len() >= 2);
    for i in 1..routes.len() {
        assert!(routes[i].total_input >= routes[i - 1].total_input);
    }
}

#[test]
fn test_reverse_multi_hop() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "gort", "GORT", 10_000_000, 997),
        make_t2t_pool(
            "p2", "gort", "GORT", 5_000_000, "sigusd", "SigUSD", 2_500_000, 997,
        ),
    ];
    let graph = build_pool_graph(&pools, 0);
    let paths = find_paths(&graph, ERG_TOKEN_ID, "sigusd", 3);
    assert!(!paths.is_empty(), "Should find ERG->GORT->SigUSD path");

    let reverse = quote_route_reverse(&paths[0], 100);
    assert!(reverse.is_some());
    let route = reverse.unwrap();
    assert_eq!(route.hops.len(), 2);
    assert!(route.total_input > 0);
    assert!(route.hops[1].output_amount >= 99);
}

#[test]
fn test_oracle_arb_snapshot_has_window() {
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

#[test]
fn test_find_cycles_triangle() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "token_a", "TokenA", 50_000, 997),
        make_n2t_pool("p3", 200_000_000_000, "token_b", "TokenB", 50_000, 997),
        make_t2t_pool(
            "p2", "token_a", "TokenA", 50_000, "token_b", "TokenB", 50_000, 997,
        ),
    ];
    let graph = build_pool_graph(&pools, 0);
    let cycles = find_cycles(&graph, 4);
    assert!(cycles.len() >= 2);
    for cycle in &cycles {
        assert!(cycle.len() >= 2);
        assert_eq!(cycle.last().unwrap().token_out, ERG_TOKEN_ID);
    }
}

#[test]
fn test_find_cycles_no_loop() {
    let pools = vec![make_n2t_pool(
        "p1",
        100_000_000_000,
        "token_a",
        "TokenA",
        50_000,
        997,
    )];
    let graph = build_pool_graph(&pools, 0);
    let cycles = find_cycles(&graph, 4);
    assert_eq!(cycles.len(), 0);
}

#[test]
fn test_find_cycles_respects_max_hops() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "a", "A", 50_000, 997),
        make_n2t_pool("p2", 100_000_000_000, "b", "B", 50_000, 997),
        make_t2t_pool("p3", "a", "A", 50_000, "b", "B", 50_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let cycles_2 = find_cycles(&graph, 2);
    for c in &cycles_2 {
        assert!(c.len() <= 2);
    }
    let cycles_4 = find_cycles(&graph, 4);
    assert!(cycles_4.len() >= cycles_2.len());
}

#[test]
fn test_circular_arb_profitable() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "aa", "TokenA", 200_000, 997),
        make_n2t_pool("p3", 200_000_000_000, "bb", "TokenB", 50_000, 997),
        make_t2t_pool("p2", "aa", "TokenA", 200_000, "bb", "TokenB", 100_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let snap = find_circular_arbs(&graph, 4, 0);
    assert!(!snap.windows.is_empty(), "Should find profitable arbs");
    let best = &snap.windows[0];
    assert!(best.net_profit_nano > 0, "Best arb should be profitable");
    assert!(best.optimal_input_nano > 0);
    assert!(best.output_nano > best.optimal_input_nano);
}

#[test]
fn test_circular_arb_no_opportunity() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "aa", "A", 50_000, 997),
        make_n2t_pool("p2", 100_000_000_000, "bb", "B", 50_000, 997),
        make_t2t_pool("p3", "aa", "A", 50_000, "bb", "B", 50_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let snap = find_circular_arbs(&graph, 4, 1_000_000);
    assert!(snap.windows.is_empty(), "Balanced pools should have no arb");
}

#[test]
fn test_quote_erg_out_nets_miner_fees() {
    let pool = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 1_000_000, 997);
    let graph = build_pool_graph(&[pool], 0);
    let paths = find_paths(&graph, "tok", ERG_TOKEN_ID, 1);
    let route = quote_route(&paths[0], 10_000).unwrap();
    assert_eq!(route.total_miner_fees, MINER_FEE_PER_HOP);
    assert_eq!(
        route.net_output,
        route.total_output.saturating_sub(MINER_FEE_PER_HOP)
    );
}

#[test]
fn test_quote_token_out_net_equals_gross() {
    let pool = make_n2t_pool("p1", 100_000_000_000, "tok", "Token", 1_000_000, 997);
    let graph = build_pool_graph(&[pool], 0);
    let paths = find_paths(&graph, ERG_TOKEN_ID, "tok", 1);
    let route = quote_route(&paths[0], 1_000_000_000).unwrap();
    assert_eq!(route.total_miner_fees, MINER_FEE_PER_HOP);
    assert_eq!(route.net_output, route.total_output);
}

#[test]
fn test_erg_out_ranking_prefers_fewer_hops_when_gross_close() {
    // Direct pool slightly worse gross than a 3-hop path, but far better after fees.
    // Direct: large output. Multi-hop: slightly higher gross, 3× miner fees.
    let direct = make_n2t_pool("direct", 100_000_000_000, "comet", "COMET", 50_000_000, 997);
    // Build a long path comet -> mid -> mid2 -> ERG that needs intermediate tokens.
    let mid = make_t2t_pool(
        "t2t1", "comet", "COMET", 50_000_000, "mid", "MID", 50_000_000, 997,
    );
    let mid2 = make_t2t_pool(
        "t2t2", "mid", "MID", 50_000_000, "mid2", "MID2", 50_000_000, 997,
    );
    let to_erg = make_n2t_pool("to_erg", 200_000_000_000, "mid2", "MID2", 100_000_000, 997);
    let graph = build_pool_graph(&[direct, mid, mid2, to_erg], 0);

    let input = 1_000_000u64; // 1M COMET (0 decimals in test helpers)
    let routes = find_best_routes(&graph, "comet", ERG_TOKEN_ID, input, 4, 10);
    assert!(!routes.is_empty());
    // Best by net should be the 1-hop when multi-hop fees dominate small gains.
    let best = &routes[0];
    assert_eq!(best.hops.len(), 1, "fee-aware ranking should prefer 1-hop");
    assert!(best.net_output <= best.total_output);
}

#[test]
fn test_split_detailed_uses_net_improvement() {
    // Two equal ERG-out pools: split improves gross/net similarly (1 hop each).
    let p1 = make_n2t_pool("p1", 50_000_000_000, "comet", "COMET", 50_000_000, 997);
    let p2 = make_n2t_pool("p2", 50_000_000_000, "comet", "COMET", 50_000_000, 997);
    let graph = build_pool_graph(&[p1, p2], 0);
    let split = optimize_split_detailed(&graph, "comet", ERG_TOKEN_ID, 20_000_000, 2, 2, None);
    if let Some(s) = split {
        assert_eq!(s.total_miner_fees, MINER_FEE_PER_HOP * 2);
        assert_eq!(
            s.net_output,
            s.total_output.saturating_sub(s.total_miner_fees)
        );
        assert!(s.improvement_pct > 0.5);
    }
}

#[test]
fn test_max_swap_hint_when_amount_too_large() {
    let pool = make_n2t_pool("thin", 10_000_000, "etosi", "eTOSI", 1_000, 997);
    let graph = build_pool_graph(&[pool], 0);
    let requested = 20_000u64;
    assert!(find_best_routes(&graph, "etosi", ERG_TOKEN_ID, requested, 2, 5).is_empty());
    let hint = max_swap_hint_if_needed(&graph, "etosi", ERG_TOKEN_ID, requested, 2).unwrap();
    assert!(hint.max_input > 0 && hint.max_input < requested);
    assert!(!find_best_routes(&graph, "etosi", ERG_TOKEN_ID, hint.max_input, 2, 5).is_empty());
    assert_eq!(hint.reason, "pool_min_erg");
}

#[test]
fn test_max_swap_hint_absent_when_amount_ok() {
    let pool = make_n2t_pool("deep", 100_000_000_000, "etosi", "eTOSI", 1_000_000, 997);
    let graph = build_pool_graph(&[pool], 0);
    assert!(max_swap_hint_if_needed(&graph, "etosi", ERG_TOKEN_ID, 1_000, 2).is_none());
}

#[test]
fn test_circular_arb_tx_fees_deducted() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "aa", "TokenA", 200_000, 997),
        make_n2t_pool("p3", 200_000_000_000, "bb", "TokenB", 50_000, 997),
        make_t2t_pool("p2", "aa", "TokenA", 200_000, "bb", "TokenB", 100_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let snap = find_circular_arbs(&graph, 4, 0);
    for arb in &snap.windows {
        assert_eq!(arb.tx_fee_nano, MINER_FEE_PER_HOP * arb.hops as u64);
        assert_eq!(
            arb.net_profit_nano,
            arb.gross_profit_nano - arb.tx_fee_nano as i64
        );
    }
}

#[test]
fn test_circular_arb_reverse_tightened() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "aa", "TokenA", 200_000, 997),
        make_n2t_pool("p3", 200_000_000_000, "bb", "TokenB", 50_000, 997),
        make_t2t_pool("p2", "aa", "TokenA", 200_000, "bb", "TokenB", 100_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let snap = find_circular_arbs(&graph, 4, 0);

    for arb in &snap.windows {
        let cycle = find_cycles(&graph, 4);
        for c in &cycle {
            if let Some(forward) = quote_route(c, arb.optimal_input_nano) {
                if forward.total_output >= arb.output_nano {
                    assert!(
                        arb.optimal_input_nano <= forward.total_input,
                        "Tightened input {} should be <= forward input {}",
                        arb.optimal_input_nano,
                        forward.total_input
                    );
                }
            }
        }

        let hops = &arb.route.hops;
        for i in 1..hops.len() {
            assert!(
                hops[i].input_amount <= hops[i - 1].output_amount,
                "Hop {} input {} should be <= hop {} output {}",
                i + 1,
                hops[i].input_amount,
                i,
                hops[i - 1].output_amount
            );
        }
    }
}

#[test]
fn test_direct_pair_included_despite_low_liquidity() {
    // 1 ERG pool: below the 10 ERG floor, dropped from the normal graph.
    let tiny = make_n2t_pool("tiny_pool", 1_000_000_000, "etosi", "eTosi", 1_000_000, 997);
    let pools = vec![tiny];

    let mut graph = build_pool_graph(&pools, DEFAULT_MIN_LIQUIDITY_NANO);
    assert!(
        find_paths(&graph, ERG_TOKEN_ID, "etosi", 3).is_empty(),
        "precondition: tiny pool filtered out"
    );

    ensure_direct_pair_edges(&mut graph, &pools, ERG_TOKEN_ID, "etosi");

    let paths = find_paths(&graph, ERG_TOKEN_ID, "etosi", 3);
    assert_eq!(paths.len(), 1, "direct pair must be routable");
    assert_eq!(paths[0][0].pool.pool_id, "tiny_pool");

    // Reverse direction works too, and re-adding is idempotent.
    assert_eq!(find_paths(&graph, "etosi", ERG_TOKEN_ID, 3).len(), 1);
    ensure_direct_pair_edges(&mut graph, &pools, ERG_TOKEN_ID, "etosi");
    assert_eq!(find_paths(&graph, ERG_TOKEN_ID, "etosi", 3).len(), 1);
}
