use std::collections::HashMap;

use citadel_core::ProtocolError;
use ergo_tx::{with_test_dev_fee, DevFeeConfig, Eip12Asset, Eip12InputBox};

use super::*;
use crate::constants::DexyVariant;
use crate::state::DexyState;

mod lp_tests;
mod mint_tests;
mod swap_tests;

fn no_citadel_fee<R>(f: impl FnOnce() -> R) -> R {
    with_test_dev_fee(DevFeeConfig::disabled(), f)
}


// The builders decode box ids as hex, so fixtures need valid even-length hex.
const FREE_MINT_BOX_ID: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";
const BANK_BOX_ID: &str =
    "2222222222222222222222222222222222222222222222222222222222222222";
const BUYBACK_BOX_ID: &str =
    "3333333333333333333333333333333333333333333333333333333333333333";
const ORACLE_BOX_ID: &str =
    "4444444444444444444444444444444444444444444444444444444444444444";
const LP_BOX_ID: &str =
    "5555555555555555555555555555555555555555555555555555555555555555";

fn create_dummy_ergo_box() -> ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox {
    use ergo_lib::ergotree_ir::chain::ergo_box::{
        box_value::BoxValue, ErgoBox, NonMandatoryRegisters,
    };
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let ergo_tree_bytes = base16::decode(
        "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    )
    .unwrap();
    let ergo_tree = ErgoTree::sigma_parse_bytes(&ergo_tree_bytes).unwrap();
    let tx_id = TxId::zero();

    ErgoBox::new(
        BoxValue::new(1_000_000).unwrap(),
        ergo_tree,
        None,
        NonMandatoryRegisters::empty(),
        100000,
        tx_id,
        0,
    )
    .unwrap()
}

/// A protocol box in EIP-12 form. The builders read these; the `ErgoBox`
/// members of `DexyTxContext` are only needed by callers that sign.
fn dexy_protocol_input(
    box_id: &str,
    value: i64,
    tokens: Vec<(&str, i64)>,
) -> Eip12InputBox {
    Eip12InputBox {
        box_id: box_id.to_string(),
        transaction_id: format!("{box_id}_tx"),
        index: 0,
        value: value.to_string(),
        ergo_tree: format!("{box_id}_ergo_tree"),
        assets: tokens
            .into_iter()
            .map(|(id, amt)| Eip12Asset::new(id, amt))
            .collect(),
        creation_height: 100_000,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    }
}

fn dexy_data_input(box_id: &str, value: i64) -> ergo_tx::Eip12DataInputBox {
    ergo_tx::Eip12DataInputBox {
        box_id: box_id.to_string(),
        transaction_id: format!("{box_id}_tx"),
        index: 0,
        value: value.to_string(),
        ergo_tree: format!("{box_id}_ergo_tree"),
        assets: vec![],
        creation_height: 100_000,
        additional_registers: HashMap::new(),
    }
}

/// Context for a FreeMint build. `free_mint_r4_height` is deliberately low so
/// `validate_free_mint_preflight` treats the counter as reset, capping the
/// mintable amount at `lp_dexy_reserves / 100`.
fn create_mint_context(dexy_in_bank: i64, free_mint_available: i64) -> crate::fetch::DexyTxContext {
    let dummy = create_dummy_ergo_box();
    let dexy_token_id = create_test_state(dexy_in_bank, true).dexy_token_id;

    crate::fetch::DexyTxContext {
        free_mint_input: dexy_protocol_input(FREE_MINT_BOX_ID, 1_000_000, vec![("free_mint_nft", 1)]),
        free_mint_erg_nano: 1_000_000,
        free_mint_ergo_tree: "free_mint_box_ergo_tree".to_string(),
        free_mint_r4_height: 1,
        free_mint_r5_available: free_mint_available,
        free_mint_box: dummy.clone(),

        bank_input: dexy_protocol_input(
            "bank_box",
            1_000_000_000_000,
            vec![("bank_nft", 1), (dexy_token_id.as_str(), dexy_in_bank)],
        ),
        bank_erg_nano: 1_000_000_000_000,
        dexy_in_bank,
        bank_ergo_tree: "bank_box_ergo_tree".to_string(),
        bank_box: dummy.clone(),

        buyback_input: dexy_protocol_input(BUYBACK_BOX_ID, 1_000_000_000, vec![("buyback_nft", 1)]),
        buyback_erg_nano: 1_000_000_000,
        buyback_ergo_tree: "buyback_box_ergo_tree".to_string(),
        buyback_box: dummy.clone(),

        oracle_data_input: dexy_data_input(ORACLE_BOX_ID, 1_000_000),
        oracle_rate_nano: 1_000_000_000,
        oracle_box: dummy.clone(),

        lp_data_input: dexy_data_input(LP_BOX_ID, 500_000_000_000),
        lp_erg_reserves: 500_000_000_000,
        lp_dexy_reserves: 500_000,
        lp_box: dummy,
    }
}

fn create_test_state(dexy_in_bank: i64, can_mint: bool) -> DexyState {
    DexyState {
        variant: DexyVariant::Gold,
        bank_erg_nano: 1_000_000_000_000,
        dexy_in_bank,
        bank_box_id: "bank_box_123".to_string(),
        dexy_token_id: "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad"
            .to_string(),
        free_mint_available: 5_000,
        free_mint_reset_height: 1_000_000,
        current_height: 999_500,
        oracle_rate_nano: 1_000_000_000,
        oracle_box_id: "oracle_box_456".to_string(),
        lp_erg_reserves: 500_000_000_000,
        lp_dexy_reserves: 500_000,
        lp_box_id: "lp_box_789".to_string(),
        lp_rate_nano: 1_000_000,
        lp_token_reserves: 0,
        lp_circulating: 0,
        can_redeem_lp: true,
        can_mint,
        rate_difference_pct: 0.0,
        dexy_circulating: 0,
    }
}

fn create_test_input(value: i64, tokens: Vec<(&str, i64)>) -> Eip12InputBox {
    Eip12InputBox {
        box_id: "test_box".to_string(),
        transaction_id: "test_tx".to_string(),
        index: 0,
        value: value.to_string(),
        ergo_tree: "0008cd...".to_string(),
        assets: tokens
            .into_iter()
            .map(|(id, amt)| Eip12Asset::new(id, amt))
            .collect(),
        creation_height: 12345,
        additional_registers: HashMap::new(),
        extension: HashMap::new(),
    }
}
