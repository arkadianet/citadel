/**
 * Node Discovery API
 *
 * TypeScript types and invoke wrappers for node discovery Tauri commands.
 */

import { invoke } from '@tauri-apps/api/core'

export interface NodeProbeResult {
  url: string
  name: string | null
  chain_height: number
  capability_tier: string
  latency_ms: number
}

/** Discover and probe available nodes (hardcoded + peers). */
export function discoverNodes(): Promise<NodeProbeResult[]> {
  return invoke<NodeProbeResult[]>('discover_nodes')
}

/** Probe a single node URL for capability info. */
export function probeSingleNode(url: string): Promise<NodeProbeResult | null> {
  return invoke<NodeProbeResult | null>('probe_single_node', { url })
}
