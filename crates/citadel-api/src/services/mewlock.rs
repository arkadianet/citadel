//! MewLock use-case orchestration: state fetch, duration presets, lock/unlock tx building.

use super::error::{IntoServiceError, ServiceResult};
use crate::AppState;

pub async fn fetch_state(
    state: &AppState,
    user_address: Option<&str>,
) -> ServiceResult<mewlock::MewLockState> {
    let client = state.require_node_client().await?;
    let height = client.current_height().await.into_service()?;

    mewlock::fetch_mewlock_state(&client, user_address, height as u32)
        .await
        .into_service()
}

pub fn get_durations() -> Vec<serde_json::Value> {
    mewlock::DURATION_PRESETS
        .iter()
        .map(|(label, blocks)| {
            serde_json::json!({
                "label": label,
                "blocks": blocks,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn build_lock(
    user_ergo_tree: String,
    lock_erg: u64,
    lock_tokens: Vec<(String, u64)>,
    unlock_height: i32,
    timestamp: Option<i64>,
    lock_name: Option<String>,
    lock_description: Option<String>,
    user_inputs: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    let req = mewlock::tx_builder::LockRequest {
        user_ergo_tree,
        lock_erg,
        lock_tokens,
        unlock_height,
        timestamp,
        lock_name,
        lock_description,
        user_inputs,
        current_height,
    };

    mewlock::build_lock_tx(&req).into_service()
}

pub async fn build_unlock(
    state: &AppState,
    box_id: &str,
    user_ergo_tree: String,
    user_inputs: Vec<ergo_tx::Eip12InputBox>,
    current_height: i32,
) -> ServiceResult<ergo_tx::Eip12UnsignedTx> {
    let client = state.require_node_client().await?;
    let lock_box = client
        .get_eip12_box_by_id(box_id)
        .await
        .map_err(|e| format!("Failed to fetch lock box: {}", e))?;

    let req = mewlock::tx_builder::UnlockRequest {
        lock_box,
        user_ergo_tree,
        user_inputs,
        current_height,
    };

    mewlock::build_unlock_tx(&req).into_service()
}
