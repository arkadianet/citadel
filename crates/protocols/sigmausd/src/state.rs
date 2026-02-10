//! SigmaUSD State Parsing
//!
//! Parses protocol state from bank and oracle boxes.

use citadel_core::{BoxId, ProtocolError};
use ergo_tx::decode_sigma_long;
use serde::{Deserialize, Serialize};

use crate::calculator::{calculate_state, ProtocolInput, ProtocolState};

/// SigmaUSD protocol state for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdState {
    // Bank state
    pub bank_erg_nano: i64,
    pub sigusd_circulating: i64,
    pub sigrsv_circulating: i64,
    pub bank_box_id: String,

    // Oracle state
    pub oracle_erg_per_usd_nano: i64,
    pub oracle_box_id: String,

    // Derived state
    pub reserve_ratio_pct: f64,
    pub sigusd_price_nano: i64,
    pub sigrsv_price_nano: i64,
    pub liabilities_nano: i64,
    pub equity_nano: i64,

    // Action availability
    pub can_mint_sigusd: bool,
    pub can_mint_sigrsv: bool,
    pub can_redeem_sigusd: bool,
    pub can_redeem_sigrsv: bool,

    // Limits
    pub max_sigusd_mintable: i64,
    pub max_sigrsv_mintable: i64,
    pub max_sigrsv_redeemable: i64,
}

/// Raw box data needed to construct state
#[derive(Debug, Clone)]
pub struct BankBoxData {
    pub box_id: BoxId,
    pub value_nano: i64,
    pub sigusd_circulating: i64,
    pub sigrsv_circulating: i64,
}

#[derive(Debug, Clone)]
pub struct OracleBoxData {
    pub box_id: BoxId,
    pub nanoerg_per_usd: i64,
}

impl SigmaUsdState {
    /// Build state from parsed box data
    pub fn from_boxes(bank: &BankBoxData, oracle: &OracleBoxData) -> Self {
        let input = ProtocolInput {
            bank_erg_nano: bank.value_nano,
            sigusd_circulating: bank.sigusd_circulating,
            sigrsv_circulating: bank.sigrsv_circulating,
            nanoerg_per_usd: oracle.nanoerg_per_usd,
        };

        let state = calculate_state(&input);

        Self {
            bank_erg_nano: bank.value_nano,
            sigusd_circulating: bank.sigusd_circulating,
            sigrsv_circulating: bank.sigrsv_circulating,
            bank_box_id: bank.box_id.to_string(),

            oracle_erg_per_usd_nano: oracle.nanoerg_per_usd,
            oracle_box_id: oracle.box_id.to_string(),

            reserve_ratio_pct: state.reserve_ratio_pct,
            sigusd_price_nano: state.sigusd_price_nano,
            sigrsv_price_nano: state.sigrsv_price_nano,
            liabilities_nano: state.liabilities_nano as i64,
            equity_nano: state.equity_nano as i64,

            can_mint_sigusd: state.can_mint_sigusd,
            can_mint_sigrsv: state.can_mint_sigrsv,
            can_redeem_sigusd: true, // Always can redeem if have tokens
            can_redeem_sigrsv: state.can_redeem_sigrsv,

            max_sigusd_mintable: state.max_sigusd_mintable,
            max_sigrsv_mintable: state.max_sigrsv_mintable,
            max_sigrsv_redeemable: state.max_sigrsv_redeemable,
        }
    }

    /// Get the calculated protocol state
    pub fn protocol_state(&self) -> ProtocolState {
        let input = ProtocolInput {
            bank_erg_nano: self.bank_erg_nano,
            sigusd_circulating: self.sigusd_circulating,
            sigrsv_circulating: self.sigrsv_circulating,
            nanoerg_per_usd: self.oracle_erg_per_usd_nano,
        };
        calculate_state(&input)
    }
}

/// Parse bank box R4 and R5 registers to extract circulating supplies
///
/// R4 = SigUSD circulating (Sigma Long)
/// R5 = SigRSV circulating (Sigma Long)
pub fn parse_bank_registers(r4_hex: &str, r5_hex: &str) -> Result<(i64, i64), ProtocolError> {
    let sigusd = decode_sigma_long(r4_hex).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse R4 (SigUSD circulating): {}", e),
    })?;

    let sigrsv = decode_sigma_long(r5_hex).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse R5 (SigRSV circulating): {}", e),
    })?;

    Ok((sigusd, sigrsv))
}

/// Parse oracle box R4 register to extract ERG/USD rate
///
/// R4 = nanoERG per 1 USD (Sigma Long)
pub fn parse_oracle_register(r4_hex: &str) -> Result<i64, ProtocolError> {
    decode_sigma_long(r4_hex).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse oracle R4 (ERG/USD rate): {}", e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ergo_tx::encode_sigma_long;

    #[test]
    fn test_parse_bank_registers() {
        let sigusd = 500_000_000i64;
        let sigrsv = 100_000_000_000i64;

        let r4 = encode_sigma_long(sigusd);
        let r5 = encode_sigma_long(sigrsv);

        let (parsed_sigusd, parsed_sigrsv) = parse_bank_registers(&r4, &r5).unwrap();

        assert_eq!(parsed_sigusd, sigusd);
        assert_eq!(parsed_sigrsv, sigrsv);
    }

    #[test]
    fn test_parse_oracle_register() {
        let rate = 1_851_851_851i64;
        let r4 = encode_sigma_long(rate);

        let parsed = parse_oracle_register(&r4).unwrap();
        assert_eq!(parsed, rate);
    }

    #[test]
    fn test_state_from_boxes() {
        let bank = BankBoxData {
            box_id: BoxId::new("bank123"),
            value_nano: 10_000_000_000_000_000,
            sigusd_circulating: 500_000_000,
            sigrsv_circulating: 100_000_000_000,
        };

        let oracle = OracleBoxData {
            box_id: BoxId::new("oracle456"),
            nanoerg_per_usd: 1_851_851_851,
        };

        let state = SigmaUsdState::from_boxes(&bank, &oracle);

        assert_eq!(state.bank_erg_nano, bank.value_nano);
        assert_eq!(state.sigusd_circulating, bank.sigusd_circulating);
        assert!(state.reserve_ratio_pct > 0.0);
        assert!(state.sigusd_price_nano > 0);
    }
}
