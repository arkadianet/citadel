//! Multi-hop routing, depth, SigUSD acquisition compare, and circular arb scan.

use crate::services::error::IntoServiceError;
use crate::AppState;

pub async fn find_swap_routes(
    state: &AppState,
    source_token: &str,
    target_token: &str,
    input_amount: u64,
    max_hops: Option<usize>,
    max_routes: Option<usize>,
    slippage: Option<f64>,
    min_rate: Option<f64>,
) -> Result<serde_json::Value, String> {
    if input_amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;
    let pools = amm::discover_pools(&client).await.into_service()?;

    let mut graph = amm::build_pool_graph(&pools, amm::DEFAULT_MIN_LIQUIDITY_NANO);
    // The requested pair is always routable even if its pools sit below the
    // liquidity floor -- price impact on the quote conveys thinness.
    amm::ensure_direct_pair_edges(&mut graph, &pools, source_token, target_token);
    let max_hops = max_hops.unwrap_or(3);
    let max_routes = max_routes.unwrap_or(5);
    let slippage_pct = slippage.unwrap_or(0.5);

    let routes = amm::find_best_routes(
        &graph,
        source_token,
        target_token,
        input_amount,
        max_hops,
        max_routes,
    );

    let route_quotes: Vec<amm::RouteQuote> = routes
        .into_iter()
        .map(|r| amm::make_route_quote(r, slippage_pct))
        .collect();

    let depth_tiers = amm::calculate_all_depth_tiers(&graph, source_token);

    // min_rate excludes routes below a price floor (e.g. oracle rate) from the split
    let min_rate_filter = min_rate.map(|r| (r, 2u8)); // SigUSD decimals = 2
    let split = amm::optimize_split_detailed(
        &graph,
        source_token,
        target_token,
        input_amount,
        max_hops,
        3,
        min_rate_filter,
    );

    let max_swap =
        amm::max_swap_hint_if_needed(&graph, source_token, target_token, input_amount, max_hops);

    let response = serde_json::json!({
        "routes": route_quotes,
        "depth_tiers": depth_tiers,
        "split": split,
        "max_swap": max_swap,
    });

    Ok(response)
}

pub async fn find_split_route(
    state: &AppState,
    source_token: &str,
    target_token: &str,
    input_amount: u64,
    max_splits: Option<usize>,
    slippage: Option<f64>,
) -> Result<serde_json::Value, String> {
    if input_amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;
    let pools = amm::discover_pools(&client).await.into_service()?;

    let mut graph = amm::build_pool_graph(&pools, amm::DEFAULT_MIN_LIQUIDITY_NANO);
    amm::ensure_direct_pair_edges(&mut graph, &pools, source_token, target_token);
    let max_hops = 3;
    let max_splits = max_splits.unwrap_or(2);
    let _slippage_pct = slippage.unwrap_or(0.5);

    let paths = amm::find_paths(&graph, source_token, target_token, max_hops);

    let mut quoted: Vec<(usize, u64)> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, p)| amm::quote_route(p, input_amount).map(|r| (i, r.total_output)))
        .collect();
    quoted.sort_by_key(|b| std::cmp::Reverse(b.1));
    quoted.truncate(max_splits);

    let top_paths: Vec<Vec<amm::PoolEdge>> =
        quoted.iter().map(|(i, _)| paths[*i].clone()).collect();

    let split = amm::optimize_split(&top_paths, input_amount, max_splits);

    serde_json::to_value(&split).into_service()
}

pub async fn compare_sigusd_options(
    state: &AppState,
    input_erg_nano: u64,
) -> Result<serde_json::Value, String> {
    if input_erg_nano == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let pools = amm::discover_pools(&client).await.into_service()?;
    let graph = amm::build_pool_graph(&pools, amm::DEFAULT_MIN_LIQUIDITY_NANO);

    let sigmausd_params = fetch_sigmausd_params(state).await.ok();

    let sigusd_token_id = sigmausd::constants::mainnet::SIGUSD_TOKEN_ID;

    let comparison = amm::compare_acquisition(
        &graph,
        sigusd_token_id,
        "SigUSD",
        input_erg_nano,
        sigmausd_params.as_ref(),
    );

    serde_json::to_value(&comparison).into_service()
}

