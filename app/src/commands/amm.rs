//! Spectrum AMM Tauri IPC — thin wrappers over `citadel_api::services::amm`.

use citadel_api::dto::{AmmPoolsResponse, SwapQuoteResponse};
use citadel_api::services::amm::{
    self as amm_svc, AmmLpBuildResponse, AmmLpDepositPreviewResponse, AmmLpRedeemPreviewResponse,
    ArbChainBuildResponse, ArbChainSubmitResponse, ArbLegSignResponse, CircularArbSnapshot,
    DepthTiers, DirectSwapBuildResponse, DirectSwapPreviewResponse, MempoolSwapDto,
    OracleArbSnapshot, PendingOrderDto, PoolCreatePreviewResponse, SplitAllocationInput,
    SplitChainBuildResponse, SwapBuildResponse, SwapChainBuildResponse, SwapPreviewResponse,
};
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_amm_pools(state: State<'_, AppState>) -> Result<AmmPoolsResponse, String> {
    amm_svc::get_amm_pools(&state).await
}

#[tauri::command]
pub async fn get_amm_quote(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
    token_id: Option<String>,
) -> Result<SwapQuoteResponse, String> {
    amm_svc::get_amm_quote(&state, &pool_id, &input_type, amount, token_id).await
}

#[tauri::command]
pub async fn preview_swap(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
    token_id: Option<String>,
    slippage: Option<f64>,
    nitro: Option<f64>,
) -> Result<SwapPreviewResponse, String> {
    amm_svc::preview_swap(
        &state,
        &pool_id,
        &input_type,
        amount,
        token_id,
        slippage,
        nitro,
    )
    .await
}

#[tauri::command]
pub async fn build_swap_tx(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
    token_id: Option<String>,
    min_output: u64,
    user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    execution_fee_nano: Option<u64>,
    recipient_address: Option<String>,
) -> Result<SwapBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_pk = super::extract_p2pk_pubkey(&parsed_utxos[0].ergo_tree)?;
    amm_svc::build_swap_tx(
        &state,
        &pool_id,
        &input_type,
        amount,
        token_id,
        min_output,
        user_address,
        parsed_utxos,
        user_pk,
        current_height,
        execution_fee_nano,
        recipient_address,
    )
    .await
}

/// No execution fee -- direct swaps have no bot involved.
#[tauri::command]
pub async fn preview_direct_swap(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
    token_id: Option<String>,
    slippage: Option<f64>,
) -> Result<DirectSwapPreviewResponse, String> {
    amm_svc::preview_direct_swap(&state, &pool_id, &input_type, amount, token_id, slippage).await
}

#[tauri::command]
pub async fn build_direct_swap_tx(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
    token_id: Option<String>,
    min_output: u64,
    _user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    recipient_address: Option<String>,
    // Optional custom miner fee in nanoERG. None = network default.
    miner_fee_nano: Option<u64>,
) -> Result<DirectSwapBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    amm_svc::build_direct_swap_tx(
        &state,
        &pool_id,
        &input_type,
        amount,
        token_id,
        min_output,
        parsed_utxos,
        current_height,
        recipient_address,
        miner_fee_nano,
    )
    .await
}

#[tauri::command]
pub async fn get_pending_orders(
    state: State<'_, AppState>,
) -> Result<Vec<PendingOrderDto>, String> {
    amm_svc::get_pending_orders(&state).await
}

#[tauri::command]
pub async fn get_mempool_swaps(state: State<'_, AppState>) -> Result<Vec<MempoolSwapDto>, String> {
    amm_svc::get_mempool_swaps(&state).await
}

#[tauri::command]
pub async fn build_swap_refund_tx(
    state: State<'_, AppState>,
    box_id: String,
    user_ergo_tree: String,
) -> Result<SwapBuildResponse, String> {
    amm_svc::build_swap_refund_tx(&state, box_id, user_ergo_tree).await
}

#[tauri::command]
pub async fn preview_amm_lp_deposit(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
) -> Result<AmmLpDepositPreviewResponse, String> {
    amm_svc::preview_amm_lp_deposit(&state, &pool_id, &input_type, amount).await
}

