import { TX_FEE_NANO } from '../../constants'
import { formatErg, formatTokenAmount, truncateAddress } from '../../utils/format'
import { truncBoxId } from './helpers'
import type { TokenBreakdownItem } from './types'
import type { CSSProperties } from 'react'

export interface UtxoConsolidatePanelProps {
  selectedCount: number
  selectedErg: number
  ergShare: number
  tokenShare: number
  nftShare: number
  donutStyle: CSSProperties
  tokenBreakdown: TokenBreakdownItem[]
  walletAddress: string
  error: string | null
  onPreview: () => void
}

export function UtxoConsolidatePanel({
  selectedCount,
  selectedErg,
  ergShare,
  tokenShare,
  nftShare,
  donutStyle,
  tokenBreakdown,
  walletAddress,
  error,
  onPreview,
}: UtxoConsolidatePanelProps) {
  return (
    <>
      <section className="utxo-side-section">
        <h3 className="utxo-side-title">Consolidation Preview</h3>
        <div className="utxo-donut-wrap">
          <div className="utxo-donut" style={donutStyle}>
            <div className="utxo-donut-hole">
              <span className="utxo-donut-value mono">
                {selectedCount > 0 ? formatErg(selectedErg, 2, 4) : '0'}
              </span>
              <span className="utxo-donut-unit">ERG</span>
            </div>
          </div>
          <ul className="utxo-donut-legend">
            <li><i className="utxo-swatch erg" /> ERG ({ergShare})</li>
            <li><i className="utxo-swatch token" /> Tokens ({tokenShare})</li>
            <li><i className="utxo-swatch nft" /> NFTs ({nftShare})</li>
          </ul>
        </div>
        <div className="utxo-info-box utxo-why-box">
          <p>
            Why consolidate? Fewer boxes means simpler coin selection and lower chance of
            needing many inputs on the next spend.
          </p>
        </div>
      </section>

      <section className="utxo-side-section">
        <h3 className="utxo-side-title">Token Breakdown (Selected)</h3>
        {tokenBreakdown.length === 0 ? (
          <p className="utxo-side-empty">No tokens in selection</p>
        ) : (
          <ul className="utxo-token-list">
            {tokenBreakdown.map(t => (
              <li key={t.tokenId}>
                <span className="utxo-token-avatar" aria-hidden>
                  {t.name.slice(0, 1).toUpperCase()}
                </span>
                <div className="utxo-token-meta">
                  <span className="utxo-token-name">{t.name}</span>
                  <span className="utxo-token-id mono">{truncBoxId(t.tokenId)}</span>
                </div>
                <span className="utxo-token-amt mono">
                  {formatTokenAmount(t.amount, t.decimals)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="utxo-side-section">
        <h3 className="utxo-side-title">Consolidation Settings</h3>
        <div className="utxo-settings">
          <div className="utxo-setting-row">
            <span>Result</span>
            <span className="mono">1 UTXO</span>
          </div>
          <div className="utxo-setting-row">
            <span>Network Fee</span>
            <span className="mono">{formatErg(TX_FEE_NANO)} ERG (fixed)</span>
          </div>
          <div className="utxo-setting-row">
            <span>Change Address</span>
            <span className="mono utxo-setting-addr" title={walletAddress}>
              {truncateAddress(walletAddress, 6)}
            </span>
          </div>
        </div>
      </section>

      {error && <div className="message error">{error}</div>}

      <button
        type="button"
        className="utxo-submit-btn"
        onClick={onPreview}
        disabled={selectedCount < 2}
      >
        {selectedCount < 2
          ? 'Select ≥2 boxes'
          : 'Preview Consolidation →'}
      </button>
      <p className="utxo-step-hint">Step 1 of 3</p>
    </>
  )
}
