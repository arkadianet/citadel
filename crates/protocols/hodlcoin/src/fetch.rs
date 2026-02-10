//! Bank Discovery and Fetching
//!
//! Functions for discovering HodlCoin banks from the Ergo node.

use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisterId};
use ergo_tx::ergo_box_utils::read_register_long;

use crate::calculator;
use crate::constants::{self, FEE_DENOM};
use crate::state::{HodlBankState, HodlError};

/// Parse a bank box into HodlBankState
pub fn parse_bank_box(ergo_box: &ErgoBox) -> Result<HodlBankState, HodlError> {
    let tokens = ergo_box
        .tokens
        .as_ref()
        .ok_or_else(|| HodlError::InvalidLayout("bank box has no tokens".to_string()))?;

    if tokens.len() < 2 {
        return Err(HodlError::InvalidLayout(format!(
            "bank box has {} tokens, expected at least 2",
            tokens.len()
        )));
    }

    // tokens[0] = singleton NFT (qty=1)
    let singleton = tokens
        .get(constants::bank_tokens::SINGLETON)
        .ok_or_else(|| {
            HodlError::InvalidLayout("missing singleton token at index 0".to_string())
        })?;
    let singleton_token_id = hex::encode(singleton.token_id.as_ref());
    let singleton_qty = u64::from(singleton.amount);
    if singleton_qty != 1 {
        return Err(HodlError::InvalidLayout(format!(
            "singleton token qty is {}, expected 1",
            singleton_qty
        )));
    }

    // tokens[1] = hodlToken
    let hodl_token = tokens
        .get(constants::bank_tokens::HODL_TOKEN)
        .ok_or_else(|| HodlError::InvalidLayout("missing hodl token at index 1".to_string()))?;
    let hodl_token_id = hex::encode(hodl_token.token_id.as_ref());
    let hodl_tokens_in_bank = u64::from(hodl_token.amount) as i64;

    // Box value = reserve nanoERG
    let reserve_nano_erg = u64::from(ergo_box.value) as i64;
    let bank_box_id = hex::encode(ergo_box.box_id().as_ref());

    // Read registers
    let total_token_supply = read_register_long(ergo_box, NonMandatoryRegisterId::R4)
        .ok_or_else(|| HodlError::InvalidLayout("missing R4 (total supply)".to_string()))?;

    let precision_factor = read_register_long(ergo_box, NonMandatoryRegisterId::R5)
        .ok_or_else(|| HodlError::InvalidLayout("missing R5 (precision)".to_string()))?;

    let min_bank_value = read_register_long(ergo_box, NonMandatoryRegisterId::R6)
        .ok_or_else(|| HodlError::InvalidLayout("missing R6 (min bank value)".to_string()))?;

    let dev_fee_num = read_register_long(ergo_box, NonMandatoryRegisterId::R7)
        .ok_or_else(|| HodlError::InvalidLayout("missing R7 (dev fee)".to_string()))?;

    let bank_fee_num = read_register_long(ergo_box, NonMandatoryRegisterId::R8)
        .ok_or_else(|| HodlError::InvalidLayout("missing R8 (bank fee)".to_string()))?;

    // Derived state
    let circulating_supply = total_token_supply - hodl_tokens_in_bank;

    let price_nano_per_hodl = if circulating_supply > 0 {
        let price = calculator::hodl_price(reserve_nano_erg, circulating_supply, precision_factor);
        price as f64 / precision_factor as f64
    } else {
        0.0
    };

    let total_fee_pct = (dev_fee_num + bank_fee_num) as f64 / FEE_DENOM as f64 * 100.0;
    let bank_fee_pct = bank_fee_num as f64 / FEE_DENOM as f64 * 100.0;
    let dev_fee_pct = dev_fee_num as f64 / FEE_DENOM as f64 * 100.0;

    Ok(HodlBankState {
        bank_box_id,
        singleton_token_id,
        hodl_token_id,
        hodl_token_name: None, // resolved later
        total_token_supply,
        precision_factor,
        min_bank_value,
        dev_fee_num,
        bank_fee_num,
        reserve_nano_erg,
        hodl_tokens_in_bank,
        circulating_supply,
        price_nano_per_hodl,
        tvl_nano_erg: reserve_nano_erg,
        total_fee_pct,
        bank_fee_pct,
        dev_fee_pct,
    })
}

