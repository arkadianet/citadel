//! Dexy Transaction Builder
//!
//! Builds unsigned transactions for Dexy FreeMint operations.
//!
//! # FreeMint Transaction Structure
//!
//! **Inputs:**
//! - 0: FreeMint box (freeMintNFT)
//! - 1: Bank box (bankNFT)
//! - 2: Buyback box (buybackNFT)
//! - 3+: User UTXOs
//!
//! **Data Inputs:**
//! - 0: Oracle box
//! - 1: LP box
//!
//! **Outputs:**
//! - 0: FreeMint box (updated R4, R5)
//! - 1: Bank box (updated ERG + tokens)
//! - 2: Buyback box (receives fee)
//! - 3: User output (receives minted tokens)
//! - 4: Miner fee
//! - 5: Change output (if needed)
//!
//! # Fee Structure
//!
//! - Bank fee: 0.3% (bankFeeNum=3, feeDenom=1000)
//! - Buyback fee: 0.2% (buybackFeeNum=2, feeDenom=1000)
//! - Total fee: 0.5%

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use citadel_core::{constants, ProtocolError, TxError};
use ergo_tx::{
    collect_change_tokens, select_erg_boxes, select_token_boxes, Eip12Asset, Eip12InputBox,
    Eip12Output, Eip12UnsignedTx,
};

use crate::calculator::{
    calculate_lp_swap_output, calculate_lp_swap_price_impact, validate_lp_swap,
};
use crate::constants::{
    DexyVariant, BANK_FEE_NUM, BUYBACK_FEE_NUM, FEE_DENOM, LP_SWAP_FEE_DENOM, LP_SWAP_FEE_NUM,
};
use crate::fetch::{DexySwapTxContext, DexyTxContext};
use crate::state::DexyState;

/// FreeMint period length in blocks (1/2 day on mainnet)
const T_FREE: i32 = 360;
/// Max delay buffer for tx confirmation.
/// Must match contract's T_buffer exactly (5 blocks).
/// The contract validates: successorR4 >= HEIGHT + T_free && successorR4 <= HEIGHT + T_free + T_buffer
const T_BUFFER: i32 = 5;
/// Buyback action type for top-up (used during FreeMint/ArbMint)
/// Action 0 = swap (buy GORT), Action 1 = top-up (receive ERG), Action 2 = return (give GORT)
const BUYBACK_ACTION_TOPUP: i32 = 1;

/// Request to mint Dexy tokens (Gold or USD)
#[derive(Debug, Clone)]
pub struct MintDexyRequest {
    /// Which Dexy variant (Gold or USD)
    pub variant: DexyVariant,
    /// Amount of Dexy tokens to mint (raw units)
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

/// Transaction summary for display
#[derive(Debug, Clone)]
pub struct TxSummary {
    /// Action description (e.g., "mint_dexy_gold")
    pub action: String,
    /// ERG amount involved (total cost for mint)
    pub erg_amount_nano: i64,
    /// Token amount (Dexy tokens minted)
    pub token_amount: i64,
    /// Token name (e.g., "DexyGold", "DexyUSD")
    pub token_name: String,
    /// Transaction fee in nanoERG
    pub tx_fee_nano: i64,
    /// Bank fee in nanoERG
    pub bank_fee_nano: i64,
    /// Buyback fee in nanoERG
    pub buyback_fee_nano: i64,
}

/// Build result with unsigned transaction and summary
#[derive(Debug, Clone)]
pub struct BuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: TxSummary,
}

/// Validate a mint Dexy action before building
///
/// Checks:
/// - Amount is positive
/// - Minting is currently available (bank has tokens)
/// - Amount does not exceed available tokens in bank
/// - LP rate is within acceptable range of oracle rate
pub fn validate_mint_dexy(amount: i64, state: &DexyState) -> Result<(), ProtocolError> {
    if amount <= 0 {
        return Err(ProtocolError::InvalidAmount {
            message: "Amount must be positive".to_string(),
        });
    }

    if !state.can_mint {
        return Err(ProtocolError::ActionNotAllowed {
            reason: "Minting not available: rate condition not met or bank empty".to_string(),
        });
    }

    if amount > state.dexy_in_bank {
        return Err(ProtocolError::InvalidAmount {
            message: format!(
                "Amount {} exceeds available tokens {} in bank",
                amount, state.dexy_in_bank
            ),
        });
    }

    Ok(())
}

/// Pre-flight validation for FreeMint transaction
///
/// This validates all the conditions that the FreeMint contract will check,
/// providing detailed error messages before building the transaction.
///
/// Conditions checked (matching freemint.es contract):
/// 1. validRateFreeMint: lpRate * 100 > oracleRate * 98
/// 2. validAmount: dexyMinted <= availableToMint
/// 3. Bank has enough tokens
/// 4. FreeMint period check
pub fn validate_free_mint_preflight(
    ctx: &DexyTxContext,
    amount: i64,
    current_height: i32,
    variant: DexyVariant,
) -> Result<(), TxError> {
    // Apply oracle divisor (same as contract's / 1000L for USD)
    let oracle_rate = ctx.oracle_rate_nano / variant.oracle_divisor();

    if oracle_rate <= 0 {
        return Err(TxError::BuildFailed {
            message: format!(
                "Invalid oracle rate: {} (raw: {} / divisor: {}). Oracle may be stale or unavailable.",
                oracle_rate, ctx.oracle_rate_nano, variant.oracle_divisor()
            ),
        });
    }

    // LP rate = ERG reserves / Dexy reserves (same as contract)
    let lp_rate = if ctx.lp_dexy_reserves > 0 {
        ctx.lp_erg_reserves / ctx.lp_dexy_reserves
    } else {
        return Err(TxError::BuildFailed {
            message: "LP has no Dexy reserves".to_string(),
        });
    };

    // Check validRateFreeMint: lpRate * 100 > oracleRate * 98
    // This ensures LP price is at least ~98% of oracle price
    let lp_rate_scaled = lp_rate * 100;
    let oracle_threshold = oracle_rate * 98;

    if lp_rate_scaled <= oracle_threshold {
        let lp_pct_of_oracle = (lp_rate as f64 / oracle_rate as f64) * 100.0;
        return Err(TxError::BuildFailed {
            message: format!(
                "FreeMint rate condition not met: LP rate ({} nanoERG/token, {:.2}% of oracle) must be > 98% of oracle rate ({} nanoERG/token). \
                Wait for LP/oracle prices to converge or use LP swap instead.",
                lp_rate, lp_pct_of_oracle, oracle_rate
            ),
        });
    }

    // Check FreeMint period and availability
    //
    // Use conservative counter reset check: if (height + T_BUFFER) > R4,
    // treat as if counter will reset. This prevents race conditions where the
    // transaction is validated at a later height than when we built it.
    let is_counter_reset = current_height > ctx.free_mint_r4_height - T_BUFFER;
    let max_allowed_if_reset = ctx.lp_dexy_reserves / 100; // 1% of LP reserves
    let available_to_mint = if is_counter_reset {
        max_allowed_if_reset
    } else {
        ctx.free_mint_r5_available
    };

    // Check validAmount: dexyMinted <= availableToMint
    if amount > available_to_mint {
        return Err(TxError::BuildFailed {
            message: format!(
                "Amount {} exceeds FreeMint limit {} for this period. \
                {} to mint more.",
                amount,
                available_to_mint,
                if is_counter_reset {
                    "Wait for LP reserves to increase"
                } else {
                    "Wait for FreeMint counter to reset"
                }
            ),
        });
    }

    // Check bank has enough tokens
    if amount > ctx.dexy_in_bank {
        return Err(TxError::BuildFailed {
            message: format!(
                "Amount {} exceeds bank reserves {}",
                amount, ctx.dexy_in_bank
            ),
        });
    }

    Ok(())
}

