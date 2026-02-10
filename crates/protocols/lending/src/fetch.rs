//! Lending State Fetching from Node
//!
//! Fetches pool boxes from node and parses into protocol state.

use citadel_core::{ProtocolError, TokenId};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_node_client::{NodeCapabilities, NodeClient};
use ergo_tx::ergo_box_utils::{
    extract_long, extract_long_coll, find_token_amount, map_node_error, token_at_index,
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

                    // Fetch borrow positions (only for pools with borrow tokens)
                    if !config.borrow_token_id.is_empty() {
                        match fetch_user_borrow_positions(
                            client,
                            capabilities,
                            config,
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

    // Populate collateral options from hardcoded config (matches Duckpools off-chain pattern).
    // All token pools accept ERG as collateral; the ERG pool has no collateral (threshold=0).
    if config.liquidation_threshold > 0 {
        pool_state.collateral_options = vec![CollateralOption {
            token_id: "native".to_string(),
            token_name: "ERG".to_string(),
            liquidation_threshold: config.liquidation_threshold,
            liquidation_penalty: 0,
            dex_nft: config.collateral_dex_nft.map(|s| s.to_string()),
        }];
    }

    Ok(pool_state)
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
    let borrow_tokens_in_circulation = match token_at_index(ergo_box, 2) {
        Some(remaining) => supply::MAX_BORROW_TOKENS.saturating_sub(remaining),
        None => 0, // No borrow token found = no borrowing activity
    };

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

/// Fetch user's borrow positions (collateral boxes) for a pool.
///
/// For the ERG pool, collateral boxes hold the borrow token and the user's collateral.
/// We search by `borrow_token_id`, then filter for boxes whose R4 matches the user's ErgoTree.
///
/// MVP simplifications:
/// - `total_owed = borrowed_amount` (no interest accrual calculation)
/// - `health_factor = 0.0` (would need DEX price lookup)
pub async fn fetch_user_borrow_positions(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    config: &PoolConfig,
    user_address: &str,
) -> Result<Vec<BorrowPosition>, ProtocolError> {
    use ergo_lib::ergotree_ir::chain::address::{AddressEncoder, NetworkPrefix};
    use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;
    use ergo_lib::ergotree_ir::mir::value::{CollKind, NativeColl};
    use ergo_lib::ergotree_ir::mir::constant::Literal;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    if config.borrow_token_id.is_empty() {
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

    // Search for boxes containing the borrow token
    let borrow_token_id = TokenId::new(config.borrow_token_id);
    let boxes = ergo_node_client::queries::get_boxes_by_token_id(
        client.inner(),
        capabilities,
        &borrow_token_id,
        100, // Fetch up to 100 collateral boxes
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
            None => continue, // Skip boxes without valid R4
        };

        // Check if this box belongs to the user
        if r4_bytes != user_tree_bytes {
            continue;
        }

        let box_id = ergo_box.box_id().to_string();

        // Extract collateral token info and borrow amount from box tokens
        let first_token = ergo_box
            .tokens
            .as_ref()
            .and_then(|toks| toks.get(0));
        let (collateral_token, collateral_amount) = match first_token {
            Some(tok) => {
                let tid: String = tok.token_id.into();
                let amt = *tok.amount.as_u64();
                (tid, amt)
            }
            None => {
                // ERG collateral (value is the collateral)
                ("native".to_string(), *ergo_box.value.as_u64())
            }
        };

        // Determine collateral name
        let collateral_name = if collateral_token == "native" {
            "ERG".to_string()
        } else {
            "Token".to_string()
        };

        // Extract borrow amount from borrow token
        let borrowed_amount = find_token_amount(ergo_box, config.borrow_token_id)
            .unwrap_or(0);

        positions.push(BorrowPosition {
            collateral_box_id: box_id,
            collateral_token: collateral_token.clone(),
            collateral_name,
            collateral_amount,
            borrowed_amount,
            total_owed: borrowed_amount, // MVP: no interest calculation
            health_factor: 0.0,         // MVP: would need DEX price lookup
            liquidation_threshold: config.liquidation_threshold as u16,
            at_risk: false, // MVP: can't determine without price data
        });
    }

    Ok(positions)
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
}
