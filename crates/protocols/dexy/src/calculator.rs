#[derive(Debug, Clone)]
pub struct DexyInput {
    pub oracle_rate_nano: i64,
    pub lp_erg_reserves: i64,
    pub lp_dexy_reserves: i64,
    pub dexy_in_bank: i64,
    pub total_supply: i64,
}

#[derive(Debug, Clone)]
pub struct DexyCalculatedState {
    pub lp_rate_nano: i64,
    pub can_mint: bool,
    pub rate_difference_pct: f64,
    pub dexy_circulating: i64,
}

pub fn calculate_state(input: &DexyInput) -> DexyCalculatedState {
    let lp_rate_nano = if input.lp_dexy_reserves > 0 {
        input.lp_erg_reserves / input.lp_dexy_reserves
    } else {
        0
    };

    // Positive = oracle higher than LP = arbitrage opportunity
    let rate_difference_pct = if input.oracle_rate_nano > 0 && lp_rate_nano > 0 {
        ((input.oracle_rate_nano - lp_rate_nano) as f64 / input.oracle_rate_nano as f64) * 100.0
    } else {
        0.0
    };

    // validRateFreeMint: LP price must be >= 98% of oracle (prevents arbitrage attacks)
    let rate_condition_met = lp_rate_nano * 100 > input.oracle_rate_nano * 98;
    let can_mint = input.dexy_in_bank > 0 && rate_condition_met;

    let dexy_circulating = input.total_supply - input.dexy_in_bank;

    DexyCalculatedState {
        lp_rate_nano,
        can_mint,
        rate_difference_pct,
        dexy_circulating,
    }
}

#[derive(Debug, Clone)]
pub struct ErgCalculation {
    pub erg_amount: i64,
}

/// oracle_rate_nano must be the ADJUSTED rate (per token, not raw oracle value).
pub fn cost_to_mint_dexy(amount: i64, oracle_rate_nano: i64, _decimals: u8) -> ErgCalculation {
    ErgCalculation {
        erg_amount: amount * oracle_rate_nano,
    }
}

/// Constant product AMM swap. Direction-agnostic: reserves_sold/reserves_bought
/// determine which asset is being sold.
///
/// Contract formula: output = rBought * input * feeNum / (rSold * feeDenom + input * feeNum)
/// where feeNum = feeDenom - fee_num (pass-through portion, e.g. 997/1000 for 0.3% fee).
pub fn calculate_lp_swap_output(
    input_amount: i64,
    reserves_sold: i64,
    reserves_bought: i64,
    fee_num: i64,
    fee_denom: i64,
) -> i64 {
    let input = input_amount as i128;
    let r_sold = reserves_sold as i128;
    let r_bought = reserves_bought as i128;
    let f_num = (fee_denom - fee_num) as i128;
    let f_denom = fee_denom as i128;

    let numerator = r_bought * input * f_num;
    let denominator = r_sold * f_denom + input * f_num;

    (numerator / denominator) as i64
}

/// Verify swap satisfies: rY * dX * feeNum >= -dY * (rX * feeDenom + dX * feeNum)
pub fn validate_lp_swap(
    reserves_x: i64,
    reserves_y: i64,
    delta_x: i64,
    delta_y: i64,
    fee_num: i64,
    fee_denom: i64,
) -> bool {
    let rx = reserves_x as i128;
    let ry = reserves_y as i128;
    let dx = delta_x as i128;
    let dy = delta_y as i128;
    let fn_ = (fee_denom - fee_num) as i128;
    let fd = fee_denom as i128;

    if dx > 0 {
        ry * dx * fn_ >= -dy * (rx * fd + dx * fn_)
    } else {
        rx * dy * fn_ >= -dx * (ry * fd + dy * fn_)
    }
}

#[derive(Debug, Clone)]
pub struct LpDepositResult {
    pub lp_tokens_out: i64,
    pub consumed_erg: i64,
    pub consumed_dexy: i64,
}

