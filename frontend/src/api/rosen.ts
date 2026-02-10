/**
 * Rosen Bridge Protocol API
 *
 * TypeScript types and invoke wrappers for Tauri bridge commands.
 *
 * Commands:
 * - init_bridge_config: Fetch config + token map from GitHub releases
 * - get_bridge_state: Get supported chains and tokens
 * - get_bridge_tokens: Get tokens bridgeable to a specific chain
 * - get_bridge_fees: Get fee estimate for a transfer
 * - build_bridge_lock_tx: Build the lock transaction
 * - start_bridge_sign: Start ErgoPay signing
 * - get_bridge_tx_status: Poll signing status
 */

import { invoke } from '@tauri-apps/api/core'

import type { SignResponse, TxStatusResponse } from './types'
export type { SignResponse, TxStatusResponse }

// =============================================================================
// TYPE DEFINITIONS
// =============================================================================

export interface BridgeTokenInfo {
  ergoTokenId: string
  name: string
  decimals: number
  targetChains: string[]
}

export interface RosenBridgeState {
  supportedChains: string[]
  availableTokens: BridgeTokenInfo[]
}

export interface BridgeFeeInfo {
  bridgeFee: string
  networkFee: string
  feeRatioBps: number
  minTransfer: string
  receivingAmount: string
  bridgeFeeRaw: number
  networkFeeRaw: number
}

export interface LockSummary {
  tokenName: string
  amount: number
  targetChain: string
  targetAddress: string
  bridgeFee: number
  networkFee: number
  totalCostErg: number
}

export interface LockBuildResult {
  unsignedTx: object
  summary: LockSummary
}

// =============================================================================
// API FUNCTIONS
// =============================================================================

export async function initBridgeConfig(): Promise<void> {
  return await invoke<void>('init_bridge_config')
}

export async function getBridgeState(): Promise<RosenBridgeState> {
  return await invoke<RosenBridgeState>('get_bridge_state')
}

export async function getBridgeTokens(targetChain: string): Promise<BridgeTokenInfo[]> {
  return await invoke<BridgeTokenInfo[]>('get_bridge_tokens', { targetChain })
}

export async function getBridgeFees(
  ergoTokenId: string,
  targetChain: string,
  amount: number
): Promise<BridgeFeeInfo> {
  return await invoke<BridgeFeeInfo>('get_bridge_fees', {
    ergoTokenId,
    targetChain,
    amount,
  })
}

export async function buildBridgeLockTx(
  ergoTokenId: string,
  amount: number,
  targetChain: string,
  targetAddress: string,
  bridgeFee: number,
  networkFee: number
): Promise<LockBuildResult> {
  return await invoke<LockBuildResult>('build_bridge_lock_tx', {
    ergoTokenId,
    amount,
    targetChain,
    targetAddress,
    bridgeFee,
    networkFee,
  })
}

export async function startBridgeSign(
  unsignedTx: object,
  message: string
): Promise<SignResponse> {
  return await invoke<SignResponse>('start_bridge_sign', {
    unsignedTx,
    message,
  })
}

export async function getBridgeTxStatus(requestId: string): Promise<TxStatusResponse> {
  return await invoke<TxStatusResponse>('get_bridge_tx_status', { requestId })
}

// =============================================================================
// HELPERS
// =============================================================================

export { formatTokenAmount } from '../utils/format'

const CHAIN_NAMES: Record<string, string> = {
  cardano: 'Cardano',
  bitcoin: 'Bitcoin',
  ethereum: 'Ethereum',
  doge: 'Dogecoin',
  binance: 'Binance',
  'bitcoin-runes': 'Bitcoin Runes',
}

export function chainDisplayName(chain: string): string {
  return CHAIN_NAMES[chain] ?? chain
}

const ADDRESS_PLACEHOLDERS: Record<string, string> = {
  cardano: 'addr1q...',
  bitcoin: 'bc1q...',
  ethereum: '0x...',
  doge: 'D...',
  binance: '0x...',
  'bitcoin-runes': 'bc1p...',
}

export function addressPlaceholder(chain: string): string {
  return ADDRESS_PLACEHOLDERS[chain] ?? 'Enter address...'
}
