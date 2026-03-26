/**
 * Shared API Types and Signing Functions
 *
 * Common response types and signing flow used across all protocol API modules.
 */

import { invoke } from '@tauri-apps/api/core'

/** Response from any start_*_sign command */
export interface SignResponse {
  request_id: string
  ergopay_url: string
  nautilus_url: string
}

/** Response from any get_*_tx_status command */
export interface TxStatusResponse {
  status: string
  tx_id: string | null
  error: string | null
}

/** Start ErgoPay signing flow for any unsigned transaction */
export async function startSign(
  unsignedTx: object,
  message?: string,
): Promise<SignResponse> {
  return await invoke<SignResponse>('start_mint_sign', {
    request: { unsigned_tx: unsignedTx, message: message ?? 'Sign transaction' },
  })
}

/** Poll signing status for any pending transaction */
export async function getTxStatus(requestId: string): Promise<TxStatusResponse> {
  return await invoke<TxStatusResponse>('get_mint_tx_status', { requestId })
}
