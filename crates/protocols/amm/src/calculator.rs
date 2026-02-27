//! AMM Calculator
//!
//! Swap math using constant product formula (x * y = k).

use num_bigint::BigInt;
use num_traits::ToPrimitive;

/// Calculate swap output using constant product formula
///
/// Formula: output = (reserves_out * input * fee_num) / (reserves_in * fee_denom + input * fee_num)
pub fn calculate_output(
    reserves_in: u64,
    reserves_out: u64,
    input_amount: u64,
    fee_num: i32,
    fee_denom: i32,
) -> u64 {
    if reserves_in == 0 || reserves_out == 0 || input_amount == 0 {
        return 0;
    }

    let numerator = BigInt::from(reserves_out) * BigInt::from(input_amount) * BigInt::from(fee_num);
    let denominator = BigInt::from(reserves_in) * BigInt::from(fee_denom)
        + BigInt::from(input_amount) * BigInt::from(fee_num);

    if denominator == BigInt::from(0) {
        return 0;
    }

    let result = numerator / denominator;
    result.try_into().unwrap_or(0)
}

/// Calculate required input for desired output (reverse calculation)
///
/// Formula: input = (reserves_in * output * fee_denom) / ((reserves_out - output) * fee_num)
pub fn calculate_input(
    reserves_in: u64,
    reserves_out: u64,
    output_amount: u64,
    fee_num: i32,
    fee_denom: i32,
) -> Option<u64> {
    if reserves_in == 0 || reserves_out == 0 || output_amount == 0 {
        return None;
    }
    if output_amount >= reserves_out {
        return None; // Can't take more than reserves
    }

    let numerator =
        BigInt::from(reserves_in) * BigInt::from(output_amount) * BigInt::from(fee_denom);
    let denominator =
        (BigInt::from(reserves_out) - BigInt::from(output_amount)) * BigInt::from(fee_num);

    if denominator <= BigInt::from(0) {
        return None;
    }

    let result = (numerator / denominator) + BigInt::from(1); // Round up
    result.try_into().ok()
}

/// Calculate spot price (reserves_out / reserves_in)
pub fn calculate_spot_price(reserves_in: u64, reserves_out: u64) -> f64 {
    if reserves_in == 0 {
        return 0.0;
    }
    reserves_out as f64 / reserves_in as f64
}

/// Calculate price impact as percentage
pub fn calculate_price_impact(
    reserves_in: u64,
    reserves_out: u64,
    input_amount: u64,
    output_amount: u64,
) -> f64 {
    if input_amount == 0 || output_amount == 0 {
        return 0.0;
    }

    let spot_price = calculate_spot_price(reserves_in, reserves_out);
    let execution_price = output_amount as f64 / input_amount as f64;

    if spot_price == 0.0 {
        return 0.0;
    }

    ((spot_price - execution_price) / spot_price).abs() * 100.0
}

/// Calculate effective rate after fees
pub fn calculate_effective_rate(input_amount: u64, output_amount: u64) -> f64 {
    if input_amount == 0 {
        return 0.0;
    }
    output_amount as f64 / input_amount as f64
}

/// Apply slippage tolerance to output amount
pub fn apply_slippage(output: u64, slippage_percent: f64) -> u64 {
    let factor = 1.0 - (slippage_percent / 100.0);
    (output as f64 * factor) as u64
}

/// Suggest minimum output with default slippage (0.5%)
pub fn suggest_min_output(output: u64) -> u64 {
    apply_slippage(output, 0.5)
}

/// Calculate LP token circulating supply
pub fn calculate_lp_supply(locked_amount: u64, total_emission: i64) -> u64 {
    (total_emission as u64).saturating_sub(locked_amount)
}

/// Calculate share of pool for given LP amount
pub fn calculate_pool_share(lp_amount: u64, lp_supply: u64) -> f64 {
    if lp_supply == 0 {
        return 0.0;
    }
    (lp_amount as f64 / lp_supply as f64) * 100.0
}

/// Calculate LP token reward for a deposit.
///
/// reward = min(input_x * supply_lp / reserves_x, input_y * supply_lp / reserves_y)
///
/// Uses BigInt to prevent overflow (reserves and supply can be up to i64::MAX).
pub fn calculate_lp_reward(
    reserves_x: u64,
    reserves_y: u64,
    supply_lp: u64,
    input_x: u64,
    input_y: u64,
) -> u64 {
    if reserves_x == 0 || reserves_y == 0 || supply_lp == 0 {
        return 0;
    }
    let reward_x = BigInt::from(input_x) * BigInt::from(supply_lp) / BigInt::from(reserves_x);
    let reward_y = BigInt::from(input_y) * BigInt::from(supply_lp) / BigInt::from(reserves_y);
    let reward = reward_x.min(reward_y);
    reward.try_into().unwrap_or(0)
}

/// Calculate proportional token needed to match a given ERG input for deposit.
///
/// token_needed = input_erg * reserves_y / reserves_x
pub fn calculate_deposit_token_needed(reserves_x: u64, reserves_y: u64, input_x: u64) -> u64 {
    if reserves_x == 0 {
        return 0;
    }
    let result = BigInt::from(input_x) * BigInt::from(reserves_y) / BigInt::from(reserves_x);
    result.try_into().unwrap_or(0)
}

