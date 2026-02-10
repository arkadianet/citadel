//! Application state shared across API handlers

use std::sync::Arc;
use std::time::Instant;

use citadel_core::{AppConfig, Network, NodeConfig};
use ergo_node_client::NodeClient;
use ergopay_server::ErgoPayServer;
use thiserror::Error;
use tokio::sync::RwLock;

/// Errors that can occur in the API layer
#[derive(Debug, Error)]
pub enum ApiError {
    /// Invalid wallet address format
    #[error("Invalid wallet address: {reason}")]
    InvalidAddress { reason: String },

    /// ErgoPay server error
    #[error("ErgoPay server error: {0}")]
    ErgoPayServer(#[from] std::io::Error),
}

/// State representing a connected wallet.
///
/// The address is stored as a P2PK address in standard Ergo mainnet format.
/// This is a Base58-encoded string starting with '9'.
#[derive(Clone, Debug)]
pub struct WalletState {
    /// The wallet's P2PK address in standard Ergo Base58 format (starts with '9').
    pub address: String,
    /// When the wallet was connected
    pub connected_at: Instant,
}

impl WalletState {
    /// Create a new wallet state with the given P2PK address.
    ///
    /// # Arguments
    /// * `address` - A valid Ergo P2PK address in Base58 format
    pub fn new(address: String) -> Self {
        Self {
            address,
            connected_at: Instant::now(),
        }
    }
}

/// Validate that an address is a valid Ergo P2PK address format.
///
/// This performs basic format validation:
/// - Mainnet P2PK addresses start with '9' and are typically 51 characters
/// - Must be at least 40 characters (Base58 encoding of address bytes)
///
/// Note: This does not perform full cryptographic validation of the address.
/// Full validation would require the ergo-lib crate.
fn validate_p2pk_address(address: &str) -> Result<(), ApiError> {
    let len = address.len();

    // Check minimum length for Base58 encoded address
    if len < 40 {
        return Err(ApiError::InvalidAddress {
            reason: format!("Address too short ({} chars, minimum 40)", len),
        });
    }

    // Check first character for mainnet P2PK prefix
    if !address.starts_with('9') {
        return Err(ApiError::InvalidAddress {
            reason: "Invalid address prefix. Mainnet P2PK addresses must start with '9'"
                .to_string(),
        });
    }

    // Check for valid Base58 characters (no 0, O, I, l)
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

/// Shared application state
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
    /// Create a new application state with default config
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

    /// Create with a specific config
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

    /// Get current config
    pub async fn config(&self) -> AppConfig {
        self.inner.config.read().await.clone()
    }

    /// Update node configuration
    pub async fn set_node_config(&self, node_config: NodeConfig) {
        let mut config = self.inner.config.write().await;
        config.node = node_config;

        // Clear cached node client
        let mut client = self.inner.node_client.write().await;
        *client = None;
    }

    /// Get or create node client
    pub async fn node_client(&self) -> Option<NodeClient> {
        // Check if we have a cached client
        {
            let client = self.inner.node_client.read().await;
            if client.is_some() {
                return client.clone();
            }
        }

        // Create new client
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

    /// Force refresh node client
    pub async fn refresh_node_client(&self) -> Option<NodeClient> {
        let mut client = self.inner.node_client.write().await;
        *client = None;
        drop(client);

        self.node_client().await
    }

    /// Get current network
    pub async fn network(&self) -> Network {
        self.inner.config.read().await.network
    }

    /// Get current wallet state
    pub async fn wallet(&self) -> Option<WalletState> {
        self.inner.wallet.read().await.clone()
    }

    /// Set connected wallet with address validation.
    ///
    /// # Arguments
    /// * `address` - A valid Ergo P2PK address in Base58 format
    ///
    /// # Errors
    /// Returns `ApiError::InvalidAddress` if the address format is invalid.
    pub async fn set_wallet(&self, address: String) -> Result<(), ApiError> {
        validate_p2pk_address(&address)?;
        let mut wallet = self.inner.wallet.write().await;
        *wallet = Some(WalletState::new(address));
        Ok(())
    }

    /// Disconnect wallet (clear wallet state)
    pub async fn disconnect_wallet(&self) {
        let mut wallet = self.inner.wallet.write().await;
        *wallet = None;
    }

    /// Get or start the ErgoPay server.
    ///
    /// # Errors
    /// Returns `ApiError::ErgoPayServer` if the server fails to start.
    pub async fn ergopay_server(&self) -> Result<Arc<ErgoPayServer>, ApiError> {
        // Check if we already have a running server
        {
            let server = self.inner.ergopay_server.read().await;
            if let Some(ref s) = *server {
                return Ok(s.clone());
            }
        }

        // Start new server
        let mut server_lock = self.inner.ergopay_server.write().await;

        // Double-check after acquiring write lock
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
