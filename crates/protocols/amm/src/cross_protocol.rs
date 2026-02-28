//! Cross-Protocol Comparison
//!
//! Compares token acquisition costs across DEX routes and protocol minting
//! (e.g., SigmaUSD) to find the cheapest option.

use serde::{Deserialize, Serialize};

use crate::router::{find_best_routes, PoolGraph, Route};

/// An acquisition option for a target token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionOption {
    /// Source protocol name (e.g., "Spectrum DEX", "SigmaUSD")
    pub protocol: String,
    /// Brief description
    pub description: String,
    /// ERG cost in nanoERG
    pub erg_cost_nano: u64,
    /// Target token output amount (raw units)
    pub output_amount: u64,
    /// Effective price: erg_cost_nano / output_amount
    pub effective_price_nano: f64,
    /// Price impact (DEX) or protocol fee percentage (mint)
    pub impact_or_fee_pct: f64,
    /// Whether this option is currently available
    pub available: bool,
    /// Reason if not available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    /// Route details for DEX options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<Route>,
}

/// Comparison result ranking all options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionComparison {
    pub target_token_id: String,
    pub target_token_name: String,
    pub input_erg_nano: u64,
    /// All options ranked by effective price (cheapest first)
    pub options: Vec<AcquisitionOption>,
    /// Index of the best *available* option
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_index: Option<usize>,
}

/// Parameters for SigmaUSD protocol state (avoids depending on sigmausd crate).
pub struct SigmaUsdParams {
    /// nanoERG cost per SigUSD unit (includes 2% fee)
    pub sigusd_price_nano: i64,
    /// Whether minting is currently allowed (RR > 400%)
    pub can_mint: bool,
    /// Current reserve ratio percentage
    pub reserve_ratio_pct: f64,
}

