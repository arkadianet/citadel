//! Lending State Fetching from Node
//!
//! Fetches pool boxes from node and parses into protocol state.

use citadel_core::{ProtocolError, TokenId};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_node_client::{NodeCapabilities, NodeClient};
use ergo_tx::ergo_box_utils::{
    extract_byte_array_coll, extract_int_pair, extract_long, extract_long_coll, find_token_amount,
    map_node_error,
};

use crate::calculator;
use crate::constants::{self, supply, PoolConfig};
use crate::state::{
    BorrowPosition, CollateralOption, LendPosition, MarketsResponse, PoolBoxData, PoolState,
};

/// Fetch all lending markets state
pub async fn fetch_all_markets(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    user_address: Option<&str>,
) -> Result<MarketsResponse, ProtocolError> {
    let pools = constants::get_pools();
    let mut pool_states = Vec::new();

    for config in pools {
        match fetch_pool_state(client, capabilities, config).await {
            Ok(mut state) => {
                // If user address provided, fetch their positions
                if let Some(address) = user_address {
                    if let Ok(position) =
                        fetch_user_lend_position(client, capabilities, config, address, &state)
                            .await
                    {
                        state.user_lend_position = Some(position);
                    }

                    // Fetch borrow positions using runtime borrow token ID from pool box
                    let effective_borrow_token = if !config.borrow_token_id.is_empty() {
                        Some(config.borrow_token_id.to_string())
                    } else {
                        state.borrow_token_id.clone()
                    };
                    if let Some(ref borrow_tid) = effective_borrow_token {
                        match fetch_user_borrow_positions(
                            client,
                            capabilities,
                            config,
                            borrow_tid,
                            address,
                        )
                        .await
                        {
                            Ok(positions) => {
                                state.user_borrow_positions = positions;
                            }
                            Err(e) => {
                                tracing::debug!(
                                    pool_id = %config.id,
                                    error = %e,
                                    "Failed to fetch borrow positions"
                                );
                            }
                        }
                    }
                }
                pool_states.push(state);
            }
            Err(e) => {
                tracing::warn!(pool_id = %config.id, error = %e, "Failed to fetch pool");
                // Continue with other pools
            }
        }
    }

    let block_height = client.current_height().await.unwrap_or(0) as u32;

    Ok(MarketsResponse {
        pools: pool_states,
        block_height,
    })
}

/// Fetch a single pool's state
pub async fn fetch_pool_state(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    config: &PoolConfig,
) -> Result<PoolState, ProtocolError> {
    // Fetch pool box by NFT
    let pool_token_id = TokenId::new(config.pool_nft);
    let pool_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &pool_token_id,
    )
    .await
    .map_err(|e| map_node_error(e, "Lending", &format!("Pool box for {}", config.id)))?;

    // Parse pool box data
    let box_data = parse_pool_box(&pool_box, config)?;

    // Calculate APY from on-chain interest rate
    let (supply_apy, borrow_apy) =
        calculate_apy_from_chain(client, capabilities, &box_data, config).await;

    let mut pool_state = PoolState::from_pool_box(
        config.id,
        config.name,
        config.symbol,
        config.decimals,
        config.is_erg_pool,
        &box_data,
        supply_apy,
        borrow_apy,
    );

    // Fetch collateral options from on-chain parameter box (dynamic, not hardcoded).
    // The Duckpools team can update threshold/penalty on-chain without code changes.
    match fetch_collateral_from_parameter_box(client, capabilities, config).await {
        Ok(options) if !options.is_empty() => {
            pool_state.collateral_options = options;
        }
        Ok(_) => {
            // No collateral options (e.g., ERG pool)
        }
        Err(e) => {
            tracing::warn!(
                pool_id = %config.id,
                error = %e,
                "Failed to fetch parameter box, using hardcoded threshold"
            );
            // Fallback to hardcoded values if parameter box fetch fails
            if config.liquidation_threshold > 0 {
                pool_state.collateral_options = vec![CollateralOption {
                    token_id: "native".to_string(),
                    token_name: "ERG".to_string(),
                    liquidation_threshold: config.liquidation_threshold,
                    liquidation_penalty: 0,
                    dex_nft: config.collateral_dex_nft.map(|s| s.to_string()),
                }];
            }
        }
    }

    Ok(pool_state)
}