#[tauri::command]
pub async fn build_amm_lp_deposit_tx(
    state: State<'_, AppState>,
    pool_id: String,
    erg_amount: u64,
    token_amount: u64,
    _user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    amm_svc::build_amm_lp_deposit_tx(
        &state,
        &pool_id,
        erg_amount,
        token_amount,
        parsed_utxos,
        current_height,
    )
    .await
}

/// Proxy order -- Spectrum bots detect and execute the deposit.
#[tauri::command]
pub async fn build_amm_lp_deposit_order(
    state: State<'_, AppState>,
    pool_id: String,
    erg_amount: u64,
    token_amount: u64,
    _user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_pk = super::extract_p2pk_pubkey(&parsed_utxos[0].ergo_tree)?;
    amm_svc::build_amm_lp_deposit_order(
        &state,
        &pool_id,
        erg_amount,
        token_amount,
        parsed_utxos,
        user_pk,
        current_height,
    )
    .await
}

#[tauri::command]
pub async fn preview_amm_lp_redeem(
    state: State<'_, AppState>,
    pool_id: String,
    lp_amount: u64,
) -> Result<AmmLpRedeemPreviewResponse, String> {
    amm_svc::preview_amm_lp_redeem(&state, &pool_id, lp_amount).await
}

#[tauri::command]
pub async fn build_amm_lp_redeem_tx(
    state: State<'_, AppState>,
    pool_id: String,
    lp_amount: u64,
    _user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    amm_svc::build_amm_lp_redeem_tx(&state, &pool_id, lp_amount, parsed_utxos, current_height).await
}

/// Proxy order -- Spectrum bots detect and execute the redemption.
#[tauri::command]
pub async fn build_amm_lp_redeem_order(
    state: State<'_, AppState>,
    pool_id: String,
    lp_amount: u64,
    _user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_pk = super::extract_p2pk_pubkey(&parsed_utxos[0].ergo_tree)?;
    amm_svc::build_amm_lp_redeem_order(
        &state,
        &pool_id,
        lp_amount,
        parsed_utxos,
        user_pk,
        current_height,
    )
    .await
}

#[tauri::command]
pub async fn preview_pool_create(
    pool_type: String,
    x_token_id: Option<String>,
    x_amount: u64,
    y_token_id: String,
    y_amount: u64,
    fee_percent: f64,
) -> Result<PoolCreatePreviewResponse, String> {
    amm_svc::preview_pool_create(
        pool_type,
        x_token_id,
        x_amount,
        y_token_id,
        y_amount,
        fee_percent,
    )
}

/// LP token ID equals the first input box_id (Ergo minting rule).
#[tauri::command]
pub async fn build_pool_bootstrap_tx(
    pool_type: String,
    x_token_id: Option<String>,
    x_amount: u64,
    y_token_id: String,
    y_amount: u64,
    fee_percent: f64,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    amm_svc::build_pool_bootstrap_tx(
        pool_type,
        x_token_id,
        x_amount,
        y_token_id,
        y_amount,
        fee_percent,
        parsed_utxos,
        current_height,
    )
}

/// TX1: takes the bootstrap box (TX0 output) and creates the on-chain pool box.
#[tauri::command]
pub async fn build_pool_create_tx(
    bootstrap_box: serde_json::Value,
    pool_type: String,
    x_token_id: Option<String>,
    x_amount: u64,
    y_token_id: String,
    y_amount: u64,
    fee_num: i32,
    lp_token_id: String,
    user_lp_share: u64,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    amm_svc::build_pool_create_tx(
        bootstrap_box,
        pool_type,
        x_token_id,
        x_amount,
        y_token_id,
        y_amount,
        fee_num,
        lp_token_id,
        user_lp_share,
        current_height,
    )
}

#[tauri::command]
pub async fn find_swap_routes(
    state: State<'_, AppState>,
    source_token: String,
    target_token: String,
    input_amount: u64,
    max_hops: Option<usize>,
    max_routes: Option<usize>,
    slippage: Option<f64>,
    min_rate: Option<f64>,
) -> Result<serde_json::Value, String> {
    amm_svc::find_swap_routes(
        &state,
        &source_token,
        &target_token,
        input_amount,
        max_hops,
        max_routes,
        slippage,
        min_rate,
    )
    .await
}

#[tauri::command]
pub async fn find_split_route(
    state: State<'_, AppState>,
    source_token: String,
    target_token: String,
    input_amount: u64,
    max_splits: Option<usize>,
    slippage: Option<f64>,
) -> Result<serde_json::Value, String> {
    amm_svc::find_split_route(
        &state,
        &source_token,
        &target_token,
        input_amount,
        max_splits,
        slippage,
    )
    .await
}

