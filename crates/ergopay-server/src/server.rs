//! Axum HTTP server for ErgoPay

use axum::{routing::get, routing::post, Router};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

use crate::handlers::{
    handle_callback, handle_connect, handle_nautilus_connect_page, handle_nautilus_page,
    handle_nautilus_tx, handle_tx,
};
use crate::types::{PendingRequest, RequestStatus};

/// Shared server state
pub struct ServerState {
    /// Port the server is running on
    pub port: u16,
    /// Host IP address for URLs (LAN IP)
    pub host: String,
    /// Pending requests by ID
    pub pending_requests: RwLock<HashMap<String, PendingRequest>>,
}

/// ErgoPay HTTP server
pub struct ErgoPayServer {
    state: Arc<ServerState>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ErgoPayServer {
    /// Start the server on an available port
    pub async fn start() -> Result<Self, std::io::Error> {
        Self::start_on_port(0).await
    }

    /// Start the server on a specific port (0 for auto-assign)
    pub async fn start_on_port(port: u16) -> Result<Self, std::io::Error> {
        // Bind to all interfaces so LAN devices can connect
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let actual_port = listener.local_addr()?.port();

        // Get LAN IP for URLs
        let host = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

        // Create state with actual port and host
        let state = Arc::new(ServerState {
            port: actual_port,
            host,
            pending_requests: RwLock::new(HashMap::new()),
        });

        // Build router with correct state
        let app = Router::new()
            .route("/connect/:id", get(handle_connect))
            .route("/tx/:id", get(handle_tx))
            .route("/callback/:id", post(handle_callback))
            .route("/nautilus/sign/:id", get(handle_nautilus_page))
            .route("/nautilus/connect/:id", get(handle_nautilus_connect_page))
            .route("/nautilus/tx/:id", get(handle_nautilus_tx))
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
            .with_state(state.clone());

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Spawn server task
        tokio::spawn(async move {
            tracing::info!("ErgoPay server starting on port {}", actual_port);

            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                    tracing::info!("ErgoPay server shutting down");
                })
                .await
                .ok();
        });

        // Spawn cleanup task
        let cleanup_state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                let mut requests = cleanup_state.pending_requests.write().await;
                requests.retain(|id, req| {
                    let expired = req.is_expired();
                    if expired {
                        tracing::debug!("Cleaning up expired request: {}", id);
                    }
                    !expired
                });
            }
        });

        Ok(Self {
            state,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Get the port the server is running on
    pub fn port(&self) -> u16 {
        self.state.port
    }

    /// Get the host address (LAN IP) the server is using
    pub fn host(&self) -> &str {
        &self.state.host
    }

    /// Create a new wallet connect request
    pub async fn create_connect_request(&self) -> (String, String) {
        let id = generate_request_id();
        let request = PendingRequest::new_connect(id.clone());

        let mut requests = self.state.pending_requests.write().await;
        requests.insert(id.clone(), request);

        let url = format!(
            "ergopay://{}:{}/connect/{}?address=#P2PK_ADDRESS#",
            self.state.host, self.state.port, id
        );

        (id, url)
    }

    /// Create a new transaction signing request
    pub async fn create_tx_request(
        &self,
        reduced_tx: Vec<u8>,
        unsigned_tx: serde_json::Value,
        message: String,
    ) -> (String, String) {
        let id = generate_request_id();
        let request = PendingRequest::new_sign_tx(id.clone(), reduced_tx, unsigned_tx, message);

        let mut requests = self.state.pending_requests.write().await;
        requests.insert(id.clone(), request);

        let url = format!(
            "ergopay://{}:{}/tx/{}",
            self.state.host, self.state.port, id
        );

        (id, url)
    }

    /// Get the Nautilus signing page URL for a request
    pub fn get_nautilus_url(&self, request_id: &str) -> String {
        format!(
            "http://{}:{}/nautilus/sign/{}",
            self.state.host, self.state.port, request_id
        )
    }

    /// Get the Nautilus connect page URL for a request
    pub fn get_nautilus_connect_url(&self, request_id: &str) -> String {
        format!(
            "http://{}:{}/nautilus/connect/{}",
            self.state.host, self.state.port, request_id
        )
    }

    /// Get the status of a request
    pub async fn get_request_status(&self, request_id: &str) -> Option<RequestStatus> {
        let requests = self.state.pending_requests.read().await;
        requests.get(request_id).map(|r| r.status.clone())
    }

    /// Cancel a pending request
    pub async fn cancel_request(&self, request_id: &str) {
        let mut requests = self.state.pending_requests.write().await;
        requests.remove(request_id);
    }
}

impl Drop for ErgoPayServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Generate a random request ID
fn generate_request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    // Simple ID: timestamp + random suffix
    let random: u32 = rand::random();
    format!("{:x}{:08x}", timestamp, random)
}

/// Get the local LAN IP address
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;

    // Create a UDP socket and "connect" to a public IP
    // This doesn't actually send data, but lets us find which local IP would be used
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let local_addr = socket.local_addr().ok()?;

    Some(local_addr.ip().to_string())
}