/// Calculate proportional ERG needed to match a given token input for deposit.
///
/// erg_needed = input_token * reserves_x / reserves_y
pub fn calculate_deposit_erg_needed(reserves_x: u64, reserves_y: u64, input_y: u64) -> u64 {
    if reserves_y == 0 {
        return 0;
    }
    let result = BigInt::from(input_y) * BigInt::from(reserves_x) / BigInt::from(reserves_y);
    result.try_into().unwrap_or(0)
}

/// Calculate user's share of pool reserves when redeeming LP tokens.
///
/// Returns (erg_out, token_out).
/// erg_out = lp_input * reserves_x / supply_lp
/// token_out = lp_input * reserves_y / supply_lp
pub fn calculate_redeem_shares(
    reserves_x: u64,
    reserves_y: u64,
    supply_lp: u64,
    lp_input: u64,
) -> (u64, u64) {
    if supply_lp == 0 {
        return (0, 0);
    }
    let erg_out = BigInt::from(lp_input) * BigInt::from(reserves_x) / BigInt::from(supply_lp);
    let token_out = BigInt::from(lp_input) * BigInt::from(reserves_y) / BigInt::from(supply_lp);
    (
        erg_out.try_into().unwrap_or(0),
        token_out.try_into().unwrap_or(0),
    )
}

use crate::state::{AmmPool, PoolType, SwapInput, SwapQuote, TokenAmount};

/// Calculate a swap quote for the given pool and input
pub fn quote_swap(pool: &AmmPool, input: &SwapInput) -> Option<SwapQuote> {
    match (pool.pool_type, input) {
        (PoolType::N2T, SwapInput::Erg { amount }) => {
            // Swap ERG for token Y
            let reserves_in = pool.erg_reserves?;
            let reserves_out = pool.token_y.amount;
            let output = calculate_output(
                reserves_in,
                reserves_out,
                *amount,
                pool.fee_num,
                pool.fee_denom,
            );

            if output == 0 {
                return None;
            }

            let price_impact = calculate_price_impact(reserves_in, reserves_out, *amount, output);
            let effective_rate = calculate_effective_rate(*amount, output);
            let fee_amount =
                (*amount as f64 * (1.0 - pool.fee_num as f64 / pool.fee_denom as f64)) as u64;

            Some(SwapQuote {
                input: input.clone(),
                output: TokenAmount {
                    token_id: pool.token_y.token_id.clone(),
                    amount: output,
                    decimals: pool.token_y.decimals,
                    name: pool.token_y.name.clone(),
                },
                price_impact,
                fee_amount,
                effective_rate,
                min_output_suggested: suggest_min_output(output),
            })
        }
        (PoolType::N2T, SwapInput::Token { token_id, amount }) => {
            // Swap token Y for ERG
            if token_id != &pool.token_y.token_id {
                return None;
            }
            let reserves_in = pool.token_y.amount;
            let reserves_out = pool.erg_reserves?;
            let output = calculate_output(
                reserves_in,
                reserves_out,
                *amount,
                pool.fee_num,
                pool.fee_denom,
            );

            if output == 0 {
                return None;
            }

            let price_impact = calculate_price_impact(reserves_in, reserves_out, *amount, output);
            let effective_rate = calculate_effective_rate(*amount, output);
            let fee_amount =
                (*amount as f64 * (1.0 - pool.fee_num as f64 / pool.fee_denom as f64)) as u64;

            Some(SwapQuote {
                input: input.clone(),
                output: TokenAmount {
                    token_id: "ERG".to_string(),
                    amount: output,
                    decimals: Some(9),
                    name: Some("ERG".to_string()),
                },
                price_impact,
                fee_amount,
                effective_rate,
                min_output_suggested: suggest_min_output(output),
            })
        }
        (PoolType::T2T, SwapInput::Token { token_id, amount }) => {
            // T2T swap
            let token_x = pool.token_x.as_ref()?;

            let (reserves_in, reserves_out, output_token) = if token_id == &token_x.token_id {
                // Swap X for Y
                (token_x.amount, pool.token_y.amount, &pool.token_y)
            } else if token_id == &pool.token_y.token_id {
                // Swap Y for X
                (pool.token_y.amount, token_x.amount, token_x)
            } else {
                return None;
            };

            let output = calculate_output(
                reserves_in,
                reserves_out,
                *amount,
                pool.fee_num,
                pool.fee_denom,
            );

            if output == 0 {
                return None;
            }

            let price_impact = calculate_price_impact(reserves_in, reserves_out, *amount, output);
            let effective_rate = calculate_effective_rate(*amount, output);
            let fee_amount =
                (*amount as f64 * (1.0 - pool.fee_num as f64 / pool.fee_denom as f64)) as u64;

            Some(SwapQuote {
                input: input.clone(),
                output: TokenAmount {
                    token_id: output_token.token_id.clone(),
                    amount: output,
                    decimals: output_token.decimals,
                    name: output_token.name.clone(),
                },
                price_impact,
                fee_amount,
                effective_rate,
                min_output_suggested: suggest_min_output(output),
            })
        }
        _ => None,
    }
}

