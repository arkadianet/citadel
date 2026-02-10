//! Dexy State Parsing
//!
//! Parses protocol state from bank, oracle, and LP boxes.

use serde::{Deserialize, Serialize};

use crate::constants::DexyVariant;

/// Dexy protocol state for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyState {
    /// Which variant (Gold or USD)
    pub variant: DexyVariant,

    // Bank state
    /// ERG value in bank box (nanoERG)
    pub bank_erg_nano: i64,
    /// Dexy tokens available in bank (can be minted)
    pub dexy_in_bank: i64,
    /// Bank box ID
    pub bank_box_id: String,
    /// Dexy token ID (the token users receive when minting)
    pub dexy_token_id: String,

    // FreeMint state
    /// FreeMint remaining this period (actual mintable via FreeMint)
    pub free_mint_available: i64,
    /// Height at which FreeMint period resets
    pub free_mint_reset_height: i32,
    /// Current blockchain height (for period calculation)
    pub current_height: i32,

    // Oracle state
    /// Oracle rate: nanoERG per 1 unit of underlying (gold/USD)
    pub oracle_rate_nano: i64,
    /// Oracle box ID
    pub oracle_box_id: String,

    // LP state (read-only context)
    /// ERG reserves in LP
    pub lp_erg_reserves: i64,
    /// Dexy token reserves in LP
    pub lp_dexy_reserves: i64,
    /// LP box ID
    pub lp_box_id: String,
    /// LP rate: nanoERG per Dexy token (calculated from reserves)
    pub lp_rate_nano: i64,

    // Derived state
    /// Whether minting is currently available
    pub can_mint: bool,
    /// Rate difference: (oracle_rate - lp_rate) / oracle_rate * 100
    /// Positive = arbitrage opportunity (mint and sell to LP)
    pub rate_difference_pct: f64,
    /// Circulating supply (minted - burned via LP)
    pub dexy_circulating: i64,
}

/// Raw bank box data
#[derive(Debug, Clone)]
pub struct DexyBankBoxData {
    pub box_id: String,
    pub erg_value: i64,
    pub dexy_tokens: i64,
    pub ergo_tree: String,
}

/// Raw oracle box data
#[derive(Debug, Clone)]
pub struct DexyOracleBoxData {
    pub box_id: String,
    pub rate_nano: i64,
}

/// Raw LP box data
#[derive(Debug, Clone)]
pub struct DexyLpBoxData {
    pub box_id: String,
    pub erg_reserves: i64,
    pub dexy_reserves: i64,
}

/// Raw FreeMint box data
#[derive(Debug, Clone)]
pub struct DexyFreeMintBoxData {
    pub box_id: String,
    /// R4: Height at which period resets
    pub reset_height: i32,
    /// R5: Remaining tokens available this period
    pub available: i64,
}

