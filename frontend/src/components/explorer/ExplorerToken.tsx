import { useState, useEffect } from 'react'
import { getToken, truncateHash, type TokenInfo } from '../../api/explorer'
import { ExplorerSkeleton } from './ExplorerSkeleton'
import type { ExplorerRoute } from '../ExplorerTab'

interface Props {
  tokenId: string
  onNavigate: (route: ExplorerRoute) => void
}

/** Map lowercase token names to local icon paths (same as SwapTab) */
const TOKEN_ICON_MAP: Record<string, string> = {
  erg: '/icons/ergo.svg',
  sigusd: '/icons/sigmausd.svg',
  sigrsv: '/icons/sigrsv.svg',
  rsn: '/icons/rosen.svg',
  rsada: '/icons/rsada.svg',
  spf: '/icons/spf.svg',
  rsbtc: '/icons/rsbtc.svg',
  quacks: '/icons/quacks.svg',
  ergopad: '/icons/ergopad.svg',
  neta: '/icons/neta.svg',
  paideia: '/icons/paideia.svg',
  exle: '/icons/exle.svg',
}

function getLocalIcon(name: string | null): string | null {
  if (!name) return null
  return TOKEN_ICON_MAP[name.toLowerCase()] ?? null
}

export function ExplorerToken({ tokenId, onNavigate }: Props) {
  const [token, setToken] = useState<TokenInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setLoading(true)
    getToken(tokenId)
      .then(t => { setToken(t); setError(null) })
      .catch(e => setError(String(e)))
      .finally(() => setLoading(false))
  }, [tokenId])

  if (loading) return (
    <div className="explorer-detail">
      <h2 className="explorer-section-title">Token</h2>
      <ExplorerSkeleton variant="card" rows={5} />
    </div>
  )
  if (error) return <div className="explorer-error">{error}</div>
  if (!token) return null

  const decimals = token.decimals ?? 0
  const emission = token.emissionAmount != null
    ? (token.emissionAmount / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
    : 'Unknown'
  const localIcon = getLocalIcon(token.name)

  return (
    <div className="explorer-detail">
      <h2 className="explorer-section-title">Token</h2>

      <div className="explorer-info-card">
        <div className="info-row">
          <span className="info-label">Name</span>
          <span className="info-value">
            <span className="token-detail-name">
              {localIcon && (
                <img src={localIcon} alt="" className="token-detail-logo" />
              )}
              {token.name || 'Unnamed'}
            </span>
          </span>
        </div>
        <div className="info-row">
          <span className="info-label">Token ID</span>
          <span className="info-value text-mono text-xs">{token.id}</span>
        </div>
        {token.description && (
          <div className="info-row">
            <span className="info-label">Description</span>
            <span className="info-value">{token.description}</span>
          </div>
        )}
        <div className="info-row">
          <span className="info-label">Decimals</span>
          <span className="info-value">{decimals}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Total Supply</span>
          <span className="info-value">{emission}</span>
        </div>
        {token.boxId && (
          <div className="info-row">
            <span className="info-label">Issuing Box</span>
            <span
              className="info-value text-mono text-link"
              onClick={() => onNavigate({ page: 'transaction', id: token.boxId })}
            >
              {truncateHash(token.boxId)}
            </span>
          </div>
        )}
      </div>
    </div>
  )
}
