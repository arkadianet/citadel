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

// =============================================================================
// Arb chain execution (pre-built 0-conf sequential legs, Nautilus sign-only)
// =============================================================================

export interface ArbChainLegSummary {
  input_amount: number
  input_token: string
  output_amount: number
  min_output: number
  output_token: string
  miner_fee: number
  total_erg_cost: number
}

export interface ArbChainLeg {
  poolId: string
  txId: string
  unsignedTx: object
  summary: ArbChainLegSummary
}

export interface ArbChainBuild {
  legs: ArbChainLeg[]
  projectedProfitNano: number
}

/** Pre-build every leg of an arb chain from a fresh pool snapshot. */
export async function buildArbChain(
  poolIds: string[],
  inputNano: number,
  userUtxos: object[],
  currentHeight: number,
  minProfitNano?: number,
): Promise<ArbChainBuild> {
  return await invoke<ArbChainBuild>('build_arb_chain_tx', {
    poolIds,
    inputNano,
    userUtxos,
    currentHeight,
    minProfitNano,
  })
}

export interface ArbLegSignResponse {
  requestId: string
  nautilusUrl: string
}

/** Start a sign-only Nautilus request for one leg (no broadcast on sign). */
export async function startArbLegSign(
  unsignedTx: object,
  message: string,
): Promise<ArbLegSignResponse> {
  return await invoke<ArbLegSignResponse>('start_arb_leg_sign', {
    unsignedTx,
    message,
  })
}

export interface ArbChainSubmitResponse {
  txIds: string[]
  failedLeg: number | null
  error: string | null
}

/** Broadcast all signed legs in order; stops at the first rejection. */
export async function submitArbChain(
  requestIds: string[],
): Promise<ArbChainSubmitResponse> {
  return await invoke<ArbChainSubmitResponse>('submit_arb_chain', {
    requestIds,
  })
}
