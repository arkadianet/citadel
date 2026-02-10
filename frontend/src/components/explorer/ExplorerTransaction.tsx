import { useState, useEffect } from 'react'
import { getTransaction, formatErg, truncateHash, formatSize, formatTimeAgo, calcFee, type Transaction, type TxOutput } from '../../api/explorer'
import { ExplorerSkeleton } from './ExplorerSkeleton'
import { TxTypeBadge } from './TxTypeBadge'
import { TokenPopover } from './TokenPopover'
import type { ExplorerRoute } from '../ExplorerTab'
import { openExternal } from '../../api/external'

interface Props {
  txId: string
  onNavigate: (route: ExplorerRoute) => void
  explorerUrl: string
}

function BoxCard({ box, label, onNavigate }: {
  box: TxOutput | (Transaction['inputs'][0] & { value?: number; assets?: TxOutput['assets']; address?: string })
  label: string
  onNavigate: (route: ExplorerRoute) => void
}) {
  const value = (box as Record<string, unknown>).value as number | undefined
  const assets = ((box as Record<string, unknown>).assets as TxOutput['assets']) ?? []
  const address = (box as Record<string, unknown>).address as string | undefined
  const boxId = (box as Record<string, unknown>).boxId as string

  return (
    <div className="explorer-box-card">
      <div className="box-card-header">
        <span className="box-card-label">{label}</span>
        <span className="text-mono text-xs">{truncateHash(boxId)}</span>
      </div>
      {address && (
        <div
          className="box-card-address text-link"
          onClick={() => onNavigate({ page: 'address', id: address })}
        >
          {truncateHash(address, 12, 8)}
        </div>
      )}
      <div className="box-card-value">
        {value != null ? `${formatErg(value)} ERG` : '?'}
      </div>
      {assets.length > 0 && (
        <div className="box-card-tokens">
          {assets.slice(0, 3).map(a => (
            <TokenPopover
              key={a.tokenId}
              tokenId={a.tokenId}
              amount={a.amount}
              onNavigate={onNavigate}
            />
          ))}
          {assets.length > 3 && (
            <span className="box-card-token-more">+{assets.length - 3} more</span>
          )}
        </div>
      )}
    </div>
  )
}

export function ExplorerTransaction({ txId, onNavigate, explorerUrl }: Props) {
  const [tx, setTx] = useState<Transaction | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let interval: ReturnType<typeof setInterval> | null = null

    setLoading(true)
    getTransaction(txId)
      .then(t => { setTx(t); setError(null) })
      .catch(e => setError(String(e)))
      .finally(() => setLoading(false))

    // Poll while unconfirmed or recently confirmed (< 10 confirmations)
    interval = setInterval(() => {
      getTransaction(txId)
        .then(t => {
          setTx(t)
          setError(null)
          // Stop polling once well-confirmed
          if ((t.numConfirmations ?? 0) >= 10 && interval) {
            clearInterval(interval)
            interval = null
          }
        })
        .catch(() => {})
    }, 5000)

    return () => { if (interval) clearInterval(interval) }
  }, [txId])

  if (loading) return (
    <div className="explorer-detail">
      <h2 className="explorer-section-title">Transaction</h2>
      <ExplorerSkeleton variant="card" rows={6} />
    </div>
  )
  if (error) return <div className="explorer-error">{error}</div>
  if (!tx || !tx.inputs || !tx.outputs) return <div className="explorer-error">Transaction data unavailable</div>

  const isConfirmed = (tx.numConfirmations ?? 0) > 0
  const fee = calcFee(tx)

  return (
    <div className="explorer-detail">
      <div className="explorer-detail-header">
        <h2 className="explorer-section-title">Transaction</h2>
        <button
          className="link-button text-xs"
          onClick={() => openExternal(`${explorerUrl}/en/transactions/${txId}`)}
        >
          View on external explorer
        </button>
      </div>

      <div className="explorer-info-card">
        <div className="info-row">
          <span className="info-label">TX ID</span>
          <span className="info-value text-mono text-xs">{tx.id}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Type</span>
          <span className="info-value"><TxTypeBadge tx={tx} /></span>
        </div>
        <div className="info-row">
          <span className="info-label">Status</span>
          <span className="info-value">
            {isConfirmed ? (
              <span className={`conf-badge ${
                (tx.numConfirmations ?? 0) >= 10 ? 'conf-badge-green' :
                (tx.numConfirmations ?? 0) >= 1 ? 'conf-badge-amber' :
                'conf-badge-red'
              }`}>
                {tx.numConfirmations} confirmation{tx.numConfirmations !== 1 ? 's' : ''}
              </span>
            ) : (
              <span className="conf-badge conf-badge-red">Unconfirmed</span>
            )}
          </span>
        </div>
        {tx.timestamp && (
          <div className="info-row">
            <span className="info-label">Timestamp</span>
            <span className="info-value">
              {new Date(tx.timestamp).toLocaleString()}
              <span className="text-muted ml-2">({formatTimeAgo(tx.timestamp)})</span>
            </span>
          </div>
        )}
        {tx.inclusionHeight && (
          <div className="info-row">
            <span className="info-label">Block</span>
            <span
              className="info-value text-link"
              onClick={() => tx.blockId && onNavigate({ page: 'block', id: tx.blockId })}
            >
              {tx.inclusionHeight.toLocaleString()}
            </span>
          </div>
        )}
        <div className="info-row">
          <span className="info-label">Size</span>
          <span className="info-value">{formatSize(tx.size)}</span>
        </div>
        <div className="info-row">
          <span className="info-label">Fee</span>
          <span className="info-value">{formatErg(fee)} ERG</span>
        </div>
      </div>

      {/* Inputs and Outputs */}
      <div className="explorer-io-grid">
        <div className="explorer-io-col">
          <h3 className="explorer-subsection-title">Inputs ({tx.inputs.length})</h3>
          {tx.inputs.map((input, i) => (
            <BoxCard key={input.boxId} box={input} label={`Input #${i}`} onNavigate={onNavigate} />
          ))}
        </div>

        <div className="explorer-io-arrow">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M5 12h14M12 5l7 7-7 7" />
          </svg>
        </div>

        <div className="explorer-io-col">
          <h3 className="explorer-subsection-title">Outputs ({tx.outputs.length})</h3>
          {tx.outputs.map((output, i) => (
            <BoxCard key={output.boxId} box={output} label={`Output #${i}`} onNavigate={onNavigate} />
          ))}
        </div>
      </div>
    </div>
  )
}
