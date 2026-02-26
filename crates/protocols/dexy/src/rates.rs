//! Dexy Rates Calculator
//!
//! Calculates rates and availability for all three minting paths:
//! - ArbMint: Arbitrage minting when LP rate > oracle rate
//! - FreeMint: Daily allocation from protocol (0.5% fee)
//! - LP Swap: Direct swap through liquidity pool (0% fee)

use serde::{Deserialize, Serialize};

use crate::state::DexyState;

// DexyVariant is used through state.variant in from_state()

/// Bank fee percentage (0.5% total: 0.3% bank + 0.2% buyback)
const BANK_FEE_PERCENT: f64 = 0.5;
/// LP swap fee percentage (0%)
const LP_FEE_PERCENT: f64 = 0.0;

/// Rates and availability for all minting paths
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexyRates {
    pub variant: String,
    pub token_name: String,
    pub token_decimals: u8,

    /// Oracle rate in nanoERG per token (adjusted)
    pub oracle_rate_nano: i64,
    /// ERG per display token
    pub erg_per_token: f64,
    /// Display tokens per ERG
    pub tokens_per_erg: f64,

    /// Peg description
    pub peg_description: String,

    /// All three minting paths
    pub paths: MintPaths,
}

/// Container for all three minting paths
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintPaths {
    pub arb_mint: MintPath,
    pub free_mint: MintPath,
    pub lp_swap: MintPath,
}

/// A single minting path with availability and rate information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintPath {
    pub name: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// ERG per display token (None if unavailable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erg_per_token: Option<f64>,
    /// Display tokens per ERG
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_per_erg: Option<f64>,
    /// Effective rate after fees (ERG per token)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_rate: Option<f64>,

    /// Max tokens for this path (raw units)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    /// Remaining today (FreeMint only, raw units)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_today: Option<i64>,

    /// Fee percentage (0.5 for bank paths, 0 for LP)
    pub fee_percent: f64,
    /// Is this the best rate among available paths?
    pub is_best_rate: bool,
}

impl DexyRates {
    /// Calculate rates from DexyState
    ///
    /// This computes availability and rates for all three minting paths:
    /// - ArbMint: Available when LP rate > oracle rate (arbitrage opportunity)
    /// - FreeMint: Available when free_mint_available > 0
    /// - LP Swap: Always available if LP has reserves
    pub fn from_state(state: &DexyState) -> Self {
        let variant = state.variant;
        let decimals = variant.decimals();
        let multiplier = 10_f64.powi(decimals as i32);

        // Oracle rate: convert nanoERG per raw token to ERG per display token
        // oracle_erg_per_display = (oracle_rate_nano / 1e9) * multiplier
        let oracle_erg_per_display = (state.oracle_rate_nano as f64 / 1e9) * multiplier;

        // LP rate: calculate from reserves
        let lp_rate_nano = if state.lp_dexy_reserves > 0 {
            state.lp_erg_reserves as f64 / state.lp_dexy_reserves as f64
        } else {
            0.0
        };
        let lp_erg_per_display = (lp_rate_nano / 1e9) * multiplier;

        // Calculate paths
        let arb_mint = Self::calculate_arb_mint(
            state,
            oracle_erg_per_display,
            lp_rate_nano,
            state.oracle_rate_nano as f64,
        );
        let free_mint = Self::calculate_free_mint(state, oracle_erg_per_display);
        let lp_swap = Self::calculate_lp_swap(state, lp_erg_per_display);

        // Determine best rate among available paths
        let mut paths = MintPaths {
            arb_mint,
            free_mint,
            lp_swap,
        };
        Self::mark_best_rate(&mut paths);

        // Tokens per ERG (using oracle rate as reference)
        let tokens_per_erg = if oracle_erg_per_display > 0.0 {
            1.0 / oracle_erg_per_display
        } else {
            0.0
        };

        DexyRates {
            variant: variant.as_str().to_string(),
            token_name: variant.token_name().to_string(),
            token_decimals: decimals,
            oracle_rate_nano: state.oracle_rate_nano,
            erg_per_token: oracle_erg_per_display,
            tokens_per_erg,
            peg_description: variant.peg_description().to_string(),
            paths,
        }
    }

