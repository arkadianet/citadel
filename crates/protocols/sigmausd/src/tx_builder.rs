//! SigmaUSD Transaction Builder
//!
//! - Bank box MUST be input[0] (contract requirement)
//! - Data inputs require FULL box data, not just box ID
//! - Token order in bank output must match bank input exactly

use std::fmt;
use std::str::FromStr;

use citadel_core::{constants, ProtocolError, TxError};
use ergo_tx::{
    append_change_output, collect_change_tokens, encode_sigma_long, select_inputs_for_spend,
    Eip12Asset, Eip12DataInputBox, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

use crate::calculator::{
    cost_to_mint_sigrsv, cost_to_mint_sigusd, erg_from_redeem_sigrsv, erg_from_redeem_sigusd,
};
use crate::state::SigmaUsdState;
use crate::NftIds;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigmaUsdAction {
    MintSigUsd,
    RedeemSigUsd,
    MintSigRsv,
    RedeemSigRsv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigmaUsdActionParseError;

impl fmt::Display for SigmaUsdActionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid SigmaUSD action (expected 'mint_sigusd', 'redeem_sigusd', 'mint_sigrsv', or 'redeem_sigrsv')"
        )
    }
}

impl FromStr for SigmaUsdAction {
    type Err = SigmaUsdActionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mint_sigusd" => Ok(Self::MintSigUsd),
            "redeem_sigusd" => Ok(Self::RedeemSigUsd),
            "mint_sigrsv" => Ok(Self::MintSigRsv),
            "redeem_sigrsv" => Ok(Self::RedeemSigRsv),
            _ => Err(SigmaUsdActionParseError),
        }
    }
}

impl SigmaUsdAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MintSigUsd => "mint_sigusd",
            Self::RedeemSigUsd => "redeem_sigusd",
            Self::MintSigRsv => "mint_sigrsv",
            Self::RedeemSigRsv => "redeem_sigrsv",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MintSigUsdRequest {
    pub amount: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    /// If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RedeemSigUsdRequest {
    pub amount: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    /// If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MintSigRsvRequest {
    pub amount: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    /// If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RedeemSigRsvRequest {
    pub amount: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    /// If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TxContext {
    pub nft_ids: NftIds,
    /// FULL box data required, not just box ID
    pub bank_input: Eip12InputBox,
    pub bank_erg_nano: i64,
    pub sigusd_circulating: i64,
    pub sigrsv_circulating: i64,
    pub sigusd_in_bank: i64,
    pub sigrsv_in_bank: i64,
    /// FULL box data required, not just box ID
    pub oracle_data_input: Eip12DataInputBox,
    pub oracle_rate: i64,
}

#[derive(Debug, Clone)]
pub struct TxSummary {
    pub action: String,
    pub erg_amount_nano: i64,
    pub token_amount: i64,
    pub token_name: String,
    pub protocol_fee_nano: i64,
    pub tx_fee_nano: i64,
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: TxSummary,
}

pub fn validate_mint_sigusd(amount: i64, state: &SigmaUsdState) -> Result<(), ProtocolError> {
    if amount <= 0 {
        return Err(ProtocolError::InvalidAmount {
            message: "Amount must be positive".to_string(),
        });
    }

    if !state.can_mint_sigusd {
        return Err(ProtocolError::RatioOutOfBounds {
            ratio: state.reserve_ratio_pct,
            action: "mint SigUSD".to_string(),
        });
    }

    if amount > state.max_sigusd_mintable {
        return Err(ProtocolError::InvalidAmount {
            message: format!(
                "Amount {} exceeds max mintable {}",
                amount, state.max_sigusd_mintable
            ),
        });
    }

    Ok(())
}

pub fn validate_redeem_sigusd(amount: i64, state: &SigmaUsdState) -> Result<(), ProtocolError> {
    if amount <= 0 {
        return Err(ProtocolError::InvalidAmount {
            message: "Amount must be positive".to_string(),
        });
    }

    // SigUSD can always be redeemed (no ratio constraint)
    if amount > state.sigusd_circulating {
        return Err(ProtocolError::InvalidAmount {
            message: format!(
                "Amount {} exceeds circulating supply {}",
                amount, state.sigusd_circulating
            ),
        });
    }

    Ok(())
}

