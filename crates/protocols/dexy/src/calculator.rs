//! Dexy Protocol Calculator
//!
//! Pure math functions for calculating protocol state and mint costs.
//! No I/O, no async - just deterministic calculations.

/// Input state from boxes
#[derive(Debug, Clone)]
pub struct DexyInput {
    /// Oracle rate: nanoERG per 1 Dexy token
    pub oracle_rate_nano: i64,
    /// ERG reserves in LP
    pub lp_erg_reserves: i64,
    /// Dexy token reserves in LP
    pub lp_dexy_reserves: i64,
    /// Dexy tokens available in bank
    pub dexy_in_bank: i64,
    /// Total token supply (from emission)
    pub total_supply: i64,
}

/// Calculated protocol state
#[derive(Debug, Clone)]
pub struct DexyCalculatedState {
    /// LP rate: nanoERG per Dexy token (from AMM formula)
    pub lp_rate_nano: i64,
    /// Whether minting is available
    pub can_mint: bool,
    /// Rate difference percentage (oracle vs LP)
    pub rate_difference_pct: f64,
    /// Estimated circulating supply
    pub dexy_circulating: i64,
}

/// Calculate full protocol state from inputs
pub fn calculate_state(input: &DexyInput) -> DexyCalculatedState {
    // LP rate = erg_reserves / dexy_reserves (in nanoERG per token)
    let lp_rate_nano = if input.lp_dexy_reserves > 0 {
        input.lp_erg_reserves / input.lp_dexy_reserves
    } else {
        0
    };

    // Rate difference: (oracle - lp) / oracle * 100
    // Positive = oracle rate higher = arbitrage opportunity
    let rate_difference_pct = if input.oracle_rate_nano > 0 && lp_rate_nano > 0 {
        ((input.oracle_rate_nano - lp_rate_nano) as f64 / input.oracle_rate_nano as f64) * 100.0
    } else {
        0.0
    };

    // Can mint requires:
    // 1. Bank has tokens
    // 2. validRateFreeMint: lpRate * 100 > oracleRate * 98
    //    This ensures LP price is at least 98% of oracle price (prevents arbitrage attacks)
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

/// ERG calculation result
#[derive(Debug, Clone)]
pub struct ErgCalculation {
    /// ERG cost at oracle rate
    pub erg_amount: i64,
}

/// Calculate ERG cost to mint Dexy tokens
///
/// Dexy mint is straightforward: pay ERG at oracle rate, receive tokens.
/// No protocol fee in the Dexy emission contract (fees are in tx_builder).
///
/// Note: oracle_rate_nano should be the ADJUSTED rate (nanoERG per token),
/// not the raw oracle value. The adjustment is applied in DexyState::from_boxes().
pub fn cost_to_mint_dexy(amount: i64, oracle_rate_nano: i64, _decimals: u8) -> ErgCalculation {
    // Cost = amount * oracle_rate (rate is already per token)
    let erg_amount = amount * oracle_rate_nano;

    ErgCalculation { erg_amount }
}

/// Calculate LP swap output amount using constant product formula with fee.
///
/// This function is direction-agnostic:
/// - For ERG→Dexy: input_amount=ERG, reserves_sold=ERG reserves, reserves_bought=Dexy reserves
/// - For Dexy→ERG: input_amount=Dexy, reserves_sold=Dexy reserves, reserves_bought=ERG reserves
///
/// Formula from swap.es contract:
///   output = reserves_bought * input * feeNum / (reserves_sold * feeDenom + input * feeNum)
///
/// Note: `fee_num` and `fee_denom` represent the fee rate (e.g. 3/1000 = 0.3%).
/// The contract formula's feeNum is the pass-through portion: `feeDenom - fee_num`.
///
/// Uses i128 to prevent overflow on large reserve values.
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
    // Contract feeNum = feeDenom - feeRate (pass-through portion)
    let f_num = (fee_denom - fee_num) as i128;
    let f_denom = fee_denom as i128;

    let numerator = r_bought * input * f_num;
    let denominator = r_sold * f_denom + input * f_num;

    (numerator / denominator) as i64
}

/// Validate that a swap satisfies the contract's validation formula.
///
/// For selling X (deltaX > 0):
///   reservesYIn * deltaX * feeNum >= -deltaY * (reservesXIn * feeDenom + deltaX * feeNum)
///
/// Note: `fee_num` and `fee_denom` represent the fee rate (e.g. 3/1000 = 0.3%).
/// The contract formula's feeNum is the pass-through portion: `feeDenom - fee_num`.
///
/// Returns true if the swap is valid.
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
    // Contract feeNum = feeDenom - feeRate (pass-through portion)
    let fn_ = (fee_denom - fee_num) as i128;
    let fd = fee_denom as i128;

    if dx > 0 {
        // Selling X (ERG) for Y (Dexy)
        ry * dx * fn_ >= -dy * (rx * fd + dx * fn_)
    } else {
        // Selling Y (Dexy) for X (ERG)
        rx * dy * fn_ >= -dx * (ry * fd + dy * fn_)
    }
}

