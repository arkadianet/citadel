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
