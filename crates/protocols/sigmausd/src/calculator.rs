//! SigmaUSD / AgeUSD Protocol Calculator
//!
//! Pure math functions for calculating protocol state and prices.
//! No I/O, no async - just deterministic calculations.
//!
//! # Units
//!
//! - ERG amounts: nanoERG (i64), 1 ERG = 1_000_000_000 nanoERG
//! - SigUSD: raw units with 2 decimals, 100 units = 1.00 SigUSD
//! - SigRSV: raw units with 0 decimals
//! - Intermediate results use i128 to avoid overflow

use super::params;

/// Input state from bank and oracle boxes
#[derive(Debug, Clone)]
pub struct ProtocolInput {
    /// ERG reserves in bank (nanoERG)
    pub bank_erg_nano: i64,
    /// Circulating SigUSD (raw units, 2 decimals)
    pub sigusd_circulating: i64,
    /// Circulating SigRSV (raw units, 0 decimals)
    pub sigrsv_circulating: i64,
    /// Oracle rate: nanoERG per 1 USD
    pub nanoerg_per_usd: i64,
}

/// Calculated protocol state
#[derive(Debug, Clone)]
pub struct ProtocolState {
    /// Reserve ratio as percentage (e.g., 542.13)
    pub reserve_ratio_pct: f64,
    /// Liabilities in nanoERG
    pub liabilities_nano: i128,
    /// Equity in nanoERG (reserves - liabilities)
    pub equity_nano: i128,
    /// SigUSD price in nanoERG (per 1 SigUSD = 1 USD)
    pub sigusd_price_nano: i64,
    /// SigRSV price in nanoERG (per 1 SigRSV)
    pub sigrsv_price_nano: i64,
    /// Can mint SigUSD (ratio > 400%)
    pub can_mint_sigusd: bool,
    /// Can mint SigRSV (ratio < 800%)
    pub can_mint_sigrsv: bool,
    /// Can redeem SigRSV (ratio > 400%)
    pub can_redeem_sigrsv: bool,
    /// Max SigUSD that can be minted
    pub max_sigusd_mintable: i64,
    /// Max SigRSV that can be minted
    pub max_sigrsv_mintable: i64,
    /// Max SigRSV that can be redeemed
    pub max_sigrsv_redeemable: i64,
}

/// Calculate full protocol state from inputs
pub fn calculate_state(input: &ProtocolInput) -> ProtocolState {
    // Liabilities = SigUSD value in nanoERG
    // SigUSD has 2 decimals, so divide by 100 to get actual USD count
    // liabilities_nano = (sigusd_circulating / 100) * nanoerg_per_usd
    let liabilities_nano: i128 =
        (input.sigusd_circulating as i128) * (input.nanoerg_per_usd as i128) / 100;

    // Equity = Reserves - Liabilities
    let equity_nano: i128 = (input.bank_erg_nano as i128) - liabilities_nano;

    // Reserve ratio = (reserves / liabilities) * 100
    let reserve_ratio_pct = if liabilities_nano > 0 {
        ((input.bank_erg_nano as f64) / (liabilities_nano as f64)) * 100.0
    } else {
        // No liabilities = infinite ratio (use max for display)
        f64::MAX
    };

    // SigUSD price = nanoERG per 1 SigUSD (= 1 USD worth of ERG)
    let sigusd_price_nano = input.nanoerg_per_usd;

    // SigRSV price = equity / circulating supply
    let sigrsv_price_nano = if input.sigrsv_circulating > 0 && equity_nano > 0 {
        (equity_nano / (input.sigrsv_circulating as i128)) as i64
    } else {
        0
    };

    // Mint/redeem status based on ratio bands
    let can_mint_sigusd = reserve_ratio_pct > params::MIN_RESERVE_RATIO_PCT as f64;
    let can_mint_sigrsv = reserve_ratio_pct < params::MAX_RESERVE_RATIO_PCT as f64;
    let can_redeem_sigrsv = reserve_ratio_pct > params::MIN_RESERVE_RATIO_PCT as f64;

    // Calculate maximums
    let max_sigusd_mintable = calculate_max_sigusd_mintable(input, liabilities_nano);
    let max_sigrsv_mintable = calculate_max_sigrsv_mintable(input, liabilities_nano);
    let max_sigrsv_redeemable = calculate_max_sigrsv_redeemable(input, reserve_ratio_pct);

    ProtocolState {
        reserve_ratio_pct,
        liabilities_nano,
        equity_nano,
        sigusd_price_nano,
        sigrsv_price_nano,
        can_mint_sigusd,
        can_mint_sigrsv,
        can_redeem_sigrsv,
        max_sigusd_mintable,
        max_sigrsv_mintable,
        max_sigrsv_redeemable,
    }
}

