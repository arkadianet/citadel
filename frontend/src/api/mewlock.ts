/**
 * MewLock Timelock Protocol API
 *
 * TypeScript types and invoke wrappers for MewLock Tauri commands.
 */

import { invoke } from '@tauri-apps/api/core'


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
  boxId: string,
  userErgoTree: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<object> {
  return await invoke<object>('mewlock_build_unlock', {
    boxId,
    userErgoTree,
    userUtxos,
    currentHeight,
  })
}

// =============================================================================
// Protocol-specific Helpers
// =============================================================================

import { blocksToTime } from '../utils/format'

export function formatUnlockStatus(blocksRemaining: number): string {
  if (blocksRemaining <= 0) return 'Unlockable'
  return `${blocksToTime(blocksRemaining)} remaining`
}

/** Calculate 3% fee for display purposes */
export function calculateFeePreview(ergValue: number): number {
  if (ergValue <= 100_000) return 0
  const fee = Math.floor((ergValue * 3000) / 100_000)
  const maxFee = Math.floor(ergValue / 10)
  return Math.min(fee, maxFee)
}
