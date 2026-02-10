use citadel_api::dto::{MintSignRequest, MintSignResponse, MintTxStatusResponse};
use citadel_api::AppState;
use tauri::State;

/// Fetch MewLock timelock state (all locks on chain)
#[tauri::command]
pub async fn mewlock_fetch_state(
    state: State<'_, AppState>,
    user_address: Option<String>,
) -> Result<mewlock::MewLockState, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let height = client.current_height().await.map_err(|e| e.to_string())?;

    mewlock::fetch_mewlock_state(&client, user_address.as_deref(), height as u32)
        .await
        .map_err(|e| e.to_string())
}

/// Get available lock duration presets
#[tauri::command]
pub async fn mewlock_get_durations() -> Result<serde_json::Value, String> {
    let durations: Vec<_> = mewlock::DURATION_PRESETS
        .iter()
        .map(|(label, blocks)| {
            serde_json::json!({
                "label": label,
                "blocks": blocks,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(durations))
}

/// Build a lock transaction
#[tauri::command]
pub async fn mewlock_build_lock(
    user_ergo_tree: String,
    lock_erg: String,
    lock_tokens_json: String,
    unlock_height: i32,
    timestamp: Option<String>,
    lock_name: Option<String>,
    lock_description: Option<String>,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let lock_erg: u64 = lock_erg
        .parse()
        .map_err(|_| "Invalid lock ERG amount".to_string())?;

    let lock_tokens: Vec<(String, u64)> =
        if lock_tokens_json.is_empty() || lock_tokens_json == "[]" {
            vec![]
        } else {
            serde_json::from_str(&lock_tokens_json)
                .map_err(|e| format!("Invalid lock tokens JSON: {}", e))?
        };

    let timestamp: Option<i64> = timestamp
        .and_then(|ts| ts.parse().ok());

    let req = mewlock::tx_builder::LockRequest {
        user_ergo_tree,
        lock_erg,
        lock_tokens,
        unlock_height,
        timestamp,
        lock_name,
        lock_description,
        user_inputs: parsed_utxos,
        current_height,
    };

    let tx = mewlock::tx_builder::build_lock_tx(&req).map_err(|e| e.to_string())?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

/// Build an unlock transaction
#[tauri::command]
pub async fn mewlock_build_unlock(
    lock_box_json: String,
    user_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let lock_box: ergo_tx::Eip12InputBox =
        serde_json::from_str(&lock_box_json).map_err(|e| format!("Invalid lock box: {}", e))?;
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let req = mewlock::tx_builder::UnlockRequest {
        lock_box,
        user_ergo_tree,
        user_inputs: parsed_utxos,
        current_height,
    };

    let tx = mewlock::tx_builder::build_unlock_tx(&req).map_err(|e| e.to_string())?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

/// Start signing a MewLock transaction (reuses ErgoPay sign flow)
#[tauri::command]
pub async fn start_mewlock_sign(
    state: State<'_, AppState>,
    unsigned_tx: serde_json::Value,
    message: Option<String>,
) -> Result<MintSignResponse, String> {
    super::start_mint_sign(
        state,
        MintSignRequest {
            unsigned_tx,
            message: message.unwrap_or_else(|| "MewLock transaction".to_string()),
        },
    )
    .await
}

/// Get MewLock transaction signing status
#[tauri::command]
pub async fn get_mewlock_tx_status(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<MintTxStatusResponse, String> {
    super::get_mint_tx_status(state, request_id).await
}
