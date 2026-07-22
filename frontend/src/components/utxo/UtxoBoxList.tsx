import { formatErg } from '../../utils/format'
import { DUST_NANO, boxPills, ergNano, formatFiat, truncBoxId } from './helpers'
import type { UtxoBox, WalletToken } from './types'

export interface UtxoBoxListProps {
  boxes: UtxoBox[]
  totalCount: number
  loading: boolean
  selectedBoxIds: Set<string>
  tokens: WalletToken[]
  ergUsdPrice?: number
  multiSelect: boolean
  ariaLabel: string
  onSelect: (boxId: string) => void
}

export function UtxoBoxList({
  boxes,
  totalCount,
  loading,
  selectedBoxIds,
  tokens,
  ergUsdPrice,
  multiSelect,
  ariaLabel,
  onSelect,
}: UtxoBoxListProps) {
  return (
    <div
      className="utxo-board-canvas"
      role="listbox"
      aria-multiselectable={multiSelect}
      aria-label={ariaLabel}
    >
      {loading ? (
        <div className="utxo-board-empty">
          <div className="spinner-small" />
          <span>Loading UTXOs…</span>
        </div>
      ) : boxes.length === 0 ? (
        <div className="utxo-board-empty">
          <span>{totalCount === 0 ? 'No UTXOs found' : 'No boxes match this filter'}</span>
        </div>
      ) : (
        <div className="utxo-card-grid">
          {boxes.map((u, i) => {
            const nano = ergNano(u)
            const selected = selectedBoxIds.has(u.boxId)
            const pills = boxPills(u, tokens)
            const fiat = formatFiat(nano, ergUsdPrice)
            return (
              <button
                key={u.boxId}
                type="button"
                role="option"
                aria-selected={selected}
                className={`utxo-card${selected ? ' selected' : ''}`}
                style={{ animationDelay: `${Math.min(i, 24) * 16}ms` }}
                onClick={() => onSelect(u.boxId)}
              >
                <div className="utxo-card-top">
                  <span className={`utxo-card-check${selected ? ' on' : ''}`} aria-hidden>
                    {selected && (
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
                        <polyline points="20 6 9 17 4 12" />
                      </svg>
                    )}
                  </span>
                </div>
                <span className="utxo-card-amount mono">
                  {formatErg(nano, 2, nano < DUST_NANO ? 4 : 4)} ERG
                </span>
                {fiat && <span className="utxo-card-fiat">{fiat}</span>}
                <code className="utxo-card-id mono">{truncBoxId(u.boxId)}</code>
                <span className="utxo-card-height">
                  Block {u.creationHeight.toLocaleString()}
                </span>
                {pills.length > 0 && (
                  <div className="utxo-card-pills">
                    {pills.map(p => (
                      <span key={p} className={`utxo-pill utxo-pill--${p}`}>
                        {p === 'dust' ? 'Dust' : p === 'large' ? 'Large' : p === 'token' ? 'Token' : 'NFT'}
                      </span>
                    ))}
                  </div>
                )}
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
