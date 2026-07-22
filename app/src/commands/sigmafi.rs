use citadel_api::services::sigmafi as sigmafi_svc;
use citadel_api::AppState;
use tauri::State;

/// Fetch the SigmaFi bond market (open orders + active bonds)
#[tauri::command]
pub async fn sigmafi_fetch_market(
    state: State<'_, AppState>,
    user_address: Option<String>,
) -> Result<sigmafi::BondMarket, String> {
    sigmafi_svc::fetch_market(&state, user_address.as_deref()).await
}

/// Get the supported loan tokens list
#[tauri::command]
pub async fn sigmafi_get_tokens() -> Result<serde_json::Value, String> {
    Ok(serde_json::Value::Array(sigmafi_svc::get_tokens()))
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

    let tx = sigmafi_svc::build_open_order(
        borrower_ergo_tree,
        loan_token_id,
        principal,
        repayment,
        maturity_blocks,
        collateral_erg,
        collateral_tokens,
        parsed_utxos,
        current_height,
    )?;
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
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let tx = sigmafi_svc::build_cancel_order(
        &state,
        &box_id,
        borrower_ergo_tree,
        parsed_utxos,
        current_height,
    )
    .await?;
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
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let tx = sigmafi_svc::build_close_order(
        &state,
        &box_id,
        lender_ergo_tree,
        ui_fee_ergo_tree,
        loan_token_id,
        parsed_utxos,
        current_height,
    )
    .await?;
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
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let tx = sigmafi_svc::build_repay(
        &state,
        &box_id,
        loan_token_id,
        borrower_ergo_tree,
        parsed_utxos,
        current_height,
    )
    .await?;
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
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let tx = sigmafi_svc::build_liquidate(
        &state,
        &box_id,
        lender_ergo_tree,
        parsed_utxos,
        current_height,
    )
    .await?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}