/// Calculate initial LP share for pool creation using geometric mean.
///
/// Formula: sqrt(x_amount * y_amount)
/// Uses BigInt to prevent overflow since x_amount * y_amount can exceed u64::MAX.
///
/// Returns 0 if either amount is 0.
pub fn calculate_initial_lp_share(x_amount: u64, y_amount: u64) -> u64 {
    if x_amount == 0 || y_amount == 0 {
        return 0;
    }
    let x = BigInt::from(x_amount);
    let y = BigInt::from(y_amount);
    let product = x * y;
    let root = product.sqrt();
    root.to_u64().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_output() {
        // 1000 ERG reserves, 10000 token reserves, swap 10 ERG
        // With 0.3% fee (997/1000)
        let output = calculate_output(1_000_000_000_000, 10_000_000_000, 10_000_000_000, 997, 1000);
        // Expected ~99 tokens (minus fee and slippage)
        assert!(output > 0);
        assert!(output < 100_000_000); // Less than 1% of reserves
    }

    #[test]
    fn test_calculate_price_impact() {
        let impact = calculate_price_impact(1000, 2000, 100, 180);
        // Spot price = 2.0, execution price = 1.8, impact = 10%
        assert!((impact - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_apply_slippage() {
        let output = apply_slippage(1000, 0.5);
        assert_eq!(output, 995);
    }

    #[test]
    fn test_quote_swap_n2t() {
        use crate::state::{AmmPool, PoolType, SwapInput, TokenAmount};

        let pool = AmmPool {
            pool_id: "test".to_string(),
            pool_type: PoolType::N2T,
            box_id: "box".to_string(),
            erg_reserves: Some(1_000_000_000_000), // 1000 ERG
            token_x: None,
            token_y: TokenAmount {
                token_id: "token_y".to_string(),
                amount: 10_000_000,
                decimals: Some(6),
                name: Some("TestToken".to_string()),
            },
            lp_token_id: "lp".to_string(),
            lp_circulating: 1000,
            fee_num: 997,
            fee_denom: 1000,
        };

        let input = SwapInput::Erg {
            amount: 1_000_000_000,
        }; // 1 ERG
        let quote = quote_swap(&pool, &input).unwrap();

        assert!(quote.output.amount > 0);
        assert!(quote.price_impact > 0.0);
        assert!(quote.min_output_suggested < quote.output.amount);
    }

    #[test]
    fn test_calculate_lp_reward() {
        let reward = calculate_lp_reward(
            100_000_000_000,
            10_000_000,
            5000,
            10_000_000_000,
            1_000_000,
        );
        assert_eq!(reward, 500);
    }

    #[test]
    fn test_calculate_lp_reward_takes_minimum() {
        let reward = calculate_lp_reward(
            100_000_000_000,
            10_000_000,
            5000,
            20_000_000_000,
            1_000_000,
        );
        assert_eq!(reward, 500);
    }

    #[test]
    fn test_calculate_deposit_token_needed() {
        let needed =
            calculate_deposit_token_needed(100_000_000_000, 10_000_000, 10_000_000_000);
        assert_eq!(needed, 1_000_000);
    }

    #[test]
    fn test_calculate_deposit_erg_needed() {
        let needed = calculate_deposit_erg_needed(100_000_000_000, 10_000_000, 1_000_000);
        assert_eq!(needed, 10_000_000_000);
    }

    #[test]
    fn test_calculate_redeem_shares() {
        let (erg_out, token_out) =
            calculate_redeem_shares(100_000_000_000, 10_000_000, 5000, 500);
        assert_eq!(erg_out, 10_000_000_000);
        assert_eq!(token_out, 1_000_000);
    }

    #[test]
    fn test_calculate_redeem_shares_zero_supply() {
        let (erg, tok) = calculate_redeem_shares(100, 200, 0, 50);
        assert_eq!(erg, 0);
        assert_eq!(tok, 0);
    }

    #[test]
    fn test_initial_lp_share_basic() {
        assert_eq!(calculate_initial_lp_share(100, 400), 200);
    }

    #[test]
    fn test_initial_lp_share_equal_amounts() {
        assert_eq!(calculate_initial_lp_share(1000, 1000), 1000);
    }

    #[test]
    fn test_initial_lp_share_large_values() {
        // 1 ERG (1e9 nanoERG) * 1000 tokens (1e6 with 3 decimals)
        assert_eq!(
            calculate_initial_lp_share(1_000_000_000, 1_000_000),
            31_622_776
        );
    }

    #[test]
    fn test_initial_lp_share_zero() {
        assert_eq!(calculate_initial_lp_share(0, 1000), 0);
        assert_eq!(calculate_initial_lp_share(1000, 0), 0);
    }

    #[test]
    fn test_initial_lp_share_overflow_safe() {
        let x = u64::MAX / 2;
        let y = u64::MAX / 2;
        let result = calculate_initial_lp_share(x, y);
        assert!(result > 0);
    }
}
