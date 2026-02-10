//! HodlCoin Calculator
//!
//! Pure math functions for mint/burn calculations. No async, no node.
//!
//! The hodlToken price formula:
//!   price = (reserve * precision) / circulating
//!
//! On mint: user deposits ERG, receives hodlTokens at current price.
//! On burn: user returns hodlTokens, receives ERG minus fees.
//!   The bank keeps bank_fee, dev contract receives dev_fee.

use crate::constants::FEE_DENOM;

/// Result of a burn calculation
#[derive(Debug, Clone)]
pub struct BurnResult {
    /// ERG the user receives (after all fees)
    pub erg_to_user: i64,
    /// ERG that stays in the bank (bank fee)
    pub bank_fee: i64,
    /// ERG sent to dev fee contract
    pub dev_fee: i64,
    /// Total ERG value before fees
    pub before_fees: i64,
}

/// Calculate the hodlToken price in nanoERG (scaled by precision).
///
/// price = (reserve * precision) / circulating
///
/// Returns 0 if circulating is 0.
pub fn hodl_price(reserve_nano: i64, circulating: i64, precision: i64) -> i64 {
    if circulating <= 0 {
        return 0;
    }
    // Use i128 to avoid overflow
    let num = reserve_nano as i128 * precision as i128;
    (num / circulating as i128) as i64
}

/// Calculate how many hodlTokens a user receives for depositing `erg_to_deposit` nanoERG.
///
/// tokens = (erg_to_deposit * precision) / price
///        = (erg_to_deposit * circulating) / reserve
pub fn mint_amount(
    reserve_nano: i64,
    circulating: i64,
    precision: i64,
    erg_to_deposit: i64,
) -> i64 {
    if reserve_nano <= 0 || circulating <= 0 || erg_to_deposit <= 0 {
        return 0;
    }
    let price = hodl_price(reserve_nano, circulating, precision);
    if price <= 0 {
        return 0;
    }
    // tokens = (erg_to_deposit * precision) / price
    let num = erg_to_deposit as i128 * precision as i128;
    (num / price as i128) as i64
}

/// Calculate the ERG received when burning `hodl_to_burn` tokens, after fees.
pub fn burn_amount(
    reserve_nano: i64,
    circulating: i64,
    precision: i64,
    hodl_to_burn: i64,
    bank_fee_num: i64,
    dev_fee_num: i64,
) -> BurnResult {
    if reserve_nano <= 0 || circulating <= 0 || hodl_to_burn <= 0 {
        return BurnResult {
            erg_to_user: 0,
            bank_fee: 0,
            dev_fee: 0,
            before_fees: 0,
        };
    }

    let price = hodl_price(reserve_nano, circulating, precision);
    // before_fees = (hodl_to_burn * price) / precision
    let before_fees = ((hodl_to_burn as i128 * price as i128) / precision as i128) as i64;

    let bank_fee = ((before_fees as i128 * bank_fee_num as i128) / FEE_DENOM as i128) as i64;
    let dev_fee = ((before_fees as i128 * dev_fee_num as i128) / FEE_DENOM as i128) as i64;
    let erg_to_user = before_fees - bank_fee - dev_fee;

    BurnResult {
        erg_to_user,
        bank_fee,
        dev_fee,
        before_fees,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hodl_price_basic() {
        // 100 ERG reserve, 1000 tokens circulating, precision 1_000_000_000
        let price = hodl_price(100_000_000_000, 1000, 1_000_000_000);
        // price = (100e9 * 1e9) / 1000 = 1e20 / 1e3 = 1e17
        assert_eq!(price, 100_000_000_000_000_000);
    }

    #[test]
    fn test_hodl_price_zero_circulating() {
        assert_eq!(hodl_price(100_000_000_000, 0, 1_000_000_000), 0);
    }

    #[test]
    fn test_mint_basic() {
        // 100 ERG reserve, 1000 tokens circulating, precision 1e9
        // Depositing 10 ERG should yield ~100 tokens
        let tokens = mint_amount(100_000_000_000, 1000, 1_000_000_000, 10_000_000_000);
        assert_eq!(tokens, 100);
    }

    #[test]
    fn test_mint_zero_deposit() {
        assert_eq!(mint_amount(100_000_000_000, 1000, 1_000_000_000, 0), 0);
    }

    #[test]
    fn test_burn_basic() {
        // 100 ERG reserve, 1000 tokens circulating, precision 1e9
        // Burn 100 tokens with bank_fee=3/1000, dev_fee=1/1000
        let result = burn_amount(100_000_000_000, 1000, 1_000_000_000, 100, 3, 1);

        // before_fees = 100 * price / precision = 100 * 100e9 * 1e9 / 1000 / 1e9 = 10 ERG
        assert_eq!(result.before_fees, 10_000_000_000);

        // bank_fee = 10e9 * 3 / 1000 = 30_000_000
        assert_eq!(result.bank_fee, 30_000_000);

        // dev_fee = 10e9 * 1 / 1000 = 10_000_000
        assert_eq!(result.dev_fee, 10_000_000);

        // erg_to_user = 10e9 - 30e6 - 10e6 = 9_960_000_000
        assert_eq!(result.erg_to_user, 9_960_000_000);
    }

    #[test]
    fn test_burn_zero_tokens() {
        let result = burn_amount(100_000_000_000, 1000, 1_000_000_000, 0, 3, 1);
        assert_eq!(result.erg_to_user, 0);
        assert_eq!(result.before_fees, 0);
    }

    #[test]
    fn test_mint_burn_roundtrip() {
        // Minting and immediately burning should lose only the fees
        let reserve = 100_000_000_000i64; // 100 ERG
        let circulating = 1000i64;
        let precision = 1_000_000_000i64;
        let deposit = 10_000_000_000i64; // 10 ERG

        let minted = mint_amount(reserve, circulating, precision, deposit);
        assert!(minted > 0);

        // After mint, new reserve = 110 ERG, new circulating = 1000 + minted
        let new_reserve = reserve + deposit;
        let new_circulating = circulating + minted;

        let burn = burn_amount(new_reserve, new_circulating, precision, minted, 3, 1);

        // User gets back less than they deposited (fees taken)
        assert!(burn.erg_to_user < deposit);
        // But not too much less (fees are 0.4% total)
        assert!(burn.erg_to_user > deposit * 99 / 100);
    }
}
