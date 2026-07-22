use citadel_api::services::mewlock as mewlock_svc;
use citadel_api::AppState;
use tauri::State;

#[tauri::command]
pub async fn mewlock_fetch_state(
    state: State<'_, AppState>,
    user_address: Option<String>,
) -> Result<mewlock::MewLockState, String> {
    mewlock_svc::fetch_state(&state, user_address.as_deref()).await
}

#[tauri::command]
pub async fn mewlock_get_durations() -> Result<serde_json::Value, String> {
    Ok(serde_json::Value::Array(mewlock_svc::get_durations()))
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

    let lock_tokens: Vec<(String, u64)> = if lock_tokens_json.is_empty() || lock_tokens_json == "[]"
    {
        vec![]
    } else {
        serde_json::from_str(&lock_tokens_json)
            .map_err(|e| format!("Invalid lock tokens JSON: {}", e))?
    };

    let timestamp: Option<i64> = timestamp.and_then(|ts| ts.parse().ok());

    let tx = mewlock_svc::build_lock(
        user_ergo_tree,
        lock_erg,
        lock_tokens,
        unlock_height,
        timestamp,
        lock_name,
        lock_description,
        parsed_utxos,
        current_height,
    )?;
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
    let parsed_utxos = super::parse_eip12_utxos(user_utxos)?;

    let tx = mewlock_svc::build_unlock(
        &state,
        &box_id,
        user_ergo_tree,
        parsed_utxos,
        current_height,
    )
    .await?;
    serde_json::to_value(&tx).map_err(|e| format!("Failed to serialize tx: {}", e))
}