/// Result of LP deposit calculation
#[derive(Debug, Clone)]
pub struct LpDepositResult {
    /// LP tokens user will receive
    pub lp_tokens_out: i64,
    /// ERG actually consumed from deposit
    pub consumed_erg: i64,
    /// Dexy actually consumed from deposit
    pub consumed_dexy: i64,
}

/// Calculate LP deposit (mint LP tokens)
///
/// Returns LP tokens received and actual amounts consumed.
/// Uses i128 intermediates to prevent overflow (matches contract's BigInt).
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
        // ERG is limiting - all ERG consumed, compute consumed Dexy
        let consumed_y = (lp_tokens_out as i128 * reserves_y as i128 / supply as i128) as i64;
        (deposit_erg, consumed_y)
    } else {
        // Dexy is limiting - all Dexy consumed, compute consumed ERG
        let consumed_x = (lp_tokens_out as i128 * reserves_x as i128 / supply as i128) as i64;
        (consumed_x, deposit_dexy)
    };

    LpDepositResult {
        lp_tokens_out,
        consumed_erg,
        consumed_dexy,
    }
}

/// Result of LP redeem calculation
#[derive(Debug, Clone)]
pub struct LpRedeemResult {
    /// ERG user receives (after 2% fee)
    pub erg_out: i64,
    /// Dexy tokens user receives (after 2% fee)
    pub dexy_out: i64,
}

/// Calculate LP redeem (burn LP tokens)
///
/// Applies 2% redemption fee (user gets 98% of proportional share).
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

/// Check if LP redeem is allowed (oracle rate gate)
///
/// Redeem is blocked when LP rate < 98% of oracle rate (depeg protection).
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

