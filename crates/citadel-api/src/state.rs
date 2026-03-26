use std::sync::Arc;
use std::time::Instant;

use citadel_core::{AppConfig, Network, NodeConfig};
use ergo_node_client::NodeClient;
use ergopay_server::ErgoPayServer;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Invalid wallet address: {reason}")]
    InvalidAddress { reason: String },

    #[error("ErgoPay server error: {0}")]
    ErgoPayServer(#[from] std::io::Error),
}

#[derive(Clone, Debug)]
pub struct WalletState {
    pub address: String,
    pub connected_at: Instant,
}

impl WalletState {
    pub fn new(address: String) -> Self {
        Self {
            address,
            connected_at: Instant::now(),
        }
    }
}

fn validate_p2pk_address(address: &str) -> Result<(), ApiError> {
    let len = address.len();

    if len < 40 {
        return Err(ApiError::InvalidAddress {
            reason: format!("Address too short ({} chars, minimum 40)", len),
        });
    }

    if !address.starts_with('9') {
        return Err(ApiError::InvalidAddress {
            reason: "Invalid address prefix. Mainnet P2PK addresses must start with '9'"
                .to_string(),
        });
    }

    for c in address.chars() {
        if c == '0' || c == 'O' || c == 'I' || c == 'l' {
            return Err(ApiError::InvalidAddress {
                reason: format!("Invalid Base58 character '{}' in address", c),
            });
        }
        if !c.is_ascii_alphanumeric() {
            return Err(ApiError::InvalidAddress {
                reason: format!("Invalid character '{}' in address", c),
            });
        }
    }

    Ok(())
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: RwLock<AppConfig>,
    node_client: RwLock<Option<NodeClient>>,
    wallet: RwLock<Option<WalletState>>,
    ergopay_server: RwLock<Option<Arc<ErgoPayServer>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                config: RwLock::new(AppConfig::default()),
                node_client: RwLock::new(None),
                wallet: RwLock::new(None),
                ergopay_server: RwLock::new(None),
            }),
        }
    }

    pub fn with_config(config: AppConfig) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                config: RwLock::new(config),
                node_client: RwLock::new(None),
                wallet: RwLock::new(None),
                ergopay_server: RwLock::new(None),
            }),
        }
    }

    pub async fn config(&self) -> AppConfig {
        self.inner.config.read().await.clone()
    }

    pub async fn set_node_config(&self, node_config: NodeConfig) {
        let mut config = self.inner.config.write().await;
        config.node = node_config;

        let mut client = self.inner.node_client.write().await;
        *client = None;
    }

    pub async fn node_client(&self) -> Option<NodeClient> {
        {
            let client = self.inner.node_client.read().await;
            if client.is_some() {
                return client.clone();
            }
        }

        let config = self.inner.config.read().await;
        tracing::info!("Creating node client for URL: {}", config.node.url);
        match NodeClient::new(config.node.clone()).await {
            Ok(client) => {
                tracing::info!("Node client created successfully");
                let mut cached = self.inner.node_client.write().await;
                *cached = Some(client.clone());
                Some(client)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create node client for {}: {}",
                    config.node.url,
                    e
                );
                None
            }
        }
    }

    pub async fn require_node_client(&self) -> Result<NodeClient, String> {
        self.node_client()
            .await
            .ok_or_else(|| "Node not connected".to_string())
    }

    pub async fn refresh_node_client(&self) -> Option<NodeClient> {
        let mut client = self.inner.node_client.write().await;
        *client = None;
        drop(client);

        self.node_client().await
    }

    pub async fn network(&self) -> Network {
        self.inner.config.read().await.network
    }

    pub async fn wallet(&self) -> Option<WalletState> {
        self.inner.wallet.read().await.clone()
    }

    pub async fn set_wallet(&self, address: String) -> Result<(), ApiError> {
        validate_p2pk_address(&address)?;
        let mut wallet = self.inner.wallet.write().await;
        *wallet = Some(WalletState::new(address));
        Ok(())
    }

    pub async fn disconnect_wallet(&self) {
        let mut wallet = self.inner.wallet.write().await;
        *wallet = None;
    }

    pub async fn ergopay_server(&self) -> Result<Arc<ErgoPayServer>, ApiError> {
        {
            let server = self.inner.ergopay_server.read().await;
            if let Some(ref s) = *server {
                return Ok(s.clone());
            }
        }

        let mut server_lock = self.inner.ergopay_server.write().await;
        if let Some(ref s) = *server_lock {
            return Ok(s.clone());
        }

        let server = ErgoPayServer::start().await.map_err(|e| {
            tracing::error!("Failed to start ErgoPay server: {}", e);
            e
        })?;

        tracing::info!("ErgoPay server started on port {}", server.port());
        let server = Arc::new(server);
        *server_lock = Some(server.clone());
        Ok(server)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