/// Fetch collateral options from the on-chain parameter box.
///
/// The parameter box stores dynamic liquidation settings that the Duckpools team
/// can update without redeploying contracts. The pool ErgoScript validates the
/// borrow proxy's threshold/penalty against these values, so they must match exactly.
///
/// **Token pool** parameter box registers:
/// - R4: `Coll[Long]` — liquidation thresholds (index 0 = ERG collateral)
/// - R7: `Coll[Long]` — liquidation penalties (index 0 = ERG collateral)
///
/// **ERG pool** parameter box registers:
/// - R4: `Coll[Long]` — liquidation thresholds (one per collateral token)
/// - R5: `Coll[Coll[Byte]]` — collateral token IDs
/// - R6: `Coll[Coll[Byte]]` — DEX NFT IDs (index key for matching)
/// - R7: `Coll[Long]` — liquidation penalties (one per collateral token)
pub async fn fetch_collateral_from_parameter_box(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    config: &PoolConfig,
) -> Result<Vec<CollateralOption>, ProtocolError> {
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;

    if config.parameter_nft.is_empty() {
        return Ok(vec![]);
    }

    let param_token_id = TokenId::new(config.parameter_nft);
    // Fetch multiple boxes — the first result from token search may be a "bank" box
    // without registers. We need the one with R4 populated.
    let param_boxes = ergo_node_client::queries::get_boxes_by_token_id(
        client.inner(),
        capabilities,
        &param_token_id,
        10,
    )
    .await
    .map_err(|e| {
        map_node_error(
            e,
            "Lending",
            &format!("Parameter box for {}", config.id),
        )
    })?;

    let param_box = param_boxes
        .into_iter()
        .find(|b| {
            b.additional_registers
                .get_constant(NonMandatoryRegisterId::R4)
                .ok()
                .flatten()
                .is_some()
        })
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!(
                "No parameter box with R4 found for {} (NFT: {})",
                config.id, config.parameter_nft
            ),
        })?;

    // Read R4: Coll[Long] — liquidation thresholds
    let r4 = param_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Parameter R4 error for {}: {}", config.id, e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Parameter box missing R4 for {}", config.id),
        })?;
    let thresholds = extract_long_coll(&r4).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Parameter R4 Coll[Long] parse error for {}: {}", config.id, e),
    })?;

    // Read R7: Coll[Long] — liquidation penalties
    let r7 = param_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R7)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Parameter R7 error for {}: {}", config.id, e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Parameter box missing R7 for {}", config.id),
        })?;
    let penalties = extract_long_coll(&r7).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Parameter R7 Coll[Long] parse error for {}: {}", config.id, e),
    })?;

    if thresholds.is_empty() || penalties.is_empty() {
        return Ok(vec![]);
    }

    if config.is_erg_pool {
        // ERG pool: multiple collateral token types, indexed by DEX NFT
        // R5: Coll[Coll[Byte]] — collateral token IDs
        // R6: Coll[Coll[Byte]] — DEX NFT IDs
        let r5 = param_box
            .additional_registers
            .get_constant(NonMandatoryRegisterId::R5)
            .map_err(|e| ProtocolError::BoxParseError {
                message: format!("Parameter R5 error for {}: {}", config.id, e),
            })?
            .ok_or_else(|| ProtocolError::BoxParseError {
                message: format!("Parameter box missing R5 for {}", config.id),
            })?;
        let token_ids = extract_byte_array_coll(&r5).map_err(|e| {
            ProtocolError::BoxParseError {
                message: format!("Parameter R5 Coll[Coll[Byte]] parse error for {}: {}", config.id, e),
            }
        })?;

        let r6 = param_box
            .additional_registers
            .get_constant(NonMandatoryRegisterId::R6)
            .map_err(|e| ProtocolError::BoxParseError {
                message: format!("Parameter R6 error for {}: {}", config.id, e),
            })?
            .ok_or_else(|| ProtocolError::BoxParseError {
                message: format!("Parameter box missing R6 for {}", config.id),
            })?;
        let dex_nfts = extract_byte_array_coll(&r6).map_err(|e| {
            ProtocolError::BoxParseError {
                message: format!("Parameter R6 Coll[Coll[Byte]] parse error for {}: {}", config.id, e),
            }
        })?;

        let count = thresholds.len().min(penalties.len()).min(token_ids.len()).min(dex_nfts.len());
        let mut options = Vec::with_capacity(count);
        for i in 0..count {
            let tid_hex = hex::encode(&token_ids[i]);
            let dex_hex = hex::encode(&dex_nfts[i]);
            let token_name = known_token_name(&tid_hex);
            options.push(CollateralOption {
                token_id: tid_hex,
                token_name,
                liquidation_threshold: thresholds[i] as u64,
                liquidation_penalty: penalties[i] as u64,
                dex_nft: Some(dex_hex),
            });
        }
        Ok(options)
    } else {
        // Token pool: single collateral type = ERG (index 0)
        Ok(vec![CollateralOption {
            token_id: "native".to_string(),
            token_name: "ERG".to_string(),
            liquidation_threshold: thresholds[0] as u64,
            liquidation_penalty: penalties[0] as u64,
            dex_nft: config.collateral_dex_nft.map(|s| s.to_string()),
        }])
    }
}

/// Map well-known Ergo token IDs to human-readable names.
fn known_token_name(token_id: &str) -> String {
    match token_id {
        "03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04" => "SigUSD".to_string(),
        "003bd19d0187117f130b62e1bcab0939929ff5c7709f843c5c4dd158949285d0" => "SigRSV".to_string(),
        "8b08cdd5449a9592a9e79711d7d79249d7a03c535d17efaee83e216e80a44c4b" => "RSN".to_string(),
        "e023c5f382b6e96fbd878f6811aac73345489032157ad5affb84aefd4956c297" => "rsADA".to_string(),
        "9a06d9e545a41fd51eeffc5e20d818073bf820c635e2a9d922269913e0de369d" => "SPF".to_string(),
        "7a51950e5f548549ec1aa63ffdc38279505b11e7e803d01bcf8347e0123c88b0" => "rsBTC".to_string(),
        "089990451bb430f05a85f4ef3bcb6ebf852b3d6ee68d86d78658b9ccef20074f" => "QUACKS".to_string(),
        _ => "Unknown".to_string(),
    }
}

/// Parse pool box into PoolBoxData
fn parse_pool_box(ergo_box: &ErgoBox, config: &PoolConfig) -> Result<PoolBoxData, ProtocolError> {
    let box_id = ergo_box.box_id().to_string();
    let value_nano = ergo_box.value.as_i64();

    // Get LP tokens remaining in pool (to calculate circulating supply)
    let lp_tokens_remaining = find_token_amount(ergo_box, config.lend_token_id).unwrap_or(0);
    let max_lp = supply::max_lend_tokens(config.is_erg_pool);
    let lp_tokens_in_circulation = max_lp.saturating_sub(lp_tokens_remaining);

    // Get borrow tokens remaining from pool box
    // Pool box structure: tokens[0]=Pool NFT, tokens[1]=Lend tokens, tokens[2]=Borrow tokens
    // If borrow token doesn't exist at index 2, assume no borrowing (0 in circulation)
    let borrow_token_at_2 = ergo_box
        .tokens
        .as_ref()
        .and_then(|tokens| tokens.get(2));
    let borrow_tokens_in_circulation = match borrow_token_at_2 {
        Some(tok) => supply::MAX_BORROW_TOKENS.saturating_sub(*tok.amount.as_u64()),
        None => 0,
    };
    // Extract borrow token ID from pool box tokens[2] for collateral box discovery
    let borrow_token_id = borrow_token_at_2.map(|tok| {
        let tid: String = tok.token_id.into();
        tid
    });

    // Get currency amount for token pools
    let currency_amount = if let Some(currency_id) = config.currency_id {
        find_token_amount(ergo_box, currency_id).unwrap_or(0)
    } else {
        0
    };

    Ok(PoolBoxData {
        box_id,
        value_nano,
        lp_tokens_in_circulation,
        borrow_tokens_in_circulation,
        currency_amount,
        borrow_token_id,
    })
}

