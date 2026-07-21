/**
 * UTXO Management API
 *
 * TypeScript types and invoke wrappers for consolidate/split/restructure Tauri commands.
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
  citadelFeeNano: number
}

export interface SplitBuildResponse {
  unsignedTx: object
  splitCount: number
  amountPerBox: string
  totalSplit: string
  changeErg: number
  minerFee: number
  citadelFeeNano: number
}

export interface RestructureTokenSpec {
  tokenId: string
  /** Raw (on-chain) amount as decimal string */
  amount: string
}

export interface RestructureOutputSpec {
  /** nanoERG */
  value: number
  tokens: RestructureTokenSpec[]
}

export interface RestructureBuildResponse {
  unsignedTx: object
  inputCount: number
  outputCount: number
  totalErgIn: number
  allocatedErg: number
  changeErg: number
  hasChange: boolean
  minerFee: number
  citadelFeeNano: number
}

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

export async function buildRestructureTx(
  selectedUtxos: object[],
  outputs: RestructureOutputSpec[],
  userErgoTree: string,
  currentHeight: number,
): Promise<RestructureBuildResponse> {
  return await invoke<RestructureBuildResponse>('build_restructure_tx', {
    selectedUtxos,
    outputs,
    userErgoTree,
    currentHeight,
  })
}

export async function startUtxoMgmtSign(
  unsignedTx: object,
  message: string,
): Promise<SignResponse> {
  return await invoke<SignResponse>('start_utxo_mgmt_sign', {
    unsignedTx,
    message,
  })
}

export async function getUtxoMgmtTxStatus(requestId: string): Promise<TxStatusResponse> {
  return await invoke<TxStatusResponse>('get_utxo_mgmt_tx_status', {
    requestId,
  })
}
