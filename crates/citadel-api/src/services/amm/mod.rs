//! Spectrum AMM use-case orchestration: pool discovery/quotes, swaps, LP,
//! multi-hop router, and pre-built arb/swap chains.
//!
//! Split into private submodules for readability; the public surface is the
//! flat `services::amm` API consumed by the Tauri command wrappers.

pub mod arb;
pub mod lp;
pub mod quote;
pub mod router;
pub mod swap;

pub use arb::*;
pub use lp::*;
pub use quote::*;
pub use router::*;
pub use swap::*;

// amm-crate response types surfaced through the façade so Tauri command
// wrappers stay free of direct `amm::` references.
pub use amm::{CircularArbSnapshot, DepthTiers, OracleArbSnapshot};

use crate::services::error::IntoServiceError;
use ergo_node_client::NodeClient;

pub(crate) async fn find_pool(client: &NodeClient, pool_id: &str) -> Result<amm::AmmPool, String> {
    amm::discover_pools(client)
        .await
        .into_service()?
        .into_iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or_else(|| format!("Pool not found: {}", pool_id))
}

pub(crate) fn parse_swap_input(
    input_type: &str,
    amount: u64,
    token_id: Option<String>,
) -> Result<amm::SwapInput, String> {
    match input_type {
        "erg" => Ok(amm::SwapInput::Erg { amount }),
        "token" => Ok(amm::SwapInput::Token {
            token_id: token_id.ok_or("token_id required for token input")?,
            amount,
        }),
        _ => Err("Invalid input_type. Use 'erg' or 'token'".to_string()),
    }
}
