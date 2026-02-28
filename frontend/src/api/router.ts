/**
 * Smart Router API
 *
 * TypeScript types and invoke wrappers for multi-hop DEX routing,
 * split optimization, liquidity depth analysis, and cross-protocol comparison.
 */

import { invoke } from '@tauri-apps/api/core'

// =============================================================================
// Type Definitions
// =============================================================================

export interface RouteHop {
  pool_id: string
  pool_type: string
  token_in: string
  token_in_name: string | null
  token_in_decimals: number
  token_out: string
  token_out_name: string | null
  token_out_decimals: number
  pool_display_name: string | null
  input_amount: number
  output_amount: number
  price_impact: number
  fee_amount: number
  fee_num: number
  fee_denom: number
  reserves_in: number
  reserves_out: number
}

export interface Route {
  hops: RouteHop[]
  total_input: number
  total_output: number
  total_price_impact: number
  total_fees: number
  effective_rate: number
}

export interface RouteQuote {
  route: Route
  min_output: number
  slippage_percent: number
}

export interface SplitAllocation {
  route_index: number
  fraction: number
  input_amount: number
  output_amount: number
}

export interface SplitRoute {
  allocations: SplitAllocation[]
  total_output: number
  total_input: number
}

/** [impact_percent, max_input_amount] */
export type DepthTierEntry = [number, number]

export interface DepthTiers {
  pool_id: string
  token_in: string
  token_out: string
  tiers: DepthTierEntry[]
}

export interface AcquisitionOption {
  protocol: string
  description: string
  erg_cost_nano: number
  output_amount: number
  effective_price_nano: number
  impact_or_fee_pct: number
  available: boolean
  unavailable_reason: string | null
  route: Route | null
}

export interface AcquisitionComparison {
  target_token_id: string
  target_token_name: string
  input_erg_nano: number
  options: AcquisitionOption[]
  best_index: number | null
}

export interface SplitAllocationDetail {
  route: Route
  fraction: number
  input_amount: number
  output_amount: number
}

export interface SplitRouteDetail {
  allocations: SplitAllocationDetail[]
  total_output: number
  total_input: number
  improvement_pct: number
}

export interface RoutesResponse {
  routes: RouteQuote[]
  depth_tiers: DepthTiers[]
  split: SplitRouteDetail | null
}

// =============================================================================
// Oracle Arb Snapshot Types
// =============================================================================

export interface OracleArbWindow {
  path_label: string
  hops: number
  pool_ids: string[]
  spot_rate_usd_per_erg: number
  discount_pct: number
  rate_at_max: number
  max_erg_input_nano: number
  price_impact_at_max: number
  sigusd_output_at_max: number
  sigusd_output_at_max_usd: number
}

export interface OracleArbSnapshot {
  oracle_rate_usd_per_erg: number
  windows: OracleArbWindow[]
  total_sigusd_below_oracle_raw: number
  total_erg_needed_nano: number
}

// =============================================================================
// API Functions
// =============================================================================

/**
 * Find best swap routes across all pools.
 *
 * Returns ranked routes with per-hop breakdown and liquidity depth tiers.
 */
export async function findSwapRoutes(
  sourceToken: string,
  targetToken: string,
  inputAmount: number,
  maxHops?: number,
  maxRoutes?: number,
  slippage?: number,
  minRate?: number,
): Promise<RoutesResponse> {
  return await invoke<RoutesResponse>('find_swap_routes', {
    sourceToken,
    targetToken,
    inputAmount,
    maxHops,
    maxRoutes,
    slippage,
    minRate,
  })
}

/**
 * Find optimal split across multiple routes to maximize total output.
 */
export async function findSplitRoute(
  sourceToken: string,
  targetToken: string,
  inputAmount: number,
  maxSplits?: number,
  slippage?: number,
): Promise<SplitRoute> {
  return await invoke<SplitRoute>('find_split_route', {
    sourceToken,
    targetToken,
    inputAmount,
    maxSplits,
    slippage,
  })
}

/**
 * Compare SigUSD acquisition options across DEX and SigmaUSD protocol.
 */
export async function compareSigusdOptions(
  inputErgNano: number,
): Promise<AcquisitionComparison> {
  return await invoke<AcquisitionComparison>('compare_sigusd_options', {
    inputErgNano,
  })
}

/**
 * Find best routes for a desired output amount (reverse routing).
 *
 * Returns routes ranked by lowest ERG input needed.
 */
export async function findSwapRoutesByOutput(
  sourceToken: string,
  targetToken: string,
  desiredOutput: number,
  maxHops?: number,
  maxRoutes?: number,
  slippage?: number,
): Promise<RoutesResponse> {
  return await invoke<RoutesResponse>('find_swap_routes_by_output', {
    sourceToken,
    targetToken,
    desiredOutput,
    maxHops,
    maxRoutes,
    slippage,
  })
}

/**
 * Get liquidity depth analysis for pools from a given token.
 */
export async function getLiquidityDepth(
  sourceToken: string,
): Promise<DepthTiers[]> {
  return await invoke<DepthTiers[]>('get_liquidity_depth', {
    sourceToken,
  })
}

/**
 * Get below-oracle SigUSD opportunity snapshot.
 *
 * Returns per-pool arb windows where SigUSD is cheaper than oracle price.
 * No input amount needed — designed for page-load display.
 */
export async function getSigusdArbSnapshot(
  oracleRateUsdPerErg: number,
): Promise<OracleArbSnapshot> {
  return await invoke<OracleArbSnapshot>('get_sigusd_arb_snapshot', {
    oracleRateUsdPerErg,
  })
}
