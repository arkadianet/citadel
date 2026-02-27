use citadel_api::dto::{
    AmmPoolDto, AmmPoolsResponse, MintSignRequest, MintSignResponse, MintTxStatusResponse,
    SwapQuoteResponse,
};
use citadel_api::AppState;
use citadel_core::BoxId;
use serde::Serialize;
use tauri::State;

/// Get all AMM pools
#[tauri::command]
pub async fn get_amm_pools(state: State<'_, AppState>) -> Result<AmmPoolsResponse, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;

    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool_dtos: Vec<AmmPoolDto> = pools.into_iter().map(Into::into).collect();
    let count = pool_dtos.len();

    Ok(AmmPoolsResponse {
        pools: pool_dtos,
        count,
    })
}

/// Get a swap quote for the given pool and input
#[tauri::command]
pub async fn get_amm_quote(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
    token_id: Option<String>,
) -> Result<SwapQuoteResponse, String> {
    // Validate amount
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    // Find the pool (discover all and filter client-side)
    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    // Build the swap input
    let input = match input_type.as_str() {
        "erg" => amm::SwapInput::Erg { amount },
        "token" => amm::SwapInput::Token {
            token_id: token_id.ok_or("token_id required for token input")?,
            amount,
        },
        _ => return Err("Invalid input_type. Use 'erg' or 'token'".to_string()),
    };

    // Calculate quote
    let quote = amm::quote_swap(&pool, &input).ok_or("Cannot calculate quote for this swap")?;

    Ok(quote.into())
}

// =============================================================================
// AMM Swap Commands (preview, build, sign, status)
// =============================================================================

/// Response for swap preview (quote + fee breakdown)
#[derive(Debug, Serialize)]
pub struct SwapPreviewResponse {
    pub output_amount: u64,
    pub output_token_id: String,
    pub output_token_name: Option<String>,
    pub output_decimals: Option<u8>,
    pub min_output: u64,
    pub price_impact: f64,
    pub fee_amount: u64,
    pub effective_rate: f64,
    pub execution_fee_nano: u64,
    pub miner_fee_nano: u64,
    pub total_erg_cost_nano: u64,
}

/// Response for building a swap transaction
#[derive(Debug, Serialize)]
pub struct SwapBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: SwapTxSummaryDto,
}

/// Summary DTO for swap transaction
#[derive(Debug, Serialize)]
pub struct SwapTxSummaryDto {
    pub input_amount: u64,
    pub input_token: String,
    pub min_output: u64,
    pub output_token: String,
    pub execution_fee: u64,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

/// Preview a swap: get quote + fee breakdown without building a transaction
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
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    // Fetch pools and find the matching one
    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    // Build swap input
    let input = match input_type.as_str() {
        "erg" => amm::SwapInput::Erg { amount },
        "token" => amm::SwapInput::Token {
            token_id: token_id.ok_or("token_id required for token input")?,
            amount,
        },
        _ => return Err("Invalid input_type. Use 'erg' or 'token'".to_string()),
    };

    // Calculate quote
    let quote = amm::quote_swap(&pool, &input).ok_or("Cannot calculate quote for this swap")?;

    // Apply slippage to get min_output
    let slippage_pct = slippage.unwrap_or(0.5);
    let min_output = amm::calculator::apply_slippage(quote.output.amount, slippage_pct);

    // Fee constants (match tx_builder.rs)
    let base_execution_fee: u64 = 2_000_000; // 0.002 ERG
    let nitro_mult = nitro.unwrap_or(1.2_f64).max(1.0);
    let execution_fee_nano: u64 = (base_execution_fee as f64 * nitro_mult) as u64;
    let proxy_box_value: u64 = 4_000_000;
    let miner_fee_nano: u64 = 1_100_000;

