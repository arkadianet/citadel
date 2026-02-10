/**
 * UTXO Management API
 *
 * TypeScript types and invoke wrappers for consolidate/split Tauri commands.
 */

import { invoke } from '@tauri-apps/api/core'

import type { SignResponse, TxStatusResponse } from './types'

// =============================================================================
// Type Definitions
// =============================================================================

export interface ConsolidateBuildResponse {
  unsignedTx: object
  inputCount: number
  totalErgIn: number
  changeErg: number
  tokenCount: number
  minerFee: number
}

export interface SplitBuildResponse {
  unsignedTx: object
  splitCount: number
  amountPerBox: string
  totalSplit: string
  changeErg: number
  minerFee: number
}

export type UtxoSignResponse = SignResponse
export type UtxoTxStatusResponse = TxStatusResponse

// =============================================================================
// API Functions
// =============================================================================

export async function buildConsolidateTx(
  selectedUtxos: object[],
  userErgoTree: string,
  currentHeight: number,
): Promise<ConsolidateBuildResponse> {
  return await invoke<ConsolidateBuildResponse>('build_consolidate_tx', {
    selectedUtxos,
    userErgoTree,
    currentHeight,
  })
}

export async function buildSplitTx(
  userUtxos: object[],
  userErgoTree: string,
  currentHeight: number,
  splitMode: 'erg' | 'token',
  amountPerBox: string,
  count: number,
  tokenId?: string,
  ergPerBox?: number,
): Promise<SplitBuildResponse> {
  return await invoke<SplitBuildResponse>('build_split_tx', {
    userUtxos,
    userErgoTree,
    currentHeight,
    splitMode,
    amountPerBox,
    count,
    tokenId,
    ergPerBox,
  })
}

export async function startUtxoMgmtSign(
  unsignedTx: object,
  message: string,
): Promise<UtxoSignResponse> {
  return await invoke<UtxoSignResponse>('start_utxo_mgmt_sign', {
    unsignedTx,
    message,
  })
}

export async function getUtxoMgmtTxStatus(requestId: string): Promise<UtxoTxStatusResponse> {
  return await invoke<UtxoTxStatusResponse>('get_utxo_mgmt_tx_status', {
    requestId,
  })
}
