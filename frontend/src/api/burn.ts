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
  burnedAmount: number
  minerFee: number
  changeErg: number
}

// Multi-burn types
export interface BurnItemInput {
  tokenId: string
  amount: number
}

export interface BurnedTokenEntry {
  tokenId: string
  amount: number
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
  burnAmount: number,
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

