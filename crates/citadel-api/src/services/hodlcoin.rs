//! HodlCoin use-case orchestration: bank discovery, preview math, tx building.

use ergo_node_client::NodeClient;

use super::error::{IntoServiceError, ServiceResult};
use crate::AppState;

async fn find_bank(
    client: &NodeClient,
    singleton_token_id: &str,
) -> ServiceResult<hodlcoin::HodlBankState> {
    let banks = hodlcoin::discover_banks(client).await.into_service()?;
    banks
        .into_iter()
        .find(|b| b.singleton_token_id == singleton_token_id)
        .ok_or_else(|| format!("Bank not found: {}", singleton_token_id))
}

pub async fn get_banks(state: &AppState) -> ServiceResult<Vec<hodlcoin::HodlBankState>> {
    let client = state.require_node_client().await?;
    hodlcoin::discover_banks(&client).await.into_service()
}

pub async fn preview_mint(
    state: &AppState,
    singleton_token_id: &str,
    erg_amount: i64,
) -> ServiceResult<hodlcoin::HodlMintPreview> {
    if erg_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;
    let bank = find_bank(&client, singleton_token_id).await?;

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

pub async fn preview_burn(
    state: &AppState,
    singleton_token_id: &str,
    hodl_amount: i64,
) -> ServiceResult<hodlcoin::HodlBurnPreview> {
    if hodl_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;
    let bank = find_bank(&client, singleton_token_id).await?;

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

pub async fn build_mint_tx(
    state: &AppState,
    singleton_token_id: &str,
    erg_amount: i64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    if erg_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;
    let bank = find_bank(&client, singleton_token_id).await?;

    let bank_box = client
        .get_eip12_box_by_id(&bank.bank_box_id)
        .await
        .map_err(|e| format!("Failed to fetch bank box: {}", e))?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    hodlcoin::build_mint_tx_eip12(
        &bank_box,
        &bank,
        erg_amount,
        &user_utxos,
        &user_ergo_tree,
        current_height,
    )
    .into_service()
}

pub async fn build_burn_tx(
    state: &AppState,
    singleton_token_id: &str,
    hodl_amount: i64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    if hodl_amount <= 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;
    let bank = find_bank(&client, singleton_token_id).await?;

    let bank_box = client
        .get_eip12_box_by_id(&bank.bank_box_id)
        .await
        .map_err(|e| format!("Failed to fetch bank box: {}", e))?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    hodlcoin::build_burn_tx_eip12(
        &bank_box,
        &bank,
        hodl_amount,
        &user_utxos,
        &user_ergo_tree,
        current_height,
    )
    .into_service()
}
