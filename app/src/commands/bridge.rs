use citadel_api::AppState;
use super::StrErr;
use rosen::{
    config, fee, token_map, tx_builder::address_to_ergo_tree as rosen_address_to_ergo_tree,
    validate::validate_target_address as rosen_validate_address, BridgeFeeInfo, BridgeTokenInfo,
    LockRequest, RosenBridgeState, RosenConfig, TokenMap,
};
use tauri::State;

pub struct RosenConfigState(pub tokio::sync::Mutex<Option<RosenConfig>>);
pub struct RosenTokenMapState(pub tokio::sync::Mutex<Option<TokenMap>>);

#[tauri::command]
pub async fn init_bridge_config(
    config_state: State<'_, RosenConfigState>,
    token_map_state: State<'_, RosenTokenMapState>,
) -> Result<(), String> {
    let cfg = config::fetch_config().await;
    *config_state.0.lock().await = Some(cfg);

    let token_map_result: Result<TokenMap, String> = async {
        let url = config::fetch_token_map_url()
            .await
            .str_err()?;
        token_map::fetch_token_map(&url)
            .await
            .str_err()
    }
    .await;

    match token_map_result {
        Ok(map) => {
            tracing::info!("Loaded {} bridgeable tokens", map.tokens.len());
            *token_map_state.0.lock().await = Some(map);
        }
        Err(e) => {
            tracing::warn!("Failed to fetch token map: {}", e);
            *token_map_state.0.lock().await = Some(TokenMap::default());
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_bridge_state(
    _config_state: State<'_, RosenConfigState>,
    token_map_state: State<'_, RosenTokenMapState>,
) -> Result<RosenBridgeState, String> {
    let map_guard = token_map_state.0.lock().await;
    let map = map_guard
        .as_ref()
        .ok_or("Bridge not initialized. Call init_bridge_config first.")?;

    let supported_chains = map.supported_chains();
    let available_tokens = map
        .tokens
        .iter()
        .map(|t| BridgeTokenInfo {
            ergo_token_id: t.ergo_token_id.clone(),
            name: t.ergo_name.clone(),
            decimals: t.ergo_decimals,
            target_chains: t.target_chains.iter().map(|c| c.chain.clone()).collect(),
        })
        .collect();

    Ok(RosenBridgeState {
        supported_chains,
        available_tokens,
    })
}

#[tauri::command]
pub async fn get_bridge_tokens(
    target_chain: String,
    token_map_state: State<'_, RosenTokenMapState>,
) -> Result<Vec<BridgeTokenInfo>, String> {
    let map_guard = token_map_state.0.lock().await;
    let map = map_guard.as_ref().ok_or("Bridge not initialized")?;

    let tokens = map
        .tokens_for_chain(&target_chain)
        .into_iter()
        .map(|t| BridgeTokenInfo {
            ergo_token_id: t.ergo_token_id.clone(),
            name: t.ergo_name.clone(),
            decimals: t.ergo_decimals,
            target_chains: t.target_chains.iter().map(|c| c.chain.clone()).collect(),
        })
        .collect();

    Ok(tokens)
}

#[tauri::command]
pub async fn get_bridge_fees(
    state: State<'_, AppState>,
    config_state: State<'_, RosenConfigState>,
    ergo_token_id: String,
    target_chain: String,
    amount: i64,
) -> Result<BridgeFeeInfo, String> {
    let cfg_guard = config_state.0.lock().await;
    let cfg = cfg_guard.as_ref().ok_or("Bridge not initialized")?;

    let client = state.require_node_client().await?;
    let caps = client
        .capabilities()
        .await
        .ok_or("Node capabilities not available")?;

    let height = caps.chain_height as i32;

    let fees = fee::fetch_bridge_fees(
        &client,
        &caps,
        &cfg.min_fee_nft_id,
        &ergo_token_id,
        &target_chain,
        height,
    )
    .await
    .str_err()?;

    let variable_fee = (amount as i128 * fees.fee_ratio as i128 / 10000) as i64;
    let total_fees = fees.bridge_fee + fees.network_fee + variable_fee;
    let receiving = if amount > total_fees {
        amount - total_fees
    } else {
        0
    };
    let min_transfer = fees.bridge_fee + fees.network_fee + 1;

    drop(cfg_guard);

    Ok(BridgeFeeInfo {
        bridge_fee: fees.bridge_fee.to_string(),
        network_fee: fees.network_fee.to_string(),
        fee_ratio_bps: fees.fee_ratio,
        min_transfer: min_transfer.to_string(),
        receiving_amount: receiving.to_string(),
        bridge_fee_raw: fees.bridge_fee,
        network_fee_raw: fees.network_fee,
    })
}

#[tauri::command]
pub async fn build_bridge_lock_tx(
    state: State<'_, AppState>,
    config_state: State<'_, RosenConfigState>,
    ergo_token_id: String,
    amount: i64,
    target_chain: String,
    target_address: String,
    bridge_fee: i64,
    network_fee: i64,
) -> Result<serde_json::Value, String> {
    rosen_validate_address(&target_chain, &target_address).str_err()?;

    let cfg_guard = config_state.0.lock().await;
    let cfg = cfg_guard.as_ref().ok_or("Bridge not initialized")?;

    let lock_ergo_tree =
        rosen_address_to_ergo_tree(&cfg.lock_address).str_err()?;

    drop(cfg_guard);

    let wallet = state.wallet().await.ok_or("No wallet connected")?;
    let client = state.require_node_client().await?;

    let caps = client
        .capabilities()
        .await
        .ok_or("Node capabilities not available")?;

    let utxos: Vec<ergo_tx::Eip12InputBox> = client
        .get_effective_utxos(&wallet.address)
        .await
        .str_err()?;

    let user_ergo_tree = rosen_address_to_ergo_tree(&wallet.address)
        .map_err(|e| format!("Failed to get user ErgoTree: {}", e))?;

    let request = LockRequest {
        ergo_token_id,
        amount,
        target_chain,
        target_address,
        bridge_fee,
        network_fee,
        user_address: wallet.address.clone(),
        user_ergo_tree,
        user_inputs: utxos,
        current_height: caps.chain_height as i32,
    };

    let result = rosen::build_lock_tx(&request, &lock_ergo_tree).str_err()?;

    serde_json::to_value(&result).map_err(|e| format!("Failed to serialize: {}", e))
}

