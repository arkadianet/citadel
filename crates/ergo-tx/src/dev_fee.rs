//! Citadel application fee helpers. Disabled in this local build.
//! Miner fees and fees required by protocol contracts are separate.

use crate::eip12::Eip12Output;
use citadel_core::constants::DEV_FEE_NANO;

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

/// This local build never charges the Citadel application fee.
/// Explicit test overrides still exercise fee-enabled builder compatibility.
pub fn resolved_config() -> DevFeeConfig {
    TEST_OVERRIDE
        .with(|cell| cell.borrow().clone())
        .unwrap_or_else(DevFeeConfig::disabled)
}

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

/// Retained for callers performing startup diagnostics. Environment variables
/// cannot enable a developer fee in this local build.
pub fn try_load_from_env() -> Result<DevFeeConfig, DevFeeError> {
    Ok(DevFeeConfig::disabled())
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
