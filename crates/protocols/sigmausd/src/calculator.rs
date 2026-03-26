//! SigmaUSD / AgeUSD calculator. All amounts in nanoERG (i64).
//! SigUSD has 2 decimals (100 raw = 1.00 SigUSD). i128 used to avoid overflow.

use super::params;

#[derive(Debug, Clone)]
pub struct ProtocolInput {
    pub bank_erg_nano: i64,
    pub sigusd_circulating: i64,
    pub sigrsv_circulating: i64,
    pub nanoerg_per_usd: i64,
}

#[derive(Debug, Clone)]
pub struct ProtocolState {
    pub reserve_ratio_pct: f64,
    pub liabilities_nano: i128,
    pub equity_nano: i128,
    pub sigusd_price_nano: i64,
    pub sigrsv_price_nano: i64,
    pub can_mint_sigusd: bool,
    pub can_mint_sigrsv: bool,
    pub can_redeem_sigrsv: bool,
    pub max_sigusd_mintable: i64,
    pub max_sigrsv_mintable: i64,
    pub max_sigrsv_redeemable: i64,
}

pub fn calculate_state(input: &ProtocolInput) -> ProtocolState {
    // liabilities = sigusd_circulating * nanoerg_per_usd / 100 (2-decimal adjustment)
    let liabilities_nano: i128 =
        (input.sigusd_circulating as i128) * (input.nanoerg_per_usd as i128) / 100;

    let equity_nano: i128 = (input.bank_erg_nano as i128) - liabilities_nano;

    let reserve_ratio_pct = if liabilities_nano > 0 {
        ((input.bank_erg_nano as f64) / (liabilities_nano as f64)) * 100.0
    } else {
        f64::MAX
    };

    let sigusd_price_nano = input.nanoerg_per_usd;

    let sigrsv_price_nano = if input.sigrsv_circulating > 0 && equity_nano > 0 {
        (equity_nano / (input.sigrsv_circulating as i128)) as i64
    } else {
        0
    };

    let can_mint_sigusd = reserve_ratio_pct > params::MIN_RESERVE_RATIO_PCT as f64;
    let can_mint_sigrsv = reserve_ratio_pct < params::MAX_RESERVE_RATIO_PCT as f64;
    let can_redeem_sigrsv = reserve_ratio_pct > params::MIN_RESERVE_RATIO_PCT as f64;

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

/// At 400% ratio: max_liabilities = reserves / 4
fn calculate_max_sigusd_mintable(input: &ProtocolInput, current_liabilities: i128) -> i64 {
    let max_liabilities = (input.bank_erg_nano as i128) / 4;

    if max_liabilities <= current_liabilities {
        return 0;
    }

    let headroom_nano = max_liabilities - current_liabilities;
    let max_sigusd = (headroom_nano * 100) / (input.nanoerg_per_usd as i128);

    max_sigusd.min(i64::MAX as i128) as i64
}

/// At 800% ratio: target_reserves = 8 * liabilities
fn calculate_max_sigrsv_mintable(input: &ProtocolInput, current_liabilities: i128) -> i64 {
    let target_reserves = current_liabilities * 8;
    let current_reserves = input.bank_erg_nano as i128;

    if target_reserves <= current_reserves {
        return 0;
    }

    let headroom_nano = target_reserves - current_reserves;
    let current_equity = current_reserves - current_liabilities;

    if current_equity <= 0 || input.sigrsv_circulating == 0 {
        return 0;
    }

    let sigrsv_price = current_equity / (input.sigrsv_circulating as i128);
    if sigrsv_price <= 0 {
        return 0;
    }

    let max_sigrsv = headroom_nano / sigrsv_price;
    max_sigrsv.min(i64::MAX as i128) as i64
}

fn calculate_max_sigrsv_redeemable(input: &ProtocolInput, current_ratio: f64) -> i64 {
    if current_ratio <= params::MIN_RESERVE_RATIO_PCT as f64 {
        return 0;
    }

    input.sigrsv_circulating
}

#[derive(Debug, Clone)]
pub struct ErgCalculation {
    pub base_amount: i64,
    pub net_amount: i64,
    pub fee: i64,
}

pub fn cost_to_mint_sigusd(amount: i64, nanoerg_per_usd: i64) -> ErgCalculation {
    let base_cost = (amount as i128) * (nanoerg_per_usd as i128) / 100;
    let fee = base_cost * (params::FEE_BPS as i128) / 10000;

    let net = (base_cost + fee).min(i64::MAX as i128) as i64;

    ErgCalculation {
        base_amount: base_cost as i64,
        net_amount: net,
        fee: fee as i64,
    }
}

pub fn erg_from_redeem_sigusd(amount: i64, nanoerg_per_usd: i64) -> ErgCalculation {
    let base_value = (amount as i128) * (nanoerg_per_usd as i128) / 100;
    let fee = base_value * (params::FEE_BPS as i128) / 10000;

    let net = (base_value - fee).max(0) as i64;

    ErgCalculation {
        base_amount: base_value as i64,
        net_amount: net,
        fee: fee as i64,
    }
}

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
            bank_erg_nano: 10_000_000_000_000_000,
            sigusd_circulating: 500_000_000,
            sigrsv_circulating: 100_000_000_000,
            nanoerg_per_usd: 1_851_851_851,
        }
    }

    #[test]
    fn test_liabilities_calculation() {
        let input = sample_input();
        let state = calculate_state(&input);

        let expected_liabilities: i128 = 9_259_259_255_000_000;
        assert_eq!(state.liabilities_nano, expected_liabilities);
    }

    #[test]
    fn test_reserve_ratio() {
        let input = sample_input();
        let state = calculate_state(&input);

        assert!(state.reserve_ratio_pct > 100.0);
        assert!(state.reserve_ratio_pct < 120.0);
    }

    #[test]
    fn test_sigusd_price() {
        let input = sample_input();
        let state = calculate_state(&input);

        assert_eq!(state.sigusd_price_nano, input.nanoerg_per_usd);
    }

    #[test]
    fn test_mint_status_low_ratio() {
        let mut input = sample_input();
        input.sigusd_circulating = 2_000_000_000;

        let state = calculate_state(&input);

        assert!(state.reserve_ratio_pct < params::MIN_RESERVE_RATIO_PCT as f64);
        assert!(!state.can_mint_sigusd);
        assert!(state.can_mint_sigrsv);
    }

    #[test]
    fn test_mint_status_high_ratio() {
        let mut input = sample_input();
        input.sigusd_circulating = 10_000_000;

        let state = calculate_state(&input);

        assert!(state.reserve_ratio_pct > params::MAX_RESERVE_RATIO_PCT as f64);
        assert!(state.can_mint_sigusd);
        assert!(!state.can_mint_sigrsv);
    }

    #[test]
    fn test_cost_to_mint_sigusd() {
        let nanoerg_per_usd = 1_851_851_851;
        let calc = cost_to_mint_sigusd(10000, nanoerg_per_usd);

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
