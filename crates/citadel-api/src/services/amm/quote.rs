//! Pool discovery and single-pool swap quotes.

use crate::dto::{AmmPoolDto, AmmPoolsResponse, SwapQuoteResponse};
use crate::services::error::IntoServiceError;
use crate::AppState;

pub async fn get_amm_pools(state: &AppState) -> Result<AmmPoolsResponse, String> {
    let client = state.require_node_client().await?;

    let pools = amm::discover_pools(&client).await.into_service()?;

    let pool_dtos: Vec<AmmPoolDto> = pools.into_iter().map(Into::into).collect();
    let count = pool_dtos.len();

    Ok(AmmPoolsResponse {
        pools: pool_dtos,
        count,
    })
}

pub async fn get_amm_quote(
    state: &AppState,
    pool_id: &str,
    input_type: &str,
    amount: u64,
    token_id: Option<String>,
) -> Result<SwapQuoteResponse, String> {
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    let client = state.require_node_client().await?;

    let pool = super::find_pool(&client, pool_id).await?;

    let input = super::parse_swap_input(input_type, amount, token_id.clone())?;

    match amm::quote_swap(&pool, &input) {
        Some(quote) => Ok(quote.into()),
        None => {
            // Token → ERG that would drain below min box: surface the max size.
            if matches!(pool.pool_type, amm::PoolType::N2T) && input_type == "token" {
                if let Some(erg) = pool.erg_reserves {
                    if let Some(max_in) = amm::max_token_in_for_erg_out(
                        pool.token_y.amount,
                        erg,
                        pool.fee_num,
                        pool.fee_denom,
                    ) {
                        if max_in > 0 && max_in < amount {
                            let max_out = amm::calculate_token_to_erg_output(
                                pool.token_y.amount,
                                erg,
                                max_in,
                                pool.fee_num,
                                pool.fee_denom,
                            );
                            return Err(format!(
                                "Amount too large for pool (would leave below min ERG). \
                                 Max swap: {} {} → {} nanoERG",
                                max_in,
                                pool.token_y.name.as_deref().unwrap_or("token"),
                                max_out
                            ));
                        }
                    }
                }
            }
            Err("Cannot calculate quote for this swap".to_string())
        }
    }
}
