//! Lending Calculator
//!
//! Pure math functions for APY, fees, and health factor calculations.
//! No I/O - just calculations.

use crate::constants::{fees, health};
use crate::state::HealthStatus;

/// Calculate service fee for a lend/withdraw operation
pub fn calculate_service_fee(amount: u64, is_erg_pool: bool) -> u64 {
    let thresholds = if is_erg_pool {
        fees::ERG_THRESHOLDS
    } else {
        fees::TOKEN_THRESHOLDS
    };
    fees::calculate_service_fee(amount, thresholds)
}

/// Calculate the amount that goes to pool after service fee
/// Returns (amount_to_pool, service_fee)
///
/// Note: Uses binary search to find the best approximation where amount + fee <= total.
/// Due to integer division in fee calculation, exact matching may not always be possible.
pub fn calculate_amount_after_fee(total_amount: u64, is_erg_pool: bool) -> (u64, u64) {
    let thresholds = if is_erg_pool {
        fees::ERG_THRESHOLDS
    } else {
        fees::TOKEN_THRESHOLDS
    };

    // Binary search to find amount where amount + fee = total
    let mut lower = 0u64;
    let mut upper = total_amount;
    let mut mid = (upper + lower) / 2;

    for _ in 0..200 {
        let fee = fees::calculate_service_fee(mid, thresholds);
        let total = mid.saturating_add(fee);

        if total > total_amount {
            upper = mid;
        } else if total < total_amount {
            lower = mid;
        } else {
            break;
        }

        let new_mid = (upper + lower) / 2;
        if new_mid == mid {
            break;
        }
        mid = new_mid;
    }

    let fee = fees::calculate_service_fee(mid, thresholds);
    (mid, fee)
}

/// Calculate health factor for a borrow position
/// health = (collateral_value * 1000) / (total_owed * liquidation_threshold)
pub fn calculate_health_factor(
    collateral_value_nano: u64,
    total_owed_nano: u64,
    liquidation_threshold: u16, // e.g., 1250 = 125%
) -> f64 {
    if total_owed_nano == 0 {
        return f64::MAX; // No debt = infinite health
    }

    let numerator = collateral_value_nano as f64 * 1000.0;
    let denominator = total_owed_nano as f64 * liquidation_threshold as f64;

    numerator / denominator
}

/// Determine health status from health factor
pub fn health_status(health_factor: f64) -> HealthStatus {
    if health_factor >= health::HEALTHY_THRESHOLD {
        HealthStatus::Healthy
    } else if health_factor >= health::WARNING_THRESHOLD {
        HealthStatus::Warning
    } else {
        HealthStatus::Danger
    }
}

/// Calculate LP tokens received for a lend amount
/// lp_tokens = (amount * total_lp_supply) / total_pool_assets
///
/// The `is_erg_pool` parameter is reserved for future use when ERG and token
/// pools may have different LP token calculation rules.
pub fn calculate_lp_tokens_for_lend(
    amount: u64,
    total_pool_assets: u64,
    total_lp_supply: u64,
    _is_erg_pool: bool,
) -> u64 {
    if total_pool_assets == 0 || total_lp_supply == 0 {
        // First deposit - 1:1 ratio
        return amount;
    }

    // Standard LP calculation
    ((amount as u128 * total_lp_supply as u128) / total_pool_assets as u128) as u64
}

/// Calculate underlying value for LP tokens
/// underlying = (lp_amount * total_pool_assets) / total_lp_supply
pub fn calculate_underlying_for_lp(
    lp_amount: u64,
    total_pool_assets: u64,
    total_lp_supply: u64,
) -> u64 {
    if total_lp_supply == 0 {
        return 0;
    }

    ((lp_amount as u128 * total_pool_assets as u128) / total_lp_supply as u128) as u64
}

/// Calculate utilization ratio as percentage
pub fn calculate_utilization(total_borrowed: u64, total_supplied: u64) -> f64 {
    if total_supplied == 0 {
        return 0.0;
    }
    (total_borrowed as f64 / total_supplied as f64) * 100.0
}

