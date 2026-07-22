import { isNftLikeToken } from '../../utils/eip4'
import type { BoardFilter, PillKind, UtxoBox, WalletToken } from './types'

/** Dust < 1 ERG */
export const DUST_NANO = 1_000_000_000
/** Large > 10 ERG */
export const LARGE_NANO = 10_000_000_000

export function ergNano(box: UtxoBox): number {
  return parseInt(box.value || '0', 10) || 0
}

export function truncBoxId(id: string): string {
  if (id.length <= 12) return id
  return `${id.slice(0, 4)}…${id.slice(-4)}`
}

export function formatFiat(nano: number, ergUsd: number | undefined): string | null {
  if (!ergUsd || ergUsd <= 0) return null
  const usd = (nano / 1e9) * ergUsd
  return `$${usd.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`
}

export function boxHasNft(box: UtxoBox, tokens: WalletToken[]): boolean {
  return box.assets.some(a => {
    const amt = parseInt(a.amount, 10) || 0
    const wt = tokens.find(t => t.token_id === a.tokenId)
    if (wt) {
      return isNftLikeToken({ amount: amt, decimals: wt.decimals })
    }
    return amt === 1
  })
}

export function boxHasFungible(box: UtxoBox, tokens: WalletToken[]): boolean {
  return box.assets.some(a => {
    const amt = parseInt(a.amount, 10) || 0
    const wt = tokens.find(t => t.token_id === a.tokenId)
    if (wt) return !isNftLikeToken({ amount: amt, decimals: wt.decimals })
    return amt !== 1
  })
}

export function boxPills(box: UtxoBox, tokens: WalletToken[]): PillKind[] {
  const pills: PillKind[] = []
  const nano = ergNano(box)
  if (boxHasNft(box, tokens)) pills.push('nft')
  else if (box.assets.length > 0) pills.push('token')
  if (nano < DUST_NANO) pills.push('dust')
  if (nano > LARGE_NANO) pills.push('large')
  return pills
}

export function matchesFilter(box: UtxoBox, filter: BoardFilter, tokens: WalletToken[]): boolean {
  const nano = ergNano(box)
  switch (filter) {
    case 'all':
      return true
    case 'dust':
      return nano < DUST_NANO
    case 'erg':
      return box.assets.length === 0
    case 'tokens':
      return boxHasFungible(box, tokens)
    case 'nfts':
      return boxHasNft(box, tokens)
    case 'large':
      return nano > LARGE_NANO
    default:
      return true
  }
}