    /// Calculate ArbMint path availability
    ///
    /// ArbMint is available when LP rate > oracle rate, allowing arbitrage:
    /// mint at oracle rate, sell on LP at higher rate.
    fn calculate_arb_mint(
        state: &DexyState,
        oracle_erg_per_display: f64,
        lp_rate_nano: f64,
        oracle_rate_nano: f64,
    ) -> MintPath {
        let available = lp_rate_nano > oracle_rate_nano && state.dexy_in_bank > 0;

        let (erg_per_token, tokens_per_erg, effective_rate, reason) = if available {
            // Effective rate includes 0.5% fee
            let effective = oracle_erg_per_display * (1.0 + BANK_FEE_PERCENT / 100.0);
            let tpe = if oracle_erg_per_display > 0.0 {
                1.0 / oracle_erg_per_display
            } else {
                0.0
            };
            (
                Some(oracle_erg_per_display),
                Some(tpe),
                Some(effective),
                None,
            )
        } else if state.dexy_in_bank == 0 {
            (None, None, None, Some("Bank has no tokens".to_string()))
        } else {
            (
                None,
                None,
                None,
                Some("LP rate <= oracle rate (no arbitrage)".to_string()),
            )
        };

        // Max tokens is limited by bank supply
        let max_tokens = if available {
            Some(state.dexy_in_bank)
        } else {
            None
        };

        MintPath {
            name: "ArbMint".to_string(),
            available,
            reason,
            erg_per_token,
            tokens_per_erg,
            effective_rate,
            max_tokens,
            remaining_today: None,
            fee_percent: BANK_FEE_PERCENT,
            is_best_rate: false,
        }
    }

    /// Calculate FreeMint path availability
    ///
    /// FreeMint is available when free_mint_available > 0 (daily allocation).
    fn calculate_free_mint(state: &DexyState, oracle_erg_per_display: f64) -> MintPath {
        let available = state.free_mint_available > 0 && state.dexy_in_bank > 0;

        let (erg_per_token, tokens_per_erg, effective_rate, reason) = if available {
            // Effective rate includes 0.5% fee
            let effective = oracle_erg_per_display * (1.0 + BANK_FEE_PERCENT / 100.0);
            let tpe = if oracle_erg_per_display > 0.0 {
                1.0 / oracle_erg_per_display
            } else {
                0.0
            };
            (
                Some(oracle_erg_per_display),
                Some(tpe),
                Some(effective),
                None,
            )
        } else if state.dexy_in_bank == 0 {
            (None, None, None, Some("Bank has no tokens".to_string()))
        } else {
            (
                None,
                None,
                None,
                Some("Daily allocation exhausted".to_string()),
            )
        };

        // Max tokens is the minimum of bank supply and free_mint_available
        let max_tokens = if available {
            Some(state.free_mint_available.min(state.dexy_in_bank))
        } else {
            None
        };

        MintPath {
            name: "FreeMint".to_string(),
            available,
            reason,
            erg_per_token,
            tokens_per_erg,
            effective_rate,
            max_tokens,
            remaining_today: Some(state.free_mint_available),
            fee_percent: BANK_FEE_PERCENT,
            is_best_rate: false,
        }
    }

    /// Calculate LP Swap path availability
    ///
    /// LP Swap is always available if LP has reserves (no fee).
    fn calculate_lp_swap(state: &DexyState, lp_erg_per_display: f64) -> MintPath {
        let available = state.lp_dexy_reserves > 0 && state.lp_erg_reserves > 0;

        let (erg_per_token, tokens_per_erg, effective_rate, reason) = if available {
            // LP has no protocol fee (effective rate = base rate)
            let tpe = if lp_erg_per_display > 0.0 {
                1.0 / lp_erg_per_display
            } else {
                0.0
            };
            (
                Some(lp_erg_per_display),
                Some(tpe),
                Some(lp_erg_per_display),
                None,
            )
        } else {
            (None, None, None, Some("LP has no liquidity".to_string()))
        };

        // Max tokens is limited by LP reserves (simplified - actual AMM would have slippage)
        let max_tokens = if available {
            Some(state.lp_dexy_reserves)
        } else {
            None
        };

        MintPath {
            name: "LP Swap".to_string(),
            available,
            reason,
            erg_per_token,
            tokens_per_erg,
            effective_rate,
            max_tokens,
            remaining_today: None,
            fee_percent: LP_FEE_PERCENT,
            is_best_rate: false,
        }
    }