/// Calculate APY from on-chain interest rate in child box.
///
/// Fetches the head child box (highest R6), reads R4 (Coll[Long]) to get the
/// current per-period interest rate, then compounds over 2190 periods/year.
/// Falls back to (0.0, 0.0) if child box fetch fails.
async fn calculate_apy_from_chain(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    box_data: &PoolBoxData,
    config: &PoolConfig,
) -> (f64, f64) {
    // Calculate utilization with corrected total_supplied
    let remaining = if config.is_erg_pool {
        box_data.value_nano as u64
    } else {
        box_data.currency_amount
    };
    let total_borrowed = box_data.borrow_tokens_in_circulation;
    let total_supplied = remaining.saturating_add(total_borrowed);
    let utilization = calculator::calculate_utilization(total_borrowed, total_supplied);
    let u = utilization / 100.0;

    if u <= 0.0 {
        return (0.0, 0.0);
    }

    // Fetch on-chain interest rate from child box
    match fetch_current_interest_rate(client, capabilities, config).await {
        Ok(rate) => calculate_real_apy(rate, u),
        Err(e) => {
            tracing::warn!(
                pool_id = %config.id,
                error = %e,
                "Failed to fetch on-chain interest rate, returning 0 APY"
            );
            (0.0, 0.0)
        }
    }
}

/// Fetch the current per-period interest rate from the head child box.
///
/// Child boxes contain the interest rate in R4 (Coll[Long]).
/// The head child (highest R6 value) has the most recent rate.
/// The last element of R4 is the current per-period rate.
async fn fetch_current_interest_rate(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    config: &PoolConfig,
) -> Result<u64, ProtocolError> {
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;

    let child_token_id = TokenId::new(config.child_nft);
    let child_boxes = ergo_node_client::queries::get_boxes_by_token_id(
        client.inner(),
        capabilities,
        &child_token_id,
        10,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to fetch child boxes for {}: {}", config.id, e),
    })?;

    if child_boxes.is_empty() {
        return Err(ProtocolError::BoxParseError {
            message: format!("No child boxes found for pool {}", config.id),
        });
    }

    // Find head child: the one with the highest R6 (Long) value
    let mut head_child = &child_boxes[0];
    let mut highest_r6: i64 = i64::MIN;

    for child_box in &child_boxes {
        if let Ok(Some(r6_const)) = child_box
            .additional_registers
            .get_constant(NonMandatoryRegisterId::R6)
        {
            if let Ok(val) = extract_long(&r6_const) {
                if val > highest_r6 {
                    highest_r6 = val;
                    head_child = child_box;
                }
            }
        }
    }

    // Parse R4 (Coll[Long]) from head child — last element is current rate
    let r4 = head_child
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Child box R4 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: format!("Child box missing R4 for pool {}", config.id),
        })?;

    let rates = extract_long_coll(&r4).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse R4 Coll[Long]: {}", e),
    })?;

    if rates.is_empty() {
        return Err(ProtocolError::BoxParseError {
            message: format!("R4 Coll[Long] is empty for pool {}", config.id),
        });
    }

    // Last element is the current per-period rate
    let current_rate = *rates.last().unwrap();
    if current_rate < 0 {
        return Err(ProtocolError::BoxParseError {
            message: format!(
                "Negative interest rate {} for pool {}",
                current_rate, config.id
            ),
        });
    }

    Ok(current_rate as u64)
}

/// Calculate real APY from on-chain per-period interest rate.
///
/// - `per_period_rate`: raw rate from R4, e.g. 100_002_400 meaning 1.000024x per period
/// - `utilization_fraction`: 0.0 to 1.0
///
/// Returns (supply_apy, borrow_apy) as decimals (e.g., 0.05 for 5%).
/// The frontend multiplies by 100 for display.
fn calculate_real_apy(per_period_rate: u64, utilization_fraction: f64) -> (f64, f64) {
    use crate::constants::interest;

    let multiplier = interest::INTEREST_MULTIPLIER as f64;
    let periods_per_year =
        interest::BLOCKS_PER_YEAR as f64 / interest::UPDATE_FREQUENCY_BLOCKS as f64;

    // per_period is e.g. 1.000024
    let per_period = per_period_rate as f64 / multiplier;

    // Compound over all periods in a year
    let borrow_apy = per_period.powf(periods_per_year) - 1.0;

    // Supply APY = utilization * borrow APY (lenders earn on the utilized portion)
    let supply_apy = utilization_fraction * borrow_apy;

    // Return as decimals — frontend formatApy() multiplies by 100 for display
    (supply_apy, borrow_apy)
}

/// Fetch user's lending position in a pool
async fn fetch_user_lend_position(
    client: &NodeClient,
    _capabilities: &NodeCapabilities,
    config: &PoolConfig,
    address: &str,
    pool_state: &PoolState,
) -> Result<LendPosition, ProtocolError> {
    // Get user's balances
    let (_, tokens) = client.get_address_balances(address).await.map_err(|e| {
        ProtocolError::StateUnavailable {
            reason: format!("Failed to get user balances: {}", e),
        }
    })?;

    // Find LP tokens for this pool
    let lp_tokens = tokens
        .iter()
        .find(|(id, _)| id == config.lend_token_id)
        .map(|(_, amount)| *amount)
        .unwrap_or(0);

    if lp_tokens == 0 {
        return Err(ProtocolError::StateUnavailable {
            reason: "User has no LP tokens".to_string(),
        });
    }

    // Calculate underlying value using actual LP tokens in circulation
    let underlying_value = calculator::calculate_underlying_for_lp(
        lp_tokens,
        pool_state.total_supplied,
        pool_state.lp_tokens_in_circulation,
    );

    // Profit is value - original deposit (would need historical data for accurate calculation)
    let unrealized_profit = 0i64; // Simplified for MVP

    Ok(LendPosition {
        lp_tokens,
        underlying_value,
        unrealized_profit,
    })
}

