/**
 * AMM Protocol API
 *
 * TypeScript types and invoke wrappers for Spectrum AMM protocol Tauri commands.
 *
 * Commands:
 * - get_amm_pools: Fetch all AMM pools (N2T and T2T)
 * - get_amm_quote: Calculate swap quote for a pool
 */

import { invoke } from '@tauri-apps/api/core'

import type { SignResponse, TxStatusResponse } from './types'

// =============================================================================
// Type Definitions
// =============================================================================

/**
 * Token amount with optional metadata
 */
export interface TokenAmount {
  token_id: string
  amount: number
  decimals?: number
  name?: string
}

/**
 * AMM Pool information
 */
export interface AmmPool {
  /** Pool NFT ID (unique identifier) */
  pool_id: string
  /** Pool type: "N2T" or "T2T" */
  pool_type: string
  /** Current UTXO box ID */
  box_id: string
  /** ERG reserves (for N2T pools) */
  erg_reserves?: number
  /** Token X (for T2T pools) */
  token_x?: TokenAmount
  /** Token Y */
  token_y: TokenAmount
  /** LP token ID */
  lp_token_id: string
  /** Circulating LP supply */
  lp_circulating: number
  /** Swap fee percentage (e.g., 0.3) */
  fee_percent: number
}

/**
 * Response from get_amm_pools
 */
export interface AmmPoolsResponse {
  pools: AmmPool[]
  count: number
}

/**
 * Swap input specification
 */
export type SwapInput =
  | { input_type: 'erg'; amount: number }
  | { input_type: 'token'; token_id: string; amount: number }

/**
 * Swap quote response
 */
export interface SwapQuote {
  input: SwapInput
  output: TokenAmount
  /** Price impact percentage */
  price_impact: number
  /** Fee amount deducted */
  fee_amount: number
  /** Effective exchange rate */
  effective_rate: number
  /** Suggested minimum output with default slippage */
  min_output_suggested: number
}

// =============================================================================
// API Functions
// =============================================================================

/**
 * Fetch all AMM pools from the network
 *
 * @returns List of discovered N2T and T2T pools with reserves and fees
 */
export async function getAmmPools(): Promise<AmmPoolsResponse> {
  return await invoke<AmmPoolsResponse>('get_amm_pools')
}

/**
 * Calculate a swap quote for the given pool and input
 *
 * @param poolId - Pool NFT ID to swap against
 * @param inputType - "erg" for ERG input, "token" for token input
 * @param amount - Amount to swap (in smallest units)
 * @param tokenId - Token ID (required if inputType is "token")
 * @returns Swap quote with expected output, price impact, and fees
 */
export async function getAmmQuote(
  poolId: string,
  inputType: 'erg' | 'token',
  amount: number,
  tokenId?: string
): Promise<SwapQuote> {
  return await invoke<SwapQuote>('get_amm_quote', {
    poolId,
    inputType,
    amount,
    tokenId,
  })
}

// =============================================================================
// Helper Functions
// =============================================================================

export { formatTokenAmount, formatErg } from '../utils/format'

/**
 * Get pool display name
 *
 * @param pool - AMM pool
 * @returns Human-readable pool name like "ERG/SigUSD" or "SigUSD/SigRSV"
 */
export function getPoolDisplayName(pool: AmmPool): string {
  if (pool.pool_type === 'N2T') {
    return `ERG/${pool.token_y.name || pool.token_y.token_id.slice(0, 8)}`
  } else {
    const xName = pool.token_x?.name || pool.token_x?.token_id.slice(0, 8) || 'X'
    const yName = pool.token_y.name || pool.token_y.token_id.slice(0, 8)
    return `${xName}/${yName}`
  }
}

// =============================================================================
// Swap Transaction Types
// =============================================================================

export interface SwapPreviewResponse {
  output_amount: number
  output_token_id: string
  output_token_name: string | null
  output_decimals: number | null
  min_output: number
  price_impact: number
  fee_amount: number
  effective_rate: number
  execution_fee_nano: number
  miner_fee_nano: number
  total_erg_cost_nano: number
}

export interface SwapBuildResponse {
  unsigned_tx: object
  summary: SwapTxSummary
}

