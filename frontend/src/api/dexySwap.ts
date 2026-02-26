/**
 * Dexy LP Swap API
 *
 * TypeScript types and invoke wrappers for Dexy LP swap Tauri commands.
 *
 * Commands:
 * - preview_dexy_swap: Get a quote for swapping ERG <-> Dexy tokens via LP pool
 * - build_dexy_swap_tx: Build an unsigned swap transaction
 * - start_mint_sign: Start the signing flow (reused from FreeMint infrastructure)
 * - get_mint_tx_status: Poll for transaction status (reused from FreeMint infrastructure)
 */

import { invoke } from '@tauri-apps/api/core'

import type { SignResponse, TxStatusResponse } from './types'

// =============================================================================
// Type Definitions
// =============================================================================

/** Dexy variant: gold-pegged or USD-pegged */
export type DexyVariant = 'gold' | 'usd'

/** Swap direction */
export type SwapDirection = 'erg_to_dexy' | 'dexy_to_erg'

/**
 * Response from preview_dexy_swap command.
 * Contains quote details for the proposed swap.
 */
export interface DexySwapPreviewResponse {
  variant: string
  direction: string
  /** Input amount in smallest units (nanoERG or raw tokens) */
  input_amount: number
  /** Expected output amount in smallest units */
  output_amount: number
  /** Human-readable name of the output token */
  output_token_name: string
  /** Number of decimal places for the output token */
  output_decimals: number
  /** Minimum output after slippage */
  min_output: number
  /** Price impact as a percentage (e.g. 0.5 = 0.5%) */
  price_impact: number
  /** Fee percentage (e.g. 0.3 = 0.3%) */
  fee_pct: number
  /** Miner fee in nanoERG */
  miner_fee_nano: number
  /** LP pool ERG reserves in nanoERG */
  lp_erg_reserves: number
  /** LP pool Dexy token reserves in raw units */
  lp_dexy_reserves: number
}

/**
 * Summary of a built swap transaction.
 */
export interface DexySwapTxSummary {
  direction: string
  input_amount: number
  output_amount: number
  min_output: number
  price_impact_pct: number
  fee_pct: number
  miner_fee_nano: number
}

/**
 * Response from build_dexy_swap_tx command.
 * Contains the unsigned transaction and a human-readable summary.
 */
export interface DexySwapBuildResponse {
  unsigned_tx: object
  summary: DexySwapTxSummary
}

export type DexySwapSignResponse = SignResponse
export type DexySwapTxStatusResponse = TxStatusResponse

// =============================================================================
// API Functions
// =============================================================================

/**
 * Preview a Dexy LP swap.
 *
 * @param variant - "gold" or "usd"
 * @param direction - "erg_to_dexy" or "dexy_to_erg"
 * @param amount - Amount in smallest units (nanoERG for erg_to_dexy, raw tokens for dexy_to_erg)
 * @param slippage - Optional slippage tolerance as a percentage (default 0.5)
 * @returns Swap preview with expected output, price impact, fees, and pool state
 */
export async function previewDexySwap(
  variant: DexyVariant,
  direction: SwapDirection,
  amount: number,
  slippage?: number,
): Promise<DexySwapPreviewResponse> {
  return await invoke<DexySwapPreviewResponse>('preview_dexy_swap', {
    variant,
    direction,
    amount,
    slippage,
  })
}

/**
 * Build an unsigned Dexy LP swap transaction.
 *
 * @param variant - "gold" or "usd"
 * @param direction - "erg_to_dexy" or "dexy_to_erg"
 * @param amount - Amount in smallest units
 * @param minOutput - Minimum acceptable output (from preview)
 * @param userAddress - User's Ergo address
 * @param userUtxos - User's unspent transaction outputs
 * @param currentHeight - Current blockchain height
 * @returns Unsigned transaction and summary
 */
export async function buildDexySwapTx(
  variant: DexyVariant,
  direction: SwapDirection,
  amount: number,
  minOutput: number,
  userAddress: string,
  userUtxos: object[],
  currentHeight: number,
  recipientAddress?: string | null,
): Promise<DexySwapBuildResponse> {
  return await invoke<DexySwapBuildResponse>('build_dexy_swap_tx', {
    variant,
    direction,
    amount,
    minOutput,
    userAddress,
    userUtxos,
    currentHeight,
    recipientAddress: recipientAddress || null,
  })
}

/**
 * Start the signing flow for a Dexy swap transaction.
 * Reuses the existing start_mint_sign Tauri command.
 *
 * @param unsignedTx - The unsigned transaction from buildDexySwapTx
 * @param message - Human-readable description of the transaction
 * @returns Request ID and signing URLs (ErgoPay QR + Nautilus)
 */