pub fn validate_mint_sigrsv(amount: i64, state: &SigmaUsdState) -> Result<(), ProtocolError> {
    if amount <= 0 {
        return Err(ProtocolError::InvalidAmount {
            message: "Amount must be positive".to_string(),
        });
    }

    if !state.can_mint_sigrsv {
        return Err(ProtocolError::RatioOutOfBounds {
            ratio: state.reserve_ratio_pct,
            action: "mint SigRSV".to_string(),
        });
    }

    if amount > state.max_sigrsv_mintable {
        return Err(ProtocolError::InvalidAmount {
            message: format!(
                "Amount {} exceeds max mintable {}",
                amount, state.max_sigrsv_mintable
            ),
        });
    }

    Ok(())
}

pub fn validate_redeem_sigrsv(amount: i64, state: &SigmaUsdState) -> Result<(), ProtocolError> {
    if amount <= 0 {
        return Err(ProtocolError::InvalidAmount {
            message: "Amount must be positive".to_string(),
        });
    }

    if !state.can_redeem_sigrsv {
        return Err(ProtocolError::RatioOutOfBounds {
            ratio: state.reserve_ratio_pct,
            action: "redeem SigRSV".to_string(),
        });
    }

    if amount > state.sigrsv_circulating {
        return Err(ProtocolError::InvalidAmount {
            message: format!(
                "Amount {} exceeds circulating supply {}",
                amount, state.sigrsv_circulating
            ),
        });
    }

    Ok(())
}

pub fn build_mint_sigusd_tx(
    request: &MintSigUsdRequest,
    ctx: &TxContext,
    _state: &SigmaUsdState,
) -> Result<BuildResult, TxError> {
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    let erg_calc = cost_to_mint_sigusd(request.amount, ctx.oracle_rate);
    let erg_cost = erg_calc.net_amount;

    let required_erg = erg_cost + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
    let selected =
        select_inputs_for_spend(&request.user_inputs, required_erg as u64, None).map_err(|e| {
            TxError::BuildFailed {
                message: e.to_string(),
            }
        })?;

    // BANK BOX MUST BE INPUT 0 (contract requirement)
    let mut inputs = vec![ctx.bank_input.clone()];
    inputs.extend(selected.boxes.clone());

    let data_inputs = vec![ctx.oracle_data_input.clone()];

    let new_bank_erg = ctx.bank_erg_nano + erg_cost;
    let new_sigusd_circ = ctx.sigusd_circulating + request.amount;
    let new_sigrsv_circ = ctx.sigrsv_circulating;
    let new_sigusd_in_bank = ctx.sigusd_in_bank - request.amount;
    let new_sigrsv_in_bank = ctx.sigrsv_in_bank;
    let mut outputs = Vec::new();

    let bank_output = build_bank_output(
        ctx,
        new_bank_erg,
        new_sigusd_in_bank,
        new_sigrsv_in_bank,
        new_sigusd_circ,
        new_sigrsv_circ,
        request.current_height,
    );
    outputs.push(bank_output);

    // Contract requires R4 = token amount, R5 = ERG amount
    let user_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_long(request.amount),
        "R5" => encode_sigma_long(erg_cost),
    );

    outputs.push(Eip12Output {
        value: constants::MIN_BOX_VALUE_NANO.to_string(),
        ergo_tree: output_ergo_tree.to_string(),
        assets: vec![Eip12Asset::new(&ctx.nft_ids.sigusd_token, request.amount)],
        creation_height: request.current_height,
        additional_registers: user_registers,
    });

    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    let erg_used = (erg_cost + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO) as u64;
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &[],
        &request.user_ergo_tree,
        request.current_height,
        constants::MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs,
        outputs,
    };

    let summary = TxSummary {
        action: "mint_sigusd".to_string(),
        erg_amount_nano: erg_cost,
        token_amount: request.amount,
        token_name: "SigUSD".to_string(),
        protocol_fee_nano: erg_calc.fee,
        tx_fee_nano: constants::TX_FEE_NANO,
    };

    Ok(BuildResult {
        unsigned_tx,
        summary,
    })
}

