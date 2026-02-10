/**
 * Explorer API — Tauri invoke wrappers for blockchain explorer functionality.
 *
 * Each function maps to an `explorer_*` Tauri command that queries the
 * connected Ergo node directly.
 */

import { invoke } from '@tauri-apps/api/core'

// =============================================================================
// Types
// =============================================================================

/** Full node info from /info endpoint */
export interface NodeInfo {
  name: string
  appVersion: string
  fullHeight: number
  headersHeight: number
  maxPeerHeight: number
  bestFullHeaderId: string
  bestHeaderId: string
  stateRoot: string
  stateType: string
  stateVersion: number
  isMining: boolean
  peersCount: number
  unconfirmedCount: number
  difficulty: number
  currentTime: number
  launchTime: number
  parameters: Record<string, unknown>
  network: string
  [key: string]: unknown
}

/** Block header as returned by the node */
export interface BlockHeader {
  id: string
  timestamp: number
  version: number
  adProofsRoot: string
  stateRoot: string
  transactionsRoot: string
  nBits: number
  extensionHash: string
  powSolutions: Record<string, unknown>
  height: number
  difficulty: string
  parentId: string
  votes: string
  size?: number
  extensionId?: string
  transactionsId?: string
  adProofsId?: string
  [key: string]: unknown
}

/** Full block (header + transactions) */
export interface Block {
  header: BlockHeader
  blockTransactions: {
    headerId: string
    transactions: Transaction[]
  }
  adProofs?: unknown
  extension: unknown
  size: number
  [key: string]: unknown
}

/** Transaction as returned by the node */
export interface Transaction {
  id: string
  inputs: TxInput[]
  dataInputs: DataInput[]
  outputs: TxOutput[]
  size: number
  inclusionHeight?: number
  numConfirmations?: number
  blockId?: string
  timestamp?: number
  [key: string]: unknown
}

/** Transaction input */
export interface TxInput {
  boxId: string
  spendingProof: {
    proofBytes: string
    extension: Record<string, string>
  }
  value?: number
  assets?: TokenAmount[]
  ergoTree?: string
  address?: string
  [key: string]: unknown
}

/** Data input (read-only reference) */
export interface DataInput {
  boxId: string
}

/** Transaction output (box) */
export interface TxOutput {
  boxId: string
  value: number
  ergoTree: string
  creationHeight: number
  assets: TokenAmount[]
  additionalRegisters: Record<string, string>
  transactionId: string
  index: number
  address?: string
  [key: string]: unknown
}

/** Token amount in a box */
export interface TokenAmount {
  tokenId: string
  amount: number
  name?: string
  decimals?: number
}

/** Box from blockchain (includes spent status) */
export interface BoxData {
  boxId: string
  value: number
  ergoTree: string
  creationHeight: number
  assets: TokenAmount[]
  additionalRegisters: Record<string, string>
  transactionId: string
  index: number
  spentTransactionId: string | null
  address?: string
  [key: string]: unknown
}

/** Token metadata */
export interface TokenInfo {
  id: string
  boxId: string
  emissionAmount: number
  name: string | null
  description: string | null
  decimals: number | null
  type?: string
  [key: string]: unknown
}

/** Address info with balance and transactions */
export interface AddressInfo {
  address: string
  balance: {
    nanoErgs: number
    tokens: { tokenId: string; amount: number }[]
  }
  transactions: Transaction[]
  totalTransactions: number
  offset: number
  limit: number
  unconfirmedBalance?: number
  unconfirmedTransactions?: Transaction[]
}

/** Search result */
export interface SearchResult {
  type: 'address' | 'transaction' | 'token' | 'block'
  id: string
  height?: number
  unconfirmed?: boolean
}

// =============================================================================
// API Functions
// =============================================================================

/** Get full node info */
export async function getNodeInfo(): Promise<NodeInfo> {
  return await invoke<NodeInfo>('explorer_node_info')
}

/** Get a transaction by ID (checks confirmed then mempool) */
export async function getTransaction(txId: string): Promise<Transaction> {
  return await invoke<Transaction>('explorer_get_transaction', { txId })
}

/** Get a full block by header ID or height */
export async function getBlock(blockId: string): Promise<Block> {
  return await invoke<Block>('explorer_get_block', { blockId })
}

/** Get the most recent block headers */
export async function getBlockHeaders(count: number = 50): Promise<BlockHeader[]> {
  const result = await invoke<BlockHeader[]>('explorer_get_block_headers', { count })
  return result
}

/** Get unconfirmed transactions from the mempool */
export async function getMempool(): Promise<Transaction[]> {
  return await invoke<Transaction[]>('explorer_get_mempool')
}

/** Get a box by ID (full blockchain data including spent status) */
export async function getBox(boxId: string): Promise<BoxData> {
  return await invoke<BoxData>('explorer_get_box', { boxId })
}

/** Get token metadata by ID */
export async function getToken(tokenId: string): Promise<TokenInfo> {
  return await invoke<TokenInfo>('explorer_get_token', { tokenId })
}

/** Get address info with balance and paginated transactions */
export async function getAddress(
  address: string,
  offset: number = 0,
  limit: number = 20,
): Promise<AddressInfo> {
  return await invoke<AddressInfo>('explorer_get_address', { address, offset, limit })
}

/** Universal search — identify what kind of entity a query refers to */
export async function search(query: string): Promise<SearchResult> {
  return await invoke<SearchResult>('explorer_search', { query })
}

// =============================================================================
// Formatting Helpers
// =============================================================================

/** Format nanoERGs to ERG with appropriate precision */
export function formatErg(nanoErgs: number, decimals: number = 4): string {
  return (nanoErgs / 1e9).toLocaleString(undefined, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  })
}

/** Format a timestamp (ms) to relative time */
export function formatTimeAgo(timestampMs: number): string {
  const seconds = Math.floor((Date.now() - timestampMs) / 1000)
  if (seconds < 60) return `${seconds}s ago`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}

/** Truncate a hash for display */
export function truncateHash(hash: string, start: number = 8, end: number = 6): string {
  if (hash.length <= start + end + 3) return hash
  return `${hash.slice(0, start)}...${hash.slice(-end)}`
}

/** Format difficulty to human-readable string */
export function formatDifficulty(difficulty: number | string | undefined | null): string {
  if (difficulty == null) return '-'
  const d = typeof difficulty === 'string' ? parseFloat(difficulty) : difficulty
  if (isNaN(d)) return '-'
  if (d >= 1e15) return `${(d / 1e15).toFixed(2)} PH`
  if (d >= 1e12) return `${(d / 1e12).toFixed(2)} TH`
  if (d >= 1e9) return `${(d / 1e9).toFixed(2)} GH`
  if (d >= 1e6) return `${(d / 1e6).toFixed(2)} MH`
  return d.toLocaleString()
}

/** Calculate transaction fee (input sum - output sum) */
export function calcFee(tx: Transaction): number {
  const inputSum = tx.inputs.reduce((s, i) => s + ((i as Record<string, unknown>).value as number ?? 0), 0)
  const outputSum = tx.outputs.reduce((s, o) => s + o.value, 0)
  return Math.max(0, inputSum - outputSum)
}

/** Format bytes to human-readable size */
export function formatSize(bytes: number | undefined | null): string {
  if (bytes == null) return '-'
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}
