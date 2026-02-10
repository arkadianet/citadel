use citadel_api::dto::{MintSignRequest, MintSignResponse, MintTxStatusResponse};
use citadel_api::AppState;
use tauri::State;

/// Get all discovered HodlCoin banks
#[tauri::command]
pub async fn get_hodlcoin_banks(
    state: State<'_, AppState>,
) -> Result<Vec<hodlcoin::HodlBankState>, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    hodlcoin::discover_banks(&client)
        .await
        .map_err(|e| e.to_string())
}

/// Preview minting hodlTokens
#[tauri::command]
pub async fn preview_hodlcoin_mint(
    state: State<'_, AppState>,
    singleton_token_id: String,
    erg_amount: i64,
) -> Result<hodlcoin::HodlMintPreview, String> {
    if erg_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    let banks = hodlcoin::discover_banks(&client)
        .await
        .map_err(|e| e.to_string())?;

    let bank = banks
        .into_iter()
        .find(|b| b.singleton_token_id == singleton_token_id)
        .ok_or_else(|| format!("Bank not found: {}", singleton_token_id))?;

    let tokens_received = hodlcoin::mint_amount(
        bank.reserve_nano_erg,
        bank.circulating_supply,
        bank.precision_factor,
        erg_amount,
    );

    let miner_fee = citadel_core::constants::TX_FEE_NANO;
    let min_box = citadel_core::constants::MIN_BOX_VALUE_NANO;

    Ok(hodlcoin::HodlMintPreview {
        erg_deposited: erg_amount,
        hodl_tokens_received: tokens_received,
        price_per_token: bank.price_nano_per_hodl,
        miner_fee,
        total_erg_cost: erg_amount + miner_fee + min_box,
    })
}

/// Preview burning hodlTokens
#[tauri::command]
pub async fn preview_hodlcoin_burn(
    state: State<'_, AppState>,
    singleton_token_id: String,
    hodl_amount: i64,
) -> Result<hodlcoin::HodlBurnPreview, String> {
    if hodl_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    let banks = hodlcoin::discover_banks(&client)
        .await
        .map_err(|e| e.to_string())?;

    let bank = banks
        .into_iter()
        .find(|b| b.singleton_token_id == singleton_token_id)
        .ok_or_else(|| format!("Bank not found: {}", singleton_token_id))?;

    let burn_result = hodlcoin::burn_amount(
        bank.reserve_nano_erg,
        bank.circulating_supply,
        bank.precision_factor,
        hodl_amount,
        bank.bank_fee_num,
        bank.dev_fee_num,
    );

    let miner_fee = citadel_core::constants::TX_FEE_NANO;

    Ok(hodlcoin::HodlBurnPreview {
        hodl_tokens_spent: hodl_amount,
        erg_received: burn_result.erg_to_user,
        bank_fee_nano: burn_result.bank_fee,
        dev_fee_nano: burn_result.dev_fee,
        erg_before_fees: burn_result.before_fees,
        price_per_token: bank.price_nano_per_hodl,
        miner_fee,
    })
}

/// Build a hodlcoin mint EIP-12 unsigned transaction
#[tauri::command]
pub async fn build_hodlcoin_mint_tx(
    state: State<'_, AppState>,
    singleton_token_id: String,
    erg_amount: i64,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    if erg_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    let banks = hodlcoin::discover_banks(&client)
        .await
        .map_err(|e| e.to_string())?;

    let bank = banks
        .into_iter()
        .find(|b| b.singleton_token_id == singleton_token_id)
        .ok_or_else(|| format!("Bank not found: {}", singleton_token_id))?;

    // Fetch bank box in EIP-12 format
    let bank_box = client
        .get_eip12_box_by_id(&bank.bank_box_id)
        .await
        .map_err(|e| format!("Failed to fetch bank box: {}", e))?;

    // Parse user UTXOs
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    let unsigned_tx = hodlcoin::build_mint_tx_eip12(
        &bank_box,
        &bank,
        erg_amount,
        &parsed_utxos,
        &user_ergo_tree,
        current_height,
    )
    .map_err(|e| e.to_string())?;

    serde_json::to_value(&unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))
}

/// Build a hodlcoin burn EIP-12 unsigned transaction
#[tauri::command]
pub async fn build_hodlcoin_burn_tx(
    state: State<'_, AppState>,
    singleton_token_id: String,
    hodl_amount: i64,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    if hodl_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.node_client().await.ok_or("Node not connected")?;

    let banks = hodlcoin::discover_banks(&client)
        .await
        .map_err(|e| e.to_string())?;

    let bank = banks
        .into_iter()
        .find(|b| b.singleton_token_id == singleton_token_id)
        .ok_or_else(|| format!("Bank not found: {}", singleton_token_id))?;

    // Fetch bank box in EIP-12 format
    let bank_box = client
        .get_eip12_box_by_id(&bank.bank_box_id)
        .await
        .map_err(|e| format!("Failed to fetch bank box: {}", e))?;

    // Parse user UTXOs
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let user_ergo_tree = parsed_utxos[0].ergo_tree.clone();

    let unsigned_tx = hodlcoin::build_burn_tx_eip12(
        &bank_box,
        &bank,
        hodl_amount,
        &parsed_utxos,
        &user_ergo_tree,
        current_height,
    )
    .map_err(|e| e.to_string())?;

    serde_json::to_value(&unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))
}

/// Start signing a hodlcoin transaction (reuses ErgoPay sign flow)
#[tauri::command]
pub async fn start_hodlcoin_sign(
    state: State<'_, AppState>,
    unsigned_tx: serde_json::Value,
    message: Option<String>,
) -> Result<MintSignResponse, String> {
    super::start_mint_sign(
        state,
        MintSignRequest {
            unsigned_tx,
            message: message.unwrap_or_else(|| "HodlCoin transaction".to_string()),
        },
    )
    .await
}

/// Get hodlcoin transaction signing status (reuses ErgoPay poll)
#[tauri::command]
pub async fn get_hodlcoin_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    super::get_mint_tx_status(state, request_id).await
}