export interface SwapTxSummary {
  input_amount: number
  input_token: string
  min_output: number
  output_token: string
  execution_fee: number
  miner_fee: number
  total_erg_cost: number
}

export type SwapSignResponse = SignResponse
export type SwapTxStatusResponse = TxStatusResponse

// =============================================================================
// Swap API Functions
// =============================================================================

export async function previewSwap(
  poolId: string,
  inputType: 'erg' | 'token',
  amount: number,
  tokenId?: string,
  slippage?: number,
  nitro?: number,
): Promise<SwapPreviewResponse> {
  return await invoke<SwapPreviewResponse>('preview_swap', {
    poolId,
    inputType,
    amount,
    tokenId,
    slippage,
    nitro,
  })
}

export async function buildSwapTx(
  poolId: string,
  inputType: 'erg' | 'token',
  amount: number,
  tokenId: string | undefined,
  minOutput: number,
  userAddress: string,
  userUtxos: object[],
  currentHeight: number,
  executionFeeNano?: number,
  recipientAddress?: string | null,
): Promise<SwapBuildResponse> {
  return await invoke<SwapBuildResponse>('build_swap_tx', {
    poolId,
    inputType,
    amount,
    tokenId,
    minOutput,
    userAddress,
    userUtxos,
    currentHeight,
    executionFeeNano,
    recipientAddress: recipientAddress || null,
  })
}

// =============================================================================
// Direct Swap Types
// =============================================================================

export interface DirectSwapPreviewResponse {
  output_amount: number
  output_token_id: string
  output_token_name: string | null
  output_decimals: number | null
  min_output: number
  price_impact: number
  fee_amount: number
  effective_rate: number
  miner_fee_nano: number
  total_erg_cost_nano: number
}

export interface DirectSwapBuildResponse {
  unsigned_tx: object
  summary: DirectSwapSummary
}

export interface DirectSwapSummary {
  input_amount: number
  input_token: string
  output_amount: number
  min_output: number
  output_token: string
  miner_fee: number
  total_erg_cost: number
}

// =============================================================================
// Direct Swap API Functions
// =============================================================================

export async function previewDirectSwap(
  poolId: string,
  inputType: 'erg' | 'token',
  amount: number,
  tokenId?: string,
  slippage?: number,
): Promise<DirectSwapPreviewResponse> {
  return await invoke<DirectSwapPreviewResponse>('preview_direct_swap', {
    poolId,
    inputType,
    amount,
    tokenId,
    slippage,
  })
}

export async function buildDirectSwapTx(
  poolId: string,
  inputType: 'erg' | 'token',
  amount: number,
  tokenId: string | undefined,
  minOutput: number,
  userAddress: string,
  userUtxos: object[],
  currentHeight: number,
  recipientAddress?: string | null,
): Promise<DirectSwapBuildResponse> {
  return await invoke<DirectSwapBuildResponse>('build_direct_swap_tx', {
    poolId,
    inputType,
    amount,
    tokenId,
    minOutput,
    userAddress,
    userUtxos,
    currentHeight,
    recipientAddress: recipientAddress || null,
  })
}

export async function startSwapSign(unsignedTx: object, message: string): Promise<SwapSignResponse> {
  return await invoke<SwapSignResponse>('start_swap_sign', {
    unsignedTx,
    message,
  })
}

export async function getSwapTxStatus(requestId: string): Promise<SwapTxStatusResponse> {
  return await invoke<SwapTxStatusResponse>('get_swap_tx_status', {
    requestId,
  })
}

// =============================================================================
// LP Deposit/Redeem Types
// =============================================================================

export interface AmmLpDepositPreviewResponse {
  lpReward: number
  ergAmount: number
  tokenAmount: number
  tokenName: string | null
  tokenDecimals: number | null
  poolSharePercent: number
  minerFeeNano: number
  totalErgCostNano: number
}

export interface AmmLpRedeemPreviewResponse {
  ergOutput: number
  tokenOutput: number
  tokenName: string | null
  tokenDecimals: number | null
  lpAmount: number
  minerFeeNano: number
  totalErgCostNano: number
}

export interface AmmLpBuildResponse {
  unsignedTx: object
  summary: object
}

