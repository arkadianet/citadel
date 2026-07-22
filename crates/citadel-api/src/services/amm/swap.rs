//! Proxy swaps, direct (bot-less) swaps, refunds, and pending/mempool order views.

use crate::services::error::IntoServiceError;
use crate::AppState;
use serde::Serialize;

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
    pub citadel_fee_nano: u64,
    pub total_erg_cost_nano: u64,
}

#[derive(Debug, Serialize)]
pub struct SwapBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: SwapTxSummaryDto,
}

#[derive(Debug, Serialize)]
pub struct SwapTxSummaryDto {
    pub input_amount: u64,
    pub input_token: String,
    pub min_output: u64,
    pub output_token: String,
    pub execution_fee: u64,
    pub miner_fee: u64,
    pub citadel_fee_nano: u64,
    pub total_erg_cost: u64,
}

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
    pub citadel_fee_nano: u64,
    pub total_erg_cost_nano: u64,
}

#[derive(Debug, Serialize)]
pub struct DirectSwapBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: DirectSwapSummaryDto,
}

#[derive(Debug, Serialize)]
pub struct DirectSwapSummaryDto {
    pub input_amount: u64,
    pub input_token: String,
    pub output_amount: u64,
    pub min_output: u64,
    pub output_token: String,
    pub miner_fee: u64,
    pub citadel_fee_nano: u64,
    pub total_erg_cost: u64,
}

pub async fn preview_swap(
    state: &AppState,
    pool_id: &str,
    input_type: &str,
    amount: u64,
    token_id: Option<String>,
    slippage: Option<f64>,
    nitro: Option<f64>,
) -> Result<SwapPreviewResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let input = super::parse_swap_input(input_type, amount, token_id)?;

    let quote = amm::quote_swap(&pool, &input).ok_or("Cannot calculate quote for this swap")?;

    let slippage_pct = slippage.unwrap_or(0.5);
    let min_output = amm::calculator::apply_slippage(quote.output.amount, slippage_pct);

    // Fee constants must match tx_builder.rs
    let base_execution_fee: u64 = 2_000_000; // 0.002 ERG
    let nitro_mult = nitro.unwrap_or(1.2_f64).max(1.0);
    let execution_fee_nano: u64 = (base_execution_fee as f64 * nitro_mult) as u64;
    let proxy_box_value: u64 = 4_000_000;
    let miner_fee_nano: u64 = 1_100_000;
    let citadel_fee_nano = ergo_tx::resolved_dev_fee_config().budget() as u64;

    let total_erg_cost_nano = match &input {
        amm::SwapInput::Erg { amount: erg_amt } => {
            erg_amt + execution_fee_nano + proxy_box_value + miner_fee_nano + citadel_fee_nano
        }
        amm::SwapInput::Token { .. } => {
            execution_fee_nano + proxy_box_value + miner_fee_nano + citadel_fee_nano
        }
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
        citadel_fee_nano,
        total_erg_cost_nano,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn build_swap_tx(
    state: &AppState,
    pool_id: &str,
    input_type: &str,
    amount: u64,
    token_id: Option<String>,
    min_output: u64,
    user_address: String,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    user_pk: String,
    current_height: i32,
    execution_fee_nano: Option<u64>,
    recipient_address: Option<String>,
) -> Result<SwapBuildResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let input = super::parse_swap_input(input_type, amount, token_id)?;

    let request = amm::SwapRequest {
        pool_id: pool.pool_id.clone(),
        input,
        min_output,
        redeemer_address: user_address,
    };

    let recipient_tree = match &recipient_address {
        Some(addr) if !addr.is_empty() => Some(ergo_tx::address_to_ergo_tree(addr).into_service()?),
        _ => None,
    };

    let result = amm::build_swap_order_eip12(
        &request,
        &pool,
        &user_utxos,
        &user_ergo_tree,
        &user_pk,
        current_height,
        execution_fee_nano,
        recipient_tree.as_deref(),
    )
    .into_service()?;

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
            citadel_fee_nano: result.summary.citadel_fee_nano,
            total_erg_cost: result.summary.total_erg_cost,
        },
    })
}

