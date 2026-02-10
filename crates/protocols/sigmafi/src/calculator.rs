//! SigmaFi bond calculation utilities

use crate::constants;

/// Calculate interest as a percentage of principal.
/// Returns e.g. 5.0 for 5% interest.
pub fn calculate_interest_percent(principal: u64, repayment: u64) -> f64 {
    if principal == 0 {
        return 0.0;
    }
    ((repayment as f64 - principal as f64) / principal as f64) * 100.0
}

/// Calculate annualized percentage rate from interest and term in blocks.
/// Assumes ~2 minute block times.
pub fn calculate_apr(interest_percent: f64, maturity_blocks: i32) -> f64 {
    if maturity_blocks <= 0 {
        return 0.0;
    }
    let days = (maturity_blocks as f64 * 2.0) / 60.0 / 24.0;
    if days <= 0.0 {
        return 0.0;
    }
    (interest_percent / days) * 365.0
}

/// Calculate collateral-to-principal ratio as a percentage.
/// Returns e.g. 150.0 for 150% collateralization.
pub fn calculate_collateral_ratio(
    collateral_value_usd: f64,
    principal_value_usd: f64,
    interest_value_usd: f64,
) -> f64 {
    if principal_value_usd == 0.0 {
        return 0.0;
    }
    ((collateral_value_usd - interest_value_usd) / principal_value_usd) * 100.0
}

/// Calculate the developer fee from principal amount.
pub fn calculate_dev_fee(principal: u64) -> u64 {
    (constants::DEV_FEE_NUM * principal) / constants::FEE_DENOM
}

/// Calculate the UI fee from principal amount.
pub fn calculate_ui_fee(principal: u64) -> u64 {
    (constants::UI_FEE_NUM * principal) / constants::FEE_DENOM
}

/// Convert blocks to approximate days
pub fn blocks_to_days(blocks: i32) -> f64 {
    (blocks as f64 * 2.0) / 60.0 / 24.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interest_percent() {
        // 10 ERG principal, 10.5 ERG repayment -> 5% interest
        let interest = calculate_interest_percent(10_000_000_000, 10_500_000_000);
        assert!((interest - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_interest_zero_principal() {
        assert_eq!(calculate_interest_percent(0, 100), 0.0);
    }

    #[test]
    fn test_apr_30_day_5_percent() {
        // 5% interest over ~30 days (21600 blocks at 2 min/block)
        let apr = calculate_apr(5.0, 21600);
        // 5% / 30 days * 365 ≈ 60.8%
        assert!((apr - 60.83).abs() < 1.0, "APR was {}", apr);
    }

    #[test]
    fn test_apr_zero_blocks() {
        assert_eq!(calculate_apr(5.0, 0), 0.0);
    }

    #[test]
    fn test_collateral_ratio() {
        // $150 collateral, $100 principal, $5 interest -> (150-5)/100 * 100 = 145%
        let ratio = calculate_collateral_ratio(150.0, 100.0, 5.0);
        assert!((ratio - 145.0).abs() < 0.001);
    }

    #[test]
    fn test_collateral_ratio_zero_principal() {
        assert_eq!(calculate_collateral_ratio(100.0, 0.0, 5.0), 0.0);
    }

    #[test]
    fn test_dev_fee() {
        // 10 ERG principal -> 0.05 ERG dev fee
        let fee = calculate_dev_fee(10_000_000_000);
        assert_eq!(fee, 50_000_000); // 0.05 ERG
    }

    #[test]
    fn test_ui_fee() {
        // 10 ERG principal -> 0.04 ERG UI fee
        let fee = calculate_ui_fee(10_000_000_000);
        assert_eq!(fee, 40_000_000); // 0.04 ERG
    }

    #[test]
    fn test_blocks_to_days() {
        // 21600 blocks * 2 min / 60 / 24 = 30 days
        let days = blocks_to_days(21600);
        assert!((days - 30.0).abs() < 0.001);
    }
}
