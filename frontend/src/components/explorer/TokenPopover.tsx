/**
 * TokenPopover — Shows token name with a hover popover for full details.
 *
 * Resolves token name async from cache. Shows truncated ID as fallback
 * until name loads.
 */

import { useState, useEffect } from 'react'
import { truncateHash } from '../../api/explorer'
import { getCachedTokenInfo, getCachedTokenName } from '../../api/tokenCache'
import type { TokenInfo } from '../../api/explorer'
import type { ExplorerRoute } from '../ExplorerTab'

interface Props {
  tokenId: string
  amount: number
  onNavigate: (route: ExplorerRoute) => void
}

export function TokenPopover({ tokenId, amount, onNavigate }: Props) {
  const [info, setInfo] = useState<TokenInfo | null>(null)

  // Try sync cache first, then fetch async
  const syncName = getCachedTokenName(tokenId)

  useEffect(() => {
    getCachedTokenInfo(tokenId)
      .then(setInfo)
      .catch(() => {})
  }, [tokenId])

  const displayName = info?.name || syncName || truncateHash(tokenId, 6, 4)
  const decimals = info?.decimals ?? 0
  const formattedAmount = decimals > 0
    ? (amount / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
    : amount.toLocaleString()

  return (
    <span className="token-popover-wrap">
      <span
        className="box-card-token text-link"
        onClick={(e) => { e.stopPropagation(); onNavigate({ page: 'token', id: tokenId }) }}
      >
        {displayName}: {formattedAmount}
      </span>
      <span className="token-popover">
        {info?.name && (
          <span className="token-popover-name">{info.name}</span>
        )}
        <span className="token-popover-id text-mono">{truncateHash(tokenId, 10, 8)}</span>
        <span className="token-popover-amount">Amount: {formattedAmount}</span>
        <span
          className="token-popover-link text-link"
          onClick={(e) => { e.stopPropagation(); onNavigate({ page: 'token', id: tokenId }) }}
        >
          View details
        </span>
      </span>
    </span>
  )
}
