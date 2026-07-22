use std::collections::HashMap;

use super::*;

mod borrow_tests;
mod common_tests;
mod lend_tests;
mod refund_tests;
mod repay_tests;
mod withdraw_tests;

const TEST_ADDRESS: &str = "9hY16vzHmmfyVBwKeFGHvb2bMFsG94A1u7To1QWtUokACyFVENQ";

fn sample_utxo(box_id: &str, value: i64, assets: Vec<(String, i64)>) -> UserUtxo {
    UserUtxo {
        box_id: box_id.to_string(),
        tx_id: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".to_string(),
        index: 0,
        value,
        ergo_tree: "0008cd0327e65711a59378c59359c3571c6b49a4c25d28e5583b8fa2c99a7b4b5de5a34f"
            .to_string(),
        assets,
        creation_height: 1000000,
        registers: HashMap::new(),
    }
}
