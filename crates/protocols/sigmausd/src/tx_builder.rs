//! SigmaUSD Transaction Builder
//!
//! Builds unsigned transactions for SigmaUSD mint/redeem operations.
//!
//! # Important Notes
//!
//! - Bank box MUST be input[0] (contract requirement)
//! - Data inputs require FULL box data, not just box ID
//! - Token order in bank output must match bank input exactly

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use citadel_core::{constants, ProtocolError, TxError};
use ergo_tx::{
    collect_change_tokens, encode_sigma_long, select_erg_boxes, select_token_boxes, Eip12Asset,
    Eip12DataInputBox, Eip12InputBox, Eip12Output, Eip12UnsignedTx,
};

use crate::calculator::{
    cost_to_mint_sigrsv, cost_to_mint_sigusd, erg_from_redeem_sigrsv, erg_from_redeem_sigusd,
};
use crate::state::SigmaUsdState;
use crate::NftIds;

/// SigmaUSD action types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigmaUsdAction {
    MintSigUsd,
    RedeemSigUsd,
    MintSigRsv,
    RedeemSigRsv,
}

/// Error returned when parsing a `SigmaUsdAction` from a string fails.
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

/// Request to build a SigmaUSD transaction
#[derive(Debug, Clone)]
pub struct MintSigUsdRequest {
    /// Amount of SigUSD to mint (raw units, 2 decimals)
    pub amount: i64,
    /// User's P2PK address
    pub user_address: String,
    /// User's ErgoTree (from first UTXO)
    pub user_ergo_tree: String,
    /// User's UTXOs (from wallet)
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current block height
    pub current_height: i32,
    /// Optional recipient ErgoTree. If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

/// Request to redeem SigUSD for ERG
#[derive(Debug, Clone)]
pub struct RedeemSigUsdRequest {
    /// Amount of SigUSD to redeem (raw units, 2 decimals)
    pub amount: i64,
    /// User's P2PK address
    pub user_address: String,
    /// User's ErgoTree (from first UTXO)
    pub user_ergo_tree: String,
    /// User's UTXOs (must contain SigUSD tokens)
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current block height
    pub current_height: i32,
    /// Optional recipient ErgoTree. If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

/// Request to mint SigRSV with ERG
#[derive(Debug, Clone)]
pub struct MintSigRsvRequest {
    /// Amount of SigRSV to mint (raw units, 0 decimals)
    pub amount: i64,
    /// User's P2PK address
    pub user_address: String,
    /// User's ErgoTree
    pub user_ergo_tree: String,
    /// User's UTXOs
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current block height
    pub current_height: i32,
    /// Optional recipient ErgoTree. If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

/// Request to redeem SigRSV for ERG
#[derive(Debug, Clone)]
pub struct RedeemSigRsvRequest {
    /// Amount of SigRSV to redeem (raw units, 0 decimals)
    pub amount: i64,
    /// User's P2PK address
    pub user_address: String,
    /// User's ErgoTree
    pub user_ergo_tree: String,
    /// User's UTXOs (must contain SigRSV tokens)
    pub user_inputs: Vec<Eip12InputBox>,
    /// Current block height
    pub current_height: i32,
    /// Optional recipient ErgoTree. If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

/// Context for building transactions
#[derive(Debug, Clone)]
pub struct TxContext {
    pub nft_ids: NftIds,
    /// Bank box as EIP-12 input (FULL data)
    pub bank_input: Eip12InputBox,
    /// Bank box ERG value
    pub bank_erg_nano: i64,
    /// Current SigUSD circulating
    pub sigusd_circulating: i64,
    /// Current SigRSV circulating
    pub sigrsv_circulating: i64,
    /// SigUSD tokens in bank
    pub sigusd_in_bank: i64,
    /// SigRSV tokens in bank
    pub sigrsv_in_bank: i64,
    /// Oracle box as EIP-12 data input (FULL data)
    pub oracle_data_input: Eip12DataInputBox,
    /// Oracle ERG/USD rate
    pub oracle_rate: i64,
}

/// Transaction summary for display
#[derive(Debug, Clone)]
pub struct TxSummary {
    pub action: String,
    pub erg_amount_nano: i64,
    pub token_amount: i64,
    pub token_name: String,
    pub protocol_fee_nano: i64,
    pub tx_fee_nano: i64,
}

/// Build result
#[derive(Debug, Clone)]
pub struct BuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: TxSummary,
}

/// Validate a mint action before building
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

/// Validate a redeem SigUSD action before building
pub fn validate_redeem_sigusd(amount: i64, state: &SigmaUsdState) -> Result<(), ProtocolError> {
    if amount <= 0 {
        return Err(ProtocolError::InvalidAmount {
            message: "Amount must be positive".to_string(),
        });
    }

    // SigUSD can always be redeemed (no ratio constraint)
    // But check amount doesn't exceed circulating supply
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

/// Validate a mint SigRSV action before building
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

/// Validate a redeem SigRSV action before building
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

/// Build a mint SigUSD transaction
///
/// This is the MVP action - builds an unsigned EIP-12 transaction that:
/// 1. Spends the bank box (input 0)
/// 2. Spends user's ERG (inputs 1+)
/// 3. Reads oracle price (data input)
/// 4. Creates new bank box with updated state (output 0)
/// 5. Creates user output with SigUSD tokens (output 1)
/// 6. Creates miner fee output (output 2)
/// 7. Creates change output if needed (output 3)
pub fn build_mint_sigusd_tx(
    request: &MintSigUsdRequest,
    ctx: &TxContext,
    _state: &SigmaUsdState,
) -> Result<BuildResult, TxError> {
    // Determine output ErgoTree (recipient or self)
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    // Calculate cost
    let erg_calc = cost_to_mint_sigusd(request.amount, ctx.oracle_rate);
    let erg_cost = erg_calc.net_amount;

    // Select minimum user UTXOs
    let required_erg = erg_cost + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
    let selected = select_erg_boxes(&request.user_inputs, required_erg as u64).map_err(|e| {
        TxError::BuildFailed {
            message: e.to_string(),
        }
    })?;

    // Build inputs: BANK BOX MUST BE INPUT 0
    let mut inputs = vec![ctx.bank_input.clone()];
    inputs.extend(selected.boxes.clone());

    // Data inputs: oracle box (FULL data, not just ID!)
    let data_inputs = vec![ctx.oracle_data_input.clone()];

    // Calculate new bank state
    let new_bank_erg = ctx.bank_erg_nano + erg_cost;
    let new_sigusd_circ = ctx.sigusd_circulating + request.amount;
    let new_sigrsv_circ = ctx.sigrsv_circulating; // Unchanged
    let new_sigusd_in_bank = ctx.sigusd_in_bank - request.amount;
    let new_sigrsv_in_bank = ctx.sigrsv_in_bank; // Unchanged

    // Build outputs
    let mut outputs = Vec::new();

    // Output 0: New bank box
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

    // Output 1: User receives SigUSD tokens (goes to recipient if set)
    // Contract requires R4 = token amount, R5 = ERG amount
    let mut user_registers = HashMap::new();
    user_registers.insert("R4".to_string(), encode_sigma_long(request.amount));
    user_registers.insert("R5".to_string(), encode_sigma_long(erg_cost));

    outputs.push(Eip12Output {
        value: constants::MIN_BOX_VALUE_NANO.to_string(),
        ergo_tree: output_ergo_tree.to_string(),
        assets: vec![Eip12Asset::new(&ctx.nft_ids.sigusd_token, request.amount)],
        creation_height: request.current_height,
        additional_registers: user_registers,
    });

    // Output 2: Miner fee
    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    // Output 3: Change to user (must preserve ALL tokens from inputs)
    let change_assets = collect_change_tokens(&selected.boxes, None);
    let change_erg = selected.total_erg as i64
        - erg_cost
        - constants::TX_FEE_NANO
        - constants::MIN_BOX_VALUE_NANO;

    // Create change output if there's enough ERG OR if there are tokens to preserve
    if change_erg >= constants::MIN_BOX_VALUE_NANO || !change_assets.is_empty() {
        // If we have tokens but not enough ERG, use minimum box value
        let change_value = if change_erg >= constants::MIN_BOX_VALUE_NANO {
            change_erg
        } else {
            constants::MIN_BOX_VALUE_NANO
        };

        outputs.push(Eip12Output::change(
            change_value,
            &request.user_ergo_tree,
            change_assets,
            request.current_height,
        ));
    }

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

/// Build a redeem SigUSD transaction
///
/// Inverse of mint - user provides SigUSD tokens, receives ERG:
/// 1. Spends the bank box (input 0)
/// 2. Spends user's UTXOs with SigUSD tokens (inputs 1+)
/// 3. Reads oracle price (data input)
/// 4. Creates new bank box with updated state (output 0)
/// 5. Creates user output with ERG (output 1)
/// 6. Creates miner fee output (output 2)
/// 7. Creates change output if needed (output 3)
pub fn build_redeem_sigusd_tx(
    request: &RedeemSigUsdRequest,
    ctx: &TxContext,
    _state: &SigmaUsdState,
) -> Result<BuildResult, TxError> {
    // Determine output ErgoTree (recipient or self)
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    // Calculate ERG to receive
    let erg_calc = erg_from_redeem_sigusd(request.amount, ctx.oracle_rate);
    let erg_to_receive = erg_calc.net_amount;

    // Verify bank has enough ERG to pay out
    let min_bank_erg = erg_to_receive + constants::MIN_BOX_VALUE_NANO;
    if ctx.bank_erg_nano < min_bank_erg {
        return Err(TxError::BuildFailed {
            message: format!(
                "Bank has insufficient ERG: need {}, have {}",
                min_bank_erg, ctx.bank_erg_nano
            ),
        });
    }

    // Select minimum UTXOs: need SigUSD tokens + enough ERG for tx fee
    let selected = select_token_boxes(
        &request.user_inputs,
        &ctx.nft_ids.sigusd_token,
        request.amount as u64,
        constants::TX_FEE_NANO as u64,
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    // Build inputs: BANK BOX MUST BE INPUT 0
    let mut inputs = vec![ctx.bank_input.clone()];
    inputs.extend(selected.boxes.clone());

    // Data inputs: oracle box (FULL data, not just ID!)
    let data_inputs = vec![ctx.oracle_data_input.clone()];

    // Calculate new bank state
    // Bank ERG decreases (pays user), SigUSD in bank increases (receives tokens)
    let new_bank_erg = ctx.bank_erg_nano - erg_to_receive;
    let new_sigusd_circ = ctx.sigusd_circulating - request.amount;
    let new_sigrsv_circ = ctx.sigrsv_circulating; // Unchanged
    let new_sigusd_in_bank = ctx.sigusd_in_bank + request.amount;
    let new_sigrsv_in_bank = ctx.sigrsv_in_bank; // Unchanged

    // Build outputs
    let mut outputs = Vec::new();

    // Output 0: New bank box
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

    // Output 1: User receives ERG
    // Contract requires R4 = token amount (being redeemed), R5 = ERG amount (received)
    let mut user_registers = HashMap::new();
    user_registers.insert("R4".to_string(), encode_sigma_long(request.amount));
    user_registers.insert("R5".to_string(), encode_sigma_long(erg_to_receive));

    // User receives ERG + any remaining SigUSD tokens
    let user_sigusd = selected.token_amount as i64;
    let remaining_sigusd = user_sigusd - request.amount;
    let mut user_assets = Vec::new();
    if remaining_sigusd > 0 {
        user_assets.push(Eip12Asset::new(&ctx.nft_ids.sigusd_token, remaining_sigusd));
    }

    // User output gets: their original ERG + ERG from redeem - tx fee
    let user_output_erg = selected.total_erg as i64 + erg_to_receive - constants::TX_FEE_NANO;

    outputs.push(Eip12Output {
        value: user_output_erg.to_string(),
        ergo_tree: output_ergo_tree.to_string(),
        assets: user_assets,
        creation_height: request.current_height,
        additional_registers: user_registers,
    });

    // Output 2: Miner fee
    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    // Output 3: Change for other tokens (excluding SigUSD which is handled above)
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

/// Build a mint SigRSV transaction
///
/// User provides ERG, receives SigRSV tokens:
/// 1. Spends the bank box (input 0)
/// 2. Spends user's ERG (inputs 1+)
/// 3. Reads oracle price (data input)
/// 4. Creates new bank box with updated state (output 0)
/// 5. Creates user output with SigRSV tokens (output 1)
/// 6. Creates miner fee output (output 2)
/// 7. Creates change output if needed (output 3)
pub fn build_mint_sigrsv_tx(
    request: &MintSigRsvRequest,
    ctx: &TxContext,
    state: &SigmaUsdState,
) -> Result<BuildResult, TxError> {
    // Determine output ErgoTree (recipient or self)
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    // Calculate cost using SigRSV price
    let erg_calc = cost_to_mint_sigrsv(request.amount, state.sigrsv_price_nano);
    let erg_cost = erg_calc.net_amount;

    // Select minimum user UTXOs
    let required_erg = erg_cost + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
    let selected = select_erg_boxes(&request.user_inputs, required_erg as u64).map_err(|e| {
        TxError::BuildFailed {
            message: e.to_string(),
        }
    })?;

    // Build inputs: BANK BOX MUST BE INPUT 0
    let mut inputs = vec![ctx.bank_input.clone()];
    inputs.extend(selected.boxes.clone());

    // Data inputs: oracle box (FULL data, not just ID!)
    let data_inputs = vec![ctx.oracle_data_input.clone()];

    // Calculate new bank state
    // Bank ERG increases (receives payment), SigRSV in bank decreases (sends tokens)
    let new_bank_erg = ctx.bank_erg_nano + erg_cost;
    let new_sigusd_circ = ctx.sigusd_circulating; // Unchanged
    let new_sigrsv_circ = ctx.sigrsv_circulating + request.amount;
    let new_sigusd_in_bank = ctx.sigusd_in_bank; // Unchanged
    let new_sigrsv_in_bank = ctx.sigrsv_in_bank - request.amount;

    // Build outputs
    let mut outputs = Vec::new();

    // Output 0: New bank box
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

    // Output 1: User receives SigRSV tokens (goes to recipient if set)
    // Contract requires R4 = token amount, R5 = ERG amount
    let mut user_registers = HashMap::new();
    user_registers.insert("R4".to_string(), encode_sigma_long(request.amount));
    user_registers.insert("R5".to_string(), encode_sigma_long(erg_cost));

    outputs.push(Eip12Output {
        value: constants::MIN_BOX_VALUE_NANO.to_string(),
        ergo_tree: output_ergo_tree.to_string(),
        assets: vec![Eip12Asset::new(&ctx.nft_ids.sigrsv_token, request.amount)],
        creation_height: request.current_height,
        additional_registers: user_registers,
    });

    // Output 2: Miner fee
    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    // Output 3: Change to user (must preserve ALL tokens from inputs)
    let change_assets = collect_change_tokens(&selected.boxes, None);
    let change_erg = selected.total_erg as i64
        - erg_cost
        - constants::TX_FEE_NANO
        - constants::MIN_BOX_VALUE_NANO;

    // Create change output if there's enough ERG OR if there are tokens to preserve
    if change_erg >= constants::MIN_BOX_VALUE_NANO || !change_assets.is_empty() {
        // If we have tokens but not enough ERG, use minimum box value
        let change_value = if change_erg >= constants::MIN_BOX_VALUE_NANO {
            change_erg
        } else {
            constants::MIN_BOX_VALUE_NANO
        };

        outputs.push(Eip12Output::change(
            change_value,
            &request.user_ergo_tree,
            change_assets,
            request.current_height,
        ));
    }

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

/// Build a redeem SigRSV transaction
///
/// Inverse of mint - user provides SigRSV tokens, receives ERG:
/// 1. Spends the bank box (input 0)
/// 2. Spends user's UTXOs with SigRSV tokens (inputs 1+)
/// 3. Reads oracle price (data input)
/// 4. Creates new bank box with updated state (output 0)
/// 5. Creates user output with ERG (output 1)
/// 6. Creates miner fee output (output 2)
/// 7. Creates change output if needed (output 3)
///
/// Note: Only allowed when reserve ratio > 400%
pub fn build_redeem_sigrsv_tx(
    request: &RedeemSigRsvRequest,
    ctx: &TxContext,
    state: &SigmaUsdState,
) -> Result<BuildResult, TxError> {
    // Determine output ErgoTree (recipient or self)
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    // Calculate ERG to receive using SigRSV price
    let erg_calc = erg_from_redeem_sigrsv(request.amount, state.sigrsv_price_nano);
    let erg_to_receive = erg_calc.net_amount;

    // Verify bank has enough ERG to pay out
    let min_bank_erg = erg_to_receive + constants::MIN_BOX_VALUE_NANO;
    if ctx.bank_erg_nano < min_bank_erg {
        return Err(TxError::BuildFailed {
            message: format!(
                "Bank has insufficient ERG: need {}, have {}",
                min_bank_erg, ctx.bank_erg_nano
            ),
        });
    }

    // Select minimum UTXOs: need SigRSV tokens + enough ERG for tx fee
    let selected = select_token_boxes(
        &request.user_inputs,
        &ctx.nft_ids.sigrsv_token,
        request.amount as u64,
        constants::TX_FEE_NANO as u64,
    )
    .map_err(|e| TxError::BuildFailed {
        message: e.to_string(),
    })?;

    // Build inputs: BANK BOX MUST BE INPUT 0
    let mut inputs = vec![ctx.bank_input.clone()];
    inputs.extend(selected.boxes.clone());

    // Data inputs: oracle box (FULL data, not just ID!)
    let data_inputs = vec![ctx.oracle_data_input.clone()];

    // Calculate new bank state
    // Bank ERG decreases (pays user), SigRSV in bank increases (receives tokens)
    let new_bank_erg = ctx.bank_erg_nano - erg_to_receive;
    let new_sigusd_circ = ctx.sigusd_circulating; // Unchanged
    let new_sigrsv_circ = ctx.sigrsv_circulating - request.amount;
    let new_sigusd_in_bank = ctx.sigusd_in_bank; // Unchanged
    let new_sigrsv_in_bank = ctx.sigrsv_in_bank + request.amount;

    // Build outputs
    let mut outputs = Vec::new();

    // Output 0: New bank box
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

    // Output 1: User receives ERG
    // Contract requires R4 = token amount (being redeemed), R5 = ERG amount (received)
    let mut user_registers = HashMap::new();
    user_registers.insert("R4".to_string(), encode_sigma_long(request.amount));
    user_registers.insert("R5".to_string(), encode_sigma_long(erg_to_receive));

    // User receives ERG + any remaining SigRSV tokens
    let user_sigrsv = selected.token_amount as i64;
    let remaining_sigrsv = user_sigrsv - request.amount;
    let mut user_assets = Vec::new();
    if remaining_sigrsv > 0 {
        user_assets.push(Eip12Asset::new(&ctx.nft_ids.sigrsv_token, remaining_sigrsv));
    }

    // User output gets: their original ERG + ERG from redeem - tx fee
    let user_output_erg = selected.total_erg as i64 + erg_to_receive - constants::TX_FEE_NANO;

    outputs.push(Eip12Output {
        value: user_output_erg.to_string(),
        ergo_tree: output_ergo_tree.to_string(),
        assets: user_assets,
        creation_height: request.current_height,
        additional_registers: user_registers,
    });

    // Output 2: Miner fee
    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    // Output 3: Change for other tokens (excluding SigRSV which is handled above)
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

/// Build the new bank box output
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
            1 // Bank NFT always 1
        } else if asset.token_id == ctx.nft_ids.sigusd_token {
            new_sigusd_tokens
        } else if asset.token_id == ctx.nft_ids.sigrsv_token {
            new_sigrsv_tokens
        } else {
            // Unknown token, preserve as-is
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

    // Build registers
    let mut registers = HashMap::new();
    registers.insert("R4".to_string(), encode_sigma_long(new_sigusd_circ));
    registers.insert("R5".to_string(), encode_sigma_long(new_sigrsv_circ));

    // Preserve other registers from current bank box (R6-R9 if present)
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