pub fn build_redeem_sigusd_tx(
    request: &RedeemSigUsdRequest,
    ctx: &TxContext,
    _state: &SigmaUsdState,
) -> Result<BuildResult, TxError> {
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    let erg_calc = erg_from_redeem_sigusd(request.amount, ctx.oracle_rate);
    let erg_to_receive = erg_calc.net_amount;

    let min_bank_erg = erg_to_receive + constants::MIN_BOX_VALUE_NANO;
    if ctx.bank_erg_nano < min_bank_erg {
        return Err(TxError::BuildFailed {
            message: format!(
                "Bank has insufficient ERG: need {}, have {}",
                min_bank_erg, ctx.bank_erg_nano
            ),
        });
    }

    let selected = select_inputs_for_spend(
        &request.user_inputs,
        constants::TX_FEE_NANO as u64,
        Some((&ctx.nft_ids.sigusd_token, request.amount as u64)),
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    // BANK BOX MUST BE INPUT 0 (contract requirement)
    let mut inputs = vec![ctx.bank_input.clone()];
    inputs.extend(selected.boxes.clone());

    let data_inputs = vec![ctx.oracle_data_input.clone()];

    let new_bank_erg = ctx.bank_erg_nano - erg_to_receive;
    let new_sigusd_circ = ctx.sigusd_circulating - request.amount;
    let new_sigrsv_circ = ctx.sigrsv_circulating;
    let new_sigusd_in_bank = ctx.sigusd_in_bank + request.amount;
    let new_sigrsv_in_bank = ctx.sigrsv_in_bank;
    let mut outputs = Vec::new();

    let bank_output = build_bank_output(
        ctx,
        new_bank_erg,
        new_sigusd_in_bank,
        new_sigrsv_in_bank,
        new_sigusd_circ,
        new_sigrsv_circ,
        request.current_height,
    );
    outputs.push(bank_output);

    // Contract requires R4 = token amount, R5 = ERG amount
    let user_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_long(request.amount),
        "R5" => encode_sigma_long(erg_to_receive),
    );

    let user_sigusd = selected.token_amount as i64;
    let remaining_sigusd = user_sigusd - request.amount;
    let mut user_assets = Vec::new();
    if remaining_sigusd > 0 {
        user_assets.push(Eip12Asset::new(&ctx.nft_ids.sigusd_token, remaining_sigusd));
    }

    let user_output_erg = selected.total_erg as i64 + erg_to_receive - constants::TX_FEE_NANO;

    outputs.push(Eip12Output {
        value: user_output_erg.to_string(),
        ergo_tree: output_ergo_tree.to_string(),
        assets: user_assets,
        creation_height: request.current_height,
        additional_registers: user_registers,
    });

    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    let other_tokens = collect_change_tokens(
        &selected.boxes,
        Some((&ctx.nft_ids.sigusd_token, request.amount as u64)),
    );
    if !other_tokens.is_empty() {
        outputs.push(Eip12Output::change(
            constants::MIN_BOX_VALUE_NANO,
            &request.user_ergo_tree,
            other_tokens,
            request.current_height,
        ));
    }

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs,
        outputs,
    };

    let summary = TxSummary {
        action: "redeem_sigusd".to_string(),
        erg_amount_nano: erg_to_receive,
        token_amount: request.amount,
        token_name: "SigUSD".to_string(),
        protocol_fee_nano: erg_calc.fee,
        tx_fee_nano: constants::TX_FEE_NANO,
    };

    Ok(BuildResult {
        unsigned_tx,
        summary,
    })
}

