//! Citadel application developer fee (0.011 ERG).
//!
//! Appended by allowlisted builders as an ERG-only P2PK output immediately
//! before the miner-fee output. Funded from user wallet inputs only — never
//! skimmed from protocol/script boxes.
//!
//! ## Config
//! - Default: enabled with [`DEFAULT_DEV_FEE_ADDRESS`].
//! - `CITADEL_DEV_FEE_ADDRESS` — override recipient (mainnet P2PK).
//! - `CITADEL_DEV_FEE_ENABLED=false` — disable fee entirely.
//!
//! ## Skipped builders (do not call [`append_dev_fee_output`])
//! - Stake recovery / Paideia paths that pin `OUTPUTS.size`
//! - AMM LP deposit/redeem / pool-setup / refund (not swap funding)
//! - HodlCoin / MewLock / SigmaFi / lending (phase 3; protocol fees or layout pins)
//! - Prefer skip when unsure rather than risk script failure

use crate::eip12::Eip12Output;
use citadel_core::constants::DEV_FEE_NANO;

#[cfg(not(test))]
use std::sync::OnceLock;

/// Hardcoded mainnet P2PK for Citadel app fee (user-requested default).
pub const DEFAULT_DEV_FEE_ADDRESS: &str =
    "9eoLQ6FFKJPqZXeBFvd3CKu7DRfXavKo7n9PFkVypSmXgD6ActU";

/// ErgoTree hex for [`DEFAULT_DEV_FEE_ADDRESS`] (P2PK `0008cd…`).
pub const DEFAULT_DEV_FEE_ERGO_TREE: &str =
    "0008cd0224f3a8909d624e7c584f215956370278324c9b3bfc206a4605a27c952121e68c";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevFeeConfig {
    pub enabled: bool,
    pub recipient_ergo_tree: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DevFeeError {
    #[error("Invalid CITADEL_DEV_FEE_ADDRESS: {0}")]
    InvalidAddress(String),
}

impl DevFeeConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            recipient_ergo_tree: String::new(),
        }
    }

    pub fn enabled_default() -> Self {
        Self {
            enabled: true,
            recipient_ergo_tree: DEFAULT_DEV_FEE_ERGO_TREE.to_string(),
        }
    }

    /// nanoERG to budget when selecting inputs / computing change (0 if off).
    pub fn budget(&self) -> i64 {
        if self.enabled {
            DEV_FEE_NANO
        } else {
            0
        }
    }
}

/// Budget helper matching the design sketch.
pub fn dev_fee_budget(cfg: &DevFeeConfig) -> u64 {
    cfg.budget() as u64
}

/// Push the Citadel fee output when enabled. Call immediately before miner fee.
pub fn append_dev_fee_output(
    outputs: &mut Vec<Eip12Output>,
    cfg: &DevFeeConfig,
    height: i32,
) -> Result<(), DevFeeError> {
    if !cfg.enabled {
        return Ok(());
    }
    if cfg.recipient_ergo_tree.is_empty() {
        return Err(DevFeeError::InvalidAddress(
            "enabled but recipient ErgoTree is empty".to_string(),
        ));
    }
    outputs.push(Eip12Output::simple(
        DEV_FEE_NANO,
        cfg.recipient_ergo_tree.clone(),
        height,
    ));
    Ok(())
}

/// Resolve fee config (env override + hardcoded default). Cached after first call.
///
/// [`with_test_dev_fee`] can override for the current thread (used by unit tests
/// in this crate and dependent protocol crates).
pub fn resolved_config() -> DevFeeConfig {
    if let Some(cfg) = TEST_OVERRIDE.with(|cell| cell.borrow().clone()) {
        return cfg;
    }
    #[cfg(test)]
    {
        // No override → disabled in ergo-tx's own unit tests
        return DevFeeConfig::disabled();
    }
    #[cfg(not(test))]
    {
        CONFIG.get_or_init(load_from_env_or_default).clone()
    }
}

#[cfg(not(test))]
static CONFIG: OnceLock<DevFeeConfig> = OnceLock::new();

