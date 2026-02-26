use citadel_api::dto::{
    DexyBuildRequest, DexyBuildResponse, DexyLpBuildResponse, DexyLpPreviewResponse,
    DexyPreviewRequest, DexyPreviewResponse, DexyStateResponse, DexySwapBuildResponse,
    DexySwapPreviewResponse, TxSummaryDto,
};
use citadel_api::AppState;
use citadel_core::constants::{MIN_BOX_VALUE_NANO, TX_FEE_NANO};
use dexy::{
    calculator::{calculate_lp_deposit, calculate_lp_redeem, can_redeem_lp, cost_to_mint_dexy},
    constants::{DexyIds, DexyVariant},
    fetch::{
        fetch_dexy_state, fetch_lp_tx_context, fetch_tx_context as fetch_dexy_tx_context,
        parse_lp_box, LpAction,
    },
    rates::DexyRates,
    tx_builder::{
        build_mint_dexy_tx, validate_mint_dexy, LpDepositRequest, LpRedeemRequest, MintDexyRequest,
    },
};
use tauri::State;

/// Get Dexy protocol state for a variant
#[tauri::command]
pub async fn get_dexy_state(
    state: State<'_, AppState>,
    variant: String,
) -> Result<DexyStateResponse, String> {
    let dexy_variant = variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}. Use 'gold' or 'usd'", variant))?;

    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available on {:?}", variant, config.network))?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    Ok(DexyStateResponse {
        variant: dexy_variant.as_str().to_string(),
        bank_erg_nano: dexy_state.bank_erg_nano,
        dexy_in_bank: dexy_state.dexy_in_bank,
        bank_box_id: dexy_state.bank_box_id,
        dexy_token_id: dexy_state.dexy_token_id,
        free_mint_available: dexy_state.free_mint_available,
        free_mint_reset_height: dexy_state.free_mint_reset_height,
        current_height: dexy_state.current_height,
        oracle_rate_nano: dexy_state.oracle_rate_nano,
        oracle_box_id: dexy_state.oracle_box_id,
        lp_erg_reserves: dexy_state.lp_erg_reserves,
        lp_dexy_reserves: dexy_state.lp_dexy_reserves,
        lp_box_id: dexy_state.lp_box_id,
        lp_rate_nano: dexy_state.lp_rate_nano,
        can_mint: dexy_state.can_mint,
        rate_difference_pct: dexy_state.rate_difference_pct,
        dexy_circulating: dexy_state.dexy_circulating,
    })
}

/// Get Dexy rates for all minting paths
#[tauri::command]
pub async fn get_dexy_rates(
    state: State<'_, AppState>,
    variant: String,
) -> Result<DexyRates, String> {
    let dexy_variant = variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}. Use 'gold' or 'usd'", variant))?;

    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available on {:?}", variant, config.network))?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    Ok(DexyRates::from_state(&dexy_state))
}

/// Preview Dexy mint operation
#[tauri::command]
pub async fn preview_mint_dexy(
    state: State<'_, AppState>,
    request: DexyPreviewRequest,
) -> Result<DexyPreviewResponse, String> {
    let variant = request
        .variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}", request.variant))?;

    if request.amount <= 0 {
        return Ok(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: request.amount.to_string(),
            token_name: variant.token_name().to_string(),
            can_execute: false,
            error: Some("Amount must be positive".to_string()),
        });
    }

    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(variant, config.network)
        .ok_or_else(|| "Dexy not available on this network".to_string())?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    if !dexy_state.can_mint {
        return Ok(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: request.amount.to_string(),
            token_name: variant.token_name().to_string(),
            can_execute: false,
            error: Some("Minting is currently unavailable".to_string()),
        });
    }

    if request.amount > dexy_state.dexy_in_bank {
        return Ok(DexyPreviewResponse {
            erg_cost_nano: "0".to_string(),
            tx_fee_nano: TX_FEE_NANO.to_string(),
            total_cost_nano: "0".to_string(),
            token_amount: request.amount.to_string(),
            token_name: variant.token_name().to_string(),
            can_execute: false,
            error: Some(format!(
                "Amount exceeds available: {}",
                dexy_state.dexy_in_bank
            )),
        });
    }

    let calc = cost_to_mint_dexy(
        request.amount,
        dexy_state.oracle_rate_nano,
        variant.decimals(),
    );
    let tx_fee = TX_FEE_NANO;
    let min_box = MIN_BOX_VALUE_NANO;
    let total = calc.erg_amount + tx_fee + min_box;

    Ok(DexyPreviewResponse {
        erg_cost_nano: calc.erg_amount.to_string(),
        tx_fee_nano: tx_fee.to_string(),
        total_cost_nano: total.to_string(),
        token_amount: request.amount.to_string(),
        token_name: variant.token_name().to_string(),
        can_execute: true,
        error: None,
    })
}