/// Calculate ERG amounts for FreeMint matching contract formulas exactly
///
/// The contract uses specific integer division order that we must match:
/// - bankRate = oracleRate * (bankFeeNum + feeDenom) / feeDenom = oracleRate * 1003 / 1000
/// - buybackRate = oracleRate * buybackFeeNum / feeDenom = oracleRate * 2 / 1000
/// - validBankDelta: ergsAdded >= dexyMinted * bankRate
/// - validBuybackDelta: buybackErgsAdded >= dexyMinted * buybackRate
///
/// CRITICAL: The contract first computes the rate (with integer division), then multiplies
/// by amount. This order matters! For example:
/// - Contract: bankRate = 2319455 * 1003 / 1000 = 2326417; required = 100 * 2326417 = 232641700
/// - Wrong:    100 * 2319455 * 1003 / 1000 = 232641336500 / 1000 = 232641336 (364 short!)
///
/// Returns (bank_erg_added, buyback_fee)
fn calculate_mint_amounts(amount: i64, oracle_rate_nano: i64, variant: DexyVariant) -> (i64, i64) {
    // Apply oracle divisor to convert raw oracle value to nanoERG per token
    // - Gold: oracle gives nanoERG per kg, divide by 1,000,000 for per mg (per token)
    // - USD: oracle gives nanoERG per USD, divide by 1,000 for per 0.001 USE (per token)
    let oracle_rate = oracle_rate_nano / variant.oracle_divisor();

    // CRITICAL: Calculate rates FIRST (with integer division), THEN multiply by amount
    // This matches the contract's order of operations exactly
    //
    // Contract: bankRate = oracleRate * (bankFeeNum + feeDenom) / feeDenom
    // Contract: validBankDelta = ergsAdded >= dexyMinted * bankRate
    let bank_rate = oracle_rate * (FEE_DENOM + BANK_FEE_NUM) / FEE_DENOM;
    let bank_erg_added = amount * bank_rate;

    // Contract: buybackRate = oracleRate * buybackFeeNum / feeDenom
    // Contract: validBuybackDelta = buybackErgsAdded >= dexyMinted * buybackRate
    let buyback_rate = oracle_rate * BUYBACK_FEE_NUM / FEE_DENOM;
    let buyback_fee = amount * buyback_rate;

    (bank_erg_added, buyback_fee)
}

/// Calculate base ERG cost (without fees) - for display purposes only
fn calculate_base_cost(amount: i64, oracle_rate_nano: i64, variant: DexyVariant) -> i64 {
    let adjusted_rate = oracle_rate_nano / variant.oracle_divisor();
    amount * adjusted_rate
}

/// Calculate just the bank fee portion - for display purposes only
fn calculate_bank_fee(amount: i64, oracle_rate_nano: i64, variant: DexyVariant) -> i64 {
    let adjusted_rate = oracle_rate_nano / variant.oracle_divisor();
    amount * adjusted_rate * BANK_FEE_NUM / FEE_DENOM
}