/// Fetch ERG price in USD from the SigmaUSD oracle
/// This can be used for collateral valuation
pub async fn fetch_erg_price_usd(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
) -> Result<f64, ProtocolError> {
    // SigmaUSD oracle NFT on mainnet
    const ORACLE_POOL_NFT: &str =
        "011d3364de07e5a26f0c4eef0852cddb387039a921b7154ef3cab22c6eda887f";

    let oracle_token_id = TokenId::new(ORACLE_POOL_NFT);
    let oracle_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &oracle_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("Oracle box not found: {}", e),
    })?;

    // Extract R4 (nanoerg per USD)
    let r4 = oracle_box
        .additional_registers
        .get_constant(ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId::R4)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Oracle R4 error: {}", e),
        })?
        .ok_or_else(|| ProtocolError::BoxParseError {
            message: "Oracle missing R4".to_string(),
        })?;

    let nanoerg_per_usd = extract_long(&r4).map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to parse oracle rate: {}", e),
    })?;

    // Convert to USD per ERG
    if nanoerg_per_usd > 0 {
        Ok(1_000_000_000.0 / nanoerg_per_usd as f64)
    } else {
        Ok(0.0)
    }
}

/// Pre-fetched interest data for a pool (parent + child boxes).
/// Fetched once per pool and reused for all borrow positions.
struct InterestData {
    parent_rates: Vec<i64>,
    head_child_rates: Vec<i64>,
    /// Map of R6 -> R4 rates for any non-head children found on-chain
    other_child_rates: Vec<(i64, Vec<i64>)>,
}

/// Fetch interest data (parent + child boxes) for a pool.
/// Returns None if parent/child boxes are missing or unparseable.
async fn fetch_interest_data(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    config: &PoolConfig,
) -> Option<InterestData> {
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;
    use ergo_lib::ergotree_ir::mir::constant::Literal;

    if config.parent_nft.is_empty() || config.child_nft.is_empty() {
        return None;
    }

    // Fetch parent box
    let parent_token_id = TokenId::new(config.parent_nft);
    let parent_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &parent_token_id,
    )
    .await
    .ok()?;

    let parent_r4 = parent_box
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()??;
    let parent_rates = extract_long_coll(&parent_r4).ok()?;

    // Fetch child boxes
    let child_token_id = TokenId::new(config.child_nft);
    let child_boxes = ergo_node_client::queries::get_boxes_by_token_id(
        client.inner(),
        capabilities,
        &child_token_id,
        20,
    )
    .await
    .ok()?;

    if child_boxes.is_empty() {
        return None;
    }

    // Parse R6 (child index) for each child — may be Int or Long
    let mut children_with_r6: Vec<(usize, i64)> = Vec::new();
    for (i, cb) in child_boxes.iter().enumerate() {
        if let Ok(Some(r6_const)) = cb
            .additional_registers
            .get_constant(NonMandatoryRegisterId::R6)
        {
            let r6_val = match &r6_const.v {
                Literal::Long(v) => Some(*v),
                Literal::Int(v) => Some(*v as i64),
                _ => None,
            };
            if let Some(val) = r6_val {
                children_with_r6.push((i, val));
            }
        }
    }

    if children_with_r6.is_empty() {
        return None;
    }

    // Find head child (highest R6)
    children_with_r6.sort_by_key(|(_, r6)| *r6);
    let &(head_idx, head_r6) = children_with_r6.last()?;

    let head_r4 = child_boxes[head_idx]
        .additional_registers
        .get_constant(NonMandatoryRegisterId::R4)
        .ok()??;
    let head_child_rates = extract_long_coll(&head_r4).ok()?;

    // Collect non-head children (for case where base child is still on-chain)
    let mut other_child_rates = Vec::new();
    for &(idx, r6) in &children_with_r6 {
        if r6 != head_r6 {
            if let Ok(Some(r4)) = child_boxes[idx]
                .additional_registers
                .get_constant(NonMandatoryRegisterId::R4)
            {
                if let Ok(rates) = extract_long_coll(&r4) {
                    other_child_rates.push((r6, rates));
                }
            }
        }
    }

    Some(InterestData {
        parent_rates,
        head_child_rates,
        other_child_rates,
    })
}