/// Discover all hodlERG banks from the node
pub async fn discover_banks(
    node: &ergo_node_client::NodeClient,
) -> Result<Vec<HodlBankState>, HodlError> {
    let boxes = node
        .inner()
        .unspent_boxes_by_ergo_tree(constants::HODLERG_BANK_ERGO_TREE, 0, 100)
        .await
        .map_err(|e| HodlError::NodeError(e.to_string()))?;

    let mut banks = Vec::new();
    for ergo_box in &boxes {
        match parse_bank_box(ergo_box) {
            Ok(bank) => banks.push(bank),
            Err(e) => {
                tracing::warn!("Failed to parse HodlCoin bank: {}", e);
            }
        }
    }

    // Resolve hodlToken names
    for bank in &mut banks {
        match node.get_token_info(&bank.hodl_token_id).await {
            Ok(info) => {
                bank.hodl_token_name = info.name;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to resolve token name for {}: {}",
                    &bank.hodl_token_id[..8],
                    e
                );
            }
        }
    }

    // Filter out test/abandoned banks with negligible TVL
    banks.retain(|b| b.tvl_nano_erg >= constants::MIN_DISPLAY_TVL);

    // Sort by TVL descending
    banks.sort_by(|a, b| b.tvl_nano_erg.cmp(&a.tvl_nano_erg));

    tracing::info!("Discovered {} HodlCoin banks", banks.len());
    Ok(banks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
    use ergo_lib::ergotree_ir::chain::ergo_box::{BoxTokens, NonMandatoryRegisters};
    use ergo_lib::ergotree_ir::chain::token::{Token, TokenAmount, TokenId};
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::types::stype::SType;
    use std::convert::TryFrom;

    /// Simple P2PK ErgoTree for tests
    fn test_ergo_tree() -> ErgoTree {
        let hex = "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let bytes = hex::decode(hex).unwrap();
        ErgoTree::sigma_parse_bytes(&bytes).unwrap()
    }

    fn make_token_id(hex_str: &str) -> TokenId {
        hex_str.parse().unwrap()
    }

    fn long_constant(val: i64) -> Constant {
        Constant {
            tpe: SType::SLong,
            v: Literal::Long(val),
        }
    }

    /// Build a valid hodlcoin bank box with the specified parameters
    fn make_bank_box(
        value_nano: u64,
        singleton_amount: u64,
        hodl_amount: u64,
        registers: Vec<Constant>,
    ) -> ErgoBox {
        let singleton_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hodl_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        let singleton = Token {
            token_id: make_token_id(singleton_id),
            amount: TokenAmount::try_from(singleton_amount).unwrap(),
        };
        let hodl = Token {
            token_id: make_token_id(hodl_id),
            amount: TokenAmount::try_from(hodl_amount).unwrap(),
        };

        let tokens = BoxTokens::from_vec(vec![singleton, hodl]).unwrap();
        let regs = NonMandatoryRegisters::try_from(registers).unwrap();

        ErgoBox::new(
            BoxValue::new(value_nano).unwrap(),
            test_ergo_tree(),
            Some(tokens),
            regs,
            100_000,
            TxId::zero(),
            0,
        )
        .unwrap()
    }

    #[test]
    fn parse_bank_box_happy_path() {
        let total_supply: i64 = 1_000_000_000;
        let precision: i64 = 1_000_000;
        let min_bank: i64 = 1_000_000;
        let dev_fee: i64 = 30; // 3%
        let bank_fee: i64 = 20; // 2%
        let hodl_in_bank: u64 = 900_000_000;
        let reserve: u64 = 500_000_000_000; // 500 ERG

        let regs = vec![
            long_constant(total_supply),
            long_constant(precision),
            long_constant(min_bank),
            long_constant(dev_fee),
            long_constant(bank_fee),
        ];

        let ergo_box = make_bank_box(reserve, 1, hodl_in_bank, regs);
        let state = parse_bank_box(&ergo_box).unwrap();

        assert_eq!(state.total_token_supply, total_supply);
        assert_eq!(state.precision_factor, precision);
        assert_eq!(state.min_bank_value, min_bank);
        assert_eq!(state.dev_fee_num, dev_fee);
        assert_eq!(state.bank_fee_num, bank_fee);
        assert_eq!(state.hodl_tokens_in_bank, hodl_in_bank as i64);
        assert_eq!(state.reserve_nano_erg, reserve as i64);
        assert_eq!(state.tvl_nano_erg, reserve as i64);

        // Derived: circulating = total - in_bank = 1B - 900M = 100M
        let expected_circulating = total_supply - hodl_in_bank as i64;
        assert_eq!(state.circulating_supply, expected_circulating);

        // Fee percentages
        let expected_total_fee = (dev_fee + bank_fee) as f64 / FEE_DENOM as f64 * 100.0;
        assert!((state.total_fee_pct - expected_total_fee).abs() < 0.001);
        assert!((state.dev_fee_pct - 3.0).abs() < 0.001);
        assert!((state.bank_fee_pct - 2.0).abs() < 0.001);

        // Price should be positive when circulating > 0
        assert!(state.price_nano_per_hodl > 0.0);

        // Token IDs should be hex encoded
        assert_eq!(state.singleton_token_id.len(), 64);
        assert_eq!(state.hodl_token_id.len(), 64);

        // hodl_token_name should be None (resolved later)
        assert!(state.hodl_token_name.is_none());
    }

    #[test]
    fn parse_bank_box_no_tokens() {
        let regs = NonMandatoryRegisters::empty();
        let ergo_box = ErgoBox::new(
            BoxValue::new(1_000_000_000).unwrap(),
            test_ergo_tree(),
            None,
            regs,
            100_000,
            TxId::zero(),
            0,
        )
        .unwrap();

        let result = parse_bank_box(&ergo_box);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("no tokens"),
            "Expected 'no tokens' in error, got: {}",
            err_msg
        );
    }

    #[test]
    fn parse_bank_box_singleton_not_one() {
        let regs = vec![
            long_constant(1_000_000_000),
            long_constant(1_000_000),
            long_constant(1_000_000),
            long_constant(30),
            long_constant(20),
        ];

        // Singleton with quantity 2 instead of 1
        let ergo_box = make_bank_box(1_000_000_000, 2, 900_000_000, regs);
        let result = parse_bank_box(&ergo_box);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("singleton token qty is 2"),
            "Expected singleton qty error, got: {}",
            err_msg
        );
    }

    #[test]
    fn parse_bank_box_missing_r4() {
        // No registers at all
        let singleton_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hodl_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        let tokens = BoxTokens::from_vec(vec![
            Token {
                token_id: make_token_id(singleton_id),
                amount: TokenAmount::try_from(1u64).unwrap(),
            },
            Token {
                token_id: make_token_id(hodl_id),
                amount: TokenAmount::try_from(900_000_000u64).unwrap(),
            },
        ])
        .unwrap();

        let ergo_box = ErgoBox::new(
            BoxValue::new(1_000_000_000).unwrap(),
            test_ergo_tree(),
            Some(tokens),
            NonMandatoryRegisters::empty(),
            100_000,
            TxId::zero(),
            0,
        )
        .unwrap();

        let result = parse_bank_box(&ergo_box);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("R4"),
            "Expected R4 error, got: {}",
            err_msg
        );
    }

    #[test]
    fn parse_bank_box_missing_r7() {
        // Only provide R4, R5, R6 (missing R7 dev fee and R8 bank fee)
        let regs = vec![
            long_constant(1_000_000_000),
            long_constant(1_000_000),
            long_constant(1_000_000),
        ];

        let ergo_box = make_bank_box(1_000_000_000, 1, 900_000_000, regs);
        let result = parse_bank_box(&ergo_box);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("R7"),
            "Expected R7 error, got: {}",
            err_msg
        );
    }

    #[test]
    fn parse_bank_box_zero_circulating() {
        // All tokens still in bank (none minted), so circulating = 0
        let total_supply: i64 = 1_000_000_000;
        let hodl_in_bank: u64 = total_supply as u64;

        let regs = vec![
            long_constant(total_supply),
            long_constant(1_000_000),
            long_constant(1_000_000),
            long_constant(30),
            long_constant(20),
        ];

        let ergo_box = make_bank_box(1_000_000_000, 1, hodl_in_bank, regs);
        let state = parse_bank_box(&ergo_box).unwrap();

        assert_eq!(state.circulating_supply, 0);
        assert_eq!(state.price_nano_per_hodl, 0.0);
    }
}
