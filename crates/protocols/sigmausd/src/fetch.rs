//! SigmaUSD State Fetching from Node
//!
//! Fetches bank and oracle boxes from node and parses into protocol state.

use citadel_core::{ProtocolError, TokenId};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_node_client::{NodeCapabilities, NodeClient};
use ergo_tx::ergo_box_utils::{extract_long, map_node_error};
use ergo_tx::{Eip12DataInputBox, Eip12InputBox};

use crate::constants::NftIds;
use crate::state::{BankBoxData, OracleBoxData};
use crate::{BoxId, SigmaUsdState};

/// Fetch SigmaUSD protocol state from node
pub async fn fetch_sigmausd_state(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    nft_ids: &NftIds,
) -> Result<SigmaUsdState, ProtocolError> {
    // Fetch bank box
    let bank_token_id = TokenId::new(&nft_ids.bank_nft);
    let bank_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &bank_token_id,
    )
    .await
    .map_err(|e| map_node_error(e, "SigmaUSD", "Bank box"))?;

    // Fetch oracle box
    let oracle_token_id = TokenId::new(&nft_ids.oracle_pool_nft);
    let oracle_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &oracle_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Oracle box not found: {}", e),
    })?;

    // Parse bank box
    let bank_data = parse_bank_box(&bank_box)?;

    // Parse oracle box
    let oracle_data = parse_oracle_box(&oracle_box)?;

    // Build state
    Ok(SigmaUsdState::from_boxes(&bank_data, &oracle_data))
}

/// Parse bank box into BankBoxData
pub(crate) fn parse_bank_box(ergo_box: &ErgoBox) -> Result<BankBoxData, ProtocolError> {
    // Get box ID as hex string
    let box_id = BoxId::new(ergo_box.box_id().to_string());

    // Get register values
    let r4 = ergo_box
        .additional_registers
        .get_constant(ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Bank box R4 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "Bank box missing R4 register".to_string(),
        })?;

    let r5 = ergo_box
        .additional_registers
        .get_constant(ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId::R5)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Bank box R5 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "Bank box missing R5 register".to_string(),
        })?;

    // Extract i64 values from registers
    let sigusd_circulating = extract_long(&r4).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse R4 (SigUSD circulating): {}", e),
    })?;

    let sigrsv_circulating = extract_long(&r5).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse R5 (SigRSV circulating): {}", e),
    })?;

    Ok(BankBoxData {
        box_id,
        value_nano: ergo_box.value.as_i64(),
        sigusd_circulating,
        sigrsv_circulating,
    })
}

/// Parse oracle box into OracleBoxData
pub(crate) fn parse_oracle_box(ergo_box: &ErgoBox) -> Result<OracleBoxData, ProtocolError> {
    // Get box ID as hex string
    let box_id = BoxId::new(ergo_box.box_id().to_string());

    // Get R4 register (oracle rate)
    let r4 = ergo_box
        .additional_registers
        .get_constant(ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Oracle box R4 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "Oracle box missing R4 register".to_string(),
        })?;

    let nanoerg_per_usd = extract_long(&r4).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse oracle R4 (ERG/USD rate): {}", e),
    })?;

    Ok(OracleBoxData {
        box_id,
        nanoerg_per_usd,
    })
}

/// Simple oracle price data
#[derive(Debug, Clone)]
pub struct OraclePrice {
    pub nanoerg_per_usd: i64,
    pub erg_usd: f64,
    pub oracle_box_id: String,
}

/// Fetch just the oracle price (lighter than full SigmaUSD state)
pub async fn fetch_oracle_price(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    nft_ids: &NftIds,
) -> Result<OraclePrice, ProtocolError> {
    let oracle_token_id = TokenId::new(&nft_ids.oracle_pool_nft);
    let oracle_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &oracle_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Oracle box not found: {}", e),
    })?;

    let oracle_data = parse_oracle_box(&oracle_box)?;

    let erg_usd = if oracle_data.nanoerg_per_usd > 0 {
        1_000_000_000.0 / oracle_data.nanoerg_per_usd as f64
    } else {
        0.0
    };

    Ok(OraclePrice {
        nanoerg_per_usd: oracle_data.nanoerg_per_usd,
        erg_usd,
        oracle_box_id: oracle_data.box_id.to_string(),
    })
}

/// Fetch context needed for building transactions
#[derive(Debug, Clone)]
pub struct TxBuildContext {
    pub bank_input: Eip12InputBox,
    pub bank_erg_nano: i64,
    pub sigusd_circulating: i64,
    pub sigrsv_circulating: i64,
    pub sigusd_in_bank: i64,
    pub sigrsv_in_bank: i64,
    pub oracle_data_input: Eip12DataInputBox,
    pub oracle_rate: i64,
    /// Raw bank box for transaction reduction
    pub bank_box: ErgoBox,
    /// Raw oracle box for transaction reduction
    pub oracle_box: ErgoBox,
}