/// Calculate price impact for an LP swap as a percentage.
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
            oracle_rate_nano: 1_000_000_000,    // 1 ERG per token
            lp_erg_reserves: 1_000_000_000_000, // 1000 ERG
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);

        // LP rate = 1000 ERG / 1000 tokens = 1 ERG per token
        assert_eq!(state.lp_rate_nano, 1_000_000_000);
    }

    #[test]
    fn test_rate_difference() {
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,  // 1 ERG (oracle)
            lp_erg_reserves: 900_000_000_000, // 900 ERG
            lp_dexy_reserves: 1000,           // LP rate = 0.9 ERG
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);

        // Oracle (1.0) > LP (0.9), difference = 10%
        assert!(state.rate_difference_pct > 9.0);
        assert!(state.rate_difference_pct < 11.0);
    }

    #[test]
    fn test_cost_to_mint_gold() {
        // DexyGold: 0 decimals, oracle_rate is already adjusted (nanoERG per mg)
        // 220_000 nanoERG per mg = 0.00022 ERG per token
        let calc = cost_to_mint_dexy(100, 220_000, 0);
        // 100 tokens * 220_000 nanoERG = 22_000_000 nanoERG = 0.022 ERG
        assert_eq!(calc.erg_amount, 22_000_000);
    }

    #[test]
    fn test_cost_to_mint_use() {
        // USE: 3 decimals, oracle_rate is already adjusted (nanoERG per 0.001 USE)
        // 1_850_000 nanoERG per token = 0.00185 ERG per smallest unit
        let calc = cost_to_mint_dexy(1000, 1_850_000, 3);
        // 1000 raw units (1 USE) * 1_850_000 = 1_850_000_000 nanoERG = 1.85 ERG
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
        // LP rate below 98% of oracle rate - minting should be disabled
        // validRateFreeMint requires: lpRate * 100 > oracleRate * 98
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,  // 1 ERG per token
            lp_erg_reserves: 970_000_000_000, // 970 ERG - gives lpRate = 970_000_000 (97% of oracle)
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        // lpRate = 970_000_000, oracleRate = 1_000_000_000
        // Check: 970_000_000 * 100 > 1_000_000_000 * 98
        //        97_000_000_000 > 98_000_000_000 - FALSE
        assert!(!state.can_mint);
    }

    #[test]
    fn test_can_mint_when_lp_rate_at_threshold() {
        // LP rate at exactly 98% of oracle rate - should still fail (need > not >=)
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 980_000_000_000, // 980 ERG - gives lpRate = 980_000_000
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        // lpRate = 980_000_000, oracleRate = 1_000_000_000
        // Check: 980_000_000 * 100 > 1_000_000_000 * 98
        //        98_000_000_000 > 98_000_000_000 - FALSE (not strictly greater)
        assert!(!state.can_mint);
    }

    #[test]
    fn test_can_mint_when_lp_rate_just_above_threshold() {
        // LP rate just above 98% of oracle rate - should succeed
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 981_000_000_000, // 981 ERG - gives lpRate = 981_000_000
            lp_dexy_reserves: 1000,
            dexy_in_bank: 10000,
            total_supply: 100000,
        };

        let state = calculate_state(&input);
        // lpRate = 981_000_000, oracleRate = 1_000_000_000
        // Check: 981_000_000 * 100 > 1_000_000_000 * 98
        //        98_100_000_000 > 98_000_000_000 - TRUE
        assert!(state.can_mint);
    }

    #[test]
    fn test_lp_rate_zero_reserves() {
        let input = DexyInput {
            oracle_rate_nano: 1_000_000_000,
            lp_erg_reserves: 1_000_000_000_000,
            lp_dexy_reserves: 0, // Zero tokens
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
            // Pool: 500 ERG, 500_000 Dexy. Sell 1 ERG.
            let result = calculate_lp_swap_output(
                1_000_000_000,   // 1 ERG input
                500_000_000_000, // reservesX (ERG)
                500_000,         // reservesY (Dexy)
                3,
                1000,
            );
            assert!(result > 0);
        }

        #[test]
        fn test_calculate_lp_swap_dexy_to_erg() {
            // Pool: 500 ERG, 500_000 Dexy. Sell 100 Dexy.
            let result = calculate_lp_swap_output(
                100,             // 100 Dexy input
                500_000,         // reservesY (Dexy) - "reserves of sold asset"
                500_000_000_000, // reservesX (ERG) - "reserves of bought asset"
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

            // Contract formula uses feeNum as pass-through portion: feeDenom - feeRate = 997
            // Contract: reservesYIn * deltaX * feeNum >= -deltaY * (reservesXIn * feeDenom + deltaX * feeNum)
            let contract_fee_num: i128 = 997; // 1000 - 3
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
            // delta_y is output (positive), but in contract terms pool loses dexy so it's negative
            assert!(validate_lp_swap(
                reserves_x, reserves_y, delta_x, -delta_y, 3, 1000
            ));
        }

        #[test]
        fn test_price_impact_small_trade() {
            let impact = calculate_lp_swap_price_impact(
                1_000_000_000,   // 1 ERG
                500_000_000_000, // 500 ERG reserves
                500_000,         // 500k Dexy reserves
                3,
                1000,
            );
            // Small trade relative to pool size - impact should be small
            assert!(impact < 1.0, "Impact too high: {}", impact);
        }

        #[test]
        fn test_price_impact_large_trade() {
            let impact = calculate_lp_swap_price_impact(
                100_000_000_000, // 100 ERG (20% of pool)
                500_000_000_000, // 500 ERG reserves
                500_000,         // 500k Dexy reserves
                3,
                1000,
            );
            // Large trade should have significant impact
            assert!(impact > 1.0, "Impact too low for large trade: {}", impact);
        }
    }

    mod lp_deposit_redeem_tests {
        use super::*;

        #[test]
        fn test_calculate_lp_deposit_proportional() {
            // Pool: 1000 ERG, 500K Dexy, 100B initialLp, 99.9B reserved = 100M circulating
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

            // supply = 100M. shares_by_x = 10B * 100M / 1000B = 1M. shares_by_y = 5000 * 100M / 500K = 1M
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

            // shares_by_x = 1M, shares_by_y = 2M -> limited by X
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

            // shares_by_x = 2M, shares_by_y = 1M -> limited by Y
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

            // raw_erg = 1M * 1000B / 100M = 10B. After 2% fee: 10B * 98 / 100 = 9.8B
            // raw_dexy = 1M * 500K / 100M = 5000. After 2% fee: 5000 * 98 / 100 = 4900
            assert_eq!(result.erg_out, 9_800_000_000);
            assert_eq!(result.dexy_out, 4_900);
        }

        #[test]
        fn test_can_redeem_lp_allowed() {
            // LP rate = 1000B / 500K = 2M. Oracle * 98/100 = 2M * 98/100 = 1.96M. 2M > 1.96M -> true
            assert!(can_redeem_lp(1_000_000_000_000, 500_000, 2_000_000));
        }

        #[test]
        fn test_can_redeem_lp_blocked() {
            // LP rate = 490K / 500K = 0. Oracle * 98/100 = 1M * 98/100 = 980K. 0 > 980K -> false
            assert!(!can_redeem_lp(490_000, 500_000, 1_000_000));
        }

        #[test]
        fn test_can_redeem_lp_zero_reserves() {
            assert!(!can_redeem_lp(1_000_000_000_000, 0, 2_000_000));
        }
    }
}
