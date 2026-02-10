/**
 * SigmaFi Bond Protocol API
 *
 * TypeScript types and invoke wrappers for SigmaFi P2P bond protocol Tauri commands.
 */

import { invoke } from '@tauri-apps/api/core'

import type { SignResponse, TxStatusResponse } from './types'
export type { SignResponse, TxStatusResponse }

// =============================================================================
// Type Definitions
// =============================================================================

export interface CollateralToken {
  tokenId: string
  amount: number
  name: string | null
  decimals: number | null
}

export interface OpenOrder {
  boxId: string
  ergoTree: string
  creationHeight: number
  borrowerAddress: string
  loanTokenId: string
  loanTokenName: string
  loanTokenDecimals: number
  principal: number
  repayment: number
  maturityBlocks: number
  collateralErg: number
  collateralTokens: CollateralToken[]
  interestPercent: number
  apr: number
  collateralRatio: number | null
  isOwn: boolean
  transactionId: string
  outputIndex: number
}

export interface ActiveBond {
  boxId: string
  ergoTree: string
  originatingOrderId: string
  borrowerAddress: string
  lenderAddress: string
  loanTokenId: string
  loanTokenName: string
  loanTokenDecimals: number
  repayment: number
  maturityHeight: number
  collateralErg: number
  collateralTokens: CollateralToken[]
  blocksRemaining: number
  isLiquidable: boolean
  isRepayable: boolean
  isOwnLend: boolean
  isOwnBorrow: boolean
  transactionId: string
  outputIndex: number
}

export interface BondMarket {
  orders: OpenOrder[]
  bonds: ActiveBond[]
  blockHeight: number
}

export interface LoanToken {
  token_id: string
  name: string
  decimals: number
}

// =============================================================================
// API Functions
// =============================================================================

export async function fetchBondMarket(userAddress?: string): Promise<BondMarket> {
  return await invoke<BondMarket>('sigmafi_fetch_market', { userAddress: userAddress ?? null })
}

export async function getSupportedTokens(): Promise<LoanToken[]> {
  return await invoke<LoanToken[]>('sigmafi_get_tokens')
}

export async function buildOpenOrder(
  borrowerErgoTree: string,
  loanTokenId: string,
  principal: string,
  repayment: string,
  maturityBlocks: number,
  collateralErg: string,
  collateralTokensJson: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('sigmafi_build_open_order', {
    borrowerErgoTree,
    loanTokenId,
    principal,
    repayment,
    maturityBlocks,
    collateralErg,
    collateralTokensJson,
    userUtxos,
    currentHeight,
  })
}

export async function buildCancelOrder(
  boxId: string,
  borrowerErgoTree: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('sigmafi_build_cancel_order', {
    boxId,
    borrowerErgoTree,
    userUtxos,
    currentHeight,
  })
}

export async function buildCloseOrder(
  boxId: string,
  lenderErgoTree: string,
  uiFeeErgoTree: string,
  loanTokenId: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('sigmafi_build_close_order', {
    boxId,
    lenderErgoTree,
    uiFeeErgoTree,
    loanTokenId,
    userUtxos,
    currentHeight,
  })
}

export async function buildRepay(
  boxId: string,
  loanTokenId: string,
  borrowerErgoTree: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('sigmafi_build_repay', {
    boxId,
    loanTokenId,
    borrowerErgoTree,
    userUtxos,
    currentHeight,
  })
}

export async function buildLiquidate(
  boxId: string,
  lenderErgoTree: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('sigmafi_build_liquidate', {
    boxId,
    lenderErgoTree,
    userUtxos,
    currentHeight,
  })
}

export async function startSigmaFiSign(
  unsignedTx: object,
  message?: string,
): Promise<SignResponse> {
  return await invoke<SignResponse>('start_sigmafi_sign', {
    unsignedTx,
    message,
  })
}

export async function getSigmaFiTxStatus(requestId: string): Promise<TxStatusResponse> {
  return await invoke<TxStatusResponse>('get_sigmafi_tx_status', { requestId })
}

// =============================================================================
// Formatting Helpers
// =============================================================================

export function formatAmount(amount: number, decimals: number): string {
  const divisor = Math.pow(10, decimals)
  return (amount / divisor).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: Math.min(decimals, 6),
  })
}

export function formatPercent(value: number): string {
  return value.toLocaleString(undefined, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 2,
  }) + '%'
}

export function blocksToTimeString(blocks: number): string {
  const minutes = blocks * 2
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (days > 0) return `${days}d ${hours % 24}h`
  if (hours > 0) return `${hours}h ${minutes % 60}m`
  return `${minutes}m`
}

export function truncateAddress(addr: string, chars = 8): string {
  if (addr.length <= chars * 2 + 3) return addr
  return `${addr.slice(0, chars)}...${addr.slice(-chars)}`
}