    // Calculate total ERG cost
    let total_erg_cost_nano = match &input {
        amm::SwapInput::Erg { amount: erg_amt } => {
            erg_amt + execution_fee_nano + proxy_box_value + miner_fee_nano
        }
        amm::SwapInput::Token { .. } => execution_fee_nano + proxy_box_value + miner_fee_nano,
    };

    Ok(SwapPreviewResponse {
        output_amount: quote.output.amount,
        output_token_id: quote.output.token_id,
        output_token_name: quote.output.name,
        output_decimals: quote.output.decimals,
        min_output,
        price_impact: quote.price_impact,
        fee_amount: quote.fee_amount,
        effective_rate: quote.effective_rate,
        execution_fee_nano,
        miner_fee_nano,
        total_erg_cost_nano,
    })
}

/// Build the actual swap EIP-12 unsigned transaction
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
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    // Find pool
    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    // Parse user UTXOs from JSON
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    // Extract user ErgoTree from first UTXO
    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    // Extract user public key from ErgoTree
    // P2PK trees start with "0008cd" then 33 bytes (66 hex chars) of compressed public key
    let user_pk = if user_ergo_tree.starts_with("0008cd") && user_ergo_tree.len() >= 72 {
        user_ergo_tree[6..72].to_string()
    } else {
        return Err(format!(
            "Cannot extract public key from ErgoTree: expected P2PK tree starting with '0008cd', got '{}'",
            &user_ergo_tree[..std::cmp::min(12, user_ergo_tree.len())]
        ));
    };

    // Build SwapInput
    let input = match input_type.as_str() {
        "erg" => amm::SwapInput::Erg { amount },
        "token" => amm::SwapInput::Token {
            token_id: token_id.ok_or("token_id required for token input")?,
            amount,
        },
        _ => return Err("Invalid input_type. Use 'erg' or 'token'".to_string()),
    };

    // Build SwapRequest
    let request = amm::SwapRequest {
        pool_id: pool.pool_id.clone(),
        input,
        min_output,
        redeemer_address: user_address,
    };

    // Convert optional recipient address
    let recipient_tree = match &recipient_address {
        Some(addr) if !addr.is_empty() => {
            Some(ergo_tx::address_to_ergo_tree(addr).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    // Build the transaction
    let result = amm::build_swap_order_eip12(
        &request,
        &pool,
        &parsed_utxos,
        &user_ergo_tree,
        &user_pk,
        current_height,
        execution_fee_nano,
        recipient_tree.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    // Serialize unsigned tx to JSON
    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;

    Ok(SwapBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: SwapTxSummaryDto {
            input_amount: result.summary.input_amount,
            input_token: result.summary.input_token,
            min_output: result.summary.min_output,
            output_token: result.summary.output_token,
            execution_fee: result.summary.execution_fee,
            miner_fee: result.summary.miner_fee,
            total_erg_cost: result.summary.total_erg_cost,
        },
    })
}

/// Start ErgoPay signing flow for a swap transaction
///
/// Delegates to the existing start_mint_sign infrastructure.
#[tauri::command]
pub async fn start_swap_sign(
    state: State<'_, AppState>,
    unsigned_tx: serde_json::Value,
    message: String,
) -> Result<MintSignResponse, String> {
    super::start_mint_sign(
        state,
        MintSignRequest {
            unsigned_tx,
            message,
        },
    )
    .await
}

/// Get status of a swap transaction signing request
///
/// Delegates to the existing get_mint_tx_status infrastructure.
#[tauri::command]
pub async fn get_swap_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    super::get_mint_tx_status(state, request_id).await
}

/// Get a box by its ID from the node (returns JSON-serializable box data)
#[tauri::command]
pub async fn get_box_by_id(
    state: State<'_, AppState>,
    box_id: String,
) -> Result<serde_json::Value, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let bid = BoxId::new(box_id);
    let ergo_box = ergo_node_client::queries::get_box_by_id(client.inner(), &bid)
        .await
        .map_err(|e| e.to_string())?;

    serde_json::to_value(&ergo_box).map_err(|e| format!("Failed to serialize box: {}", e))
}