/// Calculate total cost for a lend operation including fees
pub fn calculate_lend_cost(amount: u64, is_erg_pool: bool) -> LendCostBreakdown {
    let service_fee = calculate_service_fee(amount, is_erg_pool);
    let tx_fee = fees::TX_FEE;
    let min_box_value = fees::MIN_BOX_VALUE;

    // For ERG pool: need amount + service_fee + tx_fee + min_box
    // For token pool: need amount in tokens + tx_fee + min_box in ERG
    let total_erg_needed = if is_erg_pool {
        amount + service_fee + tx_fee + min_box_value
    } else {
        tx_fee + min_box_value * 2 // proxy box + change box
    };

    LendCostBreakdown {
        amount,
        service_fee,
        tx_fee,
        min_box_value,
        total_erg_needed,
    }
}

/// Breakdown of lend operation costs
#[derive(Debug, Clone)]
pub struct LendCostBreakdown {
    pub amount: u64,
    pub service_fee: u64,
    pub tx_fee: u64,
    pub min_box_value: u64,
    pub total_erg_needed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_fee_erg_pool() {
        // 1 ERG = 1_000_000_000 nanoERG
        let fee = calculate_service_fee(1_000_000_000, true);
        // fee = 1_000_000_000 / 160 = 6_250_000
        assert_eq!(fee, 6_250_000);
    }

    #[test]
    fn test_service_fee_token_pool() {
        let fee = calculate_service_fee(1000, false);
        // fee = 1000 / 160 = 6
        assert_eq!(fee, 6);
    }

    #[test]
    fn test_health_factor_healthy() {
        // collateral: 1000, owed: 500, threshold: 1250 (125%)
        // health = (1000 * 1000) / (500 * 1250) = 1.6
        let hf = calculate_health_factor(1000, 500, 1250);
        assert!(hf > 1.5);
        assert_eq!(health_status(hf), HealthStatus::Healthy);
    }

    #[test]
    fn test_health_factor_warning() {
        // collateral: 650, owed: 500, threshold: 1000 (100%)
        // health = (650 * 1000) / (500 * 1000) = 1.3
        let hf = calculate_health_factor(650, 500, 1000);
        assert!((1.2..1.5).contains(&hf));
        assert_eq!(health_status(hf), HealthStatus::Warning);
    }

    #[test]
    fn test_health_factor_danger() {
        // collateral: 550, owed: 500, threshold: 1000
        // health = (550 * 1000) / (500 * 1000) = 1.1
        let hf = calculate_health_factor(550, 500, 1000);
        assert!(hf < 1.2);
        assert_eq!(health_status(hf), HealthStatus::Danger);
    }

    #[test]
    fn test_health_factor_no_debt() {
        let hf = calculate_health_factor(1000, 0, 1250);
        assert_eq!(hf, f64::MAX);
    }

    #[test]
    fn test_amount_after_fee() {
        let (to_pool, fee) = calculate_amount_after_fee(1000, false);
        assert!(to_pool + fee <= 1000);
        assert!(to_pool > 990); // Most goes to pool
    }

    #[test]
    fn test_utilization() {
        assert_eq!(calculate_utilization(50, 100), 50.0);
        assert_eq!(calculate_utilization(0, 100), 0.0);
        assert_eq!(calculate_utilization(100, 0), 0.0);
    }

    #[test]
    fn test_lp_tokens_calculation() {
        // Pool has 1000 assets and 500 LP tokens
        // Depositing 100 should give 50 LP tokens
        let lp = calculate_lp_tokens_for_lend(100, 1000, 500, true);
        assert_eq!(lp, 50);
    }

    #[test]
    fn test_underlying_for_lp() {
        // Pool has 1000 assets and 500 LP tokens
        // 50 LP tokens should be worth 100 assets
        let underlying = calculate_underlying_for_lp(50, 1000, 500);
        assert_eq!(underlying, 100);
    }

    #[test]
    fn test_lend_cost_erg_pool() {
        let cost = calculate_lend_cost(1_000_000_000, true); // 1 ERG
        assert_eq!(cost.amount, 1_000_000_000);
        assert_eq!(cost.service_fee, 6_250_000);
        assert_eq!(cost.tx_fee, 1_000_000);
        assert_eq!(cost.min_box_value, 1_000_000);
        // total = amount + service_fee + tx_fee + min_box
        assert_eq!(
            cost.total_erg_needed,
            1_000_000_000 + 6_250_000 + 1_000_000 + 1_000_000
        );
    }
}
