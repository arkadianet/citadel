/**
 * Wallet API — balance, activity, and simple send (ErgoPay/Nautilus).
 */

import { invoke } from '@tauri-apps/api/core'
import type { SignResponse, TxStatusResponse } from './types'
import { startSign, getTxStatus } from './types'

export interface TokenBalance {
  token_id: string
  amount: number
  amount_str?: string
  name: string | null
  decimals: number
  pending_amount?: number
}

export interface WalletBalance {
  address: string
  addresses?: string[]
  erg_nano: number
  erg_formatted: string
  sigusd_amount: number
  sigusd_formatted: string
  sigrsv_amount: number
  tokens: TokenBalance[]
  pending_erg_nano?: number
}

export interface TokenChange {
  token_id: string
  amount: number
  name: string | null
  decimals: number
}

export interface RecentTx {
  tx_id: string
  inclusion_height: number
  num_confirmations: number
  timestamp: number
  erg_change_nano: number
  token_changes: TokenChange[]
}

export interface SendBuildResponse {
  unsignedTx: object
  recipientErg: number
  tokenId: string | null
  tokenAmount: string | null
  changeErg: number
  minerFee: number
  inputCount: number
}

export async function getWalletBalance(): Promise<WalletBalance> {
  return invoke<WalletBalance>('get_wallet_balance')
}

export async function getRecentTransactions(limit: number): Promise<{ transactions: RecentTx[] }> {
  return invoke('get_recent_transactions', { limit })
}

export async function validateErgoAddress(address: string): Promise<string> {
  return invoke<string>('validate_ergo_address', { address })
}

export async function buildSendTx(params: {
  recipientAddress: string
  changeAddress: string
  /** nanoERG as decimal string */
  ergNano: string
  tokenId?: string
  /** raw token amount as decimal string */
  tokenAmount?: string
  userUtxos: object[]
  currentHeight: number
}): Promise<SendBuildResponse> {
  return invoke<SendBuildResponse>('build_send_tx', {
    recipientAddress: params.recipientAddress,
    changeAddress: params.changeAddress,
    ergNano: params.ergNano,
    tokenId: params.tokenId ?? null,
    tokenAmount: params.tokenAmount ?? null,
    userUtxos: params.userUtxos,
    currentHeight: params.currentHeight,
  })
}

export { startSign, getTxStatus }
export type { SignResponse, TxStatusResponse }