// =============================================================================
// AMM Direct Swap Commands
// =============================================================================

/// Response for direct swap preview
#[derive(Debug, Serialize)]
pub struct DirectSwapPreviewResponse {
    pub output_amount: u64,
    pub output_token_id: String,
    pub output_token_name: Option<String>,
    pub output_decimals: Option<u8>,
    pub min_output: u64,
    pub price_impact: f64,
    pub fee_amount: u64,
    pub effective_rate: f64,
    pub miner_fee_nano: u64,
    pub total_erg_cost_nano: u64,
}

/// Response for building a direct swap transaction
#[derive(Debug, Serialize)]
pub struct DirectSwapBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: DirectSwapSummaryDto,
}

/// Summary DTO for direct swap transaction
#[derive(Debug, Serialize)]
pub struct DirectSwapSummaryDto {
    pub input_amount: u64,
    pub input_token: String,
    pub output_amount: u64,
    pub min_output: u64,
    pub output_token: String,
    pub miner_fee: u64,
    pub total_erg_cost: u64,
}

/// Preview a direct swap: get quote + fee breakdown without building a transaction
///
/// Direct swaps have no execution fee (no bot involved).
#[tauri::command]
pub async fn preview_direct_swap(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
    token_id: Option<String>,
    slippage: Option<f64>,
) -> Result<DirectSwapPreviewResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    let input = match input_type.as_str() {
        "erg" => amm::SwapInput::Erg { amount },
        "token" => amm::SwapInput::Token {
            token_id: token_id.ok_or("token_id required for token input")?,
            amount,
        },
        _ => return Err("Invalid input_type. Use 'erg' or 'token'".to_string()),
    };

    let quote = amm::quote_swap(&pool, &input).ok_or("Cannot calculate quote for this swap")?;

    let slippage_pct = slippage.unwrap_or(0.5);
    let min_output = amm::calculator::apply_slippage(quote.output.amount, slippage_pct);

    let miner_fee_nano: u64 = 1_100_000;
    let min_box_value: u64 = 1_000_000;

    let total_erg_cost_nano = match &input {
        amm::SwapInput::Erg { amount: erg_amt } => erg_amt + min_box_value + miner_fee_nano,
        amm::SwapInput::Token { .. } => miner_fee_nano,
    };

    Ok(DirectSwapPreviewResponse {
        output_amount: quote.output.amount,
        output_token_id: quote.output.token_id,
        output_token_name: quote.output.name,
        output_decimals: quote.output.decimals,
        min_output,
        price_impact: quote.price_impact,
        fee_amount: quote.fee_amount,
        effective_rate: quote.effective_rate,
        miner_fee_nano,
        total_erg_cost_nano,
    })
}

/// Build a direct swap EIP-12 unsigned transaction
///
/// Fetches the pool box and spends it directly in the user's transaction.
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
) -> Result<DirectSwapBuildResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    // Find pool
    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    // Fetch pool box in EIP-12 format
    let pool_box = client
        .get_eip12_box_by_id(&pool.box_id)
        .await
        .map_err(|e| format!("Failed to fetch pool box: {}", e))?;

    // Parse user UTXOs
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    // Build SwapInput
    let input = match input_type.as_str() {
        "erg" => amm::SwapInput::Erg { amount },
        "token" => amm::SwapInput::Token {
            token_id: token_id.ok_or("token_id required for token input")?,
            amount,
        },
        _ => return Err("Invalid input_type. Use 'erg' or 'token'".to_string()),
    };

    // Convert optional recipient address
    let recipient_tree = match &recipient_address {
        Some(addr) if !addr.is_empty() => {
            Some(ergo_tx::address_to_ergo_tree(addr).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    // Build the direct swap transaction
    let result = amm::build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        min_output,
        &parsed_utxos,
        &user_ergo_tree,
        current_height,
        recipient_tree.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;

    Ok(DirectSwapBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: DirectSwapSummaryDto {
            input_amount: result.summary.input_amount,
            input_token: result.summary.input_token,
            output_amount: result.summary.output_amount,
            min_output: result.summary.min_output,
            output_token: result.summary.output_token,
            miner_fee: result.summary.miner_fee,
            total_erg_cost: result.summary.total_erg_cost,
        },
    })
}

// =============================================================================
// AMM Order Discovery Commands
// =============================================================================

/// DTO for PendingSwapOrder (camelCase for frontend)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOrderDto {
    pub box_id: String,
    pub tx_id: String,
    pub pool_id: String,
    pub input: serde_json::Value,
    pub min_output: u64,
    pub input_decimals: u8,
    pub output_decimals: u8,
    pub redeemer_address: String,
    pub created_height: u32,
    pub value_nano_erg: u64,
    pub order_type: String,
    pub method: String,
}

