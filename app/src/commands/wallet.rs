use citadel_api::dto::{
    wallet_status, ConnectionStatusResponse, RecentTxDto, RecentTxsResponse, TokenBalance,
    TokenChangeDto, WalletBalanceResponse, WalletConnectResponse, WalletStatusResponse,
};
use citadel_api::AppState;
use ergopay_server::RequestStatus;
use tauri::State;

/// Start wallet connection flow
#[tauri::command]
pub async fn start_wallet_connect(
    state: State<'_, AppState>,
) -> Result<WalletConnectResponse, String> {
    let server = state.ergopay_server().await.map_err(|e| e.to_string())?;
    let (request_id, qr_url) = server.create_connect_request().await;
    let nautilus_url = server.get_nautilus_connect_url(&request_id);

    Ok(WalletConnectResponse {
        request_id,
        qr_url,
        nautilus_url,
    })
}

/// Get wallet connection status
#[tauri::command]
pub async fn get_wallet_status(state: State<'_, AppState>) -> Result<WalletStatusResponse, String> {
    let wallet = state.wallet().await;

    Ok(WalletStatusResponse {
        connected: wallet.is_some(),
        address: wallet.map(|w| w.address),
    })
}

/// Check status of a connection request
#[tauri::command]
pub async fn get_connection_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<ConnectionStatusResponse, String> {
    let server = state.ergopay_server().await.map_err(|e| e.to_string())?;

    match server.get_request_status(&request_id).await {
        Some(RequestStatus::Pending) => Ok(ConnectionStatusResponse {
            status: wallet_status::PENDING.to_string(),
            address: None,
        }),
        Some(RequestStatus::AddressReceived(address)) => {
            // Update the wallet state
            state
                .set_wallet(address.clone())
                .await
                .map_err(|e| e.to_string())?;

            Ok(ConnectionStatusResponse {
                status: wallet_status::CONNECTED.to_string(),
                address: Some(address),
            })
        }
        Some(RequestStatus::Expired) => Ok(ConnectionStatusResponse {
            status: wallet_status::EXPIRED.to_string(),
            address: None,
        }),
        Some(RequestStatus::Failed(msg)) => Ok(ConnectionStatusResponse {
            // Failed status includes error message as "failed: <reason>"
            status: format!("{}: {}", wallet_status::FAILED, msg),
            address: None,
        }),
        _ => Ok(ConnectionStatusResponse {
            // Unknown status - request not found
            status: "unknown".to_string(),
            address: None,
        }),
    }
}

/// Disconnect wallet
#[tauri::command]
pub async fn disconnect_wallet(state: State<'_, AppState>) -> Result<(), String> {
    state.disconnect_wallet().await;
    Ok(())
}

/// Get wallet balance (ERG and tokens)
#[tauri::command]
pub async fn get_wallet_balance(
    state: State<'_, AppState>,
) -> Result<WalletBalanceResponse, String> {
    // Get wallet address
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    // Get node client
    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    // Check capabilities - need extraIndex for balance queries
    let caps = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    if caps.capability_tier == ergo_node_client::CapabilityTier::Basic {
        return Err("Balance queries require extraIndex enabled on the node".to_string());
    }

    // Fetch balances
    let (erg_nano, tokens) = client
        .get_address_balances(&wallet.address)
        .await
        .map_err(|e| e.to_string())?;

    // Known token IDs (mainnet only — testnet not yet supported in sigmausd::constants)
    let sigusd_token_id = sigmausd::constants::mainnet::SIGUSD_TOKEN_ID;
    let sigrsv_token_id = sigmausd::constants::mainnet::SIGRSV_TOKEN_ID;

    // Extract SigUSD and SigRSV amounts
    let sigusd_amount = tokens
        .iter()
        .find(|(id, _)| id == sigusd_token_id)
        .map(|(_, amt)| *amt)
        .unwrap_or(0);

    let sigrsv_amount = tokens
        .iter()
        .find(|(id, _)| id == sigrsv_token_id)
        .map(|(_, amt)| *amt)
        .unwrap_or(0);

    // Build token list with names and decimals
    let mut known: std::collections::HashMap<String, (Option<String>, u8)> =
        std::collections::HashMap::new();
    known.insert(sigusd_token_id.to_string(), (Some("SigUSD".to_string()), 2));
    known.insert(sigrsv_token_id.to_string(), (Some("SigRSV".to_string()), 0));

    let mut token_balances: Vec<TokenBalance> = Vec::new();
    for (token_id, amount) in tokens {
        let (name, decimals) = if let Some(cached) = known.get(&token_id) {
            cached.clone()
        } else {
            let info = client.get_token_info(&token_id).await.ok();
            match info {
                Some(ti) => (ti.name, ti.decimals.unwrap_or(0) as u8),
                None => (None, 0u8),
            }
        };
        token_balances.push(TokenBalance {
            token_id,
            amount,
            name,
            decimals,
        });
    }

    Ok(WalletBalanceResponse {
        address: wallet.address,
        erg_nano,
        erg_formatted: format!("{:.4}", erg_nano as f64 / 1e9),
        sigusd_amount,
        sigusd_formatted: format!("{:.2}", sigusd_amount as f64 / 100.0),
        sigrsv_amount,
        tokens: token_balances,
    })
}