/// Fetch bank and oracle boxes in EIP12 format for transaction building
pub async fn fetch_tx_context(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    nft_ids: &NftIds,
) -> Result<TxBuildContext, ProtocolError> {
    // Fetch bank box
    let bank_token_id = TokenId::new(&nft_ids.bank_nft);
    let bank_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &bank_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Bank box not found: {}", e),
    })?;

    // Fetch oracle box
    let oracle_token_id = TokenId::new(&nft_ids.oracle_pool_nft);
    let oracle_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &oracle_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Oracle box not found: {}", e),
    })?;

    // Parse state data
    let bank_data = parse_bank_box(&bank_box)?;
    let oracle_data = parse_oracle_box(&oracle_box)?;

    // Get transaction context for boxes (we need txId and index)
    let bank_tx_id = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &bank_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;
    let oracle_tx_id = ergo_node_client::queries::get_box_creation_info(
        client.inner(),
        &oracle_box.box_id().to_string(),
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get box creation info: {}", e),
    })?;

    // Convert to EIP12 format
    let bank_input = Eip12InputBox::from_ergo_box(&bank_box, bank_tx_id.0, bank_tx_id.1);
    let oracle_data_input =
        Eip12DataInputBox::from_ergo_box(&oracle_box, oracle_tx_id.0, oracle_tx_id.1);

    // Extract token amounts from bank box
    let (sigusd_in_bank, sigrsv_in_bank) = extract_bank_tokens(&bank_box, nft_ids)?;

    Ok(TxBuildContext {
        bank_input,
        bank_erg_nano: bank_data.value_nano,
        sigusd_circulating: bank_data.sigusd_circulating,
        sigrsv_circulating: bank_data.sigrsv_circulating,
        sigusd_in_bank,
        sigrsv_in_bank,
        oracle_data_input,
        oracle_rate: oracle_data.nanoerg_per_usd,
        bank_box,
        oracle_box,
    })
}