impl PendingOrderDto {
    fn from_order(o: &amm::PendingSwapOrder) -> Self {
        let order_type = match o.order_type {
            amm::SwapOrderType::N2tSwapSell => "n2t_swap_sell",
            amm::SwapOrderType::N2tSwapBuy => "n2t_swap_buy",
        };
        // For N2T: sell = ERG(9) -> token(?), buy = token(?) -> ERG(9)
        let (input_decimals, output_decimals) = match o.order_type {
            amm::SwapOrderType::N2tSwapSell => (9u8, 0u8),
            amm::SwapOrderType::N2tSwapBuy => (0u8, 9u8),
        };
        Self {
            box_id: o.box_id.clone(),
            tx_id: o.tx_id.clone(),
            pool_id: o.pool_id.clone(),
            input: serde_json::to_value(&o.input).unwrap_or_default(),
            min_output: o.min_output,
            input_decimals,
            output_decimals,
            redeemer_address: o.redeemer_address.clone(),
            created_height: o.created_height,
            value_nano_erg: o.value_nano_erg,
            order_type: order_type.to_string(),
            method: "proxy".to_string(),
        }
    }
}

/// DTO for a direct swap found in the mempool
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MempoolSwapDto {
    pub tx_id: String,
    pub pool_id: String,
    pub receiving_erg: u64,
    /// (token_id, amount, decimals)
    pub receiving_tokens: Vec<(String, u64, u8)>,
}

impl MempoolSwapDto {
    fn from_swap(s: &amm::MempoolSwap) -> Self {
        Self {
            tx_id: s.tx_id.clone(),
            pool_id: s.pool_id.clone(),
            receiving_erg: s.receiving_erg,
            receiving_tokens: s
                .receiving_tokens
                .iter()
                .map(|(id, amt)| (id.clone(), *amt, 0u8))
                .collect(),
        }
    }
}

/// Discover pending (unspent) swap orders for the connected wallet
#[tauri::command]
pub async fn get_pending_orders(
    state: State<'_, AppState>,
) -> Result<Vec<PendingOrderDto>, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let wallet = state.wallet().await.ok_or("Wallet not connected")?;

    let utxos = client
        .get_address_utxos(&wallet.address)
        .await
        .map_err(|e| e.to_string())?;

    let user_ergo_tree = utxos
        .first()
        .map(|u| u.ergo_tree.clone())
        .ok_or("No UTXOs found for wallet")?;

    let orders = amm::find_pending_orders(&client, &wallet.address, &user_ergo_tree, 50)
        .await
        .map_err(|e| e.to_string())?;

    let mut dtos: Vec<PendingOrderDto> = orders.iter().map(PendingOrderDto::from_order).collect();

    // Resolve token decimals from node
    let mut token_cache: std::collections::HashMap<String, u8> = std::collections::HashMap::new();
    for dto in &mut dtos {
        // For N2T sell: input=ERG(9), output=token(?). For buy: input=token(?), output=ERG(9).
        // Extract the token ID from the input JSON for buy orders
        let token_id = match dto.order_type.as_str() {
            "n2t_swap_buy" => dto
                .input
                .get("tokenId")
                .and_then(|v| v.as_str())
                .map(String::from),
            _ => None,
        };
        if let Some(tid) = token_id {
            let decimals = if let Some(&d) = token_cache.get(&tid) {
                d
            } else {
                let d = client
                    .get_token_info(&tid)
                    .await
                    .ok()
                    .and_then(|ti| ti.decimals)
                    .unwrap_or(0) as u8;
                token_cache.insert(tid, d);
                d
            };
            dto.input_decimals = decimals;
        }
        // For sell orders, output_decimals needs pool token info
        // We don't have the pool loaded here, so leave as 0 — the frontend
        // already formats min_output via pool data when available
    }

    Ok(dtos)
}

