use citadel_api::dto::{MintSignRequest, MintSignResponse, MintTxStatusResponse};
use citadel_api::AppState;
use tauri::State;

/// Fetch the SigmaFi bond market (open orders + active bonds)
#[tauri::command]
pub async fn sigmafi_fetch_market(
    state: State<'_, AppState>,
    user_address: Option<String>,
) -> Result<sigmafi::BondMarket, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let height = client.current_height().await.map_err(|e| e.to_string())?;

    // Try to get oracle price for collateral ratio calculation
    let oracle_erg_usd = get_oracle_erg_usd(&state).await.ok();

    sigmafi::fetch_bond_market(
        &client,
        user_address.as_deref(),
        height as u32,
        oracle_erg_usd,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Get oracle ERG/USD price (helper)
async fn get_oracle_erg_usd(state: &State<'_, AppState>) -> Result<f64, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let capabilities = client
        .capabilities()
        .await
        .ok_or("Node capabilities not available")?;
    let config = state.config().await;
    let nft_ids = sigmausd::NftIds::for_network(config.network)
        .ok_or_else(|| format!("Oracle not available on {:?}", config.network))?;
    let price = sigmausd::fetch_oracle_price(&client, &capabilities, &nft_ids)
        .await
        .map_err(|e| e.to_string())?;
    Ok(price.erg_usd)
}

/// Get the supported loan tokens list
#[tauri::command]
pub async fn sigmafi_get_tokens() -> Result<serde_json::Value, String> {
    let tokens: Vec<_> = sigmafi::SUPPORTED_TOKENS
        .iter()
        .map(|t| {
            serde_json::json!({
                "token_id": t.token_id,
                "name": t.name,
                "decimals": t.decimals,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(tokens))
}

/// Build an open order transaction (borrower creates loan request)
#[tauri::command]
pub async fn sigmafi_build_open_order(
    borrower_ergo_tree: String,
    loan_token_id: String,
    principal: String,
    repayment: String,
    maturity_blocks: i32,
    collateral_erg: String,
    collateral_tokens_json: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let principal: u64 = principal
        .parse()
        .map_err(|_| "Invalid principal amount".to_string())?;
    let repayment: u64 = repayment
        .parse()
        .map_err(|_| "Invalid repayment amount".to_string())?;
    let collateral_erg: u64 = collateral_erg
        .parse()
        .map_err(|_| "Invalid collateral ERG amount".to_string())?;

    let collateral_tokens: Vec<(String, u64)> =
        if collateral_tokens_json.is_empty() || collateral_tokens_json == "[]" {
            vec![]
        } else {
            serde_json::from_str(&collateral_tokens_json)
                .map_err(|e| format!("Invalid collateral tokens JSON: {}", e))?
        };

    let req = sigmafi::tx_builder::OpenOrderRequest {
        borrower_ergo_tree,
        loan_token_id,
        principal,
        repayment,
        maturity_blocks,
        collateral_erg,
        collateral_tokens,
        user_inputs: parsed_utxos,
        current_height,
    };

    let tx = sigmafi::tx_builder::build_open_order(&req).map_err(|e| e.to_string())?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

/// Build a cancel order transaction (borrower withdraws unfilled order)
#[tauri::command]
pub async fn sigmafi_build_cancel_order(
    state: State<'_, AppState>,
    box_id: String,
    borrower_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let order_box = client
        .get_eip12_box_by_id(&box_id)
        .await
        .map_err(|e| format!("Failed to fetch order box: {}", e))?;
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let req = sigmafi::tx_builder::CancelOrderRequest {
        order_box,
        borrower_ergo_tree,
        user_inputs: parsed_utxos,
        current_height,
    };

    let tx = sigmafi::tx_builder::build_cancel_order(&req).map_err(|e| e.to_string())?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

/// Build a close order transaction (lender fills an order, creating a bond)
#[tauri::command]
pub async fn sigmafi_build_close_order(
    state: State<'_, AppState>,
    box_id: String,
    lender_ergo_tree: String,
    ui_fee_ergo_tree: String,
    loan_token_id: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let order_box = client
        .get_eip12_box_by_id(&box_id)
        .await
        .map_err(|e| format!("Failed to fetch order box: {}", e))?;
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let req = sigmafi::tx_builder::CloseOrderRequest {
        order_box,
        lender_ergo_tree,
        ui_fee_ergo_tree,
        loan_token_id,
        user_inputs: parsed_utxos,
        current_height,
    };

    let tx = sigmafi::tx_builder::build_close_order(&req).map_err(|e| e.to_string())?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

/// Build a repay transaction (borrower repays loan before maturity)
#[tauri::command]
pub async fn sigmafi_build_repay(
    state: State<'_, AppState>,
    box_id: String,
    loan_token_id: String,
    borrower_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let bond_box = client
        .get_eip12_box_by_id(&box_id)
        .await
        .map_err(|e| format!("Failed to fetch bond box: {}", e))?;
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let req = sigmafi::tx_builder::RepayRequest {
        bond_box,
        loan_token_id,
        borrower_ergo_tree,
        user_inputs: parsed_utxos,
        current_height,
    };

    let tx = sigmafi::tx_builder::build_repay(&req).map_err(|e| e.to_string())?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

/// Build a liquidate transaction (lender claims collateral after maturity)
#[tauri::command]
pub async fn sigmafi_build_liquidate(
    state: State<'_, AppState>,
    box_id: String,
    lender_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let bond_box = client
        .get_eip12_box_by_id(&box_id)
        .await
        .map_err(|e| format!("Failed to fetch bond box: {}", e))?;
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let req = sigmafi::tx_builder::LiquidateRequest {
        bond_box,
        lender_ergo_tree,
        user_inputs: parsed_utxos,
        current_height,
    };

    let tx = sigmafi::tx_builder::build_liquidate(&req).map_err(|e| e.to_string())?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

/// Start signing a SigmaFi transaction (reuses ErgoPay sign flow)
#[tauri::command]
pub async fn start_sigmafi_sign(
    state: State<'_, AppState>,
    unsigned_tx: serde_json::Value,
    message: Option<String>,
) -> Result<MintSignResponse, String> {
    super::start_mint_sign(
        state,
        MintSignRequest {
            unsigned_tx,
            message: message.unwrap_or_else(|| "SigmaFi transaction".to_string()),
        },
    )
    .await
}

/// Get SigmaFi transaction signing status
#[tauri::command]
pub async fn get_sigmafi_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    super::get_mint_tx_status(state, request_id).await
}
