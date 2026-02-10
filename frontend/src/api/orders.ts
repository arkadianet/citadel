/**
 * Swap Order Discovery API
 *
 * Discovers pending swap orders on-chain via template matching
 * and direct swap transactions in the mempool.
 */

import { invoke } from '@tauri-apps/api/core'
import type { SwapBuildResponse, SwapSignResponse, SwapTxStatusResponse } from './amm'

// =============================================================================
// Type Definitions
// =============================================================================

export interface PendingOrder {
  boxId: string
  txId: string
  poolId: string
  input: { type: 'Erg'; amount: number } | { type: 'Token'; tokenId: string; amount: number }
  minOutput: number
  inputDecimals: number
  outputDecimals: number
  redeemerAddress: string
  createdHeight: number
  valueNanoErg: number
  orderType: string
  method: 'proxy' | 'direct'
}

export interface MempoolSwap {
  txId: string
  poolId: string
  receivingErg: number
  receivingTokens: [string, number, number][]  // [tokenId, amount, decimals]
}

// =============================================================================
// API Functions
// =============================================================================

/**
 * Discover pending (unspent) swap orders for the connected wallet.
 * Scans on-chain transaction history and matches ErgoTree templates.
 */
export async function getPendingOrders(): Promise<PendingOrder[]> {
  return await invoke<PendingOrder[]>('get_pending_orders')
}

/**
 * Find direct swap transactions in the mempool for the connected wallet.
 */
export async function getMempoolSwaps(): Promise<MempoolSwap[]> {
  return await invoke<MempoolSwap[]>('get_mempool_swaps')
}

/**
 * Build a refund transaction for a swap proxy box.
 * Backend fetches the proxy box by ID internally.
 */
export async function buildSwapRefundTx(
  boxId: string,
  userErgoTree: string,
): Promise<SwapBuildResponse> {
  return await invoke<SwapBuildResponse>('build_swap_refund_tx', {
    boxId,
    userErgoTree,
  })
}

/**
 * Start ErgoPay signing flow for a refund transaction
 */
export async function startRefundSign(
  unsignedTx: object,
  message: string,
): Promise<SwapSignResponse> {
  return await invoke<SwapSignResponse>('start_refund_sign', {
    unsignedTx,
    message,
  })
}

/**
 * Get status of a refund transaction signing request
 */
export async function getRefundTxStatus(requestId: string): Promise<SwapTxStatusResponse> {
  return await invoke<SwapTxStatusResponse>('get_refund_tx_status', {
    requestId,
  })
}

// =============================================================================
// Helper Functions
// =============================================================================

/** Format input for display */
export function formatOrderInput(input: PendingOrder['input'], inputDecimals: number = 0): string {
  if (input.type === 'Erg') {
    return `${(input.amount / 1e9).toFixed(4)} ERG`
  }
  const divisor = Math.pow(10, inputDecimals)
  const display = inputDecimals > 0
    ? (input.amount / divisor).toLocaleString(undefined, { maximumFractionDigits: inputDecimals })
    : input.amount.toLocaleString()
  return `${display} tokens`
}
