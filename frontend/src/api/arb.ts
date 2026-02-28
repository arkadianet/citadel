/**
 * Circular Arb Scanner API
 *
 * TypeScript types and invoke wrappers for circular arbitrage detection.
 */

import { invoke } from '@tauri-apps/api/core'
import type { Route } from './router'

export interface CircularArb {
  path_label: string
  hops: number
  pool_ids: string[]
  optimal_input_nano: number
  output_nano: number
  gross_profit_nano: number
  tx_fee_nano: number
  net_profit_nano: number
  profit_pct: number
  price_impact: number
  route: Route
}

export interface CircularArbSnapshot {
  windows: CircularArb[]
  total_net_profit_nano: number
  scan_time_ms: number
}

/**
 * Scan for profitable circular arb loops (ERG → ... → ERG).
 */
export async function scanCircularArbs(
  maxHops?: number,
): Promise<CircularArbSnapshot> {
  return await invoke<CircularArbSnapshot>('scan_circular_arbs', {
    maxHops,
  })
}