/// Calculate the minimum repayment amount for a borrow position using the parent-child
/// interest box chain.
///
/// Duckpools interest model:
/// - Each child box R4 = `Coll[Long]` of per-period rates (e.g. 100_002_400 = 1.000024x)
/// - Parent box R4 = `Coll[Long]` of per-epoch compound rates (one per completed child epoch)
/// - Collateral box R5 = `(parent_idx, child_idx)` — bookmarks when the loan was created
///
/// Three cases based on `live_parent_index` (= `parent.R4.len()`) vs `parent_idx`:
/// 1. Equal: base child == head child, compound from `child_idx`
/// 2. +1: base child + head child rates
/// 3. >1: base child + intermediate parent rates + head child rates
///
/// The on-chain collateral contract computes:
///   `contract_total_owed = 1 + floor(loanAmount * compoundedInterest / InterestRateDenom)`
/// and validates the repayment with **strict greater-than**:
///   `repaymentLoanTokens > contract_total_owed`
///
/// Therefore the minimum repayment amount = `contract_total_owed + 1`:
///   `2 + floor(principal * compoundedInterest / INTEREST_MULTIPLIER)`
fn calculate_interest_compound(
    interest: &InterestData,
    parent_idx: i32,
    child_idx: i32,
    borrowed_amount: u64,
) -> u64 {
    let im = constants::interest::INTEREST_MULTIPLIER as u128;
    let live_parent_index = interest.parent_rates.len() as i32;

    let mut compound: u128 = im;

    if live_parent_index == parent_idx {
        // Case 1: Parent hasn't grown since loan creation.
        // Base child == head child. Compound head_child.R4[child_idx:]
        let start = child_idx as usize;
        for &rate in interest.head_child_rates.get(start..).unwrap_or(&[]) {
            compound = compound * rate as u128 / im;
        }
    } else {
        // Parent has grown — try to find base child on-chain
        let base_child_rates = interest
            .other_child_rates
            .iter()
            .find(|(r6, _)| *r6 == parent_idx as i64)
            .map(|(_, rates)| rates.as_slice());

        if let Some(base_rates) = base_child_rates {
            // Base child still on-chain — use exact rates
            let start = child_idx as usize;
            for &rate in base_rates.get(start..).unwrap_or(&[]) {
                compound = compound * rate as u128 / im;
            }
        } else {
            // Base child consumed — approximate with full parent epoch rate.
            // This slightly overestimates (includes rates 0..child_idx we should skip).
            if (parent_idx as usize) < interest.parent_rates.len() {
                compound = interest.parent_rates[parent_idx as usize] as u128;
            }
        }

        // Compound head child rates (current epoch, all rates)
        for &rate in &interest.head_child_rates {
            compound = compound * rate as u128 / im;
        }

        // Compound intermediate parent epoch rates (between base and head)
        if live_parent_index > parent_idx + 1 {
            let p_start = (parent_idx + 1) as usize;
            let p_end = live_parent_index as usize;
            for &rate in interest.parent_rates.get(p_start..p_end).unwrap_or(&[]) {
                compound = compound * rate as u128 / im;
            }
        }
    }

    // Contract formula: contract_total_owed = 1 + floor(principal * compound / IM)
    // Repayment check: repayment > contract_total_owed (strict >)
    // Minimum repayment = contract_total_owed + 1 = 2 + floor(principal * compound / IM)
    let min_repayment = 2u128 + (borrowed_amount as u128 * compound / im);
    min_repayment as u64
}