thread_local! {
    static TEST_OVERRIDE: std::cell::RefCell<Option<DevFeeConfig>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with a temporary fee config (for unit tests in this crate or dependents).
pub fn with_test_dev_fee<R>(cfg: DevFeeConfig, f: impl FnOnce() -> R) -> R {
    TEST_OVERRIDE.with(|cell| {
        let prev = cell.replace(Some(cfg));
        let result = f();
        cell.replace(prev);
        result
    })
}

fn load_from_env_or_default() -> DevFeeConfig {
    if env_flag_false("CITADEL_DEV_FEE_ENABLED") {
        return DevFeeConfig::disabled();
    }

    let address = std::env::var("CITADEL_DEV_FEE_ADDRESS")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    match address {
        None => DevFeeConfig::enabled_default(),
        Some(addr) if addr == DEFAULT_DEV_FEE_ADDRESS => DevFeeConfig::enabled_default(),
        Some(addr) => match address_to_tree(&addr) {
            Ok(tree) => DevFeeConfig {
                enabled: true,
                recipient_ergo_tree: tree,
            },
            Err(e) => {
                // Fail closed on bad override: keep default rather than panic in builders.
                // Callers that need strict validation can use [`try_load_from_env`].
                tracing_warn_invalid(&addr, &e);
                DevFeeConfig::enabled_default()
            }
        },
    }
}

/// Strict load for startup / diagnostics — invalid override address is an error.
pub fn try_load_from_env() -> Result<DevFeeConfig, DevFeeError> {
    if env_flag_false("CITADEL_DEV_FEE_ENABLED") {
        return Ok(DevFeeConfig::disabled());
    }

    let address = std::env::var("CITADEL_DEV_FEE_ADDRESS")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_DEV_FEE_ADDRESS.to_string());

    if address == DEFAULT_DEV_FEE_ADDRESS {
        return Ok(DevFeeConfig::enabled_default());
    }

    let tree = address_to_tree(&address)?;
    Ok(DevFeeConfig {
        enabled: true,
        recipient_ergo_tree: tree,
    })
}

fn env_flag_false(key: &str) -> bool {
    matches!(
        std::env::var(key).ok().as_deref().map(str::trim),
        Some("0" | "false" | "False" | "FALSE" | "no" | "off")
    )
}

fn address_to_tree(address: &str) -> Result<String, DevFeeError> {
    #[cfg(feature = "ergo-lib")]
    {
        crate::address_to_ergo_tree(address).map_err(|e| DevFeeError::InvalidAddress(e.to_string()))
    }
    #[cfg(not(feature = "ergo-lib"))]
    {
        let _ = address;
        Err(DevFeeError::InvalidAddress(
            "ergo-lib feature required to resolve CITADEL_DEV_FEE_ADDRESS override".to_string(),
        ))
    }
}

fn tracing_warn_invalid(addr: &str, err: &DevFeeError) {
    // Avoid hard dependency on tracing in ergo-tx — eprintln is enough for misconfig.
    eprintln!(
        "citadel: invalid CITADEL_DEV_FEE_ADDRESS '{addr}' ({err}); using default fee address"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_noop_when_disabled() {
        let mut outputs = vec![];
        append_dev_fee_output(&mut outputs, &DevFeeConfig::disabled(), 1000).unwrap();
        assert!(outputs.is_empty());
        assert_eq!(dev_fee_budget(&DevFeeConfig::disabled()), 0);
    }

    #[test]
    fn append_fee_when_enabled() {
        let cfg = DevFeeConfig::enabled_default();
        let mut outputs = vec![];
        append_dev_fee_output(&mut outputs, &cfg, 42).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].value, DEV_FEE_NANO.to_string());
        assert_eq!(outputs[0].ergo_tree, DEFAULT_DEV_FEE_ERGO_TREE);
        assert!(outputs[0].assets.is_empty());
        assert_eq!(outputs[0].creation_height, 42);
        assert_eq!(dev_fee_budget(&cfg), DEV_FEE_NANO as u64);
    }

    #[test]
    fn resolved_config_disabled_in_unit_tests_by_default() {
        assert!(!resolved_config().enabled);
    }

    #[test]
    fn with_test_dev_fee_enables() {
        with_test_dev_fee(DevFeeConfig::enabled_default(), || {
            assert!(resolved_config().enabled);
            assert_eq!(resolved_config().budget(), DEV_FEE_NANO);
        });
        assert!(!resolved_config().enabled);
    }
}
