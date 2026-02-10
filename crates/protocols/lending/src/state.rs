//! Lending State Types
//!
//! Data structures for pool state and user positions.

use serde::{Deserialize, Serialize};

/// Collateral option for a lending pool (hardcoded, matching Duckpools off-chain config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollateralOption {
    /// Token ID ("native" for ERG)
    pub token_id: String,
    /// Human-readable name ("ERG", "SigUSD", etc.)
    pub token_name: String,
    /// Liquidation threshold from R4, e.g. 1250 = 125%
    pub liquidation_threshold: u64,
    /// Liquidation penalty from R7, e.g. 500 = 5%
    pub liquidation_penalty: u64,
    /// DEX NFT ID from R6, for price discovery
    pub dex_nft: Option<String>,
}

/// Pool state for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolState {
    pub pool_id: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub is_erg_pool: bool,

    // Pool metrics
    pub total_supplied: u64,
    pub total_borrowed: u64,
    pub available_liquidity: u64,
    pub utilization_pct: f64,

    // Rates
    /// Supply APY as decimal (e.g., 0.05 means 5%). Frontend multiplies by 100 for display.
    pub supply_apy: f64,
    /// Borrow APY as decimal (e.g., 0.08 means 8%). Frontend multiplies by 100 for display.
    pub borrow_apy: f64,

    // LP supply
    pub lp_tokens_in_circulation: u64,

    // Box IDs for reference
    pub pool_box_id: String,

    // Collateral options fetched from on-chain parameter box
    pub collateral_options: Vec<CollateralOption>,

    // User positions (if address provided)
    pub user_lend_position: Option<LendPosition>,
    pub user_borrow_positions: Vec<BorrowPosition>,
}

/// User's lending position in a pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendPosition {
    pub lp_tokens: u64,
    pub underlying_value: u64,
    pub unrealized_profit: i64,
}

/// User's borrow position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowPosition {
    pub collateral_box_id: String,
    pub collateral_token: String,
    pub collateral_name: String,
    pub collateral_amount: u64,
    pub borrowed_amount: u64,
    pub total_owed: u64,
    pub health_factor: f64,
    pub liquidation_threshold: u16,
    pub at_risk: bool,
}

/// Markets response - all pools with optional user positions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketsResponse {
    pub pools: Vec<PoolState>,
    pub block_height: u32,
}

/// Raw pool box data from chain
#[derive(Debug, Clone)]
pub struct PoolBoxData {
    pub box_id: String,
    pub value_nano: i64,
    pub lp_tokens_in_circulation: u64,
    pub borrow_tokens_in_circulation: u64,
    pub currency_amount: u64, // For token pools
}

impl PoolState {
    /// Build from raw box data
    #[allow(clippy::too_many_arguments)]
    pub fn from_pool_box(
        pool_id: &str,
        name: &str,
        symbol: &str,
        decimals: u8,
        is_erg_pool: bool,
        box_data: &PoolBoxData,
        supply_apy: f64,
        borrow_apy: f64,
    ) -> Self {
        // total_supplied = remaining liquidity + borrowed amount
        let remaining = if is_erg_pool {
            box_data.value_nano as u64
        } else {
            box_data.currency_amount
        };
        let total_borrowed = box_data.borrow_tokens_in_circulation;
        let total_supplied = remaining.saturating_add(total_borrowed);

        let available_liquidity = remaining;
        let utilization_pct = if total_supplied > 0 {
            (total_borrowed as f64 / total_supplied as f64) * 100.0
        } else {
            0.0
        };

        Self {
            pool_id: pool_id.to_string(),
            name: name.to_string(),
            symbol: symbol.to_string(),
            decimals,
            is_erg_pool,
            total_supplied,
            total_borrowed,
            available_liquidity,
            utilization_pct,
            supply_apy,
            borrow_apy,
            lp_tokens_in_circulation: box_data.lp_tokens_in_circulation,
            pool_box_id: box_data.box_id.clone(),
            collateral_options: vec![],
            user_lend_position: None,
            user_borrow_positions: vec![],
        }
    }
}

impl BorrowPosition {
    /// Check if position is at risk of liquidation
    pub fn is_at_risk(&self) -> bool {
        self.health_factor < crate::constants::health::WARNING_THRESHOLD
    }