export async function startDexySwapSign(
  unsignedTx: object,
  message: string,
): Promise<DexySwapSignResponse> {
  return await invoke<DexySwapSignResponse>('start_mint_sign', {
    request: {
      unsigned_tx: unsignedTx,
      message,
    },
  })
}

/**
 * Poll for Dexy swap transaction status.
 * Reuses the existing get_mint_tx_status Tauri command.
 *
 * @param requestId - Request ID from startDexySwapSign
 * @returns Current status, optional tx_id on success, optional error on failure
 */
export async function getDexySwapTxStatus(
  requestId: string,
): Promise<DexySwapTxStatusResponse> {
  return await invoke<DexySwapTxStatusResponse>('get_mint_tx_status', {
    requestId,
  })
}

// =============================================================================
// LP Deposit/Redeem Types
// =============================================================================

/**
 * Response from preview_lp_deposit or preview_lp_redeem commands.
 * Contains preview details for LP deposit/redeem operations.
 */
export interface LpPreviewResponse {
  variant: string
  action: string
  erg_amount: string
  dexy_amount: string
  lp_tokens: string
  redemption_fee_pct: number | null
  can_execute: boolean
  error: string | null
  miner_fee_nano: string
}

/**
 * Response from build_lp_deposit_tx or build_lp_redeem_tx commands.
 * Contains the unsigned transaction and a human-readable summary.
 */
export interface LpBuildResponse {
  unsigned_tx: any
  summary: {
    action: string
    erg_amount: number
    dexy_amount: number
    lp_tokens: number
    miner_fee_nano: number
  }
}

// =============================================================================
// LP Deposit/Redeem API Functions
// =============================================================================

/**
 * Preview an LP deposit (add liquidity).
 *
 * @param variant - "gold" or "usd"
 * @param ergAmount - ERG amount in nanoERG
 * @param dexyAmount - Dexy token amount in raw units
 * @returns Preview with LP tokens to receive, consumed amounts, and feasibility
 */
export async function previewLpDeposit(
  variant: string,
  ergAmount: number,
  dexyAmount: number,
): Promise<LpPreviewResponse> {
  return invoke('preview_lp_deposit', {
    variant,
    ergAmount: Math.floor(ergAmount),
    dexyAmount: Math.floor(dexyAmount),
  })
}

/**
 * Build an unsigned LP deposit (add liquidity) transaction.
 *
 * @param variant - "gold" or "usd"
 * @param ergAmount - ERG amount in nanoERG
 * @param dexyAmount - Dexy token amount in raw units
 * @param userAddress - User's Ergo address
 * @param userUtxos - User's unspent transaction outputs
 * @param currentHeight - Current blockchain height
 * @param recipientAddress - Optional recipient address for LP tokens
 * @returns Unsigned transaction and summary
 */
export async function buildLpDepositTx(
  variant: string,
  ergAmount: number,
  dexyAmount: number,
  userAddress: string,
  userUtxos: any[],
  currentHeight: number,
  recipientAddress?: string,
): Promise<LpBuildResponse> {
  return invoke('build_lp_deposit_tx', {
    variant,
    ergAmount: Math.floor(ergAmount),
    dexyAmount: Math.floor(dexyAmount),
    userAddress,
    userUtxos,
    currentHeight,
    recipientAddress: recipientAddress || null,
  })
}

/**
 * Preview an LP redeem (remove liquidity).
 *
 * @param variant - "gold" or "usd"
 * @param lpAmount - LP token amount to redeem
 * @returns Preview with ERG and Dexy to receive, fee info, and feasibility
 */
export async function previewLpRedeem(
  variant: string,
  lpAmount: number,
): Promise<LpPreviewResponse> {
  return invoke('preview_lp_redeem', {
    variant,
    lpAmount: Math.floor(lpAmount),
  })
}

/**
 * Build an unsigned LP redeem (remove liquidity) transaction.
 *
 * @param variant - "gold" or "usd"
 * @param lpAmount - LP token amount to redeem
 * @param userAddress - User's Ergo address
 * @param userUtxos - User's unspent transaction outputs
 * @param currentHeight - Current blockchain height
 * @param recipientAddress - Optional recipient address for redeemed assets
 * @returns Unsigned transaction and summary
 */
export async function buildLpRedeemTx(
  variant: string,
  lpAmount: number,
  userAddress: string,
  userUtxos: any[],
  currentHeight: number,
  recipientAddress?: string,
): Promise<LpBuildResponse> {
  return invoke('build_lp_redeem_tx', {
    variant,
    lpAmount: Math.floor(lpAmount),
    userAddress,
    userUtxos,
    currentHeight,
    recipientAddress: recipientAddress || null,
  })
}
