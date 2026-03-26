use citadel_api::AppState;
use tauri::State;

use super::StrErr;

#[tauri::command]
pub async fn get_hodlcoin_banks(
    state: State<'_, AppState>,
) -> Result<Vec<hodlcoin::HodlBankState>, String> {
    let client = state.require_node_client().await?;
    hodlcoin::discover_banks(&client)
        .await
        .str_err()
}

#[tauri::command]
pub async fn preview_hodlcoin_mint(
    state: State<'_, AppState>,
    singleton_token_id: String,
    erg_amount: i64,
) -> Result<hodlcoin::HodlMintPreview, String> {
    if erg_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let banks = hodlcoin::discover_banks(&client)
        .await
        .str_err()?;

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

#[tauri::command]
pub async fn preview_hodlcoin_burn(
    state: State<'_, AppState>,
    singleton_token_id: String,
    hodl_amount: i64,
) -> Result<hodlcoin::HodlBurnPreview, String> {
    if hodl_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let banks = hodlcoin::discover_banks(&client)
        .await
        .str_err()?;

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

    let client = state.require_node_client().await?;

    let banks = hodlcoin::discover_banks(&client)
        .await
        .str_err()?;

    let bank = banks
        .into_iter()
        .find(|b| b.singleton_token_id == singleton_token_id)
        .ok_or_else(|| format!("Bank not found: {}", singleton_token_id))?;

    let bank_box = client
        .get_eip12_box_by_id(&bank.bank_box_id)
        .await
        .map_err(|e| format!("Failed to fetch bank box: {}", e))?;

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
    .str_err()?;

    serde_json::to_value(&unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))
}

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

    let client = state.require_node_client().await?;

    let banks = hodlcoin::discover_banks(&client)
        .await
        .str_err()?;

    let bank = banks
        .into_iter()
        .find(|b| b.singleton_token_id == singleton_token_id)
        .ok_or_else(|| format!("Bank not found: {}", singleton_token_id))?;

    let bank_box = client
        .get_eip12_box_by_id(&bank.bank_box_id)
        .await
        .map_err(|e| format!("Failed to fetch bank box: {}", e))?;

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
    .str_err()?;

    serde_json::to_value(&unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))
}