/// Get recent transactions for the connected wallet
#[tauri::command]
pub async fn get_recent_transactions(
    state: State<'_, AppState>,
    limit: u64,
) -> Result<RecentTxsResponse, String> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let caps = client
        .capabilities()
        .await
        .ok_or_else(|| "Node capabilities not available".to_string())?;

    if caps.capability_tier == ergo_node_client::CapabilityTier::Basic {
        return Err("Transaction history requires extraIndex enabled on the node".to_string());
    }

    let address = &wallet.address;

    let raw_txs = client
        .get_recent_transactions(address, limit)
        .await
        .map_err(|e| e.to_string())?;

    // Known token IDs for naming
    let sigusd_token_id = sigmausd::constants::mainnet::SIGUSD_TOKEN_ID;
    let sigrsv_token_id = sigmausd::constants::mainnet::SIGRSV_TOKEN_ID;

    // Cache for token metadata lookups
    let mut token_cache: std::collections::HashMap<String, (Option<String>, u8)> =
        std::collections::HashMap::new();
    token_cache.insert(sigusd_token_id.to_string(), (Some("SigUSD".to_string()), 2));
    token_cache.insert(sigrsv_token_id.to_string(), (Some("SigRSV".to_string()), 0));

    let mut transactions = Vec::new();
    for tx in &raw_txs {
        let tx_id = tx["id"].as_str().unwrap_or_default().to_string();
        let inclusion_height = tx["inclusionHeight"].as_u64().unwrap_or(0);
        let num_confirmations = tx["numConfirmations"].as_u64().unwrap_or(0);
        let timestamp = tx["timestamp"].as_u64().unwrap_or(0);

        // Calculate net ERG change for this address
        let mut erg_in: i64 = 0;
        let mut erg_out: i64 = 0;
        let mut token_in: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut token_out: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();

        // Sum inputs belonging to our address (ERG/tokens we spent)
        if let Some(inputs) = tx["inputs"].as_array() {
            for input in inputs {
                if input["address"].as_str() == Some(address) {
                    erg_in += input["value"].as_i64().unwrap_or(0);
                    if let Some(assets) = input["assets"].as_array() {
                        for asset in assets {
                            let tid = asset["tokenId"].as_str().unwrap_or_default().to_string();
                            let amt = asset["amount"].as_i64().unwrap_or(0);
                            *token_in.entry(tid).or_insert(0) += amt;
                        }
                    }
                }
            }
        }

        // Sum outputs belonging to our address (ERG/tokens we received)
        if let Some(outputs) = tx["outputs"].as_array() {
            for output in outputs {
                if output["address"].as_str() == Some(address) {
                    erg_out += output["value"].as_i64().unwrap_or(0);
                    if let Some(assets) = output["assets"].as_array() {
                        for asset in assets {
                            let tid = asset["tokenId"].as_str().unwrap_or_default().to_string();
                            let amt = asset["amount"].as_i64().unwrap_or(0);
                            *token_out.entry(tid).or_insert(0) += amt;
                        }
                    }
                }
            }
        }

        let erg_change_nano = erg_out - erg_in;

        // Calculate net token changes
        let mut all_token_ids: std::collections::HashSet<String> =
            token_in.keys().cloned().collect();
        all_token_ids.extend(token_out.keys().cloned());

        let mut token_changes: Vec<TokenChangeDto> = Vec::new();
        for tid in all_token_ids {
            let change = token_out.get(&tid).unwrap_or(&0) - token_in.get(&tid).unwrap_or(&0);
            if change == 0 {
                continue;
            }

            // Resolve token name + decimals, using cache
            let (name, decimals) = if let Some(cached) = token_cache.get(&tid) {
                cached.clone()
            } else {
                let info = client.get_token_info(&tid).await.ok();
                let resolved = match info {
                    Some(ti) => (ti.name, ti.decimals.unwrap_or(0) as u8),
                    None => (None, 0u8),
                };
                token_cache.insert(tid.clone(), resolved.clone());
                resolved
            };

            token_changes.push(TokenChangeDto {
                token_id: tid,
                amount: change,
                name,
                decimals,
            });
        }

        transactions.push(RecentTxDto {
            tx_id,
            inclusion_height,
            num_confirmations,
            timestamp,
            erg_change_nano,
            token_changes,
        });
    }

    Ok(RecentTxsResponse { transactions })
}

/// Get user UTXOs in EIP12 format for transaction building.
///
/// Uses mempool-aware "effective" UTXOs: confirmed boxes minus those spent in
/// mempool, plus unconfirmed change outputs. This enables 0-conf chained
/// transactions (submit tx1, immediately build tx2 using tx1's change output).
#[tauri::command]
pub async fn get_user_utxos(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    let client = state
        .node_client()
        .await
        .ok_or_else(|| "Node not connected".to_string())?;

    let utxos = client
        .get_effective_utxos(&wallet.address)
        .await
        .map_err(|e| e.to_string())?;

    // Convert to JSON values
    utxos
        .into_iter()
        .map(|u| serde_json::to_value(u).map_err(|e| e.to_string()))
        .collect()
}

/// Validate an Ergo address and return its ErgoTree hex
#[tauri::command]
pub async fn validate_ergo_address(address: String) -> Result<String, String> {
    ergo_tx::address_to_ergo_tree(&address).map_err(|e| e.to_string())
}
