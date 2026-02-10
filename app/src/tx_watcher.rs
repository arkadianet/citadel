//! Background transaction watcher
//!
//! Polls the Ergo node for transaction confirmations and order fills,
//! emitting Tauri events when transactions resolve. The frontend handles
//! both in-app toasts and OS notifications via the notification plugin.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use citadel_api::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// How often the background task polls the node (seconds).
const POLL_INTERVAL_SECS: u64 = 30;

/// Items older than this are timed out and removed (seconds).
const TIMEOUT_SECS: u64 = 40 * 60; // 40 minutes

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
enum WatchKind {
    TxConfirmation,
    OrderFill { box_id: String },
}

struct WatchItem {
    id: String,
    kind: WatchKind,
    tx_id: String,
    protocol: String,
    operation: String,
    description: String,
    submitted_at: Instant,
}

#[derive(Serialize, Clone)]
pub struct TxNotification {
    pub id: String,
    /// "confirmed" | "filled" | "dropped" | "timeout"
    pub kind: String,
    pub protocol: String,
    pub operation: String,
    pub description: String,
    pub tx_id: Option<String>,
    pub timestamp: u64,
}

#[derive(Serialize, Clone)]
pub struct WatchedItemInfo {
    pub id: String,
    pub tx_id: String,
    pub protocol: String,
    pub operation: String,
    pub description: String,
    pub kind: String,
    pub elapsed_secs: u64,
}

// ─── TxWatcher ───────────────────────────────────────────────────────────────

struct TxWatcher {
    items: Vec<WatchItem>,
}

impl TxWatcher {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn add_tx(
        &mut self,
        tx_id: String,
        protocol: String,
        operation: String,
        description: String,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.items.push(WatchItem {
            id: id.clone(),
            kind: WatchKind::TxConfirmation,
            tx_id,
            protocol,
            operation,
            description,
            submitted_at: Instant::now(),
        });
        id
    }

    fn add_order(
        &mut self,
        box_id: String,
        tx_id: String,
        protocol: String,
        description: String,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.items.push(WatchItem {
            id: id.clone(),
            kind: WatchKind::OrderFill { box_id },
            tx_id,
            protocol,
            operation: "order_fill".to_string(),
            description,
            submitted_at: Instant::now(),
        });
        id
    }

    fn watched_items(&self) -> Vec<WatchedItemInfo> {
        self.items
            .iter()
            .map(|item| WatchedItemInfo {
                id: item.id.clone(),
                tx_id: item.tx_id.clone(),
                protocol: item.protocol.clone(),
                operation: item.operation.clone(),
                description: item.description.clone(),
                kind: match &item.kind {
                    WatchKind::TxConfirmation => "tx".to_string(),
                    WatchKind::OrderFill { .. } => "order".to_string(),
                },
                elapsed_secs: item.submitted_at.elapsed().as_secs(),
            })
            .collect()
    }

    async fn poll(&mut self, state: &AppState, app_handle: &AppHandle) {
        let client = match state.node_client().await {
            Some(c) => c,
            None => return,
        };

        let mut resolved_ids: Vec<String> = Vec::new();

        for item in &self.items {
            // Check timeout first
            if item.submitted_at.elapsed().as_secs() > TIMEOUT_SECS {
                emit_notification(app_handle, &make_notification(item, "timeout"));
                resolved_ids.push(item.id.clone());
                continue;
            }

            match &item.kind {
                WatchKind::TxConfirmation => {
                    match client.get_transaction_by_id(&item.tx_id).await {
                        Ok(json) => {
                            // extraIndex returns mempool txs with numConfirmations: 0.
                            // Only declare confirmed once actually in a block.
                            let confs = json
                                .get("numConfirmations")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            if confs >= 1 {
                                emit_notification(
                                    app_handle,
                                    &make_notification(item, "confirmed"),
                                );
                                resolved_ids.push(item.id.clone());
                            }
                        }
                        Err(_) => {
                            // Not in index — check if still in mempool
                            if client
                                .get_unconfirmed_transaction_by_id(&item.tx_id)
                                .await
                                .is_err()
                            {
                                // Not in mempool and not in index → dropped
                                emit_notification(app_handle, &make_notification(item, "dropped"));
                                resolved_ids.push(item.id.clone());
                            }
                        }
                    }
                }
                WatchKind::OrderFill { box_id } => {
                    if let Ok(json) = client.get_blockchain_box_by_id(box_id).await {
                        if json
                            .get("spentTransactionId")
                            .and_then(|v| v.as_str())
                            .is_some()
                        {
                            emit_notification(app_handle, &make_notification(item, "filled"));
                            resolved_ids.push(item.id.clone());
                        }
                    }
                }
            }
        }

        self.items.retain(|item| !resolved_ids.contains(&item.id));
    }
}

fn make_notification(item: &WatchItem, kind: &str) -> TxNotification {
    TxNotification {
        id: item.id.clone(),
        kind: kind.to_string(),
        protocol: item.protocol.clone(),
        operation: item.operation.clone(),
        description: item.description.clone(),
        tx_id: Some(item.tx_id.clone()),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    }
}

fn emit_notification(app_handle: &AppHandle, notif: &TxNotification) {
    if let Err(e) = app_handle.emit("tx-notification", notif.clone()) {
        tracing::warn!("Failed to emit tx-notification event: {}", e);
    }
}

// ─── Managed state ───────────────────────────────────────────────────────────

pub struct TxWatcherState {
    watcher: tokio::sync::Mutex<TxWatcher>,
    polling: Arc<AtomicBool>,
}

impl Default for TxWatcherState {
    fn default() -> Self {
        Self {
            watcher: tokio::sync::Mutex::new(TxWatcher::new()),
            polling: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl TxWatcherState {
    pub fn new() -> Self {
        Self::default()
    }
}

fn ensure_poll_loop(watcher_state: &State<'_, TxWatcherState>, app_handle: AppHandle) {
    if watcher_state.polling.swap(true, Ordering::SeqCst) {
        return; // Already running
    }

    let polling = watcher_state.polling.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

            let watcher_state = app_handle.state::<TxWatcherState>();
            let app_state = app_handle.state::<AppState>();

            let mut watcher = watcher_state.watcher.lock().await;
            if watcher.items.is_empty() {
                drop(watcher);
                polling.store(false, Ordering::SeqCst);
                break;
            }
            watcher.poll(&app_state, &app_handle).await;
        }

        tracing::debug!("TxWatcher poll loop stopped (no items)");
    });
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn watch_tx(
    watcher_state: State<'_, TxWatcherState>,
    app_handle: AppHandle,
    tx_id: String,
    protocol: String,
    operation: String,
    description: String,
) -> Result<String, String> {
    let id = {
        let mut watcher = watcher_state.watcher.lock().await;
        watcher.add_tx(tx_id, protocol, operation, description)
    };
    ensure_poll_loop(&watcher_state, app_handle);
    Ok(id)
}

#[tauri::command]
pub async fn watch_order(
    watcher_state: State<'_, TxWatcherState>,
    app_handle: AppHandle,
    box_id: String,
    tx_id: String,
    protocol: String,
    description: String,
) -> Result<String, String> {
    let id = {
        let mut watcher = watcher_state.watcher.lock().await;
        watcher.add_order(box_id, tx_id, protocol, description)
    };
    ensure_poll_loop(&watcher_state, app_handle);
    Ok(id)
}

#[tauri::command]
pub async fn get_watched_items(
    watcher_state: State<'_, TxWatcherState>,
) -> Result<Vec<WatchedItemInfo>, String> {
    let watcher = watcher_state.watcher.lock().await;
    Ok(watcher.watched_items())
}