pub fn calculate_lp_deposit(
    deposit_erg: i64,
    deposit_dexy: i64,
    reserves_x: i64,
    reserves_y: i64,
    lp_reserves: i64,
    initial_lp: i64,
) -> LpDepositResult {
    let supply = initial_lp - lp_reserves;

    let shares_by_x = (deposit_erg as i128 * supply as i128 / reserves_x as i128) as i64;
    let shares_by_y = (deposit_dexy as i128 * supply as i128 / reserves_y as i128) as i64;
    let lp_tokens_out = shares_by_x.min(shares_by_y);

    let (consumed_erg, consumed_dexy) = if shares_by_x <= shares_by_y {
        // ERG is limiting; ceiling division ensures enough Dexy to justify shares
        let s = supply as i128;
        let consumed_y =
            ((lp_tokens_out as i128 * reserves_y as i128 + s - 1) / s) as i64;
        (deposit_erg, consumed_y)
    } else {
        // Dexy is limiting; ceiling division ensures enough ERG to justify shares
        let s = supply as i128;
        let consumed_x =
            ((lp_tokens_out as i128 * reserves_x as i128 + s - 1) / s) as i64;
        (consumed_x, deposit_dexy)
    };

    LpDepositResult {
        lp_tokens_out,
        consumed_erg,
        consumed_dexy,
    }
}

#[derive(Debug, Clone)]
pub struct LpRedeemResult {
    pub erg_out: i64,
    pub dexy_out: i64,
}

/// 2% redemption fee applied (user gets 98% of proportional share).
pub fn calculate_lp_redeem(
    lp_to_burn: i64,
    reserves_x: i64,
    reserves_y: i64,
    lp_reserves: i64,
    initial_lp: i64,
) -> LpRedeemResult {
    let supply = initial_lp - lp_reserves;

    let erg_out = (lp_to_burn as i128 * reserves_x as i128 / supply as i128 * 98 / 100) as i64;
    let dexy_out = (lp_to_burn as i128 * reserves_y as i128 / supply as i128 * 98 / 100) as i64;

    LpRedeemResult { erg_out, dexy_out }
}

/// Blocked when LP rate < 98% of oracle rate (depeg protection).
pub fn can_redeem_lp(
    lp_erg_reserves: i64,
    lp_dexy_reserves: i64,
    oracle_rate_adjusted: i64,
) -> bool {
    if lp_dexy_reserves == 0 {
        return false;
    }
    let lp_rate = lp_erg_reserves / lp_dexy_reserves;
    lp_rate > oracle_rate_adjusted * 98 / 100
}

