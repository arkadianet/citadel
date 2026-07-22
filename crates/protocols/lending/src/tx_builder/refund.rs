use ergo_tx::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};
use std::collections::HashMap;

use super::common::miner_fee_output;
use super::{BuildError, ProxyBoxData, RefundResponse, MIN_BOX_VALUE_NANO, TX_FEE_NANO};

/// Lend/Withdraw/Borrow proxies: `proveDlog(userPk)` — 2 outputs, user spends anytime.
///
/// Repay proxies (NO proveDlog!): `operationPath` — 3 outputs triggers operation path
/// which checks OUTPUTS(0).propositionBytes == R5, value >= R4(=0), R4 == SELF.id.
/// This avoids the `refundPath` which requires R6[Int] (fails on old Long-encoded R6).
pub fn build_refund_tx(
    proxy_box: ProxyBoxData,
    current_height: i32,
) -> Result<RefundResponse, BuildError> {
    let use_three_outputs = proxy_box.is_repay_proxy;

    let min_required = if use_three_outputs {
        MIN_BOX_VALUE_NANO * 2 + TX_FEE_NANO
    } else {
        MIN_BOX_VALUE_NANO + TX_FEE_NANO
    };
    if proxy_box.value < min_required {
        return Err(BuildError::InsufficientBalance {
            required: min_required,
            available: proxy_box.value,
        });
    }

    let primary_value = if use_three_outputs {
        proxy_box.value - TX_FEE_NANO - MIN_BOX_VALUE_NANO
    } else {
        proxy_box.value - TX_FEE_NANO
    };

    let input = Eip12InputBox {
        box_id: proxy_box.box_id.clone(),
        transaction_id: proxy_box.tx_id.clone(),
        index: proxy_box.index,
        value: proxy_box.value.to_string(),
        ergo_tree: proxy_box.ergo_tree.clone(),
        assets: proxy_box
            .assets
            .iter()
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid.clone(),
                amount: amt.to_string(),
            })
            .collect(),
        creation_height: proxy_box.creation_height,
        additional_registers: proxy_box.additional_registers.clone(),
        extension: HashMap::new(),
    };

    // R4 = proxy box ID (required by operation path contract check)
    let refund_registers = ergo_tx::sigma_registers!("R4" => format!("0e20{}", proxy_box.box_id));

    let primary_output = Eip12Output {
        value: primary_value.to_string(),
        ergo_tree: proxy_box.user_ergo_tree.clone(),
        assets: proxy_box
            .assets
            .iter()
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid.clone(),
                amount: amt.to_string(),
            })
            .collect(),
        creation_height: current_height,
        additional_registers: refund_registers,
    };

    let fee_output = miner_fee_output(current_height);

    let outputs = if use_three_outputs {
        let dummy_output = Eip12Output {
            value: MIN_BOX_VALUE_NANO.to_string(),
            ergo_tree: proxy_box.user_ergo_tree.clone(),
            assets: vec![],
            creation_height: current_height,
            additional_registers: HashMap::new(),
        };
        vec![primary_output, dummy_output, fee_output]
    } else {
        vec![primary_output, fee_output]
    };

    let unsigned_tx = Eip12UnsignedTx {
        inputs: vec![input],
        data_inputs: vec![],
        outputs,
    };

    let unsigned_tx_json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;

    Ok(RefundResponse {
        unsigned_tx: unsigned_tx_json,
        fee_nano: TX_FEE_NANO,
        refundable_after_height: proxy_box.r6_refund_height,
    })
}