// =============================================================================
// LP Deposit/Redeem API Functions
// =============================================================================

export async function previewAmmLpDeposit(
  poolId: string,
  inputType: 'erg' | 'token',
  amount: number,
): Promise<AmmLpDepositPreviewResponse> {
  return await invoke<AmmLpDepositPreviewResponse>('preview_amm_lp_deposit', {
    poolId,
    inputType,
    amount,
  })
}

export async function buildAmmLpDepositTx(
  poolId: string,
  ergAmount: number,
  tokenAmount: number,
  userAddress: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<AmmLpBuildResponse> {
  return await invoke<AmmLpBuildResponse>('build_amm_lp_deposit_tx', {
    poolId,
    ergAmount,
    tokenAmount,
    userAddress,
    userUtxos,
    currentHeight,
  })
}

export async function buildAmmLpDepositOrder(
  poolId: string,
  ergAmount: number,
  tokenAmount: number,
  userAddress: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<AmmLpBuildResponse> {
  return await invoke<AmmLpBuildResponse>('build_amm_lp_deposit_order', {
    poolId,
    ergAmount,
    tokenAmount,
    userAddress,
    userUtxos,
    currentHeight,
  })
}

export async function previewAmmLpRedeem(
  poolId: string,
  lpAmount: number,
): Promise<AmmLpRedeemPreviewResponse> {
  return await invoke<AmmLpRedeemPreviewResponse>('preview_amm_lp_redeem', {
    poolId,
    lpAmount,
  })
}

export async function buildAmmLpRedeemTx(
  poolId: string,
  lpAmount: number,
  userAddress: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<AmmLpBuildResponse> {
  return await invoke<AmmLpBuildResponse>('build_amm_lp_redeem_tx', {
    poolId,
    lpAmount,
    userAddress,
    userUtxos,
    currentHeight,
  })
}

export async function buildAmmLpRedeemOrder(
  poolId: string,
  lpAmount: number,
  userAddress: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<AmmLpBuildResponse> {
  return await invoke<AmmLpBuildResponse>('build_amm_lp_redeem_order', {
    poolId,
    lpAmount,
    userAddress,
    userUtxos,
    currentHeight,
  })
}

// =============================================================================
// Pool Creation Types
// =============================================================================

export interface PoolCreatePreviewResponse {
  pool_type: string
  lp_share: number
  fee_percent: number
  fee_num: number
  miner_fee_nano: number
  total_erg_cost_nano: number
}

// =============================================================================
// Pool Creation API Functions
// =============================================================================

export async function previewPoolCreate(
  poolType: string,
  xTokenId: string | undefined,
  xAmount: number,
  yTokenId: string,
  yAmount: number,
  feePercent: number,
): Promise<PoolCreatePreviewResponse> {
  return await invoke<PoolCreatePreviewResponse>('preview_pool_create', {
    poolType,
    xTokenId: xTokenId || null,
    xAmount,
    yTokenId,
    yAmount,
    feePercent,
  })
}

export async function buildPoolBootstrapTx(
  poolType: string,
  xTokenId: string | undefined,
  xAmount: number,
  yTokenId: string,
  yAmount: number,
  feePercent: number,
  userUtxos: object[],
  currentHeight: number,
): Promise<AmmLpBuildResponse> {
  return await invoke<AmmLpBuildResponse>('build_pool_bootstrap_tx', {
    poolType,
    xTokenId: xTokenId || null,
    xAmount,
    yTokenId,
    yAmount,
    feePercent,
    userUtxos,
    currentHeight,
  })
}

export async function buildPoolCreateTx(
  bootstrapBox: object,
  poolType: string,
  xTokenId: string | undefined,
  xAmount: number,
  yTokenId: string,
  yAmount: number,
  feeNum: number,
  lpTokenId: string,
  userLpShare: number,
  currentHeight: number,
): Promise<AmmLpBuildResponse> {
  return await invoke<AmmLpBuildResponse>('build_pool_create_tx', {
    bootstrapBox,
    poolType,
    xTokenId: xTokenId || null,
    xAmount,
    yTokenId,
    yAmount,
    feeNum,
    lpTokenId,
    userLpShare,
    currentHeight,
  })
}
