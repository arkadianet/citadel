use citadel_api::AppState;
use tauri::State;

use super::StrErr;

#[tauri::command]
pub async fn mewlock_fetch_state(
    state: State<'_, AppState>,
    user_address: Option<String>,
) -> Result<mewlock::MewLockState, String> {
    let client = state.require_node_client().await?;
    let height = client.current_height().await.str_err()?;

    mewlock::fetch_mewlock_state(&client, user_address.as_deref(), height as u32)
        .await
        .str_err()
}

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

    let tx = mewlock::tx_builder::build_lock_tx(&req).str_err()?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

#[tauri::command]
pub async fn mewlock_build_unlock(
    state: State<'_, AppState>,
    box_id: String,
    user_ergo_tree: String,
    user_utxos: Vec<serde_json::Value>,
    current_height: i32,
) -> Result<serde_json::Value, String> {
    let client = state.require_node_client().await?;
    let lock_box = client
        .get_eip12_box_by_id(&box_id)
        .await
        .map_err(|e| format!("Failed to fetch lock box: {}", e))?;
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let req = mewlock::tx_builder::UnlockRequest {
        lock_box,
        user_ergo_tree,
        user_inputs: parsed_utxos,
        current_height,
    };

    let tx = mewlock::tx_builder::build_unlock_tx(&req).str_err()?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}

