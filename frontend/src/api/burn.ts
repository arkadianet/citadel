/**
 * Token Burn API
 *
 * TypeScript types and invoke wrappers for token burn Tauri commands.
 */

import { invoke } from '@tauri-apps/api/core'


// =============================================================================
// Type Definitions
// =============================================================================

export interface BurnBuildResponse {
  unsignedTx: object
  burnedTokenId: string
  /// String-form precise amount (see note on BurnItemInput.amount).
  burnedAmount: string
  minerFee: number
  changeErg: number
}

// Multi-burn types
export interface BurnItemInput {
  tokenId: string
  /// Raw integer amount as a decimal string. Use a string (not number) so
  /// LP-size values above 2^53 − 1 survive the JS→JSON round-trip.
  amount: string
}

export interface BurnedTokenEntry {
  tokenId: string
  /// String-form precise amount (see `BurnItemInput.amount`).
  amount: string
}

export interface MultiBurnBuildResponse {
  unsignedTx: object
  burnedTokens: BurnedTokenEntry[]
  minerFee: number
  changeErg: number
}


// =============================================================================
// API Functions
// =============================================================================

export async function buildBurnTx(
  tokenId: string,
  /// Raw integer amount as a decimal string (see note on BurnItemInput.amount).
  burnAmount: string,
  userErgoTree: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<BurnBuildResponse> {
  return await invoke<BurnBuildResponse>('build_burn_tx', {
    tokenId,
    burnAmount,
    userErgoTree,
    userUtxos,
    currentHeight,
  })
}

export async function buildMultiBurnTx(
  burnItems: BurnItemInput[],
  userErgoTree: string,
  userUtxos: object[],
  currentHeight: number,
): Promise<MultiBurnBuildResponse> {
  return await invoke<MultiBurnBuildResponse>('build_multi_burn_tx', {
    burnItems,
    userErgoTree,
    userUtxos,
    currentHeight,
  })
}

