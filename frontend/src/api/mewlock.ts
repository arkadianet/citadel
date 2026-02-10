/**
 * MewLock Timelock Protocol API
 *
 * TypeScript types and invoke wrappers for MewLock Tauri commands.
 */

import { invoke } from '@tauri-apps/api/core'

import type { SignResponse, TxStatusResponse } from './types'
export type { SignResponse, TxStatusResponse }

// =============================================================================
// Type Definitions
// =============================================================================

export interface LockedToken {
  tokenId: string
  amount: number
  name: string | null
  decimals: number | null
}

export interface MewLockBox {
  boxId: string
  depositorAddress: string
  unlockHeight: number
  timestamp: number | null
  lockName: string | null
  lockDescription: string | null
  ergValue: number
  tokens: LockedToken[]
  transactionId: string
  outputIndex: number
  creationHeight: number
  isOwn: boolean
  isUnlockable: boolean
  blocksRemaining: number
}

export interface MewLockState {
  locks: MewLockBox[]
  currentHeight: number
  totalLocks: number
  ownLocks: number
}

export interface LockDuration {
  label: string
  blocks: number
}

// =============================================================================
// API Functions
// =============================================================================

export async function fetchMewLockState(userAddress?: string): Promise<MewLockState> {
  return await invoke<MewLockState>('mewlock_fetch_state', { userAddress: userAddress ?? null })
}

export async function getLockDurations(): Promise<LockDuration[]> {
  return await invoke<LockDuration[]>('mewlock_get_durations')
}

export async function buildLockTx(
  userErgoTree: string,
  lockErg: string,
  lockTokensJson: string,
  unlockHeight: number,
  timestamp: string | null,
  lockName: string | null,
  lockDescription: string | null,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('mewlock_build_lock', {
    userErgoTree,
    lockErg,
    lockTokensJson,
    unlockHeight,
    timestamp,
    lockName,
    lockDescription,
    userUtxos,
    currentHeight,
  })
}

export async function buildUnlockTx(
  lockBoxJson: string,
  userErgoTree: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('mewlock_build_unlock', {
    lockBoxJson,
    userErgoTree,
    userUtxos,
    currentHeight,
  })
}

export async function startMewLockSign(
  unsignedTx: object,
  message?: string,
): Promise<SignResponse> {
  return await invoke<SignResponse>('start_mewlock_sign', {
    unsignedTx,
    message,
  })
}

export async function getMewLockTxStatus(requestId: string): Promise<TxStatusResponse> {
  return await invoke<TxStatusResponse>('get_mewlock_tx_status', { requestId })
}

// =============================================================================
// Formatting Helpers
// =============================================================================

export function formatErg(nanoErg: number): string {
  return (nanoErg / 1_000_000_000).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 4,
  })
}

export function blocksToTime(blocks: number): string {
  const minutes = Math.abs(blocks) * 2
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)
  const months = Math.floor(days / 30)

  if (months > 0) return `${months}mo ${days % 30}d`
  if (days > 0) return `${days}d ${hours % 24}h`
  if (hours > 0) return `${hours}h ${minutes % 60}m`
  return `${minutes}m`
}

export function formatUnlockStatus(blocksRemaining: number): string {
  if (blocksRemaining <= 0) return 'Unlockable'
  return `${blocksToTime(blocksRemaining)} remaining`
}

export function truncateAddress(addr: string, chars = 8): string {
  if (addr.length <= chars * 2 + 3) return addr
  return `${addr.slice(0, chars)}...${addr.slice(-chars)}`
}

/** Calculate 3% fee for display purposes */
export function calculateFeePreview(ergValue: number): number {
  if (ergValue <= 100_000) return 0
  const fee = Math.floor((ergValue * 3000) / 100_000)
  const maxFee = Math.floor(ergValue / 10)
  return Math.min(fee, maxFee)
}