/// Find direct swap transactions in the mempool for the connected wallet
#[tauri::command]
pub async fn get_mempool_swaps(state: State<'_, AppState>) -> Result<Vec<MempoolSwapDto>, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let wallet = state.wallet().await.ok_or("Wallet not connected")?;

    let utxos = client
        .get_address_utxos(&wallet.address)
        .await
        .map_err(|e| e.to_string())?;

    let user_ergo_tree = utxos
        .first()
        .map(|u| u.ergo_tree.clone())
        .ok_or("No UTXOs found for wallet")?;

    let swaps = amm::find_mempool_swaps(&client, &wallet.address, &user_ergo_tree)
        .await
        .map_err(|e| e.to_string())?;

    let mut dtos: Vec<MempoolSwapDto> = swaps.iter().map(MempoolSwapDto::from_swap).collect();

    // Resolve token decimals
    let mut token_cache: std::collections::HashMap<String, u8> = std::collections::HashMap::new();
    for dto in &mut dtos {
        for (tid, _, decimals) in &mut dto.receiving_tokens {
            *decimals = if let Some(&d) = token_cache.get(tid.as_str()) {
                d
            } else {
                let d = client
                    .get_token_info(tid)
                    .await
                    .ok()
                    .and_then(|ti| ti.decimals)
                    .unwrap_or(0) as u8;
                token_cache.insert(tid.clone(), d);
                d
            };
        }
    }

    Ok(dtos)
}

/// Build a refund transaction for a swap proxy box.
/// Takes just the box_id -- fetches the proxy box from the node internally.
#[tauri::command]
pub async fn build_swap_refund_tx(
    state: State<'_, AppState>,
    box_id: String,
    user_ergo_tree: String,
) -> Result<SwapBuildResponse, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;

    let proxy_input = client
        .get_eip12_box_by_id(&box_id)
        .await
        .map_err(|e| format!("Cannot fetch proxy box: {}. It may have been spent.", e))?;

    let current_height = client.current_height().await.map_err(|e| e.to_string())? as i32;

    let result = amm::build_refund_tx_eip12(&proxy_input, &user_ergo_tree, current_height, &[])
        .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(SwapBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: SwapTxSummaryDto {
            input_amount: result.summary.refunded_erg,
            input_token: "Refund".to_string(),
            min_output: result.summary.refunded_erg,
            output_token: "ERG".to_string(),
            execution_fee: 0,
            miner_fee: result.summary.miner_fee,
            total_erg_cost: result.summary.miner_fee,
        },
    })
}

/// Start ErgoPay signing flow for a refund transaction
#[tauri::command]
pub async fn start_refund_sign(
    state: State<'_, AppState>,
    unsigned_tx: serde_json::Value,
    message: String,
) -> Result<MintSignResponse, String> {
    super::start_mint_sign(
        state,
        MintSignRequest {
            unsigned_tx,
            message,
        },
    )
    .await
}

/// Get status of a refund transaction signing request
#[tauri::command]
pub async fn get_refund_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    super::get_mint_tx_status(state, request_id).await
}

// =============================================================================
// AMM LP Deposit/Redeem Commands
// =============================================================================

