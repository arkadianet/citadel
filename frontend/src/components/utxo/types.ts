/** Shared UTXO management presentational types. */

export interface UtxoBox {
  boxId: string
  value: string
  ergoTree: string
  assets: Array<{ tokenId: string; amount: string }>
  creationHeight: number
  transactionId: string
  index: number
  additionalRegisters: Record<string, string>
  extension: Record<string, string>
}

export type WalletToken = {
  token_id: string
  amount: number
  name: string | null
  decimals: number
}

export type PillKind = 'dust' | 'large' | 'token' | 'nft'
export type SplitType = 'erg' | 'token'
export type BoardFilter = 'all' | 'dust' | 'erg' | 'tokens' | 'nfts' | 'large'
export type SortKey = 'value-desc' | 'value-asc' | 'height-desc' | 'height-asc'

export interface TokenBreakdownItem {
  tokenId: string
  amount: number
  decimals: number
  name: string
}