/// Build a FreeMint transaction for Dexy
///
/// Creates an unsigned EIP-12 transaction following the FreeMint protocol.
pub fn build_mint_dexy_tx(
    request: &MintDexyRequest,
    ctx: &DexyTxContext,
    state: &DexyState,
) -> Result<BuildResult, TxError> {
    // Determine output ErgoTree (recipient or self)
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    // Pre-flight validation: check all conditions that FreeMint contract will check
    // This provides better error messages before building the transaction
    validate_free_mint_preflight(ctx, request.amount, request.current_height, request.variant)?;

    // Diagnostic logging to help debug contract validation failures
    tracing::info!("=== FreeMint Transaction Build ===");
    tracing::info!(
        "Request: amount={}, height={}, variant={:?}",
        request.amount,
        request.current_height,
        request.variant
    );
    tracing::info!("Oracle rate (raw): {} nanoERG", ctx.oracle_rate_nano);
    tracing::info!(
        "Oracle rate (adjusted): {} nanoERG/token",
        ctx.oracle_rate_nano / request.variant.oracle_divisor()
    );
    tracing::info!(
        "LP: erg={}, dexy={}",
        ctx.lp_erg_reserves,
        ctx.lp_dexy_reserves
    );
    tracing::info!(
        "LP rate: {} nanoERG/token",
        ctx.lp_erg_reserves / ctx.lp_dexy_reserves
    );
    tracing::info!("FreeMint R4 (reset height): {}", ctx.free_mint_r4_height);
    tracing::info!("FreeMint R5 (available): {}", ctx.free_mint_r5_available);
    tracing::info!("Bank: erg={}, dexy={}", ctx.bank_erg_nano, ctx.dexy_in_bank);
    tracing::info!("Buyback: erg={}", ctx.buyback_erg_nano);

    // Calculate costs and fees using raw oracle rate
    // CRITICAL: Use calculate_mint_amounts which matches contract's integer division exactly
    let (bank_erg_added, buyback_fee) =
        calculate_mint_amounts(request.amount, ctx.oracle_rate_nano, request.variant);

    // Calculate display values (base cost and bank fee separately for logging)
    let base_cost = calculate_base_cost(request.amount, ctx.oracle_rate_nano, request.variant);
    let bank_fee = calculate_bank_fee(request.amount, ctx.oracle_rate_nano, request.variant);

    // Total cost = bank payment + buyback fee + tx fee + min box for user output
    let total_cost =
        bank_erg_added + buyback_fee + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;

    // Select minimum user UTXOs
    let selected = select_erg_boxes(&request.user_inputs, total_cost as u64).map_err(|e| {
        TxError::BuildFailed {
            message: e.to_string(),
        }
    })?;

    // Validate amount against state
    if request.amount > state.dexy_in_bank {
        return Err(TxError::BuildFailed {
            message: format!(
                "Amount {} exceeds available tokens {} in bank",
                request.amount, state.dexy_in_bank
            ),
        });
    }

    // Check FreeMint availability
    //
    // CRITICAL: There's a race condition between transaction building and validation.
    // If we're close to the counter reset height, the transaction might be validated
    // at a later block where isCounterReset becomes TRUE but we built for FALSE.
    //
    // The contract checks: isCounterReset = HEIGHT > selfInR4
    // If isCounterReset is TRUE, it expects: successorR4 >= HEIGHT + 360
    //
    // If we build at height H and validation happens at height H' (where H' could be H+1, H+2, etc.),
    // the contract will see isCounterReset = H' > R4.
    //
    // To avoid mismatch, we conservatively treat the counter as "about to reset" if:
    // (H + T_BUFFER) > R4, which is equivalent to H > R4 - T_BUFFER
    //
    // This ensures our R4 calculation will be valid even if the transaction is
    // included several blocks later than when we built it.
    let is_counter_reset = request.current_height > ctx.free_mint_r4_height - T_BUFFER;

    // Log the reset logic for debugging
    tracing::info!(
        "Counter reset check: height={}, R4={}, threshold={}, is_reset={}",
        request.current_height,
        ctx.free_mint_r4_height,
        ctx.free_mint_r4_height - T_BUFFER,
        is_counter_reset
    );

    let max_allowed_if_reset = ctx.lp_dexy_reserves / 100; // 1% of LP reserves
    let available_to_mint = if is_counter_reset {
        max_allowed_if_reset
    } else {
        ctx.free_mint_r5_available
    };

    if request.amount > available_to_mint {
        return Err(TxError::BuildFailed {
            message: format!(
                "Amount {} exceeds FreeMint limit {} for this period",
                request.amount, available_to_mint
            ),
        });
    }

    // Build inputs: FreeMint (0), Bank (1), Buyback (2), User UTXOs (3+)
    // Only Buyback uses context extension (getVar) - FreeMint and Bank don't
    let free_mint_input = ctx.free_mint_input.clone();
    let bank_input = ctx.bank_input.clone();
    let mut buyback_input = ctx.buyback_input.clone();

    // Log input box tokens for NFT validation debugging
    tracing::info!("=== Input Boxes ===");
    tracing::info!("FreeMint input: box_id={}", free_mint_input.box_id);
    if let Some(first) = free_mint_input.assets.first() {
        tracing::info!(
            "  tokens(0): id={}, amount={}",
            first.token_id,
            first.amount
        );
    }
    tracing::info!("Bank input: box_id={}", bank_input.box_id);
    for (i, asset) in bank_input.assets.iter().enumerate() {
        tracing::info!(
            "  tokens({}): id={}, amount={}",
            i,
            asset.token_id,
            asset.amount
        );
    }
    tracing::info!("Buyback input: box_id={}", buyback_input.box_id);
    if let Some(first) = buyback_input.assets.first() {
        tracing::info!(
            "  tokens(0): id={}, amount={}",
            first.token_id,
            first.amount
        );
    }

    // Set context extension on Buyback: variable 0 = action type (1 for top-up during mint)
    let action_extension = serialize_int_constant(BUYBACK_ACTION_TOPUP)?;
    buyback_input
        .extension
        .insert("0".to_string(), action_extension);
    tracing::info!(
        "Buyback context extension: var(0) = {} (top-up action)",
        BUYBACK_ACTION_TOPUP
    );

    let mut inputs = vec![free_mint_input, bank_input, buyback_input];
    inputs.extend(selected.boxes.clone());

    // Data inputs: Oracle (0), LP (1)
    tracing::info!("=== Data Inputs ===");
    tracing::info!("Oracle data input: box_id={}", ctx.oracle_data_input.box_id);
    if let Some(first) = ctx.oracle_data_input.assets.first() {
        tracing::info!("  tokens(0): id={}", first.token_id);
    }
    tracing::info!("LP data input: box_id={}", ctx.lp_data_input.box_id);
    for (i, asset) in ctx.lp_data_input.assets.iter().enumerate() {
        tracing::info!(
            "  tokens({}): id={}, amount={}",
            i,
            asset.token_id,
            asset.amount
        );
    }

    let data_inputs = vec![ctx.oracle_data_input.clone(), ctx.lp_data_input.clone()];

    // Calculate new FreeMint R4 and R5
    let (new_r4, new_r5) = if is_counter_reset {
        // Reset: R4 = HEIGHT + T_free, R5 = max_allowed - amount
        (
            request.current_height + T_FREE + T_BUFFER,
            max_allowed_if_reset - request.amount,
        )
    } else {
        // No reset: R4 unchanged, R5 = available - amount
        (
            ctx.free_mint_r4_height,
            ctx.free_mint_r5_available - request.amount,
        )
    };

    tracing::info!(
        "Counter reset: {} (height {} vs R4 {})",
        is_counter_reset,
        request.current_height,
        ctx.free_mint_r4_height
    );
    tracing::info!(
        "Max allowed if reset: {} (1% of LP dexy reserves)",
        max_allowed_if_reset
    );
    tracing::info!("Available to mint: {}", available_to_mint);
    tracing::info!("New R4: {}, New R5: {}", new_r4, new_r5);

    // Calculate new bank state
    let new_bank_erg = ctx.bank_erg_nano + bank_erg_added;
    let new_dexy_in_bank = ctx.dexy_in_bank - request.amount;

    // Calculate new buyback state
    let new_buyback_erg = ctx.buyback_erg_nano + buyback_fee;

    // Log deltas for contract validation debugging
    let oracle_rate = ctx.oracle_rate_nano / request.variant.oracle_divisor();
    let lp_rate = ctx.lp_erg_reserves / ctx.lp_dexy_reserves;
    tracing::info!("=== Delta Calculations ===");
    tracing::info!("Oracle rate (for contract): {} nanoERG/token", oracle_rate);
    tracing::info!("LP rate (for contract): {} nanoERG/token", lp_rate);
    tracing::info!(
        "Rate condition: lpRate*100={} > oracleRate*98={} ? {}",
        lp_rate * 100,
        oracle_rate * 98,
        lp_rate * 100 > oracle_rate * 98
    );
    tracing::info!("Base cost: {} nanoERG", base_cost);
    tracing::info!("Bank fee: {} nanoERG", bank_fee);
    tracing::info!("Bank ERG added: {} nanoERG", bank_erg_added);
    tracing::info!(
        "Bank rate (contract): {} (oracle * 1003 / 1000)",
        oracle_rate * 1003 / 1000
    );
    tracing::info!(
        "Bank delta check: ergsAdded({}) >= dexyMinted({}) * bankRate({}) = {} ? {}",
        bank_erg_added,
        request.amount,
        oracle_rate * 1003 / 1000,
        request.amount * oracle_rate * 1003 / 1000,
        bank_erg_added >= request.amount * oracle_rate * 1003 / 1000
    );
    tracing::info!("Buyback fee: {} nanoERG", buyback_fee);
    tracing::info!(
        "Buyback rate (contract): {} (oracle * 2 / 1000)",
        oracle_rate * 2 / 1000
    );
    tracing::info!(
        "Buyback delta check: ergsAdded({}) >= dexyMinted({}) * buybackRate({}) = {} ? {}",
        buyback_fee,
        request.amount,
        oracle_rate * 2 / 1000,
        request.amount * oracle_rate * 2 / 1000,
        buyback_fee >= request.amount * oracle_rate * 2 / 1000
    );

    // Build outputs
    let mut outputs = Vec::new();

    tracing::info!("=== Building Outputs ===");

    // Output 0: FreeMint box (updated registers)
    tracing::info!(
        "FreeMint output: value={}, R4={}, R5={}",
        ctx.free_mint_erg_nano,
        new_r4,
        new_r5
    );
    if is_counter_reset {
        tracing::info!(
            "R4 validation (if reset): R4({}) in [HEIGHT+360, HEIGHT+365]",
            new_r4
        );
        tracing::info!(
            "  At execution HEIGHT={}: R4 must be in [{}, {}]",
            request.current_height,
            request.current_height + T_FREE,
            request.current_height + T_FREE + T_BUFFER
        );
    } else {
        tracing::info!(
            "R4 validation (no reset): R4({}) == selfInR4({})",
            new_r4,
            ctx.free_mint_r4_height
        );
    }
    tracing::info!(
        "R5 validation: R5({}) == availableToMint({}) - dexyMinted({}) = {}",
        new_r5,
        available_to_mint,
        request.amount,
        available_to_mint - request.amount
    );

    outputs.push(build_free_mint_output(
        ctx,
        new_r4,
        new_r5,
        request.current_height,
    ));

    // Output 1: Bank box (updated ERG + tokens)
    tracing::info!(
        "Bank output: erg={} (was {}), dexy={} (was {})",
        new_bank_erg,
        ctx.bank_erg_nano,
        new_dexy_in_bank,
        ctx.dexy_in_bank
    );
    tracing::info!(
        "  ERG added: {}, Dexy minted: {}",
        new_bank_erg - ctx.bank_erg_nano,
        ctx.dexy_in_bank - new_dexy_in_bank
    );
    outputs.push(build_bank_output(
        ctx,
        new_bank_erg,
        new_dexy_in_bank,
        request.current_height,
    ));

    // Output 2: Buyback box (receives fee)
    tracing::info!(
        "Buyback output: erg={} (was {}), added={}",
        new_buyback_erg,
        ctx.buyback_erg_nano,
        new_buyback_erg - ctx.buyback_erg_nano
    );
    outputs.push(build_buyback_output(
        ctx,
        new_buyback_erg,
        request.current_height,
    )?);

    // Output 3: User receives Dexy tokens (goes to recipient if set)
    outputs.push(Eip12Output::change(
        constants::MIN_BOX_VALUE_NANO,
        output_ergo_tree,
        vec![Eip12Asset::new(&state.dexy_token_id, request.amount)],
        request.current_height,
    ));

    // Output 4: Miner fee
    outputs.push(Eip12Output::fee(
        constants::TX_FEE_NANO,
        request.current_height,
    ));

    // Output 5: Change to user (if needed)
    let change_assets = collect_change_tokens(&selected.boxes, None);
    let change_erg = selected.total_erg as i64
        - bank_erg_added
        - buyback_fee
        - constants::TX_FEE_NANO
        - constants::MIN_BOX_VALUE_NANO;

    if change_erg >= constants::MIN_BOX_VALUE_NANO || !change_assets.is_empty() {
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

    let action = format!("mint_dexy_{}", request.variant.as_str());
    let summary = TxSummary {
        action,
        erg_amount_nano: bank_erg_added + buyback_fee,
        token_amount: request.amount,
        token_name: request.variant.token_name().to_string(),
        tx_fee_nano: constants::TX_FEE_NANO,
        bank_fee_nano: bank_fee,
        buyback_fee_nano: buyback_fee,
    };

    Ok(BuildResult {
        unsigned_tx,
        summary,
    })
}

/// Build FreeMint output box
fn build_free_mint_output(
    ctx: &DexyTxContext,
    new_r4: i32,
    new_r5: i64,
    height: i32,
) -> Eip12Output {
    use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::types::stype::SType;

    // Serialize R4 (Int) and R5 (Long)
    let r4_constant = Constant {
        tpe: SType::SInt,
        v: Literal::Int(new_r4),
    };
    let r5_constant = Constant {
        tpe: SType::SLong,
        v: Literal::Long(new_r5),
    };

    let r4_bytes = r4_constant.sigma_serialize_bytes().unwrap();
    let r5_bytes = r5_constant.sigma_serialize_bytes().unwrap();

    let mut registers = HashMap::new();
    registers.insert("R4".to_string(), base16::encode_lower(&r4_bytes));
    registers.insert("R5".to_string(), base16::encode_lower(&r5_bytes));

    // Copy tokens from input (FreeMint NFT)
    let assets: Vec<Eip12Asset> = ctx
        .free_mint_input
        .assets
        .iter()
        .map(|a| Eip12Asset::new(&a.token_id, a.amount.parse().unwrap_or(1)))
        .collect();

    Eip12Output {
        value: ctx.free_mint_erg_nano.to_string(),
        ergo_tree: ctx.free_mint_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: registers,
    }
}

/// Build bank output box
///
/// Token order must exactly match input bank box.
/// Bank output does NOT need R4 register (unlike Buyback).
fn build_bank_output(
    ctx: &DexyTxContext,
    new_erg: i64,
    new_dexy_tokens: i64,
    height: i32,
) -> Eip12Output {
    // Copy token order from current bank box, updating Dexy amount
    let mut assets: Vec<Eip12Asset> = Vec::new();

    for (i, asset) in ctx.bank_input.assets.iter().enumerate() {
        let new_amount = if i == 0 {
            // First token is bank NFT, always 1
            1
        } else {
            // Second token is Dexy
            new_dexy_tokens
        };
        assets.push(Eip12Asset::new(&asset.token_id, new_amount));
    }

    // Bank output has NO additional registers in FreeMint transactions
    // (verified against successful Crux Finance transactions)

    Eip12Output {
        value: new_erg.to_string(),
        ergo_tree: ctx.bank_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: HashMap::new(),
    }
}

/// Build buyback output box
///
/// The Buyback contract (action=1, top-up) requires R4 to contain the input box's ID.
fn build_buyback_output(
    ctx: &DexyTxContext,
    new_erg: i64,
    height: i32,
) -> Result<Eip12Output, TxError> {
    use ergo_lib::ergotree_ir::mir::constant::Constant;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    // Copy tokens from buyback input
    let assets: Vec<Eip12Asset> = ctx
        .buyback_input
        .assets
        .iter()
        .map(|a| Eip12Asset::new(&a.token_id, a.amount.parse().unwrap_or(1)))
        .collect();

    // Set R4 to input box ID (required by Buyback contract for top-up action)
    // R4: Coll[Byte] = SELF.id (the input box's ID)
    let box_id_bytes: Vec<u8> =
        base16::decode(&ctx.buyback_input.box_id).map_err(|e| TxError::BuildFailed {
            message: format!("Invalid buyback box_id hex: {}", e),
        })?;
    let r4_constant: Constant = box_id_bytes.into();
    let r4_bytes = r4_constant
        .sigma_serialize_bytes()
        .map_err(|e| TxError::BuildFailed {
            message: format!("Failed to serialize buyback R4: {}", e),
        })?;

    let mut registers = HashMap::new();
    registers.insert("R4".to_string(), base16::encode_lower(&r4_bytes));

    Ok(Eip12Output {
        value: new_erg.to_string(),
        ergo_tree: ctx.buyback_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: registers,
    })
}

/// Serialize an Int constant to hex for context extension
fn serialize_int_constant(value: i32) -> Result<String, TxError> {
    use ergo_lib::ergotree_ir::mir::constant::{Constant, Literal};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
    use ergo_lib::ergotree_ir::types::stype::SType;

    let constant = Constant {
        tpe: SType::SInt,
        v: Literal::Int(value),
    };
    let bytes = constant
        .sigma_serialize_bytes()
        .map_err(|e| TxError::BuildFailed {
            message: format!("Int serialization failed: {}", e),
        })?;
    Ok(base16::encode_lower(&bytes))
}

// =============================================================================
// LP Swap Transaction Builder
// =============================================================================

/// Direction of the LP swap
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapDirection {
    ErgToDexy,
    DexyToErg,
}

/// Request to build a Dexy LP swap transaction
#[derive(Debug, Clone)]
pub struct SwapDexyRequest {
    pub variant: DexyVariant,
    pub direction: SwapDirection,
    pub input_amount: i64,
    pub min_output: i64,
    pub user_address: String,
    pub user_ergo_tree: String,
    pub user_inputs: Vec<Eip12InputBox>,
    pub current_height: i32,
    /// Optional recipient ErgoTree. If set, primary output goes here instead of user_ergo_tree.
    pub recipient_ergo_tree: Option<String>,
}

/// Summary of a swap transaction for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapTxSummary {
    pub direction: String,
    pub input_amount: i64,
    pub output_amount: i64,
    pub min_output: i64,
    pub price_impact_pct: f64,
    pub fee_pct: f64,
    pub miner_fee_nano: i64,
}