    /// Get health status for UI
    pub fn health_status(&self) -> HealthStatus {
        if self.health_factor >= crate::constants::health::HEALTHY_THRESHOLD {
            HealthStatus::Healthy
        } else if self.health_factor >= crate::constants::health::WARNING_THRESHOLD {
            HealthStatus::Warning
        } else {
            HealthStatus::Danger
        }
    }
}

/// Health factor status for UI color coding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy, // Green: >= 1.5
    Warning, // Amber: >= 1.2 and < 1.5
    Danger,  // Red: < 1.2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pool_box_data() -> PoolBoxData {
        PoolBoxData {
            box_id: "test_box_id".to_string(),
            value_nano: 1_000_000_000_000, // 1000 ERG
            lp_tokens_in_circulation: 500_000_000_000,
            borrow_tokens_in_circulation: 100_000_000_000,
            currency_amount: 0,
        }
    }

    #[test]
    fn test_pool_state_from_box_erg_pool() {
        let box_data = sample_pool_box_data();
        let state =
            PoolState::from_pool_box("erg", "ERG Pool", "ERG", 9, true, &box_data, 2.5, 5.0);

        assert_eq!(state.pool_id, "erg");
        // total_supplied = value_nano + borrow_tokens = 1_000_000_000_000 + 100_000_000_000
        assert_eq!(state.total_supplied, 1_100_000_000_000);
        assert_eq!(state.available_liquidity, 1_000_000_000_000);
        assert!(state.is_erg_pool);
        assert_eq!(state.supply_apy, 2.5);
        assert!(state.user_lend_position.is_none());
    }

    #[test]
    fn test_pool_state_from_box_token_pool() {
        let box_data = PoolBoxData {
            box_id: "sigusd_box".to_string(),
            value_nano: 1_000_000,
            lp_tokens_in_circulation: 1000,
            borrow_tokens_in_circulation: 50,
            currency_amount: 1_000_000, // 10000 SigUSD (2 decimals)
        };
        let state = PoolState::from_pool_box(
            "sigusd",
            "SigUSD Pool",
            "SigUSD",
            2,
            false,
            &box_data,
            1.5,
            3.0,
        );

        assert_eq!(state.pool_id, "sigusd");
        // total_supplied = currency_amount + borrow_tokens = 1000000 + 50
        assert_eq!(state.total_supplied, 1000050);
        assert_eq!(state.available_liquidity, 1000000);
        assert!(!state.is_erg_pool);
    }

    #[test]
    fn test_borrow_position_health_status_healthy() {
        let position = BorrowPosition {
            collateral_box_id: "box1".to_string(),
            collateral_token: "token1".to_string(),
            collateral_name: "ERG".to_string(),
            collateral_amount: 1000,
            borrowed_amount: 500,
            total_owed: 520,
            health_factor: 1.8, // Above HEALTHY_THRESHOLD (1.5)
            liquidation_threshold: 1250,
            at_risk: false,
        };

        assert_eq!(position.health_status(), HealthStatus::Healthy);
        assert!(!position.is_at_risk());
    }

    #[test]
    fn test_borrow_position_health_status_warning() {
        let position = BorrowPosition {
            collateral_box_id: "box2".to_string(),
            collateral_token: "token1".to_string(),
            collateral_name: "ERG".to_string(),
            collateral_amount: 1000,
            borrowed_amount: 700,
            total_owed: 750,
            health_factor: 1.3, // Between WARNING (1.2) and HEALTHY (1.5)
            liquidation_threshold: 1250,
            at_risk: false,
        };

        assert_eq!(position.health_status(), HealthStatus::Warning);
        assert!(!position.is_at_risk());
    }

    #[test]
    fn test_borrow_position_health_status_danger() {
        let position = BorrowPosition {
            collateral_box_id: "box3".to_string(),
            collateral_token: "token1".to_string(),
            collateral_name: "ERG".to_string(),
            collateral_amount: 1000,
            borrowed_amount: 900,
            total_owed: 950,
            health_factor: 1.1, // Below WARNING_THRESHOLD (1.2)
            liquidation_threshold: 1250,
            at_risk: true,
        };

        assert_eq!(position.health_status(), HealthStatus::Danger);
        assert!(position.is_at_risk());
    }
}
