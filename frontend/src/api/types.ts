/**
 * Shared API Types
 *
 * Common response types used across all protocol API modules.
 */

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