/// Build result with unsigned transaction and swap summary
#[derive(Debug)]
pub struct SwapBuildResult {
    pub unsigned_tx: Eip12UnsignedTx,
    pub summary: SwapTxSummary,
}

/// Build a Dexy LP swap transaction
///
/// Creates an unsigned EIP-12 transaction for swapping ERG <-> Dexy tokens
/// through the LP pool.
///
/// # Transaction Structure
///
/// **Inputs:**
/// - 0: LP box (script-validated)
/// - 1: Swap NFT box (script-validated)
/// - 2+: User UTXOs (signature-validated)
///
/// **Outputs:**
/// - 0: Updated LP box
/// - 1: Preserved Swap NFT box
/// - 2: User output (swap proceeds)
/// - 3: Miner fee
/// - 4+: Change output (if needed)
pub fn build_swap_dexy_tx(
    request: &SwapDexyRequest,
    ctx: &DexySwapTxContext,
    state: &DexyState,
) -> Result<SwapBuildResult, TxError> {
    // Determine output ErgoTree (recipient or self)
    let output_ergo_tree = request
        .recipient_ergo_tree
        .as_deref()
        .unwrap_or(&request.user_ergo_tree);

    // 1. Validate input amount
    if request.input_amount <= 0 {
        return Err(TxError::BuildFailed {
            message: "Input amount must be positive".to_string(),
        });
    }

    // 2. Calculate output amount using constant product formula
    let (output_amount, new_lp_erg, new_lp_dexy) = match request.direction {
        SwapDirection::ErgToDexy => {
            let out = calculate_lp_swap_output(
                request.input_amount,
                ctx.lp_erg_reserves,
                ctx.lp_dexy_reserves,
                LP_SWAP_FEE_NUM,
                LP_SWAP_FEE_DENOM,
            );
            let new_erg = ctx.lp_erg_reserves + request.input_amount;
            let new_dexy = ctx.lp_dexy_reserves - out;
            (out, new_erg, new_dexy)
        }
        SwapDirection::DexyToErg => {
            let out = calculate_lp_swap_output(
                request.input_amount,
                ctx.lp_dexy_reserves,
                ctx.lp_erg_reserves,
                LP_SWAP_FEE_NUM,
                LP_SWAP_FEE_DENOM,
            );
            let new_erg = ctx.lp_erg_reserves - out;
            let new_dexy = ctx.lp_dexy_reserves + request.input_amount;
            (out, new_erg, new_dexy)
        }
    };

    if output_amount <= 0 {
        return Err(TxError::BuildFailed {
            message: "Output amount is zero or negative".to_string(),
        });
    }

    // 3. Check slippage tolerance
    if output_amount < request.min_output {
        return Err(TxError::BuildFailed {
            message: format!(
                "Output {} below minimum {}",
                output_amount, request.min_output
            ),
        });
    }

    // 4. Validate against contract formula
    let (delta_x, delta_y) = match request.direction {
        SwapDirection::ErgToDexy => (request.input_amount, -output_amount),
        SwapDirection::DexyToErg => (-output_amount, request.input_amount),
    };
    if !validate_lp_swap(
        ctx.lp_erg_reserves,
        ctx.lp_dexy_reserves,
        delta_x,
        delta_y,
        LP_SWAP_FEE_NUM,
        LP_SWAP_FEE_DENOM,
    ) {
        return Err(TxError::BuildFailed {
            message: "Swap fails contract validation".to_string(),
        });
    }

    // 5-6. Select minimum user UTXOs
    let selected = match request.direction {
        SwapDirection::ErgToDexy => {
            let needed =
                request.input_amount + constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
            select_erg_boxes(&request.user_inputs, needed as u64).map_err(|e| {
                TxError::BuildFailed {
                    message: e.to_string(),
                }
            })?
        }
        SwapDirection::DexyToErg => {
            let min_erg = constants::TX_FEE_NANO + constants::MIN_BOX_VALUE_NANO;
            select_token_boxes(
                &request.user_inputs,
                &state.dexy_token_id,
                request.input_amount as u64,
                min_erg as u64,
            )
            .map_err(|e| TxError::BuildFailed {
                message: e.to_string(),
            })?
        }
    };

    // 7. Build inputs: LP (0), Swap NFT (1), User UTXOs (2+)
    let mut inputs = vec![ctx.lp_input.clone(), ctx.swap_input.clone()];
    inputs.extend(selected.boxes.clone());

    // 8. Build LP output (Output 0) - updated reserves
    let lp_output = build_lp_swap_output(
        ctx,
        new_lp_erg,
        new_lp_dexy,
        &state.dexy_token_id,
        request.current_height,
    );

    // 9. Build Swap NFT output (Output 1) - exact preservation
    let swap_nft_output = build_swap_nft_output(ctx, request.current_height);

    // 10. Build user output, miner fee, and change
    let mut outputs = vec![lp_output, swap_nft_output];

    match request.direction {
        SwapDirection::ErgToDexy => {
            // User receives Dexy tokens + change ERG
            let user_output_erg = constants::MIN_BOX_VALUE_NANO;
            let change_erg = selected.total_erg as i64
                - request.input_amount
                - constants::TX_FEE_NANO
                - user_output_erg;

            // User output: min ERG + Dexy tokens received (goes to recipient if set)
            outputs.push(Eip12Output::change(
                user_output_erg,
                output_ergo_tree,
                vec![Eip12Asset::new(&state.dexy_token_id, output_amount)],
                request.current_height,
            ));

            // Miner fee
            outputs.push(Eip12Output::fee(
                constants::TX_FEE_NANO,
                request.current_height,
            ));

            // Change output (if needed)
            let change_tokens = collect_change_tokens(&selected.boxes, None);
            if change_erg >= constants::MIN_BOX_VALUE_NANO || !change_tokens.is_empty() {
                let change_value = change_erg.max(constants::MIN_BOX_VALUE_NANO);
                outputs.push(Eip12Output::change(
                    change_value,
                    &request.user_ergo_tree,
                    change_tokens,
                    request.current_height,
                ));
            }
        }
        SwapDirection::DexyToErg => {
            // User receives ERG from swap + their remaining ERG minus fees
            let user_output_erg =
                selected.total_erg as i64 + output_amount - constants::TX_FEE_NANO;

            // Collect remaining tokens from selected boxes (subtract spent Dexy tokens)
            let remaining_assets = collect_change_tokens(
                &selected.boxes,
                Some((&state.dexy_token_id, request.input_amount as u64)),
            );

            // User output with all ERG and remaining tokens (goes to recipient if set)
            outputs.push(Eip12Output::change(
                user_output_erg,
                output_ergo_tree,
                remaining_assets,
                request.current_height,
            ));

            // Miner fee
            outputs.push(Eip12Output::fee(
                constants::TX_FEE_NANO,
                request.current_height,
            ));
        }
    }

    // 11. Build unsigned transaction
    let unsigned_tx = Eip12UnsignedTx {
        inputs,
        data_inputs: vec![],
        outputs,
    };

    // Calculate price impact for summary
    let price_impact = calculate_lp_swap_price_impact(
        request.input_amount,
        match request.direction {
            SwapDirection::ErgToDexy => ctx.lp_erg_reserves,
            SwapDirection::DexyToErg => ctx.lp_dexy_reserves,
        },
        match request.direction {
            SwapDirection::ErgToDexy => ctx.lp_dexy_reserves,
            SwapDirection::DexyToErg => ctx.lp_erg_reserves,
        },
        LP_SWAP_FEE_NUM,
        LP_SWAP_FEE_DENOM,
    );

    let summary = SwapTxSummary {
        direction: match request.direction {
            SwapDirection::ErgToDexy => "erg_to_dexy".to_string(),
            SwapDirection::DexyToErg => "dexy_to_erg".to_string(),
        },
        input_amount: request.input_amount,
        output_amount,
        min_output: request.min_output,
        price_impact_pct: price_impact,
        fee_pct: LP_SWAP_FEE_NUM as f64 / LP_SWAP_FEE_DENOM as f64 * 100.0,
        miner_fee_nano: constants::TX_FEE_NANO,
    };

    Ok(SwapBuildResult {
        unsigned_tx,
        summary,
    })
}