impl DexyState {
    /// Build state from parsed box data
    ///
    /// Note: This method uses the calculator module for derived state calculations.
    /// The oracle rate is adjusted by the variant's divisor to convert from raw
    /// oracle units (e.g., nanoERG per kg for gold) to nanoERG per token.
    #[allow(clippy::too_many_arguments)]
    pub fn from_boxes(
        variant: DexyVariant,
        bank: &DexyBankBoxData,
        oracle: &DexyOracleBoxData,
        lp: &DexyLpBoxData,
        free_mint: &DexyFreeMintBoxData,
        dexy_token_id: &str,
        current_height: i32,
        total_supply: i64,
    ) -> Self {
        use crate::calculator::{calculate_state, DexyInput};

        // Apply oracle divisor to get nanoERG per token
        // - Gold: raw is nanoERG per kg, divide by 1,000,000 for nanoERG per mg
        // - USD: raw is nanoERG per USD, divide by 1,000 for nanoERG per 0.001 USE
        let adjusted_oracle_rate = oracle.rate_nano / variant.oracle_divisor();

        let input = DexyInput {
            oracle_rate_nano: adjusted_oracle_rate,
            lp_erg_reserves: lp.erg_reserves,
            lp_dexy_reserves: lp.dexy_reserves,
            dexy_in_bank: bank.dexy_tokens,
            total_supply,
        };

        let calculated = calculate_state(&input);

        // Calculate FreeMint availability
        // If current height > reset height, period has reset and we use 1% of LP reserves
        let free_mint_available = if current_height > free_mint.reset_height {
            // Period reset - use 1% of LP reserves
            lp.dexy_reserves / 100
        } else {
            free_mint.available
        };

        Self {
            variant,
            bank_erg_nano: bank.erg_value,
            dexy_in_bank: bank.dexy_tokens,
            bank_box_id: bank.box_id.clone(),
            dexy_token_id: dexy_token_id.to_string(),
            free_mint_available,
            free_mint_reset_height: free_mint.reset_height,
            current_height,
            oracle_rate_nano: adjusted_oracle_rate,
            oracle_box_id: oracle.box_id.clone(),
            lp_erg_reserves: lp.erg_reserves,
            lp_dexy_reserves: lp.dexy_reserves,
            lp_box_id: lp.box_id.clone(),
            lp_rate_nano: calculated.lp_rate_nano,
            can_mint: calculated.can_mint,
            rate_difference_pct: calculated.rate_difference_pct,
            dexy_circulating: calculated.dexy_circulating,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bank_box_data() {
        let bank = DexyBankBoxData {
            box_id: "bank123".to_string(),
            erg_value: 1_000_000_000_000,
            dexy_tokens: 1_000_000,
            ergo_tree: "tree".to_string(),
        };
        assert_eq!(bank.box_id, "bank123");
        assert_eq!(bank.erg_value, 1_000_000_000_000);
    }

    #[test]
    fn test_oracle_box_data() {
        let oracle = DexyOracleBoxData {
            box_id: "oracle456".to_string(),
            rate_nano: 1_000_000_000,
        };
        assert_eq!(oracle.box_id, "oracle456");
        assert_eq!(oracle.rate_nano, 1_000_000_000);
    }

    #[test]
    fn test_lp_box_data() {
        let lp = DexyLpBoxData {
            box_id: "lp789".to_string(),
            erg_reserves: 500_000_000_000,
            dexy_reserves: 500_000,
        };
        assert_eq!(lp.box_id, "lp789");
        assert_eq!(lp.erg_reserves, 500_000_000_000);
    }

    #[test]
    fn test_free_mint_box_data() {
        let free_mint = DexyFreeMintBoxData {
            box_id: "freemint123".to_string(),
            reset_height: 1_000_000,
            available: 50_000,
        };
        assert_eq!(free_mint.box_id, "freemint123");
        assert_eq!(free_mint.reset_height, 1_000_000);
        assert_eq!(free_mint.available, 50_000);
    }

    #[test]
    fn test_dexy_state_from_boxes() {
        let bank = DexyBankBoxData {
            box_id: "bank123".to_string(),
            erg_value: 1_000_000_000_000,
            dexy_tokens: 9_999_999_000_000,
            ergo_tree: "tree".to_string(),
        };
        // For DexyGold, oracle gives nanoERG per kg
        // 1 DexyGold = 1 mg, so divide by 1,000,000 to get per-token rate
        // Raw value: 1_000_000_000_000 nanoERG/kg = 1_000_000 nanoERG/mg (after division)
        let oracle = DexyOracleBoxData {
            box_id: "oracle456".to_string(),
            rate_nano: 1_000_000_000_000, // 1000 ERG per kg (raw oracle value)
        };
        let lp = DexyLpBoxData {
            box_id: "lp789".to_string(),
            erg_reserves: 1_000_000_000_000, // 1000 ERG
            dexy_reserves: 1_000_000,        // 1M dexy tokens
        };
        let free_mint = DexyFreeMintBoxData {
            box_id: "freemint123".to_string(),
            reset_height: 1_000_000,
            available: 50_000,
        };
        let current_height = 999_500; // Before reset height

        let dexy_token_id = "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";
        let state = DexyState::from_boxes(
            DexyVariant::Gold,
            &bank,
            &oracle,
            &lp,
            &free_mint,
            dexy_token_id,
            current_height,
            10_000_000_000_000,
        );

        assert_eq!(state.variant, DexyVariant::Gold);
        assert_eq!(state.bank_box_id, "bank123");
        assert_eq!(state.dexy_token_id, dexy_token_id);
        assert_eq!(state.oracle_box_id, "oracle456");
        assert_eq!(state.lp_box_id, "lp789");
        // Adjusted oracle rate: 1_000_000_000_000 / 1_000_000 = 1_000_000 nanoERG per token
        assert_eq!(state.oracle_rate_nano, 1_000_000);
        assert_eq!(state.lp_rate_nano, 1_000_000); // 1M nanoERG per dexy
        assert!(state.can_mint); // LP rate equals oracle rate

        // FreeMint state (current_height < reset_height, so use available from box)
        assert_eq!(state.free_mint_available, 50_000);
        assert_eq!(state.free_mint_reset_height, 1_000_000);
        assert_eq!(state.current_height, 999_500);
    }

    #[test]
    fn test_dexy_state_from_boxes_with_reset_period() {
        let bank = DexyBankBoxData {
            box_id: "bank123".to_string(),
            erg_value: 1_000_000_000_000,
            dexy_tokens: 9_999_999_000_000,
            ergo_tree: "tree".to_string(),
        };
        let oracle = DexyOracleBoxData {
            box_id: "oracle456".to_string(),
            rate_nano: 1_000_000_000_000,
        };
        let lp = DexyLpBoxData {
            box_id: "lp789".to_string(),
            erg_reserves: 1_000_000_000_000,
            dexy_reserves: 1_000_000, // 1M dexy tokens in LP
        };
        let free_mint = DexyFreeMintBoxData {
            box_id: "freemint123".to_string(),
            reset_height: 1_000_000,
            available: 50_000, // This value should be ignored after reset
        };
        let current_height = 1_000_001; // After reset height

        let dexy_token_id = "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";
        let state = DexyState::from_boxes(
            DexyVariant::Gold,
            &bank,
            &oracle,
            &lp,
            &free_mint,
            dexy_token_id,
            current_height,
            10_000_000_000_000,
        );

        // After reset, free_mint_available should be 1% of LP dexy reserves
        // 1_000_000 / 100 = 10_000
        assert_eq!(state.free_mint_available, 10_000);
        assert_eq!(state.free_mint_reset_height, 1_000_000);
        assert_eq!(state.current_height, 1_000_001);
    }
}