/// Preview response for LP deposit
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmmLpDepositPreviewResponse {
    pub lp_reward: u64,
    pub erg_amount: u64,
    pub token_amount: u64,
    pub token_name: Option<String>,
    pub token_decimals: Option<u8>,
    pub pool_share_percent: f64,
    pub miner_fee_nano: u64,
    pub total_erg_cost_nano: u64,
}

/// Preview response for LP redeem
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmmLpRedeemPreviewResponse {
    pub erg_output: u64,
    pub token_output: u64,
    pub token_name: Option<String>,
    pub token_decimals: Option<u8>,
    pub lp_amount: u64,
    pub miner_fee_nano: u64,
    pub total_erg_cost_nano: u64,
}

/// Build response for LP transactions (both direct and proxy)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmmLpBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: serde_json::Value,
}

/// Preview an LP deposit: calculate proportional amounts and LP reward.
///
/// Given a pool and one side of the deposit (ERG or token amount), calculates
/// the proportional other side and the LP tokens the user would receive.
#[tauri::command]
pub async fn preview_amm_lp_deposit(
    state: State<'_, AppState>,
    pool_id: String,
    input_type: String,
    amount: u64,
) -> Result<AmmLpDepositPreviewResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    let erg_reserves = pool
        .erg_reserves
        .ok_or("Only N2T pools supported for LP operations")?;

    let (erg_amount, token_amount) = match input_type.as_str() {
        "erg" => {
            let token_needed = amm::calculator::calculate_deposit_token_needed(
                erg_reserves,
                pool.token_y.amount,
                amount,
            );
            (amount, token_needed)
        }
        "token" => {
            let erg_needed = amm::calculator::calculate_deposit_erg_needed(
                erg_reserves,
                pool.token_y.amount,
                amount,
            );
            (erg_needed, amount)
        }
        _ => return Err("Invalid input_type. Use 'erg' or 'token'".to_string()),
    };

    let lp_reward = amm::calculator::calculate_lp_reward(
        erg_reserves,
        pool.token_y.amount,
        pool.lp_circulating,
        erg_amount,
        token_amount,
    );

    let pool_share_percent = if pool.lp_circulating + lp_reward > 0 {
        (lp_reward as f64) / ((pool.lp_circulating + lp_reward) as f64) * 100.0
    } else {
        0.0
    };

    let miner_fee_nano: u64 = 1_100_000;
    let total_erg_cost_nano = erg_amount + miner_fee_nano;

    Ok(AmmLpDepositPreviewResponse {
        lp_reward,
        erg_amount,
        token_amount,
        token_name: pool.token_y.name.clone(),
        token_decimals: pool.token_y.decimals,
        pool_share_percent,
        miner_fee_nano,
        total_erg_cost_nano,
    })
}