#[tauri::command]
pub async fn compare_sigusd_options(
    state: State<'_, AppState>,
    input_erg_nano: u64,
) -> Result<serde_json::Value, String> {
    amm_svc::compare_sigusd_options(&state, input_erg_nano).await
}

#[tauri::command]
pub async fn get_liquidity_depth(
    state: State<'_, AppState>,
    source_token: String,
) -> Result<Vec<DepthTiers>, String> {
    amm_svc::get_liquidity_depth(&state, &source_token).await
}

#[tauri::command]
pub async fn get_sigusd_arb_snapshot(
    state: State<'_, AppState>,
    oracle_rate_usd_per_erg: f64,
) -> Result<OracleArbSnapshot, String> {
    amm_svc::get_sigusd_arb_snapshot(&state, oracle_rate_usd_per_erg).await
}

#[tauri::command]
pub async fn find_swap_routes_by_output(
    state: State<'_, AppState>,
    source_token: String,
    target_token: String,
    desired_output: u64,
    max_hops: Option<usize>,
    max_routes: Option<usize>,
    slippage: Option<f64>,
) -> Result<serde_json::Value, String> {
    amm_svc::find_swap_routes_by_output(
        &state,
        &source_token,
        &target_token,
        desired_output,
        max_hops,
        max_routes,
        slippage,
    )
    .await
}

#[tauri::command]
pub async fn scan_circular_arbs(
    state: State<'_, AppState>,
    max_hops: Option<usize>,
) -> Result<CircularArbSnapshot, String> {
    amm_svc::scan_circular_arbs(&state, max_hops).await
}

/// Build a full arb chain over `pool_ids` (hop order). Pools are re-fetched
/// fresh; aborts if the recomputed profit dropped below `min_profit_nano`.
#[tauri::command]
pub async fn build_arb_chain_tx(
    state: State<'_, AppState>,
    pool_ids: Vec<String>,
    input_nano: u64,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    min_profit_nano: Option<i64>,
) -> Result<ArbChainBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    amm_svc::build_arb_chain_tx(
        &state,
        pool_ids,
        input_nano,
        parsed_utxos,
        current_height,
        min_profit_nano,
    )
    .await
}

/// Start a sign-only Nautilus request for one arb leg. The signed tx is
/// captured by the local server and broadcast later via `submit_arb_chain`.
#[tauri::command]
pub async fn start_arb_leg_sign(
    state: State<'_, AppState>,
    unsigned_tx: serde_json::Value,
    message: String,
) -> Result<ArbLegSignResponse, String> {
    amm_svc::start_arb_leg_sign(&state, unsigned_tx, message).await
}

/// Broadcast the signed legs in order. Stops at the first rejection so the
/// caller can report exactly which legs landed.
#[tauri::command]
pub async fn submit_arb_chain(
    state: State<'_, AppState>,
    request_ids: Vec<String>,
) -> Result<ArbChainSubmitResponse, String> {
    amm_svc::submit_arb_chain(&state, request_ids).await
}

/// Build a multi-hop swap chain over `pool_ids` (hop order) starting from
/// `source_token` (None = ERG). Same 0-conf pre-built chaining as arb
/// execution, but for open routes (ends in the target token).
#[tauri::command]
pub async fn build_swap_chain_tx(
    state: State<'_, AppState>,
    pool_ids: Vec<String>,
    source_token: Option<String>,
    input_amount: u64,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<SwapChainBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    amm_svc::build_swap_chain_tx(
        &state,
        pool_ids,
        source_token,
        input_amount,
        parsed_utxos,
        current_height,
    )
    .await
}

/// Pre-build a split as a flat list of 0-conf chained legs across allocations.
/// Allocations must use disjoint pools; UTXOs are threaded between them.
#[tauri::command]
pub async fn build_split_chains_tx(
    state: State<'_, AppState>,
    allocations: Vec<SplitAllocationInput>,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    min_total_output: Option<u64>,
) -> Result<SplitChainBuildResponse, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    amm_svc::build_split_chains_tx(
        &state,
        allocations,
        parsed_utxos,
        current_height,
        min_total_output,
    )
    .await
}