pub fn build_mint_sigrsv_tx(
    request: &MintSigRsvRequest,
    ctx: &TxContext,
    state: &SigmaUsdState,
) -> Result<BuildResult, TxError> {
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    let erg_calc = cost_to_mint_sigrsv(request.amount, state.sigrsv_price_nano);
    let erg_cost = erg_calc.net_amount;

    let required_erg = erg_cost + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
    let selected =
        select_inputs_for_spend(&request.user_inputs, required_erg as u64, None).map_err(|e| {
            TxError::BuildFailed {
                message: e.to_string(),
            }
        })?;

    // BANK BOX MUST BE INPUT 0 (contract requirement)
    let mut inputs = vec![ctx.bank_input.clone()];
    inputs.extend(selected.boxes.clone());

    let data_inputs = vec![ctx.oracle_data_input.clone()];

    let new_bank_erg = ctx.bank_erg_nano + erg_cost;
    let new_sigusd_circ = ctx.sigusd_circulating;
    let new_sigrsv_circ = ctx.sigrsv_circulating + request.amount;
    let new_sigusd_in_bank = ctx.sigusd_in_bank;
    let new_sigrsv_in_bank = ctx.sigrsv_in_bank - request.amount;

    let mut outputs = Vec::new();

    let bank_output = build_bank_output(
        ctx,
        new_bank_erg,
        new_sigusd_in_bank,
        new_sigrsv_in_bank,
        new_sigusd_circ,
        new_sigrsv_circ,
        request.current_height,
    );
    outputs.push(bank_output);

    // Contract requires R4 = token amount, R5 = ERG amount
    let user_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_long(request.amount),
        "R5" => encode_sigma_long(erg_cost),
    );

    outputs.push(Eip12Output {
        value: constants::MIN_BOX_VALUE_NANO.to_string(),
        ergo_tree: output_ergo_tree.to_string(),
        assets: vec![Eip12Asset::new(&ctx.nft_ids.sigrsv_token, request.amount)],
        creation_height: request.current_height,
        additional_registers: user_registers,
    });

    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    let erg_used = (erg_cost + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO) as u64;
    append_change_output(
        &mut outputs,
        &selected,
        erg_used,
        &[],
        &request.user_ergo_tree,
        request.current_height,
        constants::MIN_BOX_VALUE_NANO as u64,
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs,
        outputs,
    };

    let summary = TxSummary {
        action: "mint_sigrsv".to_string(),
        erg_amount_nano: erg_cost,
        token_amount: request.amount,
        token_name: "SigRSV".to_string(),
        protocol_fee_nano: erg_calc.fee,
        tx_fee_nano: constants::TX_FEE_NANO,
    };

    Ok(BuildResult {
        unsigned_tx,
        summary,
    })
}

/// Only allowed when reserve ratio > 400%
pub fn build_redeem_sigrsv_tx(
    request: &RedeemSigRsvRequest,
    ctx: &TxContext,
    state: &SigmaUsdState,
) -> Result<BuildResult, TxError> {
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    let erg_calc = erg_from_redeem_sigrsv(request.amount, state.sigrsv_price_nano);
    let erg_to_receive = erg_calc.net_amount;

    let min_bank_erg = erg_to_receive + constants::MIN_BOX_VALUE_NANO;
    if ctx.bank_erg_nano < min_bank_erg {
        return Err(TxError::BuildFailed {
            message: format!(
                "Bank has insufficient ERG: need {}, have {}",
                min_bank_erg, ctx.bank_erg_nano
            ),
        });
    }

    let selected = select_inputs_for_spend(
        &request.user_inputs,
        constants::TX_FEE_NANO as u64,
        Some((&ctx.nft_ids.sigrsv_token, request.amount as u64)),
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    // BANK BOX MUST BE INPUT 0 (contract requirement)
    let mut inputs = vec![ctx.bank_input.clone()];
    inputs.extend(selected.boxes.clone());

    let data_inputs = vec![ctx.oracle_data_input.clone()];

    let new_bank_erg = ctx.bank_erg_nano - erg_to_receive;
    let new_sigusd_circ = ctx.sigusd_circulating;
    let new_sigrsv_circ = ctx.sigrsv_circulating - request.amount;
    let new_sigusd_in_bank = ctx.sigusd_in_bank;
    let new_sigrsv_in_bank = ctx.sigrsv_in_bank + request.amount;

    let mut outputs = Vec::new();

    let bank_output = build_bank_output(
        ctx,
        new_bank_erg,
        new_sigusd_in_bank,
        new_sigrsv_in_bank,
        new_sigusd_circ,
        new_sigrsv_circ,
        request.current_height,
    );
    outputs.push(bank_output);

    // Contract requires R4 = token amount, R5 = ERG amount
    let user_registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_long(request.amount),
        "R5" => encode_sigma_long(erg_to_receive),
    );

    let user_sigrsv = selected.token_amount as i64;
    let remaining_sigrsv = user_sigrsv - request.amount;
    let mut user_assets = Vec::new();
    if remaining_sigrsv > 0 {
        user_assets.push(Eip12Asset::new(&ctx.nft_ids.sigrsv_token, remaining_sigrsv));
    }

    let user_output_erg = selected.total_erg as i64 + erg_to_receive - constants::TX_FEE_NANO;

    outputs.push(Eip12Output {
        value: user_output_erg.to_string(),
        ergo_tree: output_ergo_tree.to_string(),
        assets: user_assets,
        creation_height: request.current_height,
        additional_registers: user_registers,
    });

    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    let other_tokens = collect_change_tokens(
        &selected.boxes,
        Some((&ctx.nft_ids.sigrsv_token, request.amount as u64)),
    );
    if !other_tokens.is_empty() {
        outputs.push(Eip12Output::change(
            constants::MIN_BOX_VALUE_NANO,
            &request.user_ergo_tree,
            other_tokens,
            request.current_height,
        ));
    }

    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs,
        outputs,
    };

    let summary = TxSummary {
        action: "redeem_sigrsv".to_string(),
        erg_amount_nano: erg_to_receive,
        token_amount: request.amount,
        token_name: "SigRSV".to_string(),
        protocol_fee_nano: erg_calc.fee,
        tx_fee_nano: constants::TX_FEE_NANO,
    };

    Ok(BuildResult {
        unsigned_tx,
        summary,
    })
}

