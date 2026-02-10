/**
 * tokenCache — In-memory cache for token metadata.
 *
 * Deduplicates concurrent requests for the same token ID.
 */

import { getToken, type TokenInfo } from './explorer'

const cache = new Map<string, TokenInfo>()
const inflight = new Map<string, Promise<TokenInfo>>()

/** Get token info, using cache or fetching from node. */
export async function getCachedTokenInfo(tokenId: string): Promise<TokenInfo> {
  const cached = cache.get(tokenId)
  if (cached) return cached

  // Deduplicate concurrent requests
  let pending = inflight.get(tokenId)
  if (!pending) {
    pending = getToken(tokenId).then(info => {
      cache.set(tokenId, info)
      inflight.delete(tokenId)
      return info
    }).catch(err => {
      inflight.delete(tokenId)
      throw err
    })
    inflight.set(tokenId, pending)
  }
  return pending
}

/** Synchronous name lookup — returns cached name or null if not yet fetched. */
export function getCachedTokenName(tokenId: string): string | null {
  return cache.get(tokenId)?.name ?? null
}
