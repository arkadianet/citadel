import { invoke } from '@tauri-apps/api/core'

export interface WatchedItem {
  id: string
  tx_id: string
  protocol: string
  operation: string
  description: string
  kind: 'tx' | 'order'
  elapsed_secs: number
}

export interface TxNotification {
  id: string
  kind: 'confirmed' | 'filled' | 'dropped' | 'timeout'
  protocol: string
  operation: string
  description: string
  tx_id: string | null
  timestamp: number
}

export async function watchTx(
  txId: string,
  protocol: string,
  operation: string,
  description: string,
): Promise<string> {
  return await invoke<string>('watch_tx', { txId, protocol, operation, description })
}

export async function watchOrder(
  boxId: string,
  txId: string,
  protocol: string,
  description: string,
): Promise<string> {
  return await invoke<string>('watch_order', { boxId, txId, protocol, description })
}

export async function getWatchedItems(): Promise<WatchedItem[]> {
  return await invoke<WatchedItem[]>('get_watched_items')
}