    /// Mark the path with the best effective rate as is_best_rate
    ///
    /// Best rate means lowest ERG cost per token (lowest effective_rate).
    fn mark_best_rate(paths: &mut MintPaths) {
        let mut best_rate: Option<f64> = None;
        let mut best_path: Option<&str> = None;

        // Find best rate among available paths
        for (name, path) in [
            ("arb_mint", &paths.arb_mint),
            ("free_mint", &paths.free_mint),
            ("lp_swap", &paths.lp_swap),
        ] {
            if path.available {
                if let Some(effective) = path.effective_rate {
                    if best_rate.is_none() || effective < best_rate.unwrap() {
                        best_rate = Some(effective);
                        best_path = Some(name);
                    }
                }
            }
        }

        // Mark the best path
        if let Some(best) = best_path {
            match best {
                "arb_mint" => paths.arb_mint.is_best_rate = true,
                "free_mint" => paths.free_mint.is_best_rate = true,
                "lp_swap" => paths.lp_swap.is_best_rate = true,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::DexyVariant;
    use crate::state::{
        DexyBankBoxData, DexyFreeMintBoxData, DexyLpBoxData, DexyOracleBoxData, DexyState,
    };

    fn create_test_state(
        oracle_raw: i64,
        lp_erg: i64,
        lp_dexy: i64,
        free_mint_avail: i64,
        bank_tokens: i64,
    ) -> DexyState {
        let bank = DexyBankBoxData {
            box_id: "bank123".to_string(),
            erg_value: 1_000_000_000_000,
            dexy_tokens: bank_tokens,
            ergo_tree: "tree".to_string(),
        };
        let oracle = DexyOracleBoxData {
            box_id: "oracle456".to_string(),
            rate_nano: oracle_raw,
        };
        let lp = DexyLpBoxData {
            box_id: "lp789".to_string(),
            erg_reserves: lp_erg,
            dexy_reserves: lp_dexy,
            lp_token_reserves: 0,
        };
        let free_mint = DexyFreeMintBoxData {
            box_id: "freemint123".to_string(),
            reset_height: 1_000_000,
            available: free_mint_avail,
        };

        DexyState::from_boxes(
            DexyVariant::Gold,
            &bank,
            &oracle,
            &lp,
            &free_mint,
            "token123",
            999_500,
            10_000_000_000_000,
        )
    }

    #[test]
    fn test_rates_basic() {
        // Oracle: 1_000_000_000_000 nanoERG/kg -> 1_000_000 nanoERG/mg (after divisor)
        // LP: 1000 ERG / 1M tokens = 1_000_000 nanoERG/token
        let state = create_test_state(
            1_000_000_000_000, // Oracle raw (nanoERG per kg)
            1_000_000_000_000, // LP ERG reserves (1000 ERG)
            1_000_000,         // LP dexy reserves
            50_000,            // FreeMint available
            9_999_999_000_000, // Bank tokens
        );

        let rates = DexyRates::from_state(&state);

        assert_eq!(rates.variant, "gold");
        assert_eq!(rates.token_name, "DexyGold");
        assert_eq!(rates.token_decimals, 0);
        assert_eq!(rates.oracle_rate_nano, 1_000_000); // Adjusted rate
    }

    #[test]
    fn test_arb_mint_available() {
        // LP rate > oracle rate -> arb mint available
        // Oracle: 1_000_000 nanoERG/token
        // LP: 1100 ERG / 1M tokens = 1_100_000 nanoERG/token (10% higher)
        let state = create_test_state(
            1_000_000_000_000, // Oracle raw
            1_100_000_000_000, // LP ERG (1100 ERG - higher than oracle)
            1_000_000,         // LP dexy
            50_000,            // FreeMint available
            9_999_999_000_000, // Bank tokens
        );

        let rates = DexyRates::from_state(&state);

        assert!(rates.paths.arb_mint.available);
        assert!(rates.paths.arb_mint.reason.is_none());
        assert!(rates.paths.arb_mint.erg_per_token.is_some());
        assert_eq!(rates.paths.arb_mint.fee_percent, 0.5);
    }

    #[test]
    fn test_arb_mint_unavailable_no_arbitrage() {
        // LP rate <= oracle rate -> arb mint unavailable
        // Oracle: 1_000_000 nanoERG/token
        // LP: 900 ERG / 1M tokens = 900_000 nanoERG/token (lower)
        let state = create_test_state(
            1_000_000_000_000, // Oracle raw
            900_000_000_000,   // LP ERG (900 ERG - lower than oracle)
            1_000_000,         // LP dexy
            50_000,            // FreeMint available
            9_999_999_000_000, // Bank tokens
        );

        let rates = DexyRates::from_state(&state);

        assert!(!rates.paths.arb_mint.available);
        assert!(rates
            .paths
            .arb_mint
            .reason
            .as_ref()
            .unwrap()
            .contains("no arbitrage"));
    }

    #[test]
    fn test_free_mint_available() {
        let state = create_test_state(
            1_000_000_000_000,
            1_000_000_000_000,
            1_000_000,
            50_000, // FreeMint available
            9_999_999_000_000,
        );

        let rates = DexyRates::from_state(&state);

        assert!(rates.paths.free_mint.available);
        assert_eq!(rates.paths.free_mint.remaining_today, Some(50_000));
        assert_eq!(rates.paths.free_mint.fee_percent, 0.5);
    }

    #[test]
    fn test_free_mint_unavailable() {
        let state = create_test_state(
            1_000_000_000_000,
            1_000_000_000_000,
            1_000_000,
            0, // FreeMint exhausted
            9_999_999_000_000,
        );

        let rates = DexyRates::from_state(&state);

        assert!(!rates.paths.free_mint.available);
        assert!(rates
            .paths
            .free_mint
            .reason
            .as_ref()
            .unwrap()
            .contains("exhausted"));
        assert_eq!(rates.paths.free_mint.remaining_today, Some(0));
    }

    #[test]
    fn test_lp_swap_available() {
        let state = create_test_state(
            1_000_000_000_000,
            1_000_000_000_000,
            1_000_000,
            50_000,
            9_999_999_000_000,
        );

        let rates = DexyRates::from_state(&state);

        assert!(rates.paths.lp_swap.available);
        assert_eq!(rates.paths.lp_swap.fee_percent, 0.0);
        // LP swap effective rate should equal base rate (no fee)
        assert_eq!(
            rates.paths.lp_swap.effective_rate,
            rates.paths.lp_swap.erg_per_token
        );
    }

    #[test]
    fn test_lp_swap_unavailable_no_liquidity() {
        let state = create_test_state(
            1_000_000_000_000,
            0, // No LP ERG
            0, // No LP dexy
            50_000,
            9_999_999_000_000,
        );

        let rates = DexyRates::from_state(&state);

        assert!(!rates.paths.lp_swap.available);
        assert!(rates
            .paths
            .lp_swap
            .reason
            .as_ref()
            .unwrap()
            .contains("no liquidity"));
    }

    #[test]
    fn test_best_rate_lp_when_cheaper() {
        // LP rate < oracle rate -> LP is cheaper (best rate)
        let state = create_test_state(
            1_000_000_000_000, // Oracle
            800_000_000_000,   // LP ERG (800 ERG - cheaper)
            1_000_000,         // LP dexy
            50_000,            // FreeMint available
            9_999_999_000_000, // Bank tokens
        );

        let rates = DexyRates::from_state(&state);

        // LP swap should be best (cheaper than oracle + fee)
        assert!(rates.paths.lp_swap.is_best_rate);
        assert!(!rates.paths.free_mint.is_best_rate);
        assert!(!rates.paths.arb_mint.is_best_rate);
    }

    #[test]
    fn test_best_rate_bank_when_cheaper() {
        // LP rate > oracle rate -> bank paths are cheaper
        let state = create_test_state(
            1_000_000_000_000, // Oracle
            1_200_000_000_000, // LP ERG (1200 ERG - more expensive)
            1_000_000,         // LP dexy
            50_000,            // FreeMint available
            9_999_999_000_000, // Bank tokens
        );

        let rates = DexyRates::from_state(&state);

        // Bank paths (arb_mint or free_mint) should be best
        // Both have same rate, so either could be marked
        let bank_is_best = rates.paths.arb_mint.is_best_rate || rates.paths.free_mint.is_best_rate;
        assert!(bank_is_best);
    }

    #[test]
    fn test_effective_rate_includes_fee() {
        let state = create_test_state(
            1_000_000_000_000,
            1_000_000_000_000,
            1_000_000,
            50_000,
            9_999_999_000_000,
        );

        let rates = DexyRates::from_state(&state);

        // FreeMint effective rate should be base rate * 1.005 (0.5% fee)
        if let (Some(base), Some(effective)) = (
            rates.paths.free_mint.erg_per_token,
            rates.paths.free_mint.effective_rate,
        ) {
            let expected = base * 1.005;
            assert!((effective - expected).abs() < 1e-10);
        }
    }

    #[test]
    fn test_bank_empty() {
        let state = create_test_state(
            1_000_000_000_000,
            1_100_000_000_000, // Would be arb opportunity
            1_000_000,
            50_000, // FreeMint available
            0,      // Bank empty
        );

        let rates = DexyRates::from_state(&state);

        // Both bank paths should be unavailable
        assert!(!rates.paths.arb_mint.available);
        assert!(!rates.paths.free_mint.available);
        assert!(rates
            .paths
            .arb_mint
            .reason
            .as_ref()
            .unwrap()
            .contains("no tokens"));
        assert!(rates
            .paths
            .free_mint
            .reason
            .as_ref()
            .unwrap()
            .contains("no tokens"));
    }

    #[test]
    fn test_dexy_usd_decimals() {
        // Test with DexyUSD (3 decimals)
        let bank = DexyBankBoxData {
            box_id: "bank123".to_string(),
            erg_value: 1_000_000_000_000,
            dexy_tokens: 9_999_999_000_000,
            ergo_tree: "tree".to_string(),
        };
        // USD oracle: nanoERG per USD, divide by 1000 for per-token
        // Raw: 1_850_000_000 nanoERG/USD -> 1_850_000 nanoERG per 0.001 USE
        let oracle = DexyOracleBoxData {
            box_id: "oracle456".to_string(),
            rate_nano: 1_850_000_000, // 1.85 ERG per USD
        };
        let lp = DexyLpBoxData {
            box_id: "lp789".to_string(),
            erg_reserves: 1_850_000_000_000, // 1850 ERG
            dexy_reserves: 1_000_000,        // 1M tokens
            lp_token_reserves: 0,
        };
        let free_mint = DexyFreeMintBoxData {
            box_id: "freemint123".to_string(),
            reset_height: 1_000_000,
            available: 50_000,
        };

        let state = DexyState::from_boxes(
            DexyVariant::Usd,
            &bank,
            &oracle,
            &lp,
            &free_mint,
            "token123",
            999_500,
            10_000_000_000_000,
        );

        let rates = DexyRates::from_state(&state);

        assert_eq!(rates.variant, "usd");
        assert_eq!(rates.token_name, "DexyUSD");
        assert_eq!(rates.token_decimals, 3);
        // 1.85 ERG per USD = 0.00185 ERG per 0.001 USE
        // erg_per_token = (1_850_000 / 1e9) * 1000 = 0.00185 * 1000 = 1.85 ERG per display token (1 USE = 1000 raw)
        assert!((rates.erg_per_token - 1.85).abs() < 0.01);
    }
}