/// Max SigUSD that can be minted while keeping ratio >= 400%
fn calculate_max_sigusd_mintable(input: &ProtocolInput, current_liabilities: i128) -> i64 {
    // At 400% ratio: reserves = 4 * liabilities
    // max_liabilities = reserves / 4
    let max_liabilities = (input.bank_erg_nano as i128) / 4;

    if max_liabilities <= current_liabilities {
        return 0;
    }

    // Headroom in nanoERG
    let headroom_nano = max_liabilities - current_liabilities;

    // Convert to SigUSD units (multiply by 100, divide by nanoerg_per_usd)
    let max_sigusd = (headroom_nano * 100) / (input.nanoerg_per_usd as i128);

    max_sigusd.min(i64::MAX as i128) as i64
}

/// Max SigRSV that can be minted while keeping ratio <= 800%
fn calculate_max_sigrsv_mintable(input: &ProtocolInput, current_liabilities: i128) -> i64 {
    // At 800% ratio: reserves = 8 * liabilities
    let target_reserves = current_liabilities * 8;
    let current_reserves = input.bank_erg_nano as i128;

    if target_reserves <= current_reserves {
        return 0;
    }

    // Headroom in nanoERG (how much more ERG we can accept)
    let headroom_nano = target_reserves - current_reserves;

    // Calculate current equity
    let current_equity = current_reserves - current_liabilities;

    if current_equity <= 0 || input.sigrsv_circulating == 0 {
        return 0;
    }

    // Rough estimate: headroom / current_sigrsv_price
    let sigrsv_price = current_equity / (input.sigrsv_circulating as i128);
    if sigrsv_price <= 0 {
        return 0;
    }

    let max_sigrsv = headroom_nano / sigrsv_price;
    max_sigrsv.min(i64::MAX as i128) as i64
}

/// Max SigRSV that can be redeemed while keeping ratio >= 400%
fn calculate_max_sigrsv_redeemable(input: &ProtocolInput, current_ratio: f64) -> i64 {
    if current_ratio <= params::MIN_RESERVE_RATIO_PCT as f64 {
        return 0;
    }

    // All SigRSV can be redeemed if ratio is healthy
    input.sigrsv_circulating
}

/// ERG calculation result with base and net amounts
#[derive(Debug, Clone)]
pub struct ErgCalculation {
    /// Base value before fee
    pub base_amount: i64,
    /// Amount after fee (what user actually pays/receives)
    pub net_amount: i64,
    /// Protocol fee
    pub fee: i64,
}

/// Calculate ERG cost to mint SigUSD
pub fn cost_to_mint_sigusd(amount: i64, nanoerg_per_usd: i64) -> ErgCalculation {
    // Base cost = amount * price / 100 (SigUSD has 2 decimals)
    let base_cost = (amount as i128) * (nanoerg_per_usd as i128) / 100;

    // Add protocol fee (2%)
    let fee = base_cost * (params::FEE_BPS as i128) / 10000;

    let net = (base_cost + fee).min(i64::MAX as i128) as i64;

    ErgCalculation {
        base_amount: base_cost as i64,
        net_amount: net,
        fee: fee as i64,
    }
}

/// Calculate ERG received from redeeming SigUSD
pub fn erg_from_redeem_sigusd(amount: i64, nanoerg_per_usd: i64) -> ErgCalculation {
    // Base value = amount * price / 100
    let base_value = (amount as i128) * (nanoerg_per_usd as i128) / 100;

    // Subtract protocol fee (2%)
    let fee = base_value * (params::FEE_BPS as i128) / 10000;

    let net = (base_value - fee).max(0) as i64;

    ErgCalculation {
        base_amount: base_value as i64,
        net_amount: net,
        fee: fee as i64,
    }
}