/// Build Dexy mint transaction
#[tauri::command]
pub async fn build_mint_dexy(
    state: State<'_, AppState>,
    request: DexyBuildRequest,
) -> Result<DexyBuildResponse, String> {
    let variant = request
        .variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}", request.variant))?;

    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    let config = state.config().await;
    let ids = DexyIds::for_variant(variant, config.network)
        .ok_or_else(|| "Dexy not available on this network".to_string())?;

    // Fetch state and context
    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    // Validate
    validate_mint_dexy(request.amount, &dexy_state).map_err(|e| e.to_string())?;

    let tx_ctx = fetch_dexy_tx_context(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    // Parse user UTXOs
    let user_inputs = super::parse_eip12_utxos(request.user_utxos)?;

    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

    let recipient_ergo_tree = match &request.recipient_address {
        Some(addr) if !addr.is_empty() => {
            Some(ergo_tx::address_to_ergo_tree(addr).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    let mint_request = MintDexyRequest {
        variant,
        amount: request.amount,
        user_address: request.user_address,
        user_ergo_tree,
        user_inputs,
        current_height: request.current_height,
        recipient_ergo_tree,
    };

    let result =
        build_mint_dexy_tx(&mint_request, &tx_ctx, &dexy_state).map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(DexyBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: TxSummaryDto {
            action: result.summary.action,
            erg_amount_nano: result.summary.erg_amount_nano.to_string(),
            token_amount: result.summary.token_amount.to_string(),
            token_name: result.summary.token_name,
            protocol_fee_nano: "0".to_string(), // Dexy has no protocol fee
            tx_fee_nano: result.summary.tx_fee_nano.to_string(),
        },
    })
}

/// Preview a Dexy LP swap (calculate output without building tx)
#[tauri::command]
pub async fn preview_dexy_swap(
    state: State<'_, AppState>,
    variant: String,
    direction: String,
    amount: i64,
    slippage: Option<f64>,
) -> Result<DexySwapPreviewResponse, String> {
    let dexy_variant = variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}", variant))?;

    if amount <= 0 {
        return Err("Amount must be positive".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or("Node capabilities not available")?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available", variant))?;

    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    // Calculate output
    let (output_amount, reserves_sold, reserves_bought) = match direction.as_str() {
        "erg_to_dexy" => {
            let out = dexy::calculator::calculate_lp_swap_output(
                amount,
                dexy_state.lp_erg_reserves,
                dexy_state.lp_dexy_reserves,
                dexy::constants::LP_SWAP_FEE_NUM,
                dexy::constants::LP_SWAP_FEE_DENOM,
            );
            (out, dexy_state.lp_erg_reserves, dexy_state.lp_dexy_reserves)
        }
        "dexy_to_erg" => {
            let out = dexy::calculator::calculate_lp_swap_output(
                amount,
                dexy_state.lp_dexy_reserves,
                dexy_state.lp_erg_reserves,
                dexy::constants::LP_SWAP_FEE_NUM,
                dexy::constants::LP_SWAP_FEE_DENOM,
            );
            (out, dexy_state.lp_dexy_reserves, dexy_state.lp_erg_reserves)
        }
        _ => {
            return Err(format!(
                "Invalid direction: {}. Use 'erg_to_dexy' or 'dexy_to_erg'",
                direction
            ))
        }
    };

    let slippage_pct = slippage.unwrap_or(0.5);
    let min_output = (output_amount as f64 * (1.0 - slippage_pct / 100.0)) as i64;

    let price_impact = dexy::calculator::calculate_lp_swap_price_impact(
        amount,
        reserves_sold,
        reserves_bought,
        dexy::constants::LP_SWAP_FEE_NUM,
        dexy::constants::LP_SWAP_FEE_DENOM,
    );

    let (output_token_name, output_decimals) = match direction.as_str() {
        "erg_to_dexy" => (
            dexy_variant.token_name().to_string(),
            dexy_variant.decimals(),
        ),
        _ => ("ERG".to_string(), 9),
    };

    Ok(DexySwapPreviewResponse {
        variant: variant.clone(),
        direction,
        input_amount: amount,
        output_amount,
        output_token_name,
        output_decimals,
        min_output,
        price_impact,
        fee_pct: dexy::constants::LP_SWAP_FEE_NUM as f64
            / dexy::constants::LP_SWAP_FEE_DENOM as f64
            * 100.0,
        miner_fee_nano: TX_FEE_NANO,
        lp_erg_reserves: dexy_state.lp_erg_reserves,
        lp_dexy_reserves: dexy_state.lp_dexy_reserves,
    })
}

/// Build Dexy LP swap transaction
#[tauri::command]
pub async fn build_dexy_swap_tx(
    state: State<'_, AppState>,
    variant: String,
    direction: String,
    amount: i64,
    min_output: i64,
    user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    recipient_address: Option<String>,
) -> Result<DexySwapBuildResponse, String> {
    let dexy_variant = variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}", variant))?;

    let swap_direction = match direction.as_str() {
        "erg_to_dexy" => dexy::tx_builder::SwapDirection::ErgToDexy,
        "dexy_to_erg" => dexy::tx_builder::SwapDirection::DexyToErg,
        _ => return Err(format!("Invalid direction: {}", direction)),
    };

    let client = state.node_client().await.ok_or("Node not connected")?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or("Node capabilities not available")?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available", variant))?;

    // Fetch state and swap context
    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    let swap_ctx = dexy::fetch::fetch_swap_tx_context(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    // Parse user UTXOs
    let user_inputs = super::parse_eip12_utxos(user_utxos)?;

    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

    let recipient_ergo_tree = match &recipient_address {
        Some(addr) if !addr.is_empty() => {
            Some(ergo_tx::address_to_ergo_tree(addr).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    let request = dexy::tx_builder::SwapDexyRequest {
        variant: dexy_variant,
        direction: swap_direction,
        input_amount: amount,
        min_output,
        user_address,
        user_ergo_tree,
        user_inputs,
        current_height,
        recipient_ergo_tree,
    };

    let result = dexy::tx_builder::build_swap_dexy_tx(&request, &swap_ctx, &dexy_state)
        .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(DexySwapBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: result.summary,
    })
}

/// Preview LP deposit (add liquidity) - calculate LP tokens received
#[tauri::command]
pub async fn preview_lp_deposit(
    state: State<'_, AppState>,
    variant: String,
    erg_amount: i64,
    dexy_amount: i64,
) -> Result<DexyLpPreviewResponse, String> {
    let dexy_variant = variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}. Use 'gold' or 'usd'", variant))?;

    if erg_amount <= 0 || dexy_amount <= 0 {
        return Ok(DexyLpPreviewResponse {
            variant: variant.clone(),
            action: "lp_deposit".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: "0".to_string(),
            redemption_fee_pct: None,
            can_execute: false,
            error: Some("Both ERG and Dexy amounts must be positive".to_string()),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available on {:?}", variant, config.network))?;

    // Fetch LP box to get lp_token_reserves
    let lp_token_id = citadel_core::TokenId::new(&ids.lp_nft);
    let lp_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        &capabilities,
        &lp_token_id,
    )
    .await
    .map_err(|e| format!("LP box not found: {}", e))?;
    let lp_data = parse_lp_box(&lp_box, &ids).map_err(|e| e.to_string())?;

    let calc = calculate_lp_deposit(
        erg_amount,
        dexy_amount,
        lp_data.erg_reserves,
        lp_data.dexy_reserves,
        lp_data.lp_token_reserves,
        dexy_variant.initial_lp(),
    );

    if calc.lp_tokens_out <= 0 {
        return Ok(DexyLpPreviewResponse {
            variant: variant.clone(),
            action: "lp_deposit".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: "0".to_string(),
            redemption_fee_pct: None,
            can_execute: false,
            error: Some("Deposit too small: would receive 0 LP tokens".to_string()),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    Ok(DexyLpPreviewResponse {
        variant: variant.clone(),
        action: "lp_deposit".to_string(),
        erg_amount: calc.consumed_erg.to_string(),
        dexy_amount: calc.consumed_dexy.to_string(),
        lp_tokens: calc.lp_tokens_out.to_string(),
        redemption_fee_pct: None,
        can_execute: true,
        error: None,
        miner_fee_nano: TX_FEE_NANO.to_string(),
    })
}

/// Build LP deposit (add liquidity) transaction
#[tauri::command]
pub async fn build_lp_deposit_tx(
    state: State<'_, AppState>,
    variant: String,
    erg_amount: i64,
    dexy_amount: i64,
    user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    recipient_address: Option<String>,
) -> Result<DexyLpBuildResponse, String> {
    let dexy_variant = variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}", variant))?;

    let client = state.node_client().await.ok_or("Node not connected")?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or("Node capabilities not available")?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available", variant))?;

    // Fetch LP tx context (LP box + LP Mint NFT box)
    let ctx = fetch_lp_tx_context(&client, &capabilities, &ids, LpAction::Deposit)
        .await
        .map_err(|e| e.to_string())?;

    // Parse user UTXOs
    let user_inputs = super::parse_eip12_utxos(user_utxos)?;
    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

    let recipient_ergo_tree = match &recipient_address {
        Some(addr) if !addr.is_empty() => {
            Some(ergo_tx::address_to_ergo_tree(addr).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    let request = LpDepositRequest {
        variant: dexy_variant,
        deposit_erg: erg_amount,
        deposit_dexy: dexy_amount,
        user_address,
        user_ergo_tree,
        user_inputs,
        current_height,
        recipient_ergo_tree,
    };

    let result = dexy::tx_builder::build_lp_deposit_tx(
        &request,
        &ctx,
        &ids.dexy_token,
        &ids.lp_token_id,
        dexy_variant.initial_lp(),
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(DexyLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: result.summary,
    })
}

/// Preview LP redeem (remove liquidity) - calculate ERG and Dexy received
#[tauri::command]
pub async fn preview_lp_redeem(
    state: State<'_, AppState>,
    variant: String,
    lp_amount: i64,
) -> Result<DexyLpPreviewResponse, String> {
    let dexy_variant = variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}. Use 'gold' or 'usd'", variant))?;

    if lp_amount <= 0 {
        return Ok(DexyLpPreviewResponse {
            variant: variant.clone(),
            action: "lp_redeem".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: "0".to_string(),
            redemption_fee_pct: Some(2.0),
            can_execute: false,
            error: Some("LP token amount must be positive".to_string()),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available on {:?}", variant, config.network))?;

    // Fetch dexy state (for oracle rate) and LP box (for lp_token_reserves)
    let dexy_state = fetch_dexy_state(&client, &capabilities, &ids)
        .await
        .map_err(|e| e.to_string())?;

    let lp_token_id = citadel_core::TokenId::new(&ids.lp_nft);
    let lp_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        &capabilities,
        &lp_token_id,
    )
    .await
    .map_err(|e| format!("LP box not found: {}", e))?;
    let lp_data = parse_lp_box(&lp_box, &ids).map_err(|e| e.to_string())?;

    // Check oracle rate gate (depeg protection)
    if !can_redeem_lp(
        dexy_state.lp_erg_reserves,
        dexy_state.lp_dexy_reserves,
        dexy_state.oracle_rate_nano,
    ) {
        return Ok(DexyLpPreviewResponse {
            variant: variant.clone(),
            action: "lp_redeem".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: lp_amount.to_string(),
            redemption_fee_pct: Some(2.0),
            can_execute: false,
            error: Some(
                "LP redeem blocked: LP rate below 98% of oracle rate (depeg protection)"
                    .to_string(),
            ),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    let calc = calculate_lp_redeem(
        lp_amount,
        lp_data.erg_reserves,
        lp_data.dexy_reserves,
        lp_data.lp_token_reserves,
        dexy_variant.initial_lp(),
    );

    if calc.erg_out <= 0 || calc.dexy_out <= 0 {
        return Ok(DexyLpPreviewResponse {
            variant: variant.clone(),
            action: "lp_redeem".to_string(),
            erg_amount: "0".to_string(),
            dexy_amount: "0".to_string(),
            lp_tokens: lp_amount.to_string(),
            redemption_fee_pct: Some(2.0),
            can_execute: false,
            error: Some("Redeem too small: would receive 0 ERG or Dexy tokens".to_string()),
            miner_fee_nano: TX_FEE_NANO.to_string(),
        });
    }

    Ok(DexyLpPreviewResponse {
        variant: variant.clone(),
        action: "lp_redeem".to_string(),
        erg_amount: calc.erg_out.to_string(),
        dexy_amount: calc.dexy_out.to_string(),
        lp_tokens: lp_amount.to_string(),
        redemption_fee_pct: Some(2.0),
        can_execute: true,
        error: None,
        miner_fee_nano: TX_FEE_NANO.to_string(),
    })
}

/// Build LP redeem (remove liquidity) transaction
#[tauri::command]
pub async fn build_lp_redeem_tx(
    state: State<'_, AppState>,
    variant: String,
    lp_amount: i64,
    user_address: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
    recipient_address: Option<String>,
) -> Result<DexyLpBuildResponse, String> {
    let dexy_variant = variant
        .parse::<DexyVariant>()
        .map_err(|_| format!("Invalid variant: {}", variant))?;

    let client = state.node_client().await.ok_or("Node not connected")?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or("Node capabilities not available")?;
    let config = state.config().await;
    let ids = DexyIds::for_variant(dexy_variant, config.network)
        .ok_or_else(|| format!("Dexy {} not available", variant))?;

    // Fetch LP tx context (LP box + LP Redeem NFT box + Oracle box)
    let ctx = fetch_lp_tx_context(&client, &capabilities, &ids, LpAction::Redeem)
        .await
        .map_err(|e| e.to_string())?;

    // Parse user UTXOs
    let user_inputs = super::parse_eip12_utxos(user_utxos)?;
    let user_ergo_tree = user_inputs[0].ergo_tree.clone();

    let recipient_ergo_tree = match &recipient_address {
        Some(addr) if !addr.is_empty() => {
            Some(ergo_tx::address_to_ergo_tree(addr).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    let request = LpRedeemRequest {
        variant: dexy_variant,
        lp_to_burn: lp_amount,
        user_address,
        user_ergo_tree,
        user_inputs,
        current_height,
        recipient_ergo_tree,
    };

    let result = dexy::tx_builder::build_lp_redeem_tx(
        &request,
        &ctx,
        &ids.dexy_token,
        &ids.lp_token_id,
        dexy_variant.initial_lp(),
    )
    .map_err(|e| e.to_string())?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize tx: {}", e))?;

    Ok(DexyLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: result.summary,
    })
}