pub fn calculate_lp_swap_price_impact(
    input_amount: i64,
    reserves_sold: i64,
    reserves_bought: i64,
    fee_num: i64,
    fee_denom: i64,
) -> f64 {
    if reserves_sold == 0 || reserves_bought == 0 || input_amount == 0 {
        return 0.0;
    }
    let output = calculate_lp_swap_output(
        input_amount,
        reserves_sold,
        reserves_bought,
        fee_num,
        fee_denom,
    );
    let spot_rate = reserves_bought as f64 / reserves_sold as f64;
    let effective_rate = output as f64 / input_amount as f64;
    let fee_adjusted_spot = spot_rate * (1.0 - fee_num as f64 / fee_denom as f64);
    ((fee_adjusted_spot - effective_rate) / fee_adjusted_spot * 100.0).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_lp_rate() {
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 1_000_000_000_000,
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        assert_eq!(state.lp_rate_nano, 1_000_000_000);
    }

    #[test]
    fn test_rate_difference() {
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 900_000_000_000,
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        assert!(state.rate_difference_pct > 9.0);
        assert!(state.rate_difference_pct < 11.0);
    }

    #[test]
    fn test_cost_to_mint_gold() {
        let calc = cost_to_mint_dexy(100, 220_000, 0);
        assert_eq!(calc.erg_amount, 22_000_000);
    }

    #[test]
    fn test_cost_to_mint_use() {
        let calc = cost_to_mint_dexy(1000, 1_850_000, 3);
        assert_eq!(calc.erg_amount, 1_850_000_000);
    }

    #[test]
    fn test_can_mint_with_tokens() {
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 1_000_000_000_000,
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        assert!(state.can_mint);
    }

    #[test]
    fn test_cannot_mint_without_tokens() {
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 1_000_000_000_000,
            lp_dexy_reserves: 1000,
            dexy_in_bank: 0,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        assert!(!state.can_mint);
    }

    #[test]
    fn test_cannot_mint_when_lp_rate_too_low() {
        // lpRate=970M (97% of oracle) fails: 970M*100 > 1B*98 is false
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 970_000_000_000,
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        assert!(!state.can_mint);
    }

    #[test]
    fn test_can_mint_when_lp_rate_at_threshold() {
        // Exactly 98% fails because condition is strict >
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 980_000_000_000,
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        assert!(!state.can_mint);
    }

    #[test]
    fn test_can_mint_when_lp_rate_just_above_threshold() {
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 981_000_000_000,
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        assert!(state.can_mint);
    }

    #[test]
    fn test_lp_rate_zero_reserves() {
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 1_000_000_000_000,
            lp_dexy_reserves: 0,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        assert_eq!(state.lp_rate_nano, 0);
    }

    mod lp_swap_tests {
        use super::*;

        #[test]
        fn test_calculate_lp_swap_erg_to_dexy() {
            let result = calculate_lp_swap_output(
                1_000_000_000,
                500_000_000_000,
                500_000,
                3,
                1000,
            );
            assert!(result > 0);
        }

        #[test]
        fn test_calculate_lp_swap_dexy_to_erg() {
            let result = calculate_lp_swap_output(
                100,
                500_000,
                500_000_000_000,
                3,
                1000,
            );
            assert!(result > 0);
        }

        #[test]
        fn test_lp_swap_validate_matches_contract() {
            let reserves_x: i64 = 500_000_000_000;
            let reserves_y: i64 = 500_000;
            let delta_x: i64 = 1_000_000_000;
            let delta_y = calculate_lp_swap_output(delta_x, reserves_x, reserves_y, 3, 1000);

            // Contract feeNum = feeDenom - feeRate = 997
            let contract_fee_num: i128 = 997;
            let contract_fee_denom: i128 = 1000;
            let lhs = (reserves_y as i128) * (delta_x as i128) * contract_fee_num;
            let rhs = (delta_y as i128)
                * ((reserves_x as i128) * contract_fee_denom
                    + (delta_x as i128) * contract_fee_num);
            assert!(lhs >= rhs, "Contract validation failed: {} < {}", lhs, rhs);
        }

        #[test]
        fn test_validate_lp_swap_function() {
            let reserves_x: i64 = 500_000_000_000;
            let reserves_y: i64 = 500_000;
            let delta_x: i64 = 1_000_000_000;
            let delta_y = calculate_lp_swap_output(delta_x, reserves_x, reserves_y, 3, 1000);
            assert!(validate_lp_swap(
                reserves_x, reserves_y, delta_x, -delta_y, 3, 1000
            ));
        }

        #[test]
        fn test_price_impact_small_trade() {
            let impact = calculate_lp_swap_price_impact(
                1_000_000_000,
                500_000_000_000,
                500_000,
                3,
                1000,
            );
            assert!(impact < 1.0, "Impact too high: {}", impact);
        }

        #[test]
        fn test_price_impact_large_trade() {
            let impact = calculate_lp_swap_price_impact(
                100_000_000_000,
                500_000_000_000,
                500_000,
                3,
                1000,
            );
            assert!(impact > 1.0, "Impact too low for large trade: {}", impact);
        }
    }

    mod lp_deposit_redeem_tests {
        use super::*;

        #[test]
        fn test_calculate_lp_deposit_proportional() {
            let initial_lp: i64 = 100_000_000_000;
            let lp_reserves: i64 = 99_900_000_000;
            let erg_reserves: i64 = 1_000_000_000_000;
            let dexy_reserves: i64 = 500_000;

            let result = calculate_lp_deposit(
                10_000_000_000,
                5_000,
                erg_reserves,
                dexy_reserves,
                lp_reserves,
                initial_lp,
            );

            assert_eq!(result.lp_tokens_out, 1_000_000);
            assert_eq!(result.consumed_erg, 10_000_000_000);
            assert_eq!(result.consumed_dexy, 5_000);
        }

        #[test]
        fn test_calculate_lp_deposit_unbalanced_excess_dexy() {
            let initial_lp: i64 = 100_000_000_000;
            let lp_reserves: i64 = 99_900_000_000;
            let erg_reserves: i64 = 1_000_000_000_000;
            let dexy_reserves: i64 = 500_000;

            let result = calculate_lp_deposit(
                10_000_000_000,
                10_000,
                erg_reserves,
                dexy_reserves,
                lp_reserves,
                initial_lp,
            );

            assert_eq!(result.lp_tokens_out, 1_000_000);
            assert_eq!(result.consumed_erg, 10_000_000_000);
            assert_eq!(result.consumed_dexy, 5_000);
        }

        #[test]
        fn test_calculate_lp_deposit_unbalanced_excess_erg() {
            let initial_lp: i64 = 100_000_000_000;
            let lp_reserves: i64 = 99_900_000_000;
            let erg_reserves: i64 = 1_000_000_000_000;
            let dexy_reserves: i64 = 500_000;

            let result = calculate_lp_deposit(
                20_000_000_000,
                5_000,
                erg_reserves,
                dexy_reserves,
                lp_reserves,
                initial_lp,
            );

            assert_eq!(result.lp_tokens_out, 1_000_000);
            assert_eq!(result.consumed_erg, 10_000_000_000);
            assert_eq!(result.consumed_dexy, 5_000);
        }

        #[test]
        fn test_calculate_lp_redeem() {
            let initial_lp: i64 = 100_000_000_000;
            let lp_reserves: i64 = 99_900_000_000;
            let erg_reserves: i64 = 1_000_000_000_000;
            let dexy_reserves: i64 = 500_000;

            let result = calculate_lp_redeem(
                1_000_000,
                erg_reserves,
                dexy_reserves,
                lp_reserves,
                initial_lp,
            );

            assert_eq!(result.erg_out, 9_800_000_000);
            assert_eq!(result.dexy_out, 4_900);
        }

        #[test]
        fn test_can_redeem_lp_allowed() {
            assert!(can_redeem_lp(1_000_000_000_000, 500_000, 2_000_000));
        }

        #[test]
        fn test_can_redeem_lp_blocked() {
            assert!(!can_redeem_lp(490_000, 500_000, 1_000_000));
        }

        #[test]
        fn test_can_redeem_lp_zero_reserves() {
            assert!(!can_redeem_lp(1_000_000_000_000, 0, 2_000_000));
        }

        #[test]
        fn test_lp_deposit_real_world_use_pool() {
            // Real pool values that previously triggered "Script reduced to false"
            let initial_lp: i64 = 9_223_372_036_854_775_000;
            let lp_reserves: i64 = 9_223_371_891_932_916_706;
            let erg_reserves: i64 = 243_129_173_608_123;
            let dexy_reserves: i64 = 86_588_538;

            let result = calculate_lp_deposit(
                10_000_000,
                4,
                erg_reserves,
                dexy_reserves,
                lp_reserves,
                initial_lp,
            );

            assert_eq!(result.lp_tokens_out, 5960);

            // Verify consumed_dexy justifies the shares (contract validation)
            let supply = initial_lp - lp_reserves;
            let shares_check =
                (result.consumed_dexy as i128 * supply as i128 / dexy_reserves as i128) as i64;
            assert!(
                shares_check >= result.lp_tokens_out,
                "Contract would fail: {} shares but dexy only justifies {}",
                result.lp_tokens_out,
                shares_check
            );
        }
    }
}