/// Build updated LP box output for swap transaction
///
/// Preserves token order from the input LP box while updating the ERG value
/// and Dexy token amount to reflect the swap.
fn build_lp_swap_output(
    ctx: &DexySwapTxContext,
    new_erg: i64,
    new_dexy: i64,
    dexy_token_id: &str,
    height: i32,
) -> Eip12Output {
    // Preserve token order from input LP box, updating amounts
    let mut assets = Vec::new();
    for (token_id, amount) in &ctx.lp_tokens {
        if token_id == dexy_token_id {
            assets.push(Eip12Asset::new(token_id, new_dexy));
        } else {
            assets.push(Eip12Asset::new(token_id, *amount as i64));
        }
    }

    // Copy registers from input LP box
    let additional_registers = ctx.lp_input.additional_registers.clone();

    Eip12Output {
        value: new_erg.to_string(),
        ergo_tree: ctx.lp_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers,
    }
}

/// Build preserved Swap NFT box output (exact copy)
///
/// The Swap NFT box must be reproduced exactly in the output to satisfy
/// the contract's self-preservation check.
fn build_swap_nft_output(ctx: &DexySwapTxContext, height: i32) -> Eip12Output {
    let assets: Vec<Eip12Asset> = ctx
        .swap_tokens
        .iter()
        .map(|(id, amt)| Eip12Asset::new(id, *amt as i64))
        .collect();

    Eip12Output {
        value: ctx.swap_erg_value.to_string(),
        ergo_tree: ctx.swap_ergo_tree.clone(),
        assets,
        creation_height: height,
        additional_registers: ctx.swap_input.additional_registers.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_mint_positive_amount() {
        let state = create_test_state(10000, true);

        // Valid amount
        assert!(validate_mint_dexy(100, &state).is_ok());

        // Zero amount
        let result = validate_mint_dexy(0, &state);
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtocolError::InvalidAmount { .. })));

        // Negative amount
        let result = validate_mint_dexy(-100, &state);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mint_can_mint() {
        // Bank has no tokens
        let state = create_test_state(0, false);

        let result = validate_mint_dexy(100, &state);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ProtocolError::ActionNotAllowed { .. })
        ));
    }

    #[test]
    fn test_validate_mint_exceeds_available() {
        let state = create_test_state(1000, true);

        // Amount exceeds available
        let result = validate_mint_dexy(2000, &state);
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtocolError::InvalidAmount { .. })));
    }

    #[test]
    fn test_calculate_mint_amounts_use() {
        // Test with USE (oracle divisor = 1000)
        // Oracle rate: 1_850_000_000 nanoERG per USD (1.85 ERG per USD)
        // Amount: 1_000 (1 USE with 3 decimals)
        let (bank_erg_added, buyback_fee) =
            calculate_mint_amounts(1_000, 1_850_000_000, DexyVariant::Usd);

        // Adjusted rate = 1_850_000_000 / 1000 = 1_850_000
        // Contract formula (order matters!):
        //   bankRate = 1_850_000 * 1003 / 1000 = 1_855_550
        //   bank_erg_added = 1_000 * 1_855_550 = 1_855_550_000
        //   buybackRate = 1_850_000 * 2 / 1000 = 3_700
        //   buyback_fee = 1_000 * 3_700 = 3_700_000
        assert_eq!(bank_erg_added, 1_855_550_000);
        assert_eq!(buyback_fee, 3_700_000);
    }

    #[test]
    fn test_calculate_mint_amounts_gold() {
        // Test with DexyGold (oracle divisor = 1_000_000)
        // Oracle rate: 220_000_000_000 nanoERG per kg (220 ERG per kg)
        // Amount: 10 (10 DexyGold tokens = 10 mg)
        let (bank_erg_added, buyback_fee) =
            calculate_mint_amounts(10, 220_000_000_000, DexyVariant::Gold);

        // Adjusted rate = 220_000_000_000 / 1_000_000 = 220_000 nanoERG per mg
        // Contract formula (order matters!):
        //   bankRate = 220_000 * 1003 / 1000 = 220_660
        //   bank_erg_added = 10 * 220_660 = 2_206_600
        //   buybackRate = 220_000 * 2 / 1000 = 440
        //   buyback_fee = 10 * 440 = 4_400
        assert_eq!(bank_erg_added, 2_206_600);
        assert_eq!(buyback_fee, 4_400);
    }

    #[test]
    fn test_integer_division_order_matters() {
        // This test verifies our calculation matches contract's integer division exactly
        //
        // The order of operations matters due to integer division:
        // - (amount * rate * 1003) / 1000 gives different result than
        // - amount * (rate * 1003 / 1000)
        //
        // The contract uses the latter form, so we must match it.

        let amount: i64 = 100;
        let oracle_rate_nano: i64 = 2_319_455_000; // Example oracle value
        let variant = DexyVariant::Usd;

        let oracle_rate = oracle_rate_nano / variant.oracle_divisor(); // 2_319_455

        // Wrong order: (amount * rate * 1003) / 1000
        // = (100 * 2_319_455 * 1003) / 1000 = 232_641_336_500 / 1000 = 232_641_336
        let wrong_order = amount * oracle_rate * 1003 / 1000;

        // Contract's order: amount * (rate * 1003 / 1000)
        // = 100 * (2_319_455 * 1003 / 1000) = 100 * 2_326_413 = 232_641_300
        let bank_rate = oracle_rate * 1003 / 1000;
        let contract_order = amount * bank_rate;

        // Our new (correct) calculation
        let (new_bank_erg_added, _) = calculate_mint_amounts(amount, oracle_rate_nano, variant);

        // The two orders give different results (36 nanoERG difference in this case)
        assert_ne!(
            wrong_order, contract_order,
            "Different division order should give different results"
        );

        // New calculation matches contract order exactly
        assert_eq!(
            new_bank_erg_added, contract_order,
            "New calculation {} should equal contract order {}",
            new_bank_erg_added, contract_order
        );
    }

    #[test]
    fn test_tx_summary() {
        let summary = TxSummary {
            action: "mint_dexy_gold".to_string(),
            erg_amount_nano: 1_000_000_000,
            token_amount: 100,
            token_name: "DexyGold".to_string(),
            tx_fee_nano: 1_100_000,
            bank_fee_nano: 3_000_000,
            buyback_fee_nano: 2_000_000,
        };

        assert_eq!(summary.action, "mint_dexy_gold");
        assert_eq!(summary.token_name, "DexyGold");
    }

    // Helper functions for tests

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

    mod swap_tests {
        use super::*;
        use crate::fetch::DexySwapTxContext;

        const DEXY_TOKEN_ID: &str =
            "6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad";
        const LP_NFT_ID: &str = "905ecdef97381b92c2f0ea9b516f312bfb18082c61b24b40affa6a55555c77c7";
        const LP_TOKEN_ID: &str = "lp_token_id_placeholder";
        const SWAP_NFT_ID: &str =
            "ff7b7eff3c818f9dc573ca03a723a7f6ed1615bf27980ebd4a6c91986b26f801";

        /// Create a dummy ErgoBox for test contexts.
        ///
        /// The tx builder only uses the EIP-12 and parsed fields from
        /// DexySwapTxContext, never the raw ErgoBox fields. However, we must
        /// provide valid ErgoBox values to satisfy the struct.
        fn create_dummy_ergo_box() -> ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox {
            use ergo_lib::ergotree_ir::chain::ergo_box::{
                box_value::BoxValue, ErgoBox, NonMandatoryRegisters,
            };
            use ergo_lib::ergotree_ir::chain::tx_id::TxId;
            use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
            use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

            // Use a minimal P2PK ErgoTree (simplest valid tree)
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

        /// Create a minimal DexySwapTxContext for testing.
        ///
        /// Uses only EIP-12 fields and parsed values. The `lp_box` and `swap_box`
        /// (ErgoBox) fields are provided as dummy values since the tx builder
        /// never accesses them directly.
        fn create_test_swap_context(lp_erg: i64, lp_dexy: i64) -> DexySwapTxContext {
            let lp_input = Eip12InputBox {
                box_id: "lp_box_id".to_string(),
                transaction_id: "lp_tx_id".to_string(),
                index: 0,
                value: lp_erg.to_string(),
                ergo_tree: "lp_ergo_tree_hex".to_string(),
                assets: vec![
                    Eip12Asset::new(LP_NFT_ID, 1),
                    Eip12Asset::new(LP_TOKEN_ID, 9_000_000_000_000_000i64),
                    Eip12Asset::new(DEXY_TOKEN_ID, lp_dexy),
                ],
                creation_height: 100000,
                additional_registers: HashMap::new(),
                extension: HashMap::new(),
            };

            let swap_input = Eip12InputBox {
                box_id: "swap_box_id".to_string(),
                transaction_id: "swap_tx_id".to_string(),
                index: 0,
                value: "1000000".to_string(),
                ergo_tree: "swap_ergo_tree_hex".to_string(),
                assets: vec![Eip12Asset::new(SWAP_NFT_ID, 1)],
                creation_height: 100000,
                additional_registers: HashMap::new(),
                extension: HashMap::new(),
            };

            // Create dummy ErgoBox values (not accessed by tx builder)
            let dummy_box = create_dummy_ergo_box();

            DexySwapTxContext {
                lp_input,
                lp_erg_reserves: lp_erg,
                lp_dexy_reserves: lp_dexy,
                lp_ergo_tree: "lp_ergo_tree_hex".to_string(),
                lp_box: dummy_box.clone(),
                lp_tokens: vec![
                    (LP_NFT_ID.to_string(), 1),
                    (LP_TOKEN_ID.to_string(), 9_000_000_000_000_000),
                    (DEXY_TOKEN_ID.to_string(), lp_dexy as u64),
                ],
                swap_input,
                swap_erg_value: 1_000_000,
                swap_ergo_tree: "swap_ergo_tree_hex".to_string(),
                swap_box: dummy_box,
                swap_tokens: vec![(SWAP_NFT_ID.to_string(), 1)],
            }
        }

        fn create_swap_state() -> DexyState {
            create_test_state(10000, true)
        }

        fn create_erg_to_dexy_request(
            input_amount: i64,
            min_output: i64,
            user_erg: i64,
        ) -> SwapDexyRequest {
            SwapDexyRequest {
                variant: DexyVariant::Gold,
                direction: SwapDirection::ErgToDexy,
                input_amount,
                min_output,
                user_address: "user_address".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(user_erg, vec![])],
                current_height: 100000,
                recipient_ergo_tree: None,
            }
        }

        fn create_dexy_to_erg_request(
            input_amount: i64,
            min_output: i64,
            user_erg: i64,
            user_dexy: i64,
        ) -> SwapDexyRequest {
            SwapDexyRequest {
                variant: DexyVariant::Gold,
                direction: SwapDirection::DexyToErg,
                input_amount,
                min_output,
                user_address: "user_address".to_string(),
                user_ergo_tree: "user_ergo_tree".to_string(),
                user_inputs: vec![create_test_input(
                    user_erg,
                    vec![(DEXY_TOKEN_ID, user_dexy)],
                )],
                current_height: 100000,
                recipient_ergo_tree: None,
            }
        }

        // --- Helper function tests ---

        #[test]
        fn test_build_lp_swap_output_updates_dexy_amount() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);

            let output = build_lp_swap_output(
                &ctx,
                1_001_000_000_000, // new ERG (added 1 ERG)
                999_000,           // new Dexy (removed 1000)
                DEXY_TOKEN_ID,
                100001,
            );

            assert_eq!(output.value, "1001000000000");
            assert_eq!(output.ergo_tree, "lp_ergo_tree_hex");
            assert_eq!(output.assets.len(), 3);

            // LP NFT preserved
            assert_eq!(output.assets[0].token_id, LP_NFT_ID);
            assert_eq!(output.assets[0].amount, "1");

            // LP token preserved
            assert_eq!(output.assets[1].token_id, LP_TOKEN_ID);
            assert_eq!(output.assets[1].amount, "9000000000000000");

            // Dexy amount updated
            assert_eq!(output.assets[2].token_id, DEXY_TOKEN_ID);
            assert_eq!(output.assets[2].amount, "999000");
        }

        #[test]
        fn test_build_swap_nft_output_preserves_exactly() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);

            let output = build_swap_nft_output(&ctx, 100001);

            assert_eq!(output.value, "1000000");
            assert_eq!(output.ergo_tree, "swap_ergo_tree_hex");
            assert_eq!(output.assets.len(), 1);
            assert_eq!(output.assets[0].token_id, SWAP_NFT_ID);
            assert_eq!(output.assets[0].amount, "1");
        }

        // --- Validation tests ---

        #[test]
        fn test_swap_rejects_zero_input() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(0, 1, 10_000_000_000);

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            let err = result.unwrap_err();
            match err {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("positive"), "Got: {}", message);
                }
                _ => panic!("Expected BuildFailed, got {:?}", err),
            }
        }

        #[test]
        fn test_swap_rejects_negative_input() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(-100, 1, 10_000_000_000);

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
        }

        #[test]
        fn test_swap_rejects_insufficient_erg() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            // Try to swap 10 ERG with only 1 ERG
            let request = create_erg_to_dexy_request(
                10_000_000_000, // 10 ERG input
                1,
                1_000_000_000, // only 1 ERG available
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("Insufficient ERG"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_swap_rejects_insufficient_dexy_tokens() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            // Try to sell 1000 Dexy but only have 100
            let request = create_dexy_to_erg_request(
                1000,           // sell 1000 Dexy
                1,              // min output
                10_000_000_000, // user has 10 ERG for fees
                100,            // but only 100 Dexy
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("Insufficient token"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_swap_rejects_slippage_violation() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            // Set min_output absurdly high so slippage check fails
            let request = create_erg_to_dexy_request(
                1_000_000_000,   // 1 ERG
                999_999_999,     // impossibly high min output
                100_000_000_000, // 100 ERG available
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("below minimum"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        // --- Successful swap tests ---

        #[test]
        fn test_erg_to_dexy_swap_builds_correctly() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(
                1_000_000_000,   // swap 1 ERG
                1,               // min 1 Dexy output
                100_000_000_000, // 100 ERG available
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_ok(), "Build failed: {:?}", result.err());

            let build = result.unwrap();
            let tx = &build.unsigned_tx;

            // Check inputs: LP + Swap NFT + 1 user input
            assert_eq!(tx.inputs.len(), 3);
            assert_eq!(tx.inputs[0].box_id, "lp_box_id");
            assert_eq!(tx.inputs[1].box_id, "swap_box_id");

            // No data inputs for LP swap
            assert_eq!(tx.data_inputs.len(), 0);

            // Outputs: LP + Swap NFT + User + Fee + Change
            assert!(tx.outputs.len() >= 4);

            // Output 0: LP box (updated)
            assert_eq!(tx.outputs[0].ergo_tree, "lp_ergo_tree_hex");

            // Output 1: Swap NFT (preserved)
            assert_eq!(tx.outputs[1].ergo_tree, "swap_ergo_tree_hex");
            assert_eq!(tx.outputs[1].value, "1000000");

            // Output 2: User output with Dexy tokens
            assert_eq!(tx.outputs[2].ergo_tree, "user_ergo_tree");
            assert_eq!(tx.outputs[2].assets.len(), 1);
            assert_eq!(tx.outputs[2].assets[0].token_id, DEXY_TOKEN_ID);
            let user_dexy_out: i64 = tx.outputs[2].assets[0].amount.parse().unwrap();
            assert!(user_dexy_out > 0, "User should receive Dexy tokens");

            // Output 3: Miner fee
            assert_eq!(tx.outputs[3].value, constants::TX_FEE_NANO.to_string());

            // Summary checks
            assert_eq!(build.summary.direction, "erg_to_dexy");
            assert_eq!(build.summary.input_amount, 1_000_000_000);
            assert!(build.summary.output_amount > 0);
            assert_eq!(build.summary.fee_pct, 0.3);
        }

        #[test]
        fn test_dexy_to_erg_swap_builds_correctly() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_dexy_to_erg_request(
                100,            // sell 100 Dexy
                1,              // min 1 nanoERG output
                10_000_000_000, // 10 ERG for fees
                1000,           // have 1000 Dexy
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_ok(), "Build failed: {:?}", result.err());

            let build = result.unwrap();
            let tx = &build.unsigned_tx;

            // Check inputs
            assert_eq!(tx.inputs.len(), 3);

            // Outputs: LP + Swap NFT + User + Fee
            assert_eq!(tx.outputs.len(), 4);

            // Output 2: User receives ERG + remaining Dexy
            let user_output = &tx.outputs[2];
            assert_eq!(user_output.ergo_tree, "user_ergo_tree");
            let user_erg_out: i64 = user_output.value.parse().unwrap();
            // User should get back their ERG + swap output - fees
            assert!(
                user_erg_out > 10_000_000_000,
                "User should receive more ERG than started with"
            );

            // Check remaining Dexy tokens (1000 - 100 = 900)
            let remaining_dexy = user_output
                .assets
                .iter()
                .find(|a| a.token_id == DEXY_TOKEN_ID);
            assert!(remaining_dexy.is_some(), "User should have remaining Dexy");
            assert_eq!(remaining_dexy.unwrap().amount, "900");

            // Summary checks
            assert_eq!(build.summary.direction, "dexy_to_erg");
            assert_eq!(build.summary.input_amount, 100);
            assert!(build.summary.output_amount > 0);
        }

        #[test]
        fn test_swap_summary_price_impact() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();

            // Small swap - should have small price impact
            let small_request = create_erg_to_dexy_request(
                1_000_000_000, // 1 ERG (0.1% of pool)
                1,
                100_000_000_000,
            );
            let small_result = build_swap_dexy_tx(&small_request, &ctx, &state).unwrap();

            // Large swap - should have larger price impact
            let large_request = create_erg_to_dexy_request(
                100_000_000_000, // 100 ERG (10% of pool)
                1,
                200_000_000_000,
            );
            let large_result = build_swap_dexy_tx(&large_request, &ctx, &state).unwrap();

            assert!(
                large_result.summary.price_impact_pct > small_result.summary.price_impact_pct,
                "Large swap should have higher price impact ({} vs {})",
                large_result.summary.price_impact_pct,
                small_result.summary.price_impact_pct
            );
        }

        #[test]
        fn test_swap_lp_output_preserves_token_order() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            let request = create_erg_to_dexy_request(1_000_000_000, 1, 100_000_000_000);

            let result = build_swap_dexy_tx(&request, &ctx, &state).unwrap();
            let lp_output = &result.unsigned_tx.outputs[0];

            // Token order must match: LP NFT, LP Token, Dexy Token
            assert_eq!(lp_output.assets.len(), 3);
            assert_eq!(lp_output.assets[0].token_id, LP_NFT_ID);
            assert_eq!(lp_output.assets[1].token_id, LP_TOKEN_ID);
            assert_eq!(lp_output.assets[2].token_id, DEXY_TOKEN_ID);

            // LP NFT amount unchanged
            assert_eq!(lp_output.assets[0].amount, "1");
            // LP token amount unchanged
            assert_eq!(lp_output.assets[1].amount, "9000000000000000");
        }

        #[test]
        fn test_swap_direction_enum() {
            assert_eq!(SwapDirection::ErgToDexy, SwapDirection::ErgToDexy);
            assert_ne!(SwapDirection::ErgToDexy, SwapDirection::DexyToErg);
        }

        #[test]
        fn test_swap_tx_summary_serialization() {
            let summary = SwapTxSummary {
                direction: "erg_to_dexy".to_string(),
                input_amount: 1_000_000_000,
                output_amount: 997,
                min_output: 990,
                price_impact_pct: 0.1,
                fee_pct: 0.3,
                miner_fee_nano: 1_100_000,
            };

            let json = serde_json::to_string(&summary).unwrap();
            assert!(json.contains("erg_to_dexy"));
            assert!(json.contains("1000000000"));

            // Roundtrip
            let parsed: SwapTxSummary = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.direction, "erg_to_dexy");
            assert_eq!(parsed.input_amount, 1_000_000_000);
            assert_eq!(parsed.output_amount, 997);
        }

        #[test]
        fn test_dexy_to_erg_insufficient_erg_for_fees() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            // User has very little ERG, not enough for fees
            let request = create_dexy_to_erg_request(
                100,     // sell 100 Dexy
                1,       // min output
                100_000, // only 0.0001 ERG - not enough for fee + min box
                1000,    // enough Dexy
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state);
            assert!(result.is_err());
            match result.unwrap_err() {
                TxError::BuildFailed { message } => {
                    assert!(message.contains("Insufficient ERG"), "Got: {}", message);
                }
                other => panic!("Expected BuildFailed, got {:?}", other),
            }
        }

        #[test]
        fn test_erg_to_dexy_change_output_when_needed() {
            let ctx = create_test_swap_context(1_000_000_000_000, 1_000_000);
            let state = create_swap_state();
            // User has much more ERG than needed - should produce change
            let request = create_erg_to_dexy_request(
                1_000_000_000, // swap 1 ERG
                1,
                100_000_000_000, // 100 ERG - lots of change
            );

            let result = build_swap_dexy_tx(&request, &ctx, &state).unwrap();
            let tx = &result.unsigned_tx;

            // Should have 5 outputs: LP, Swap NFT, User, Fee, Change
            assert_eq!(tx.outputs.len(), 5, "Should have change output");

            // Change output should be to user
            let change = &tx.outputs[4];
            assert_eq!(change.ergo_tree, "user_ergo_tree");
            let change_erg: i64 = change.value.parse().unwrap();
            assert!(change_erg >= constants::MIN_BOX_VALUE_NANO);
        }
    }
}
