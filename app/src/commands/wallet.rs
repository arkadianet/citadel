use citadel_api::dto::{
    wallet_status, ConnectionStatusResponse, RecentTxDto, RecentTxsResponse, TokenBalance,
    TokenChangeDto, WalletBalanceResponse, WalletConnectResponse, WalletStatusResponse,
};
use citadel_api::AppState;
use ergopay_server::RequestStatus;
use tauri::State;

use super::StrErr;

#[tauri::command]
pub async fn start_wallet_connect(
    state: State<'_, AppState>,
) -> Result<WalletConnectResponse, String> {
    let server = state.ergopay_server().await.str_err()?;
    let (request_id, qr_url) = server.create_connect_request().await;
    let nautilus_url = server.get_nautilus_connect_url(&request_id);

    Ok(WalletConnectResponse {
        request_id,
        qr_url,
        nautilus_url,
    })
}

#[tauri::command]
pub async fn get_wallet_status(state: State<'_, AppState>) -> Result<WalletStatusResponse, String> {
    let wallet = state.wallet().await;

    Ok(WalletStatusResponse {
        connected: wallet.is_some(),
        address: wallet.map(|w| w.address),
    })
}

#[tauri::command]
pub async fn get_connection_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<ConnectionStatusResponse, String> {
    let server = state.ergopay_server().await.str_err()?;

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
                .str_err()?;

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
            status: format!("{}: {}", wallet_status::FAILED, msg),
            address: None,
        }),
        _ => Ok(ConnectionStatusResponse {
            status: "unknown".to_string(),
            address: None,
        }),
    }
}

#[tauri::command]
pub async fn disconnect_wallet(state: State<'_, AppState>) -> Result<(), String> {
    state.disconnect_wallet().await;
    Ok(())
}

#[tauri::command]
pub async fn get_wallet_balance(
    state: State<'_, AppState>,
) -> Result<WalletBalanceResponse, String> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    let client = state.require_node_client().await?;
    let caps = client.require_capabilities().await?;

    if caps.capability_tier == ergo_node_client::CapabilityTier::Basic {
        return Err("Balance queries require extraIndex enabled on the node".to_string());
    }

    let (confirmed_erg, confirmed_tokens) = client
        .get_address_balances(&wallet.address)
        .await
        .str_err()?;

    // Effective = confirmed − mempool-spent + unconfirmed outputs. Enables the
    // UI to show (and immediately re-spend) 0-conf swap proceeds. Falls back to
    // confirmed-only inside get_effective_utxos if the mempool query fails.
    let effective_utxos = client
        .get_effective_utxos(&wallet.address)
        .await
        .str_err()?;
    let (erg_nano, tokens) = sum_eip12_utxos(&effective_utxos);

    let pending_erg_nano = erg_nano as i64 - confirmed_erg as i64;
    let confirmed_map: std::collections::HashMap<String, u64> =
        confirmed_tokens.into_iter().collect();

    let sigusd_token_id = sigmausd::constants::mainnet::SIGUSD_TOKEN_ID;
    let sigrsv_token_id = sigmausd::constants::mainnet::SIGRSV_TOKEN_ID;

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
        let pending_amount =
            amount as i64 - confirmed_map.get(&token_id).copied().unwrap_or(0) as i64;
        token_balances.push(TokenBalance {
            token_id,
            amount,
            amount_str: amount.to_string(),
            name,
            decimals,
            pending_amount,
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
        pending_erg_nano,
    })
}

/// Sum ERG and per-token totals over a set of EIP-12 boxes. Values that fail
/// to parse are skipped (they would also fail to spend).
fn sum_eip12_utxos(utxos: &[ergo_tx::Eip12InputBox]) -> (u64, Vec<(String, u64)>) {
    let mut erg_total: u64 = 0;
    let mut token_totals: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for utxo in utxos {
        erg_total += utxo.value.parse::<u64>().unwrap_or(0);
        for asset in &utxo.assets {
            if let Ok(amount) = asset.amount.parse::<u64>() {
                *token_totals.entry(asset.token_id.clone()).or_insert(0) += amount;
            }
        }
    }

    (erg_total, token_totals.into_iter().collect())
}