/// Fetch user's borrow positions (collateral boxes) for a pool.
///
/// Collateral box structure:
/// - **Token pool** (e.g. SigUSD): collateral = ERG (box value), tokens contain borrow receipt
/// - **ERG pool**: collateral = token (non-borrow token), box value = min ERG
///
/// We search by `borrow_token_id`, then filter for boxes whose R4 matches the user's ErgoTree.
/// Interest accrual is calculated from the parent-child interest box chain.
pub async fn fetch_user_borrow_positions(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    config: &PoolConfig,
    borrow_token_id_str: &str,
    user_address: &str,
) -> Result<Vec<BorrowPosition>, ProtocolError> {
    use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;
    use ergo_lib::ergotree_ir::mir::value::{CollKind, NativeColl};
    use ergo_lib::ergotree_ir::mir::constant::Literal;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    if borrow_token_id_str.is_empty() {
        return Ok(vec![]);
    }

    // Get user's ErgoTree bytes for comparison
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    let user_addr = encoder
        .parse_address_from_str(user_address)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Invalid user address: {}", e),
        })?;
    let user_tree = user_addr.script().map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get ErgoTree: {}", e),
    })?;
    let user_tree_bytes = user_tree
        .sigma_serialize_bytes()
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Failed to serialize ErgoTree: {}", e),
        })?;

    // Fetch DEX prices for health factor calculation.
    // Token pools: single DEX price from config.collateral_dex_nft (ERG/token pair).
    // ERG pool: per-collateral DEX prices from CollateralOption.dex_nft.
    let dex_price = if let Some(dex_nft) = config.collateral_dex_nft {
        fetch_dex_price(client, capabilities, dex_nft).await.ok()
    } else {
        None
    };

    // For ERG pool: pre-fetch DEX prices for each collateral token
    let collateral_dex_prices: std::collections::HashMap<String, (f64, f64)> =
        if config.is_erg_pool {
            let collateral_options =
                fetch_collateral_from_parameter_box(client, capabilities, config)
                    .await
                    .unwrap_or_default();
            let mut prices = std::collections::HashMap::new();
            for opt in &collateral_options {
                if let Some(ref dex_nft) = opt.dex_nft {
                    if let Ok(price) = fetch_dex_price(client, capabilities, dex_nft).await {
                        prices.insert(opt.token_id.clone(), price);
                    }
                }
            }
            prices
        } else {
            std::collections::HashMap::new()
        };

    // Pre-fetch interest data (parent + child boxes) once for all positions
    let interest_data = fetch_interest_data(client, capabilities, config).await;

    // Search for boxes containing the borrow token
    let borrow_token_id = TokenId::new(borrow_token_id_str);
    let boxes = ergo_node_client::queries::get_boxes_by_token_id(
        client.inner(),
        capabilities,
        &borrow_token_id,
        100,
    )
    .await
    .map_err(|e| map_node_error(e, "Lending", &format!("Borrow positions for {}", config.id)))?;

    let mut positions = Vec::new();

    for ergo_box in &boxes {
        // Read R4: Coll[Byte] containing the borrower's ErgoTree
        let r4_bytes = match ergo_box
            .additional_registers
            .get_constant(NonMandatoryRegisterId::R4)
        {
            Ok(Some(constant)) => match &constant.v {
                Literal::Coll(CollKind::NativeColl(NativeColl::CollByte(bytes))) => {
                    let u8_bytes: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
                    Some(u8_bytes)
                }
                _ => None,
            },
            _ => None,
        };

        let r4_bytes = match r4_bytes {
            Some(b) => b,
            None => continue,
        };

        if r4_bytes != user_tree_bytes {
            continue;
        }

        let box_id = ergo_box.box_id().to_string();

        // Extract borrowed amount from the borrow receipt token
        let borrowed_amount = find_token_amount(ergo_box, borrow_token_id_str)
            .unwrap_or(0);

        // Calculate total_owed with interest accrual from R5 bookmarks
        let total_owed = if let Some(ref interest) = interest_data {
            // Read R5: (Int, Int) = (parent_idx, child_idx) interest bookmarks
            let r5_pair = ergo_box
                .additional_registers
                .get_constant(NonMandatoryRegisterId::R5)
                .ok()
                .flatten()
                .and_then(|c| extract_int_pair(&c).ok());

            if let Some((parent_idx, child_idx)) = r5_pair {
                calculate_interest_compound(interest, parent_idx, child_idx, borrowed_amount)
            } else {
                borrowed_amount
            }
        } else {
            borrowed_amount
        };

        // Extract collateral based on pool type:
        // Token pools: collateral = ERG (box value). The only tokens are borrow receipts.
        // ERG pool: collateral = non-borrow token in the box.
        let (collateral_token, collateral_name, collateral_amount) = if !config.is_erg_pool {
            // Token pool: collateral is ERG
            ("native".to_string(), "ERG".to_string(), *ergo_box.value.as_u64())
        } else {
            // ERG pool: collateral is the non-borrow token
            let mut found = None;
            if let Some(tokens) = ergo_box.tokens.as_ref() {
                for tok in tokens.iter() {
                    let tid: String = tok.token_id.into();
                    if tid != borrow_token_id_str {
                        let name = known_token_name(&tid);
                        found = Some((tid, name, *tok.amount.as_u64()));
                        break;
                    }
                }
            }
            found.unwrap_or(("unknown".to_string(), "Unknown".to_string(), 0))
        };

        // Calculate health factor from DEX price using total_owed (with interest)
        // health_factor = collateral_value / total_owed_value
        // For token pool: collateral is ERG, total_owed is in token raw units
        //   health = (collateral_nanoerg) / (total_owed_raw * erg_per_token)
        // For ERG pool: collateral is token, total_owed is in nanoERG
        //   health = (collateral_raw * erg_per_token) / (total_owed_nanoerg)
        let effective_dex_price = if config.is_erg_pool {
            // ERG pool: look up per-collateral DEX price
            collateral_dex_prices.get(&collateral_token).copied()
        } else {
            dex_price
        };

        let health_factor = if let Some((erg_per_token, _token_per_erg)) = effective_dex_price {
            if !config.is_erg_pool && erg_per_token > 0.0 && total_owed > 0 {
                // Token pool: collateral in nanoERG, owed in token raw units
                let owed_value_nano = total_owed as f64 * erg_per_token;
                if owed_value_nano > 0.0 {
                    collateral_amount as f64 / owed_value_nano
                } else {
                    0.0
                }
            } else if config.is_erg_pool && erg_per_token > 0.0 && total_owed > 0 {
                // ERG pool: collateral in token raw units, owed in nanoERG
                let collateral_value_nano = collateral_amount as f64 * erg_per_token;
                collateral_value_nano / total_owed as f64
            } else {
                0.0
            }
        } else {
            0.0
        };

        let at_risk = health_factor > 0.0 && health_factor < constants::health::WARNING_THRESHOLD;

        positions.push(BorrowPosition {
            collateral_box_id: box_id,
            collateral_token,
            collateral_name,
            collateral_amount,
            borrowed_amount,
            total_owed,
            health_factor,
            liquidation_threshold: config.liquidation_threshold as u16,
            at_risk,
        });
    }

    Ok(positions)
}

/// Fetch DEX pool price ratios (nanoERG per raw token unit).
/// Returns (erg_per_token, token_per_erg) in raw units.
async fn fetch_dex_price(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    dex_nft: &str,
) -> Result<(f64, f64), ProtocolError> {
    let dex_token_id = TokenId::new(dex_nft);
    let dex_box = ergo_node_client::queries::get_box_by_token_id(
        client.inner(),
        capabilities,
        &dex_token_id,
    )
    .await
    .map_err(|e| ProtocolError::BoxParseError {
        message: format!("DEX box not found for NFT {}: {}", dex_nft, e),
    })?;

    let erg_reserves = dex_box.value.as_i64() as f64;
    let tokens = dex_box.tokens.as_ref().ok_or_else(|| ProtocolError::BoxParseError {
        message: "DEX box has no tokens".to_string(),
    })?;
    if tokens.len() < 3 {
        return Err(ProtocolError::BoxParseError {
            message: "DEX box has fewer than 3 tokens".to_string(),
        });
    }
    let token_reserves = *tokens.as_slice()[2].amount.as_u64() as f64;

    if erg_reserves <= 0.0 || token_reserves <= 0.0 {
        return Err(ProtocolError::BoxParseError {
            message: "DEX pool has zero reserves".to_string(),
        });
    }

    Ok((erg_reserves / token_reserves, token_reserves / erg_reserves))
}

