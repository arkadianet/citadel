//! MinimumFeeBox on-chain fee parsing
//!
//! The Rosen Bridge stores fee configuration in special "MinimumFeeBox" boxes
//! on the Ergo blockchain. Each fee box contains an NFT (MinFeeNFT) and
//! registers R4-R9 encoding fee schedules per chain and height range.
//!
//! Register layout:
//! - R4: `Coll[Coll[SByte]]` -- chain names as UTF-8 byte arrays
//! - R5: `Coll[Coll[SLong]]` -- height ranges (breakpoints)
//! - R6: `Coll[Coll[SLong]]` -- bridge fees per chain per height range
//! - R7: `Coll[Coll[SLong]]` -- network fees per chain per height range
//! - R8: `Coll[Coll[Coll[SLong]]]` -- RSN ratio pairs (not used here)
//! - R9: `Coll[Coll[SLong]]` -- fee ratios (basis points, divisor=10000)

use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisterId;
use ergo_lib::ergotree_ir::mir::constant::TryExtractInto;
use ergo_node_client::{NodeCapabilities, NodeClient};
use serde::{Deserialize, Serialize};

use citadel_core::TokenId;

/// Fee configuration for a specific token bridging to a target chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeFee {
    /// Protocol fee in source token base units
    pub bridge_fee: i64,
    /// Target chain tx fee in source token base units
    pub network_fee: i64,
    /// Variable fee ratio (basis points, divisor=10000)
    pub fee_ratio: i64,
}

