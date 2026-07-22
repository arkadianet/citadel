//! LP deposit/redeem (direct + proxy order) and pool bootstrap/create.

use crate::services::error::IntoServiceError;
use crate::AppState;
use serde::Serialize;

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmmLpBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub summary: serde_json::Value,
}

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

pub async fn preview_amm_lp_deposit(
    state: &AppState,
    pool_id: &str,
    input_type: &str,
    amount: u64,
) -> Result<AmmLpDepositPreviewResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let erg_reserves = pool
        .erg_reserves
        .ok_or("Only N2T pools supported for LP operations")?;

    let (erg_amount, token_amount) = match input_type {
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

pub async fn build_amm_lp_deposit_tx(
    state: &AppState,
    pool_id: &str,
    erg_amount: u64,
    token_amount: u64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let pool_box = client
        .get_eip12_box_by_id(&pool.box_id)
        .await
        .map_err(|e| format!("Failed to fetch pool box: {}", e))?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let result = amm::build_lp_deposit_eip12(
        &pool_box,
        &pool,
        erg_amount,
        token_amount,
        &user_utxos,
        &user_ergo_tree,
        current_height,
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

/// Proxy order -- Spectrum bots detect and execute the deposit.
pub async fn build_amm_lp_deposit_order(
    state: &AppState,
    pool_id: &str,
    erg_amount: u64,
    token_amount: u64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    user_pk: String,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let result = amm::build_lp_deposit_order_eip12(
        &pool,
        erg_amount,
        token_amount,
        &user_utxos,
        &user_ergo_tree,
        &user_pk,
        current_height,
        None,
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

pub async fn preview_amm_lp_redeem(
    state: &AppState,
    pool_id: &str,
    lp_amount: u64,
) -> Result<AmmLpRedeemPreviewResponse, String> {
    if lp_amount == 0 {
        return Err("LP amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

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

pub async fn build_amm_lp_redeem_tx(
    state: &AppState,
    pool_id: &str,
    lp_amount: u64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let pool_box = client
        .get_eip12_box_by_id(&pool.box_id)
        .await
        .map_err(|e| format!("Failed to fetch pool box: {}", e))?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let result = amm::build_lp_redeem_eip12(
        &pool_box,
        &pool,
        lp_amount,
        &user_utxos,
        &user_ergo_tree,
        current_height,
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

/// Proxy order -- Spectrum bots detect and execute the redemption.
pub async fn build_amm_lp_redeem_order(
    state: &AppState,
    pool_id: &str,
    lp_amount: u64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    user_pk: String,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

    let result = amm::build_lp_redeem_order_eip12(
        &pool,
        lp_amount,
        &user_utxos,
        &user_ergo_tree,
        &user_pk,
        current_height,
        None,
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

pub fn preview_pool_create(
    pool_type: String,
    x_token_id: Option<String>,
    x_amount: u64,
    y_token_id: String,
    y_amount: u64,
    fee_percent: f64,
) -> Result<PoolCreatePreviewResponse, String> {
    // Tauri macro requires all args used
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

/// LP token ID equals the first input box_id (Ergo minting rule).
pub fn build_pool_bootstrap_tx(
    pool_type: String,
    x_token_id: Option<String>,
    x_amount: u64,
    y_token_id: String,
    y_amount: u64,
    fee_percent: f64,
    user_utxos: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> Result<AmmLpBuildResponse, String> {
    let pool_type_enum = match pool_type.as_str() {
        "N2T" => amm::state::PoolType::N2T,
        "T2T" => amm::state::PoolType::T2T,
        _ => return Err(format!("Invalid pool type: {}", pool_type)),
    };

    let fee_num = ((1.0 - fee_percent / 100.0) * amm::constants::fees::DEFAULT_FEE_DENOM as f64)
        .round() as i32;

    let user_ergo_tree = user_utxos[0].ergo_tree.clone();

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
        &user_utxos,
        &user_ergo_tree,
        current_height,
    )
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}

/// TX1: takes the bootstrap box (TX0 output) and creates the on-chain pool box.
pub fn build_pool_create_tx(
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
    .into_service()?;

    let unsigned_tx_json = serde_json::to_value(&result.unsigned_tx)
        .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
    let summary_json = serde_json::to_value(&result.summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;

    Ok(AmmLpBuildResponse {
        unsigned_tx: unsigned_tx_json,
        summary: summary_json,
    })
}