fn build_bank_output(
    ctx: &TxContext,
    new_erg: i64,
    new_sigusd_tokens: i64,
    new_sigrsv_tokens: i64,
    new_sigusd_circ: i64,
    new_sigrsv_circ: i64,
    height: i32,
) -> Eip12Output {
    // CRITICAL: Token order must EXACTLY match the input bank box order!
    // Copy order from current bank box assets
    let mut assets: Vec<Eip12Asset> = Vec::new();

    for asset in &ctx.bank_input.assets {
        let new_amount = if asset.token_id == ctx.nft_ids.bank_nft {
            1
        } else if asset.token_id == ctx.nft_ids.sigusd_token {
            new_sigusd_tokens
        } else if asset.token_id == ctx.nft_ids.sigrsv_token {
            new_sigrsv_tokens
        } else {
            asset.amount.parse().unwrap_or_else(|_| {
                tracing::warn!(
                    token_id = %asset.token_id,
                    raw = %asset.amount,
                    "Failed to parse unknown token amount in bank box, defaulting to 0"
                );
                0
            })
        };

        assets.push(Eip12Asset::new(&asset.token_id, new_amount));
    }

    let mut registers = ergo_tx::sigma_registers!(
        "R4" => encode_sigma_long(new_sigusd_circ),
        "R5" => encode_sigma_long(new_sigrsv_circ),
    );

    for (reg_id, value) in &ctx.bank_input.additional_registers {
        if reg_id != "R4" && reg_id != "R5" {
            registers.insert(reg_id.clone(), value.clone());
        }
    }

    Eip12Output {
        value: new_erg.to_string(),
        ergo_tree: ctx.bank_input.ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: registers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_from_str() {
        assert_eq!(
            "mint_sigusd".parse::<SigmaUsdAction>(),
            Ok(SigmaUsdAction::MintSigUsd)
        );
        assert_eq!(
            "redeem_sigusd".parse::<SigmaUsdAction>(),
            Ok(SigmaUsdAction::RedeemSigUsd)
        );
        assert!("invalid".parse::<SigmaUsdAction>().is_err());
    }

    #[test]
    fn test_action_as_str() {
        assert_eq!(SigmaUsdAction::MintSigUsd.as_str(), "mint_sigusd");
        assert_eq!(SigmaUsdAction::RedeemSigRsv.as_str(), "redeem_sigrsv");
    }
}