/// No execution fee -- direct swaps have no bot involved.
pub async fn preview_direct_swap(
    state: &AppState,
    pool_id: &str,
    input_type: &str,
    amount: u64,
    token_id: Option<String>,
    slippage: Option<f64>,
) -> Result<DirectSwapPreviewResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let input = super::parse_swap_input(input_type, amount, token_id)?;

    let quote = amm::quote_swap(&pool, &input).ok_or("Cannot calculate quote for this swap")?;

    let slippage_pct = slippage.unwrap_or(0.5);
    let min_output = amm::calculator::apply_slippage(quote.output.amount, slippage_pct);

    let miner_fee_nano: u64 = 1_100_000;
    let citadel_fee_nano = ergo_tx::resolved_dev_fee_config().budget() as u64;
    let min_box_value: u64 = 1_000_000;

    let total_erg_cost_nano = match &input {
        amm::SwapInput::Erg { amount: erg_amt } => {
            erg_amt + min_box_value + miner_fee_nano + citadel_fee_nano
        }
        amm::SwapInput::Token { .. } => miner_fee_nano + citadel_fee_nano,
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
        citadel_fee_nano,
        total_erg_cost_nano,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn build_direct_swap_tx(
    state: &AppState,
    pool_id: &str,
    input_type: &str,
    amount: u64,
    token_id: Option<String>,
    min_output: u64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
    recipient_address: Option<String>,
    // Optional custom miner fee in nanoERG. None = network default.
    miner_fee_nano: Option<u64>,
) -> Result<DirectSwapBuildResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let pool_box = client
        .get_eip12_box_by_id(&pool.box_id)
        .await
        .map_err(|e| format!("Failed to fetch pool box: {}", e))?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let input = super::parse_swap_input(input_type, amount, token_id)?;

    let recipient_tree = match &recipient_address {
        Some(addr) if !addr.is_empty() => Some(ergo_tx::address_to_ergo_tree(addr).into_service()?),
        _ => None,
    };

    let result = amm::build_direct_swap_eip12(
        &pool_box,
        &pool,
        &input,
        min_output,
        &user_utxos,
        &user_ergo_tree,
        current_height,
        recipient_tree.as_deref(),
        miner_fee_nano,
    )
    .into_service()?;

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
            citadel_fee_nano: result.summary.citadel_fee_nano,
            total_erg_cost: result.summary.total_erg_cost,
        },
    })
}

pub async fn build_swap_refund_tx(
    state: &AppState,
    box_id: String,
    user_ergo_tree: String,
) -> Result<SwapBuildResponse, String> {
    let client = state.require_node_client().await?;

    let proxy_input = client
        .get_eip12_box_by_id(&box_id)
        .await
        .map_err(|e| format!("Cannot fetch proxy box: {}. It may have been spent.", e))?;

    let current_height = client.current_height().await.into_service()? as i32;

    let result = amm::build_refund_tx_eip12(&proxy_input, &user_ergo_tree, current_height, &[])
        .into_service()?;

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
            citadel_fee_nano: 0,
            total_erg_cost: result.summary.miner_fee,
        },
    })
}

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

pub async fn get_pending_orders(state: &AppState) -> Result<Vec<PendingOrderDto>, String> {
    let client = state.require_node_client().await?;
    let wallet = state.wallet().await.ok_or("Wallet not connected")?;

    // Scan recent history per wallet address (orders may live under any index).
    let mut orders = Vec::new();
    let mut seen_box = std::collections::HashSet::new();
    for addr in &wallet.addresses {
        let Some(tree) = ergo_node_client::address_to_ergo_tree(addr) else {
            continue;
        };
        let batch = amm::find_pending_orders(&client, addr, &tree, 50)
            .await
            .into_service()?;
        for o in batch {
            if seen_box.insert(o.box_id.clone()) {
                orders.push(o);
            }
        }
    }

    let mut dtos: Vec<PendingOrderDto> = orders.iter().map(PendingOrderDto::from_order).collect();

    let mut token_cache: std::collections::HashMap<String, u8> = std::collections::HashMap::new();
    for dto in &mut dtos {
        // For N2T sell: input=ERG(9), output=token(?). For buy: input=token(?), output=ERG(9).
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
        // For sell orders, output_decimals stays 0 -- the frontend
        // formats min_output via pool data when available
    }

    Ok(dtos)
}

pub async fn get_mempool_swaps(state: &AppState) -> Result<Vec<MempoolSwapDto>, String> {
    let client = state.require_node_client().await?;
    let wallet = state.wallet().await.ok_or("Wallet not connected")?;

    let mut swaps = Vec::new();
    let mut seen_tx = std::collections::HashSet::new();
    for addr in &wallet.addresses {
        let Some(tree) = ergo_node_client::address_to_ergo_tree(addr) else {
            continue;
        };
        let batch = amm::find_mempool_swaps(&client, addr, &tree)
            .await
            .into_service()?;
        for s in batch {
            if seen_tx.insert(s.tx_id.clone()) {
                swaps.push(s);
            }
        }
    }

    let mut dtos: Vec<MempoolSwapDto> = swaps.iter().map(MempoolSwapDto::from_swap).collect();

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