/// Errors from fee fetching/parsing
#[derive(Debug, thiserror::Error)]
pub enum FeeError {
    #[error("Node error: {0}")]
    NodeError(#[from] citadel_core::NodeError),
    #[error("No MinimumFeeBox found for token {token_id}")]
    NoFeeBox { token_id: String },
    #[error("Failed to parse register {register}: {reason}")]
    RegisterParse { register: String, reason: String },
    #[error("Chain '{chain}' not found in fee box")]
    ChainNotFound { chain: String },
    #[error("No fee entry for height {height}")]
    NoFeeForHeight { height: i32 },
}

/// Fetch and parse fees from on-chain MinimumFeeBox.
///
/// 1. Finds boxes with the MinFeeNFT token
/// 2. For non-ERG tokens, finds the fee box that also contains the target token
/// 3. Parses R4-R9 registers to extract fee schedule
/// 4. Finds the fee entry matching the current height and target chain
pub async fn fetch_bridge_fees(
    client: &NodeClient,
    capabilities: &NodeCapabilities,
    min_fee_nft_id: &str,
    ergo_token_id: &str,
    target_chain: &str,
    current_height: i32,
) -> Result<BridgeFee, FeeError> {
    let nft_token_id = TokenId::new(min_fee_nft_id.to_string());

    // Find all boxes containing the MinFeeNFT
    let fee_boxes = ergo_node_client::queries::get_boxes_by_token_id(
        client.inner(),
        capabilities,
        &nft_token_id,
        100,
    )
    .await?;

    if fee_boxes.is_empty() {
        return Err(FeeError::NoFeeBox {
            token_id: min_fee_nft_id.to_string(),
        });
    }

    // Find the right fee box:
    // - Skip "bank" boxes (many tokens, no registers) by requiring R4 to exist
    // - For ERG bridging: box with only 1 token (just the MinFeeNFT)
    // - For token bridging: box that also contains the target token
    let fee_box = if ergo_token_id == "erg" {
        // For ERG (native), find the fee box with only the MinFeeNFT and valid registers
        fee_boxes
            .into_iter()
            .find(|b| {
                let has_r4 = b
                    .additional_registers
                    .get_constant(NonMandatoryRegisterId::R4)
                    .ok()
                    .flatten()
                    .is_some();
                let token_count = b.tokens.as_ref().map_or(0, |t| t.len());
                has_r4 && token_count == 1
            })
            .ok_or_else(|| FeeError::NoFeeBox {
                token_id: "erg".to_string(),
            })?
    } else {
        // For tokens, find the box that contains both MinFeeNFT and the target token
        fee_boxes
            .into_iter()
            .find(|b| {
                let has_r4 = b
                    .additional_registers
                    .get_constant(NonMandatoryRegisterId::R4)
                    .ok()
                    .flatten()
                    .is_some();
                has_r4
                    && b.tokens.as_ref().is_some_and(|tokens| {
                        tokens.iter().any(|t| {
                            let tid_str = hex::encode(t.token_id.as_ref());
                            tid_str == ergo_token_id
                        })
                    })
            })
            .ok_or_else(|| FeeError::NoFeeBox {
                token_id: ergo_token_id.to_string(),
            })?
    };

    parse_fee_box(&fee_box, target_chain, current_height)
}

/// Parse a MinimumFeeBox's registers to extract fees for a given chain and height.
///
/// Register layout (all indexed as `[height_range][chain]`):
/// - R4: chain names
/// - R5: per-chain block height thresholds for each range
/// - R6: bridge fees per range per chain
/// - R7: network fees per range per chain
/// - R9: fee ratios per range per chain
///
/// To find the current fee: use the "ergo" column in R5 to find
/// which height range matches the current Ergo block height, then
/// read the target chain's fee from that range.
fn parse_fee_box(
    fee_box: &ErgoBox,
    target_chain: &str,
    current_height: i32,
) -> Result<BridgeFee, FeeError> {
    // Parse R4: chain names as Coll[Coll[SByte]]
    let chains = parse_r4_chains(fee_box)?;

    // Find the index of our target chain and the "ergo" chain
    let chain_index = chains
        .iter()
        .position(|c| c == target_chain)
        .ok_or_else(|| FeeError::ChainNotFound {
            chain: target_chain.to_string(),
        })?;

    let ergo_index =
        chains
            .iter()
            .position(|c| c == "ergo")
            .ok_or_else(|| FeeError::ChainNotFound {
                chain: "ergo".to_string(),
            })?;

    // Parse R5-R9. Layout: [height_range_index][chain_index]
    let heights = parse_2d_long_register(fee_box, NonMandatoryRegisterId::R5)?;
    let bridge_fees = parse_2d_long_register(fee_box, NonMandatoryRegisterId::R6)?;
    let network_fees = parse_2d_long_register(fee_box, NonMandatoryRegisterId::R7)?;
    let fee_ratios = parse_2d_long_register(fee_box, NonMandatoryRegisterId::R9)?;

    // Find the height range: last range where R5[range][ergo_index] <= current_height
    let height_index = heights
        .iter()
        .rposition(|range| {
            range
                .get(ergo_index)
                .is_some_and(|&h| h <= current_height as i64)
        })
        .ok_or(FeeError::NoFeeForHeight {
            height: current_height,
        })?;

    // Read fees for the target chain from the matching height range
    let bridge_fee = bridge_fees
        .get(height_index)
        .and_then(|row| row.get(chain_index))
        .copied()
        .unwrap_or(0);

    let network_fee = network_fees
        .get(height_index)
        .and_then(|row| row.get(chain_index))
        .copied()
        .unwrap_or(0);

    let fee_ratio = fee_ratios
        .get(height_index)
        .and_then(|row| row.get(chain_index))
        .copied()
        .unwrap_or(0);

    Ok(BridgeFee {
        bridge_fee,
        network_fee,
        fee_ratio,
    })
}

/// Get a register constant from an ErgoBox by ID.
fn get_register(
    fee_box: &ErgoBox,
    reg_id: NonMandatoryRegisterId,
) -> Result<ergo_lib::ergotree_ir::mir::constant::Constant, FeeError> {
    fee_box
        .additional_registers
        .get_constant(reg_id)
        .map_err(|e| FeeError::RegisterParse {
            register: format!("{:?}", reg_id),
            reason: format!("Failed to get register: {:?}", e),
        })?
        .ok_or_else(|| FeeError::RegisterParse {
            register: format!("{:?}", reg_id),
            reason: "Register is empty".to_string(),
        })
}

/// Parse R4 register as `Coll[Coll[SByte]]` containing chain name strings.
fn parse_r4_chains(fee_box: &ErgoBox) -> Result<Vec<String>, FeeError> {
    let r4 = get_register(fee_box, NonMandatoryRegisterId::R4)?;

    // R4 is Coll[Coll[SByte]] -- extract as Vec<Vec<i8>>
    let coll_coll: Vec<Vec<i8>> = r4.try_extract_into().map_err(|e| FeeError::RegisterParse {
        register: "R4".to_string(),
        reason: format!("Failed to extract Coll[Coll[SByte]]: {:?}", e),
    })?;

    // Convert i8 bytes to UTF-8 strings
    coll_coll
        .iter()
        .map(|bytes| {
            let u8_bytes: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
            String::from_utf8(u8_bytes).map_err(|e| FeeError::RegisterParse {
                register: "R4".to_string(),
                reason: format!("Invalid UTF-8 in chain name: {}", e),
            })
        })
        .collect()
}

/// Parse a register as a 2D numeric array.
///
/// Tries `Coll[Coll[SLong]]` (i64) first, then falls back to `Coll[Coll[SInt]]` (i32)
/// since Rosen fee boxes may use either type depending on the register.
fn parse_2d_long_register(
    fee_box: &ErgoBox,
    reg_id: NonMandatoryRegisterId,
) -> Result<Vec<Vec<i64>>, FeeError> {
    let constant = get_register(fee_box, reg_id)?;

    // Try i64 first
    if let Ok(vals) = constant.clone().try_extract_into::<Vec<Vec<i64>>>() {
        return Ok(vals);
    }

    // Fall back to i32 and widen to i64
    let int_vals: Vec<Vec<i32>> =
        constant
            .try_extract_into()
            .map_err(|e| FeeError::RegisterParse {
                register: format!("{:?}", reg_id),
                reason: format!(
                    "Failed to extract as Coll[Coll[SLong]] or Coll[Coll[SInt]]: {:?}",
                    e
                ),
            })?;

    Ok(int_vals
        .into_iter()
        .map(|row| row.into_iter().map(|v| v as i64).collect())
        .collect())
}
