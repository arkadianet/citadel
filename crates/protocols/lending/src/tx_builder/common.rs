use std::collections::HashMap;

use ergo_tx::{Eip12Asset, Eip12InputBox, Eip12Output, Eip12UnsignedTx};

use super::{BuildError, UserUtxo, TX_FEE_NANO};

#[derive(Debug, Clone)]
pub struct SelectedInputs {
    pub boxes: Vec<UserUtxo>,
    pub total_erg: i64,
    pub token_amount: i64,
}

/// Selects largest-first until ERG requirement is met.
pub fn select_erg_inputs(
    utxos: &[UserUtxo],
    required_erg: i64,
) -> Result<SelectedInputs, BuildError> {
    let mut selected = Vec::new();
    let mut total = 0i64;

    let mut sorted_utxos: Vec<_> = utxos.iter().collect();
    sorted_utxos.sort_by_key(|b| std::cmp::Reverse(b.value));

    for utxo in sorted_utxos {
        if total >= required_erg {
            break;
        }
        selected.push(utxo.clone());
        total += utxo.value;
    }

    if total < required_erg {
        return Err(BuildError::InsufficientBalance {
            required: required_erg,
            available: total,
        });
    }

    Ok(SelectedInputs {
        boxes: selected,
        total_erg: total,
        token_amount: 0,
    })
}

/// Selects boxes with the required token first, then adds more for ERG if needed.
pub fn select_token_inputs(
    utxos: &[UserUtxo],
    token_id: &str,
    required_amount: i64,
    min_erg: i64,
) -> Result<SelectedInputs, BuildError> {
    let mut selected = Vec::new();
    let mut total_erg = 0i64;
    let mut total_tokens = 0i64;

    for utxo in utxos {
        for (tid, amt) in &utxo.assets {
            if tid == token_id {
                selected.push(utxo.clone());
                total_erg += utxo.value;
                total_tokens += amt;
                break;
            }
        }
        if total_tokens >= required_amount && total_erg >= min_erg {
            break;
        }
    }

    if total_tokens < required_amount {
        return Err(BuildError::InsufficientTokens {
            token: token_id.to_string(),
            required: required_amount,
            available: total_tokens,
        });
    }

    if total_erg < min_erg {
        for utxo in utxos {
            if selected.iter().any(|u| u.box_id == utxo.box_id) {
                continue;
            }
            selected.push(utxo.clone());
            total_erg += utxo.value;
            if total_erg >= min_erg {
                break;
            }
        }
    }

    if total_erg < min_erg {
        return Err(BuildError::InsufficientBalance {
            required: min_erg,
            available: total_erg,
        });
    }

    Ok(SelectedInputs {
        boxes: selected,
        total_erg,
        token_amount: total_tokens,
    })
}

pub(crate) fn user_utxo_to_eip12(utxo: &UserUtxo) -> Eip12InputBox {
    Eip12InputBox {
        box_id: utxo.box_id.clone(),
        transaction_id: utxo.tx_id.clone(),
        index: utxo.index,
        value: utxo.value.to_string(),
        ergo_tree: utxo.ergo_tree.clone(),
        assets: utxo
            .assets
            .iter()
            .map(|(tid, amt)| Eip12Asset {
                token_id: tid.clone(),
                amount: amt.to_string(),
            })
            .collect(),
        creation_height: utxo.creation_height,
        additional_registers: utxo.registers.clone(),
        extension: HashMap::new(),
    }
}

pub(crate) fn to_ergo_tx_selected(
    local: &SelectedInputs,
    eip12_boxes: Vec<Eip12InputBox>,
) -> ergo_tx::SelectedInputs {
    ergo_tx::SelectedInputs {
        boxes: eip12_boxes,
        total_erg: local.total_erg as u64,
        token_amount: local.token_amount as u64,
    }
}

pub(crate) fn resolve_user_ergo_tree(address: &str) -> Result<(String, Vec<u8>), BuildError> {
    let tree = ergo_tx::address::address_to_ergo_tree(address)
        .map_err(|e| BuildError::InvalidAddress(e.to_string()))?;
    let bytes = hex::decode(&tree)
        .map_err(|e| BuildError::TxBuildError(format!("Invalid user ErgoTree: {}", e)))?;
    Ok((tree, bytes))
}

pub(crate) fn miner_fee_output(current_height: i32) -> Eip12Output {
    Eip12Output::fee(TX_FEE_NANO, current_height)
}

pub(crate) fn finalize_proxy_tx(
    eip12_inputs: Vec<Eip12InputBox>,
    outputs: Vec<Eip12Output>,
    proxy_ergo_tree: &str,
) -> Result<(String, String), BuildError> {
    let unsigned_tx = Eip12UnsignedTx {
        inputs: eip12_inputs,
        data_inputs: vec![],
        outputs,
    };
    let json = serde_json::to_string(&unsigned_tx)
        .map_err(|e| BuildError::TxBuildError(format!("Failed to serialize tx: {}", e)))?;
    let addr = ergo_tx::address::ergo_tree_to_address(proxy_ergo_tree)
        .map_err(|e| BuildError::TxBuildError(e.to_string()))?;
    Ok((json, addr))
}