/// Build a direct LP deposit transaction (spends pool box directly).
///
/// Fetches the pool box and builds an EIP-12 unsigned transaction that deposits
/// ERG and tokens into the pool, receiving LP tokens in return.
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
    let client = state.node_client().await.ok_or("Node not connected")?;

    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    let pool_box = client
        .get_eip12_box_by_id(&pool.box_id)
        .await
        .map_err(|e| format!("Failed to fetch pool box: {}", e))?;

    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    let result = amm::build_lp_deposit_eip12(
        &pool_box,
        &pool,
        erg_amount,
        token_amount,
        &parsed_utxos,
        &user_ergo_tree,
        current_height,
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

/// Build an LP deposit proxy order (creates proxy box for Spectrum bot execution).
///
/// Creates a proxy box containing the deposit contract. Off-chain Spectrum bots
/// detect and execute the deposit against the pool.
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
    let client = state.node_client().await.ok_or("Node not connected")?;

    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    let user_pk = if user_ergo_tree.starts_with("0008cd") && user_ergo_tree.len() >= 72 {
        user_ergo_tree[6..72].to_string()
    } else {
        return Err(format!(
            "Cannot extract public key from ErgoTree: expected P2PK tree starting with '0008cd', got '{}'",
            &user_ergo_tree[..std::cmp::min(12, user_ergo_tree.len())]
        ));
    };

    let result = amm::build_lp_deposit_order_eip12(
        &pool,
        erg_amount,
        token_amount,
        &parsed_utxos,
        &user_ergo_tree,
        &user_pk,
        current_height,
        None,
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

/// Preview an LP redeem: calculate the ERG and token output for a given LP amount.
#[tauri::command]
pub async fn preview_amm_lp_redeem(
    state: State<'_, AppState>,
    pool_id: String,
    lp_amount: u64,
) -> Result<AmmLpRedeemPreviewResponse, String> {
    if lp_amount == 0 {
        return Err("LP amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    let erg_reserves = pool
        .erg_reserves
        .ok_or("Only N2T pools supported for LP operations")?;

    let (erg_output, token_output) = amm::calculator::calculate_redeem_shares(
        erg_reserves,
        pool.token_y.amount,
        pool.lp_circulating,
        lp_amount,
    );

    let miner_fee_nano: u64 = 1_100_000;
    // User only needs ERG for miner fee; redeemed ERG comes from the pool
    let total_erg_cost_nano = miner_fee_nano;

    Ok(AmmLpRedeemPreviewResponse {
        erg_output,
        token_output,
        token_name: pool.token_y.name.clone(),
        token_decimals: pool.token_y.decimals,
        lp_amount,
        miner_fee_nano,
        total_erg_cost_nano,
    })
}

/// Build a direct LP redeem transaction (spends pool box directly).
///
/// Fetches the pool box and builds an EIP-12 unsigned transaction that returns
/// LP tokens to the pool, receiving proportional ERG and tokens in return.
#[tauri::command]
pub async fn build_amm_lp_redeem_tx(
    state: State<'_, AppState>,
    pool_id: String,
    lp_amount: u64,
    _user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;

    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    let pool_box = client
        .get_eip12_box_by_id(&pool.box_id)
        .await
        .map_err(|e| format!("Failed to fetch pool box: {}", e))?;

    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    let result = amm::build_lp_redeem_eip12(
        &pool_box,
        &pool,
        lp_amount,
        &parsed_utxos,
        &user_ergo_tree,
        current_height,
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

/// Build an LP redeem proxy order (creates proxy box for Spectrum bot execution).
///
/// Creates a proxy box containing the redeem contract and LP tokens. Off-chain
/// Spectrum bots detect and execute the redemption, sending ERG and tokens to the user.
#[tauri::command]
pub async fn build_amm_lp_redeem_order(
    state: State<'_, AppState>,
    pool_id: String,
    lp_amount: u64,
    _user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;

    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let pool = pools
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    let user_pk = if user_ergo_tree.starts_with("0008cd") && user_ergo_tree.len() >= 72 {
        user_ergo_tree[6..72].to_string()
    } else {
        return Err(format!(
            "Cannot extract public key from ErgoTree: expected P2PK tree starting with '0008cd', got '{}'",
            &user_ergo_tree[..std::cmp::min(12, user_ergo_tree.len())]
        ));
    };

    let result = amm::build_lp_redeem_order_eip12(
        &pool,
        lp_amount,
        &parsed_utxos,
        &user_ergo_tree,
        &user_pk,
        current_height,
        None,
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

// =============================================================================
// AMM Pool Creation Commands
// =============================================================================

/// Preview response for pool creation
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolCreatePreviewResponse {
    pub pool_type: String,
    pub lp_share: u64,
    pub fee_percent: f64,
    pub fee_num: i32,
    pub miner_fee_nano: u64,
    pub total_erg_cost_nano: u64,
}

/// Preview a new pool creation: calculate LP share and cost without building a transaction.
///
/// Pure calculation — no node state or wallet needed.
#[tauri::command]
pub async fn preview_pool_create(
    pool_type: String,
    x_token_id: Option<String>,
    x_amount: u64,
    y_token_id: String,
    y_amount: u64,
    fee_percent: f64,
) -> Result<PoolCreatePreviewResponse, String> {
    // Suppress unused variable warnings for args needed by Tauri's macro
    let _ = (&x_token_id, &y_token_id);

    let fee_num = ((1.0 - fee_percent / 100.0) * amm::constants::fees::DEFAULT_FEE_DENOM as f64)
        .round() as i32;
    if fee_num <= 0 || fee_num >= amm::constants::fees::DEFAULT_FEE_DENOM {
        return Err("Fee must be between 0% and 100% (exclusive)".to_string());
    }

    let lp_share = amm::calculator::calculate_initial_lp_share(x_amount, y_amount);
    if lp_share == 0 {
        return Err("Initial LP share would be 0. Increase deposit amounts.".to_string());
    }

    let tx_fee = 1_100_000u64;
    let min_box_value = 1_000_000u64;

    let pool_type_enum = match pool_type.as_str() {
        "N2T" => amm::state::PoolType::N2T,
        "T2T" => amm::state::PoolType::T2T,
        _ => return Err(format!("Invalid pool type: {}", pool_type)),
    };

    let total_erg_cost = match pool_type_enum {
        amm::state::PoolType::N2T => x_amount + tx_fee * 2 + min_box_value,
        amm::state::PoolType::T2T => min_box_value * 2 + tx_fee * 2,
    };

    Ok(PoolCreatePreviewResponse {
        pool_type,
        lp_share,
        fee_percent,
        fee_num,
        miner_fee_nano: tx_fee * 2,
        total_erg_cost_nano: total_erg_cost,
    })
}

/// Build the bootstrap transaction (TX0) for a new pool.
///
/// Mints LP tokens and creates a bootstrap box at the user's address.
/// The LP token ID equals the first input box_id (Ergo minting rule).
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
    let pool_type_enum = match pool_type.as_str() {
        "N2T" => amm::state::PoolType::N2T,
        "T2T" => amm::state::PoolType::T2T,
        _ => return Err(format!("Invalid pool type: {}", pool_type)),
    };

    let fee_num = ((1.0 - fee_percent / 100.0) * amm::constants::fees::DEFAULT_FEE_DENOM as f64)
        .round() as i32;

    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;
    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    let params = amm::pool_setup::PoolSetupParams {
        pool_type: pool_type_enum,
        x_token_id,
        x_amount,
        y_token_id,
        y_amount,
        fee_num,
    };

    let result = amm::pool_setup::build_pool_bootstrap_eip12(
        &params,
        &parsed_utxos,
        &user_ergo_tree,
        current_height,
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

/// Build the pool create transaction (TX1) that creates the on-chain pool box.
///
/// Takes the bootstrap box (TX0 output) as JSON. Mints a pool NFT and creates
/// the pool box under the appropriate pool contract ErgoTree.
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
    let pool_type_enum = match pool_type.as_str() {
        "N2T" => amm::state::PoolType::N2T,
        "T2T" => amm::state::PoolType::T2T,
        _ => return Err(format!("Invalid pool type: {}", pool_type)),
    };

    let bootstrap: ergo_tx::Eip12InputBox = serde_json::from_value(bootstrap_box)
        .map_err(|e| format!("Failed to parse bootstrap box: {}", e))?;

    let user_ergo_tree = bootstrap.ergo_tree.clone();

    let params = amm::pool_setup::PoolSetupParams {
        pool_type: pool_type_enum,
        x_token_id,
        x_amount,
        y_token_id,
        y_amount,
        fee_num,
    };

    let result = amm::pool_setup::build_pool_create_eip12(
        &bootstrap,
        &params,
        &lp_token_id,
        user_lp_share,
        &user_ergo_tree,
        current_height,
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}