#[tauri::command]
pub async fn get_recent_transactions(
    state: State<'_, AppState>,
    limit: u64,
) -> Result<RecentTxsResponse, String> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    let client = state.require_node_client().await?;

    let caps = client.require_capabilities().await?;

    if caps.capability_tier == ergo_node_client::CapabilityTier::Basic {
        return Err("Transaction history requires extraIndex enabled on the node".to_string());
    }

    let address = &wallet.address;

    let raw_txs = client
        .get_recent_transactions(address, limit)
        .await
        .str_err()?;

    let sigusd_token_id = sigmausd::constants::mainnet::SIGUSD_TOKEN_ID;
    let sigrsv_token_id = sigmausd::constants::mainnet::SIGRSV_TOKEN_ID;

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

        let mut erg_in: i64 = 0;
        let mut erg_out: i64 = 0;
        let mut token_in: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut token_out: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();

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

        let mut all_token_ids: std::collections::HashSet<String> =
            token_in.keys().cloned().collect();
        all_token_ids.extend(token_out.keys().cloned());

        let mut token_changes: Vec<TokenChangeDto> = Vec::new();
        for tid in all_token_ids {
            let change = token_out.get(&tid).unwrap_or(&0) - token_in.get(&tid).unwrap_or(&0);
            if change == 0 {
                continue;
            }

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

/// Returns mempool-aware UTXOs (confirmed minus spent-in-mempool, plus unconfirmed change).
#[tauri::command]
pub async fn get_user_utxos(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let wallet = state
        .wallet()
        .await
        .ok_or_else(|| "No wallet connected".to_string())?;

    let client = state.require_node_client().await?;

    let utxos = client
        .get_effective_utxos(&wallet.address)
        .await
        .str_err()?;

    utxos
        .into_iter()
        .map(|u| serde_json::to_value(u).str_err())
        .collect()
}

#[tauri::command]
pub async fn validate_ergo_address(address: String) -> Result<String, String> {
    ergo_tx::address_to_ergo_tree(&address).str_err()
}

#[cfg(test)]
mod tests {
    use super::sum_eip12_utxos;
    use ergo_tx::{Eip12Asset, Eip12InputBox};
    use std::collections::HashMap;

    fn make_box(value: &str, assets: Vec<(&str, &str)>) -> Eip12InputBox {
        Eip12InputBox {
            box_id: "b".to_string(),
            transaction_id: "t".to_string(),
            index: 0,
            value: value.to_string(),
            ergo_tree: "0008cd".to_string(),
            assets: assets
                .into_iter()
                .map(|(id, amt)| Eip12Asset {
                    token_id: id.to_string(),
                    amount: amt.to_string(),
                })
                .collect(),
            creation_height: 1,
            additional_registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    #[test]
    fn sums_erg_and_tokens_across_boxes() {
        let utxos = vec![
            make_box("1000000000", vec![("tok_a", "5")]),
            make_box("500000000", vec![("tok_a", "3"), ("tok_b", "7")]),
        ];
        let (erg, tokens) = sum_eip12_utxos(&utxos);
        assert_eq!(erg, 1_500_000_000);
        let map: HashMap<String, u64> = tokens.into_iter().collect();
        assert_eq!(map["tok_a"], 8);
        assert_eq!(map["tok_b"], 7);
    }

    #[test]
    fn pending_delta_math() {
        // Mempool-only token: effective has it, confirmed doesn't -> positive pending.
        let effective = vec![make_box("900000000", vec![("tok_new", "42")])];
        let (erg, tokens) = sum_eip12_utxos(&effective);
        let confirmed_erg: u64 = 1_000_000_000; // spent 0.1 ERG in mempool
        let confirmed: HashMap<String, u64> = HashMap::new();

        let pending_erg = erg as i64 - confirmed_erg as i64;
        assert_eq!(pending_erg, -100_000_000);

        let map: HashMap<String, u64> = tokens.into_iter().collect();
        let pending_tok =
            map["tok_new"] as i64 - confirmed.get("tok_new").copied().unwrap_or(0) as i64;
        assert_eq!(pending_tok, 42);
    }

    #[test]
    fn unparseable_values_are_skipped() {
        let utxos = vec![make_box("not-a-number", vec![("tok_a", "bad")])];
        let (erg, tokens) = sum_eip12_utxos(&utxos);
        assert_eq!(erg, 0);
        assert!(tokens.is_empty());
    }
}