/// Compare ERG -> target token acquisition across DEX routes and SigmaUSD mint.
///
/// Returns ranked options. The `sigmausd_params` is `Some` only when the target
/// token is SigUSD.
pub fn compare_acquisition(
    graph: &PoolGraph,
    target_token_id: &str,
    target_token_name: &str,
    input_erg_nano: u64,
    sigmausd_params: Option<&SigmaUsdParams>,
) -> AcquisitionComparison {
    let mut options = Vec::new();

    // DEX routes
    let routes = find_best_routes(
        graph,
        crate::router::ERG_TOKEN_ID,
        target_token_id,
        input_erg_nano,
        3,
        5,
    );

    for (i, route) in routes.iter().enumerate() {
        let hop_desc = if route.hops.len() == 1 {
            "Direct DEX swap".to_string()
        } else {
            let intermediates: Vec<String> = route.hops[..route.hops.len() - 1]
                .iter()
                .map(|h| {
                    h.token_out_name
                        .clone()
                        .unwrap_or_else(|| h.token_out[..8.min(h.token_out.len())].to_string())
                })
                .collect();
            format!("{}-hop via {}", route.hops.len(), intermediates.join(" -> "))
        };

        let effective_price = if route.total_output > 0 {
            input_erg_nano as f64 / route.total_output as f64
        } else {
            f64::INFINITY
        };

        options.push(AcquisitionOption {
            protocol: "Spectrum DEX".to_string(),
            description: format!("{} (route #{})", hop_desc, i + 1),
            erg_cost_nano: input_erg_nano,
            output_amount: route.total_output,
            effective_price_nano: effective_price,
            impact_or_fee_pct: route.total_price_impact,
            available: route.total_output > 0,
            unavailable_reason: None,
            route: Some(route.clone()),
        });
    }

    // SigmaUSD mint option (only if params provided)
    if let Some(params) = sigmausd_params {
        let mint_output = if params.sigusd_price_nano > 0 {
            // SigUSD has 2 decimal places; sigusd_price_nano is nanoERG per cent
            (input_erg_nano as i64 / params.sigusd_price_nano) as u64
        } else {
            0
        };

        let effective_price = if mint_output > 0 {
            input_erg_nano as f64 / mint_output as f64
        } else {
            f64::INFINITY
        };

        let available = params.can_mint && mint_output > 0;
        let unavailable_reason = if !params.can_mint {
            Some(format!(
                "Reserve ratio {:.0}% < 400% minimum",
                params.reserve_ratio_pct
            ))
        } else if mint_output == 0 {
            Some("Amount too small to mint".to_string())
        } else {
            None
        };

        options.push(AcquisitionOption {
            protocol: "SigmaUSD".to_string(),
            description: format!(
                "Protocol mint (2% fee, RR={:.0}%)",
                params.reserve_ratio_pct
            ),
            erg_cost_nano: input_erg_nano,
            output_amount: mint_output,
            effective_price_nano: effective_price,
            impact_or_fee_pct: 2.0,
            available,
            unavailable_reason,
            route: None,
        });
    }

    // Sort by effective price ascending (cheapest first)
    options.sort_by(|a, b| {
        a.effective_price_nano
            .partial_cmp(&b.effective_price_nano)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let best_index = options.iter().position(|o| o.available);

    AcquisitionComparison {
        target_token_id: target_token_id.to_string(),
        target_token_name: target_token_name.to_string(),
        input_erg_nano,
        options,
        best_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::build_pool_graph;
    use crate::state::{AmmPool, PoolType, TokenAmount};

    fn make_sigusd_pool(erg_reserves: u64, sigusd_reserves: u64) -> AmmPool {
        AmmPool {
            pool_id: "sigusd_pool".to_string(),
            pool_type: PoolType::N2T,
            box_id: "box1".to_string(),
            erg_reserves: Some(erg_reserves),
            token_x: None,
            token_y: TokenAmount {
                token_id: "sigusd_token".to_string(),
                amount: sigusd_reserves,
                decimals: Some(2),
                name: Some("SigUSD".to_string()),
            },
            lp_token_id: "lp1".to_string(),
            lp_circulating: 1000,
            fee_num: 997,
            fee_denom: 1000,
        }
    }

    #[test]
    fn test_dex_only_comparison() {
        let pools = vec![make_sigusd_pool(100_000_000_000, 5_000_000)]; // 100 ERG, 50000 SigUSD
        let graph = build_pool_graph(&pools, 10_000_000_000);

        let result = compare_acquisition(
            &graph,
            "sigusd_token",
            "SigUSD",
            1_000_000_000, // 1 ERG
            None,
        );

        assert!(!result.options.is_empty());
        assert!(result.options[0].available);
        assert_eq!(result.options[0].protocol, "Spectrum DEX");
        assert!(result.best_index.is_some());
    }

    #[test]
    fn test_mint_cheaper_than_dex() {
        // Pool with bad rate (DEX premium)
        let pools = vec![make_sigusd_pool(100_000_000_000, 1_000_000)]; // Very few SigUSD
        let graph = build_pool_graph(&pools, 10_000_000_000);

        let params = SigmaUsdParams {
            sigusd_price_nano: 2_857_142, // ~0.35 ERG per SigUSD cent
            can_mint: true,
            reserve_ratio_pct: 500.0,
        };

        let result = compare_acquisition(
            &graph,
            "sigusd_token",
            "SigUSD",
            1_000_000_000,
            Some(&params),
        );

        // SigmaUSD mint should be cheaper since DEX has very low liquidity
        let mint_option = result.options.iter().find(|o| o.protocol == "SigmaUSD");
        assert!(mint_option.is_some());
        assert!(mint_option.unwrap().available);
    }

    #[test]
    fn test_mint_unavailable_rr_low() {
        let pools = vec![make_sigusd_pool(100_000_000_000, 5_000_000)];
        let graph = build_pool_graph(&pools, 10_000_000_000);

        let params = SigmaUsdParams {
            sigusd_price_nano: 2_857_142,
            can_mint: false,
            reserve_ratio_pct: 320.0,
        };

        let result = compare_acquisition(
            &graph,
            "sigusd_token",
            "SigUSD",
            1_000_000_000,
            Some(&params),
        );

        let mint_option = result.options.iter().find(|o| o.protocol == "SigmaUSD");
        assert!(mint_option.is_some());
        assert!(!mint_option.unwrap().available);
        assert!(mint_option
            .unwrap()
            .unavailable_reason
            .as_ref()
            .unwrap()
            .contains("320%"));
    }

    #[test]
    fn test_no_dex_routes() {
        let graph = build_pool_graph(&[], 0);

        let params = SigmaUsdParams {
            sigusd_price_nano: 2_857_142,
            can_mint: true,
            reserve_ratio_pct: 500.0,
        };

        let result = compare_acquisition(
            &graph,
            "sigusd_token",
            "SigUSD",
            1_000_000_000,
            Some(&params),
        );

        // Only mint option should exist
        assert_eq!(result.options.len(), 1);
        assert_eq!(result.options[0].protocol, "SigmaUSD");
    }
}