pub async fn get_liquidity_depth(
    state: &AppState,
    source_token: &str,
) -> Result<Vec<amm::DepthTiers>, String> {
    let client = state.require_node_client().await?;
    let pools = amm::discover_pools(&client).await.into_service()?;

    let graph = amm::build_pool_graph(&pools, amm::DEFAULT_MIN_LIQUIDITY_NANO);
    Ok(amm::calculate_all_depth_tiers(&graph, source_token))
}

pub async fn get_sigusd_arb_snapshot(
    state: &AppState,
    oracle_rate_usd_per_erg: f64,
) -> Result<amm::OracleArbSnapshot, String> {
    if oracle_rate_usd_per_erg <= 0.0 {
        return Err("Oracle rate must be positive".to_string());
    }

    let client = state.require_node_client().await?;
    let pools = amm::discover_pools(&client).await.into_service()?;

    // Use lower liquidity threshold (1 ERG) and higher per-pair limit (10)
    // for arb snapshot to include small pools that offer above-oracle rates.
    // The regular router uses 10 ERG minimum and 3 per pair for performance.
    let graph = amm::build_pool_graph_with_limit(&pools, 1_000_000_000, 10);
    let sigusd_token_id = sigmausd::constants::mainnet::SIGUSD_TOKEN_ID;

    Ok(amm::calculate_oracle_arb_snapshot(
        &graph,
        sigusd_token_id,
        oracle_rate_usd_per_erg,
        2, // SigUSD decimals
    ))
}

pub async fn find_swap_routes_by_output(
    state: &AppState,
    source_token: &str,
    target_token: &str,
    desired_output: u64,
    max_hops: Option<usize>,
    max_routes: Option<usize>,
    slippage: Option<f64>,
) -> Result<serde_json::Value, String> {
    if desired_output == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;
    let pools = amm::discover_pools(&client).await.into_service()?;

    let mut graph = amm::build_pool_graph(&pools, amm::DEFAULT_MIN_LIQUIDITY_NANO);
    amm::ensure_direct_pair_edges(&mut graph, &pools, source_token, target_token);
    let max_hops = max_hops.unwrap_or(3);
    let max_routes = max_routes.unwrap_or(5);
    let slippage_pct = slippage.unwrap_or(0.5);

    let routes = amm::find_best_routes_by_output(
        &graph,
        source_token,
        target_token,
        desired_output,
        max_hops,
        max_routes,
    );

    let route_quotes: Vec<amm::RouteQuote> = routes
        .into_iter()
        .map(|r| amm::make_route_quote(r, slippage_pct))
        .collect();

    let depth_tiers = amm::calculate_all_depth_tiers(&graph, source_token);

    let response = serde_json::json!({
        "routes": route_quotes,
        "depth_tiers": depth_tiers,
    });

    Ok(response)
}

async fn fetch_sigmausd_params(state: &AppState) -> Result<amm::SigmaUsdParams, String> {
    let client = state.require_node_client().await?;
    let capabilities = client.require_capabilities().await?;
    let config = state.config().await;
    let nft_ids = sigmausd::constants::NftIds::for_network(config.network)
        .ok_or("SigmaUSD not available on this network")?;

    let sigmausd_state = sigmausd::fetch_sigmausd_state(&client, &capabilities, &nft_ids)
        .await
        .into_service()?;

    Ok(amm::SigmaUsdParams {
        sigusd_price_nano: sigmausd_state.sigusd_price_nano,
        can_mint: sigmausd_state.can_mint_sigusd,
        reserve_ratio_pct: sigmausd_state.reserve_ratio_pct,
    })
}

pub async fn scan_circular_arbs(
    state: &AppState,
    max_hops: Option<usize>,
) -> Result<amm::CircularArbSnapshot, String> {
    let client = state.require_node_client().await?;
    let pools = amm::discover_pools(&client).await.into_service()?;

    let graph = amm::build_pool_graph(&pools, amm::DEFAULT_MIN_LIQUIDITY_NANO);
    let max_hops = max_hops.unwrap_or(4);
    // Min profit: 0.0001 ERG = 100_000 nanoERG
    Ok(amm::find_circular_arbs(&graph, max_hops, 100_000))
}