/// Calculate ERG cost to mint SigRSV
pub fn cost_to_mint_sigrsv(amount: i64, sigrsv_price_nano: i64) -> ErgCalculation {
    let base_cost = (amount as i128) * (sigrsv_price_nano as i128);
    let fee = base_cost * (params::FEE_BPS as i128) / 10000;
    let net = (base_cost + fee).min(i64::MAX as i128) as i64;

    ErgCalculation {
        base_amount: base_cost as i64,
        net_amount: net,
        fee: fee as i64,
    }
}

/// Calculate ERG received from redeeming SigRSV
pub fn erg_from_redeem_sigrsv(amount: i64, sigrsv_price_nano: i64) -> ErgCalculation {
    let base_value = (amount as i128) * (sigrsv_price_nano as i128);
    let fee = base_value * (params::FEE_BPS as i128) / 10000;
    let net = (base_value - fee).max(0) as i64;

    ErgCalculation {
        base_amount: base_value as i64,
        net_amount: net,
        fee: fee as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input() -> ProtocolInput {
        ProtocolInput {
            // 10 million ERG in bank
            bank_erg_nano: 10_000_000_000_000_000,
            // 5 million SigUSD circulating (2 decimals, so * 100)
            sigusd_circulating: 500_000_000,
            // 100 billion SigRSV circulating
            sigrsv_circulating: 100_000_000_000,
            // ~0.54 USD/ERG means ~1.85 ERG/USD = 1,851,851,851 nanoERG/USD
            nanoerg_per_usd: 1_851_851_851,
        }
    }

    #[test]
    fn test_liabilities_calculation() {
        let input = sample_input();
        let state = calculate_state(&input);

        // liabilities = 500_000_000 * 1_851_851_851 / 100 = 9,259,259,255,000,000
        let expected_liabilities: i128 = 9_259_259_255_000_000;
        assert_eq!(state.liabilities_nano, expected_liabilities);
    }

    #[test]
    fn test_reserve_ratio() {
        let input = sample_input();
        let state = calculate_state(&input);

        // ratio = 10e15 / 9.26e15 * 100 ≈ 108%
        assert!(state.reserve_ratio_pct > 100.0);
        assert!(state.reserve_ratio_pct < 120.0);
    }

    #[test]
    fn test_sigusd_price() {
        let input = sample_input();
        let state = calculate_state(&input);

        // SigUSD price = nanoerg_per_usd
        assert_eq!(state.sigusd_price_nano, input.nanoerg_per_usd);
    }

    #[test]
    fn test_mint_status_low_ratio() {
        let mut input = sample_input();
        // Set very high liabilities to push ratio below 400%
        input.sigusd_circulating = 2_000_000_000; // 20M SigUSD

        let state = calculate_state(&input);

        assert!(state.reserve_ratio_pct < params::MIN_RESERVE_RATIO_PCT as f64);
        assert!(!state.can_mint_sigusd);
        assert!(state.can_mint_sigrsv);
    }

    #[test]
    fn test_mint_status_high_ratio() {
        let mut input = sample_input();
        // Set very low liabilities to push ratio above 800%
        input.sigusd_circulating = 10_000_000; // 100k SigUSD

        let state = calculate_state(&input);

        assert!(state.reserve_ratio_pct > params::MAX_RESERVE_RATIO_PCT as f64);
        assert!(state.can_mint_sigusd);
        assert!(!state.can_mint_sigrsv);
    }

    #[test]
    fn test_cost_to_mint_sigusd() {
        let nanoerg_per_usd = 1_851_851_851;
        // Mint 100 SigUSD (10000 raw units with 2 decimals)
        let calc = cost_to_mint_sigusd(10000, nanoerg_per_usd);

        // Base: 10000 * 1_851_851_851 / 100 = 185,185,185,100 nanoERG
        // Fee (2%): 3,703,703,702
        // Total: ~188,888,888,802
        assert!(calc.net_amount > 185_000_000_000);
        assert!(calc.net_amount < 200_000_000_000);
        assert!(calc.fee > 0);
    }

    #[test]
    fn test_zero_supply_edge_case() {
        let input = ProtocolInput {
            bank_erg_nano: 1_000_000_000_000_000,
            sigusd_circulating: 0,
            sigrsv_circulating: 0,
            nanoerg_per_usd: 1_851_851_851,
        };

        let state = calculate_state(&input);

        assert_eq!(state.liabilities_nano, 0);
        assert!(state.reserve_ratio_pct > 1000.0);
        assert_eq!(state.sigrsv_price_nano, 0);
    }
}