/// Discover stuck proxy boxes belonging to the user across all Duckpools proxy contracts.
///
/// Scans all unique proxy addresses (~16) for unspent boxes where R4 (Coll[Byte])
/// matches the user's serialized ErgoTree. R6 is parsed as the refund height.
pub async fn discover_stuck_proxy_boxes(
    client: &NodeClient,
    user_address: &str,
    current_height: u32,
) -> Result<Vec<crate::state::StuckProxyBox>, ProtocolError> {
    use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;
    use ergo_lib::ergotree_ir::mir::constant::Literal;
    use ergo_lib::ergotree_ir::mir::value::{CollKind, NativeColl};
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    // Serialize user's ErgoTree for R4 comparison
    let encoder = AddressEncoder::new(NetworkPrefix::Mainnet);
    let user_addr = encoder
        .parse_address_from_str(user_address)
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Invalid user address: {}", e),
        })?;
    let user_tree = user_addr.script().map_err(|e| ProtocolError::BoxParseError {
        message: format!("Failed to get ErgoTree: {}", e),
    })?;
    let user_tree_bytes = user_tree
        .sigma_serialize_bytes()
        .map_err(|e| ProtocolError::BoxParseError {
            message: format!("Failed to serialize ErgoTree: {}", e),
        })?;

    let proxy_addresses = constants::unique_proxy_addresses();
    let mut stuck_boxes = Vec::new();

    for addr_info in &proxy_addresses {
        let addr_string = addr_info.address.to_string();
        let boxes = match client
            .inner()
            .unspent_boxes_by_address(&addr_string, 0, 500)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                tracing::debug!(
                    address = %&addr_info.address[..20],
                    error = %e,
                    "Failed to query proxy address, skipping"
                );
                continue;
            }
        };

        for ergo_box in &boxes {
            // Extract Coll[Byte] from a register, if present
            let extract_coll_byte = |reg: NonMandatoryRegisterId| -> Option<Vec<u8>> {
                match ergo_box.additional_registers.get_constant(reg) {
                    Ok(Some(constant)) => match &constant.v {
                        Literal::Coll(CollKind::NativeColl(NativeColl::CollByte(bytes))) => {
                            Some(bytes.iter().map(|&b| b as u8).collect())
                        }
                        _ => None,
                    },
                    _ => None,
                }
            };

            // Different proxy types store user ErgoTree in different registers:
            // - Lend/Withdraw/Borrow: R4 = Coll[Byte] (user ErgoTree)
            // - Repay/PartialRepay:   R5 = Coll[Byte] (user ErgoTree), R4 = Long
            let is_repay = matches!(
                addr_info.operation,
                constants::ProxyOperationType::Repay
                    | constants::ProxyOperationType::PartialRepay
            );

            let user_tree_register = if is_repay {
                NonMandatoryRegisterId::R5
            } else {
                NonMandatoryRegisterId::R4
            };

            let tree_bytes = match extract_coll_byte(user_tree_register) {
                Some(b) => b,
                None => continue,
            };

            if tree_bytes != user_tree_bytes {
                continue;
            }

            // Parse R6: Int or Long (refund height)
            let refund_height: i64 = match ergo_box
                .additional_registers
                .get_constant(NonMandatoryRegisterId::R6)
            {
                Ok(Some(constant)) => match &constant.v {
                    Literal::Long(v) => *v,
                    Literal::Int(v) => *v as i64,
                    _ => 0,
                },
                _ => 0,
            };

            // Extract tokens
            let tokens: Vec<crate::state::StuckBoxToken> = ergo_box
                .tokens
                .as_ref()
                .map(|toks| {
                    toks.iter()
                        .map(|t| {
                            let tid: String = t.token_id.into();
                            crate::state::StuckBoxToken {
                                token_id: tid,
                                amount: *t.amount.as_u64(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            let blocks_remaining = (refund_height - current_height as i64).max(0);

            // can_refund is always true: the proxy contract has `proveDlog(userPk)`
            // as an OR spending path, so the user can reclaim funds at any time
            // by signing with their wallet — no height check needed.
            stuck_boxes.push(crate::state::StuckProxyBox {
                box_id: ergo_box.box_id().to_string(),
                operation: addr_info.operation.label().to_string(),
                value_nano: ergo_box.value.as_i64(),
                refund_height,
                current_height,
                can_refund: true,
                blocks_remaining,
                tokens,
            });
        }
    }

    // Sort by refund_height (soonest first)
    stuck_boxes.sort_by_key(|b| b.refund_height);

    Ok(stuck_boxes)
}

/// Error types for fetch operations
#[derive(Debug, Clone)]
pub enum FetchError {
    NodeError(String),
    ParseError(String),
    NotFound(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodeError(msg) => write!(f, "Node error: {}", msg),
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
        }
    }
}

impl std::error::Error for FetchError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_real_apy_zero_utilization() {
        // With 0 utilization, supply APY should be 0 (lenders earn nothing)
        // Borrow APY is still calculated (rate exists on-chain regardless)
        let (supply_apy, borrow_apy) = calculate_real_apy(100_002_400, 0.0);
        assert_eq!(supply_apy, 0.0);
        assert!(borrow_apy > 0.0); // Borrow rate exists even at 0 utilization
    }

    #[test]
    fn test_calculate_real_apy_typical_rate() {
        // Rate of 100_002_400 means 1.000024 per period
        // Compounded over 2190 periods: 1.000024^2190 ≈ 1.054 → ~0.054 borrow APY
        // Returns as decimal (frontend multiplies by 100 for display)
        let rate = 100_002_400_u64;
        let utilization = 0.5;
        let (supply_apy, borrow_apy) = calculate_real_apy(rate, utilization);

        // Borrow APY should be roughly 0.04-0.08 (4-8% as decimal)
        assert!(
            borrow_apy > 0.04,
            "borrow_apy {} should be > 0.04",
            borrow_apy
        );
        assert!(
            borrow_apy < 0.08,
            "borrow_apy {} should be < 0.08",
            borrow_apy
        );

        // Supply APY = utilization * borrow APY
        assert!(supply_apy > 0.0);
        assert!(supply_apy < borrow_apy);
        let expected_supply = utilization * borrow_apy;
        assert!((supply_apy - expected_supply).abs() < 0.0001);
    }

    #[test]
    fn test_calculate_real_apy_base_rate() {
        // Rate exactly at INTEREST_MULTIPLIER means 1.0 per period = 0 APY
        let rate = 100_000_000_u64;
        let (supply_apy, borrow_apy) = calculate_real_apy(rate, 0.5);
        assert!(borrow_apy.abs() < 0.00001, "0% rate should give ~0 APY");
        assert!(supply_apy.abs() < 0.00001);
    }

    #[test]
    fn test_fetch_error_display() {
        let err = FetchError::NodeError("connection failed".to_string());
        assert_eq!(err.to_string(), "Node error: connection failed");

        let err = FetchError::ParseError("invalid format".to_string());
        assert_eq!(err.to_string(), "Parse error: invalid format");

        let err = FetchError::NotFound("pool not found".to_string());
        assert_eq!(err.to_string(), "Not found: pool not found");
    }

    // =========================================================================
    // Interest compound calculation tests
    // =========================================================================

    fn make_interest_data(
        parent_rates: Vec<i64>,
        head_child_rates: Vec<i64>,
        _head_child_r6: i64,
        other_children: Vec<(i64, Vec<i64>)>,
    ) -> InterestData {
        InterestData {
            parent_rates,
            head_child_rates,
            other_child_rates: other_children,
        }
    }

    #[test]
    fn test_interest_compound_case1_no_parent_growth() {
        // Case 1: live_parent_index == parent_idx (parent hasn't grown)
        // Parent has 3 entries, head child R6=3, loan at parent_idx=3, child_idx=2
        // Head child rates: [100_010_000, 100_010_000, 100_010_000, 100_010_000, 100_010_000]
        // We compound from index 2 onward: 3 rates of 1.0001x
        let im = 100_000_000i64;
        let rate = im + 10_000; // 100_010_000 = 1.0001 per period
        let interest = make_interest_data(
            vec![im; 3], // parent has 3 entries
            vec![rate; 5],
            3,
            vec![],
        );

        let borrowed = 10000u64; // 100.00 SigUSD in cents
        let total = calculate_interest_compound(&interest, 3, 2, borrowed);

        // Compound 3 rates of 1.0001: 1.0001^3 ≈ 1.0003
        // min_repayment = 2 + floor(10000 * compound / IM)
        // compound = IM * 1.0001^3 ≈ 100_030_003
        // total = 2 + floor(10000 * 100030003 / 100000000) = 2 + 10003 = 10005
        assert!(total > borrowed, "total_owed {} should exceed borrowed {}", total, borrowed);
        assert!(total < borrowed + 100, "interest should be small for 3 periods");
    }

    #[test]
    fn test_interest_compound_case1_from_start() {
        // Loan created at very start of child (child_idx=0)
        let im = 100_000_000i64;
        let rate = im + 100_000; // 1.001 per period (0.1%)
        let interest = make_interest_data(
            vec![im; 5],
            vec![rate; 10], // 10 periods of 0.1%
            5,
            vec![],
        );

        let borrowed = 100_000u64;
        let total = calculate_interest_compound(&interest, 5, 0, borrowed);

        // 1.001^10 ≈ 1.01005 → min_repayment = 2 + floor(100000 * 1.01005) ≈ 101006
        assert!(total > 101_000, "total {} should be > 101000", total);
        assert!(total < 101_100, "total {} should be < 101100", total);
    }

    #[test]
    fn test_interest_compound_case2_one_parent_growth_base_available() {
        // Case 2: live_parent_index == parent_idx + 1, base child on-chain
        let im = 100_000_000i64;
        let rate = im + 50_000; // 1.0005 per period
        let base_child_rates = vec![rate; 20]; // base child had 20 periods
        let head_child_rates = vec![rate; 5];  // head child has 5 periods

        let interest = make_interest_data(
            vec![im; 4], // parent has 4 entries → live_parent_index = 4
            head_child_rates,
            4,
            vec![(3, base_child_rates)], // base child R6=3
        );

        let borrowed = 100_000u64;
        // Loan at parent_idx=3, child_idx=15 → 5 rates from base + 5 from head = 10 total
        let total = calculate_interest_compound(&interest, 3, 15, borrowed);

        // 1.0005^10 ≈ 1.005012 → min_repayment ≈ 100503
        assert!(total > 100_400, "total {} should be > 100400", total);
        assert!(total < 100_700, "total {} should be < 100700", total);
    }

    #[test]
    fn test_interest_compound_case3_base_consumed_approximation() {
        // Case 3: live_parent_index > parent_idx + 1, base child gone
        let im = 100_000_000i64;
        let rate = im + 50_000; // 1.0005 per period
        let epoch_compound = 100_500_000i64; // ~1.005 for a full epoch

        let interest = make_interest_data(
            vec![
                epoch_compound, // epoch 0
                epoch_compound, // epoch 1
                epoch_compound, // epoch 2 — loan started here
                epoch_compound, // epoch 3
                epoch_compound, // epoch 4
            ],
            vec![rate; 3], // head child (epoch 5) has 3 periods
            5,
            vec![], // no base child on-chain
        );

        let borrowed = 100_000u64;
        // Loan at parent_idx=2, child_idx=10
        // Approximate: parent[2] (full epoch) + head child (3 rates) + parent[3..5] (2 epochs)
        let total = calculate_interest_compound(&interest, 2, 10, borrowed);

        // Should include some interest
        assert!(total > borrowed, "total {} should exceed borrowed {}", total, borrowed);
    }

    #[test]
    fn test_interest_compound_no_rates() {
        // Edge case: no rates in head child (fresh epoch)
        let im = 100_000_000i64;
        let interest = make_interest_data(vec![im; 3], vec![], 3, vec![]);

        let borrowed = 10000u64;
        let total = calculate_interest_compound(&interest, 3, 0, borrowed);
        // No rates to compound → min repayment = 2 + borrowed (strict > check)
        assert_eq!(total, borrowed + 2);
    }

    #[test]
    fn test_interest_compound_zero_borrowed() {
        let im = 100_000_000i64;
        let rate = im + 100_000;
        let interest = make_interest_data(vec![im; 3], vec![rate; 10], 3, vec![]);

        let total = calculate_interest_compound(&interest, 3, 0, 0);
        // 0 borrowed → min repayment = 2 + 0 = 2 (strict > check)
        assert_eq!(total, 2);
    }
}
