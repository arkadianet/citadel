import { invoke } from '@tauri-apps/api/core'

export interface ProtocolInteraction {
  tx_id: string
  height: number
  timestamp: number
  protocol: string
  operation: string
  token: string
  erg_change_nano: number
  token_amount_change: number
}

export async function getProtocolActivity(count: number = 5): Promise<ProtocolInteraction[]> {
  return invoke<ProtocolInteraction[]>('get_protocol_activity', { count })
}

export async function getDexyActivity(count: number = 10): Promise<ProtocolInteraction[]> {
  return invoke<ProtocolInteraction[]>('get_dexy_activity', { count })
}

export async function getSigmaUsdActivity(count: number = 10): Promise<ProtocolInteraction[]> {
  return invoke<ProtocolInteraction[]>('get_sigmausd_activity', { count })
}