/// Extract SigUSD and SigRSV token amounts from bank box
pub(crate) fn extract_bank_tokens(
    bank_box: &ErgoBox,
    nft_ids: &NftIds,
) -> Result<(i64, i64), ProtocolError> {
    let tokens = bank_box
        .tokens
        .as_ref()
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "Bank box has no tokens".to_string(),
        })?;

    let mut sigusd_amount: i64 = 0;
    let mut sigrsv_amount: i64 = 0;

    for token in tokens.iter() {
        let token_id: String = token.token_id.into();
        if token_id == nft_ids.sigusd_token {
            sigusd_amount = *token.amount.as_u64() as i64;
        } else if token_id == nft_ids.sigrsv_token {
            sigrsv_amount = *token.amount.as_u64() as i64;
        }
    }

    Ok((sigusd_amount, sigrsv_amount))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
    use ergo_lib::ergotree_ir::chain::ergo_box::{BoxTokens, NonMandatoryRegisters};
    use ergo_lib::ergotree_ir::chain::token::{Token, TokenAmount, TokenId as ErgoTokenId};
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::types::stype::SType;
    use std::convert::TryFrom;

    fn test_ergo_tree() -> ErgoTree {
        let hex = "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let bytes = hex::decode(hex).unwrap();
        ErgoTree::sigma_parse_bytes(&bytes).unwrap()
    }

    fn make_token_id(hex_str: &str) -> ErgoTokenId {
        hex_str.parse().unwrap()
    }

    fn long_constant(val: i64) -> Constant {
        Constant {
            tpe: SType::SLong,
            v: Literal::Long(val),
        }
    }

    fn make_box_with_registers(value_nano: u64, registers: Vec<Constant>) -> ErgoBox {
        let regs = NonMandatoryRegisters::try_from(registers).unwrap();
        ErgoBox::new(
            BoxValue::new(value_nano).unwrap(),
            test_ergo_tree(),
            None,
            regs,
            100_000,
            TxId::zero(),
            0,
        )
        .unwrap()
    }

    // ==================== parse_bank_box tests ====================

    #[test]
    fn parse_bank_box_happy_path() {
        let sigusd_circ: i64 = 500_000_000;
        let sigrsv_circ: i64 = 100_000_000_000;
        let bank_value: u64 = 10_000_000_000_000;

        let regs = vec![long_constant(sigusd_circ), long_constant(sigrsv_circ)];
        let ergo_box = make_box_with_registers(bank_value, regs);
        let result = parse_bank_box(&ergo_box).unwrap();

        assert_eq!(result.value_nano, bank_value as i64);
        assert_eq!(result.sigusd_circulating, sigusd_circ);
        assert_eq!(result.sigrsv_circulating, sigrsv_circ);
    }

    #[test]
    fn parse_bank_box_missing_r4() {
        let ergo_box = make_box_with_registers(1_000_000_000, vec![]);
        let result = parse_bank_box(&ergo_box);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BoxParseError { message } => {
                assert!(
                    message.contains("R4"),
                    "Expected R4 in error, got: {}",
                    message
                );
            }
            other => panic!("Expected BoxParseError, got: {:?}", other),
        }
    }

    #[test]
    fn parse_bank_box_missing_r5() {
        // Only provide R4, R5 is missing
        let regs = vec![long_constant(500_000_000)];
        let ergo_box = make_box_with_registers(1_000_000_000, regs);
        let result = parse_bank_box(&ergo_box);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BoxParseError { message } => {
                assert!(
                    message.contains("R5"),
                    "Expected R5 in error, got: {}",
                    message
                );
            }
            other => panic!("Expected BoxParseError, got: {:?}", other),
        }
    }

    #[test]
    fn parse_bank_box_wrong_register_type() {
        // R4 is Int instead of Long
        let r4 = Constant {
            tpe: SType::SInt,
            v: Literal::Int(42),
        };
        let r5 = long_constant(100_000_000);
        let regs = vec![r4, r5];
        let ergo_box = make_box_with_registers(1_000_000_000, regs);
        let result = parse_bank_box(&ergo_box);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BoxParseError { message } => {
                assert!(
                    message.contains("R4"),
                    "Expected R4 in error, got: {}",
                    message
                );
            }
            other => panic!("Expected BoxParseError, got: {:?}", other),
        }
    }

    // ==================== parse_oracle_box tests ====================

    #[test]
    fn parse_oracle_box_happy_path() {
        let rate: i64 = 1_851_851_851; // ~0.54 USD per ERG
        let regs = vec![long_constant(rate)];
        let ergo_box = make_box_with_registers(1_000_000_000, regs);
        let result = parse_oracle_box(&ergo_box).unwrap();

        assert_eq!(result.nanoerg_per_usd, rate);
    }

    #[test]
    fn parse_oracle_box_missing_r4() {
        let ergo_box = make_box_with_registers(1_000_000_000, vec![]);
        let result = parse_oracle_box(&ergo_box);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProtocolError::BoxParseError { message } => {
                assert!(
                    message.contains("R4"),
                    "Expected R4 in error, got: {}",
                    message
                );
            }
            other => panic!("Expected BoxParseError, got: {:?}", other),
        }
    }

    // ==================== extract_bank_tokens tests ====================

    #[test]
    fn extract_bank_tokens_happy_path() {
        let sigusd_id = "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04";
        let sigrsv_id = "003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0";
        let bank_nft_id = "7d672d1def471720ca5782fd6473e47e796d9ac0c138d9911346f118b2f6d9d9";

        let nft_ids = NftIds {
            bank_nft: bank_nft_id.to_string(),
            sigusd_token: sigusd_id.to_string(),
            sigrsv_token: sigrsv_id.to_string(),
            oracle_pool_nft: "oracle".to_string(),
        };

        let tokens = BoxTokens::from_vec(vec![
            Token {
                token_id: make_token_id(bank_nft_id),
                amount: TokenAmount::try_from(1u64).unwrap(),
            },
            Token {
                token_id: make_token_id(sigusd_id),
                amount: TokenAmount::try_from(5_000_000u64).unwrap(),
            },
            Token {
                token_id: make_token_id(sigrsv_id),
                amount: TokenAmount::try_from(50_000_000u64).unwrap(),
            },
        ])
        .unwrap();

        let regs = vec![long_constant(100), long_constant(200)];
        let ergo_box = ErgoBox::new(
            BoxValue::new(1_000_000_000).unwrap(),
            test_ergo_tree(),
            Some(tokens),
            NonMandatoryRegisters::try_from(regs).unwrap(),
            100_000,
            TxId::zero(),
            0,
        )
        .unwrap();

        let (sigusd, sigrsv) = extract_bank_tokens(&ergo_box, &nft_ids).unwrap();
        assert_eq!(sigusd, 5_000_000);
        assert_eq!(sigrsv, 50_000_000);
    }

    #[test]
    fn extract_bank_tokens_no_tokens() {
        let nft_ids = NftIds {
            bank_nft: "bank".to_string(),
            sigusd_token: "sigusd".to_string(),
            sigrsv_token: "sigrsv".to_string(),
            oracle_pool_nft: "oracle".to_string(),
        };

        let ergo_box = make_box_with_registers(1_000_000_000, vec![]);
        let result = extract_bank_tokens(&ergo_box, &nft_ids);
        assert!(result.is_err());
    }

    #[test]
    fn extract_bank_tokens_missing_token() {
        // Box has tokens but not the SigUSD or SigRSV ones
        let other_id = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let nft_ids = NftIds {
            bank_nft: "bank".to_string(),
            sigusd_token: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            sigrsv_token: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_string(),
            oracle_pool_nft: "oracle".to_string(),
        };

        let tokens = BoxTokens::from_vec(vec![Token {
            token_id: make_token_id(other_id),
            amount: TokenAmount::try_from(1u64).unwrap(),
        }])
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

        // Should return (0, 0) when tokens not found (not an error)
        let (sigusd, sigrsv) = extract_bank_tokens(&ergo_box, &nft_ids).unwrap();
        assert_eq!(sigusd, 0);
        assert_eq!(sigrsv, 0);
    }
}
