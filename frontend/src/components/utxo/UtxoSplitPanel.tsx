import { DEV_FEE_NANO, TX_FEE_NANO } from '../../constants'
import { formatErg, formatTokenAmount } from '../../utils/format'
import { ergNano } from './helpers'
import type { SplitType, UtxoBox, WalletToken } from './types'

export interface UtxoSplitPanelProps {
  selectedSplitBox: UtxoBox | null
  splitType: SplitType
  splitAmount: string
  splitCount: string
  splitCountNum: number
  splitTokenId: string
  splitErgPerBox: string
  splitErgPerBoxNano: number
  splitAmountNano: number
  splitSourceTokens: WalletToken[]
  splitTotalDisplay: string
  splitIsValid: boolean
  error: string | null
  getTokenName: (tokenId: string, fallback: string | null) => string
  onSplitTypeChange: (type: SplitType) => void
  onSplitAmountChange: (v: string) => void
  onSplitCountChange: (v: string) => void
  onSplitTokenIdChange: (v: string) => void
  onSplitErgPerBoxChange: (v: string) => void
  onPreview: () => void
}

export function UtxoSplitPanel({
  selectedSplitBox,
  splitType,
  splitAmount,
  splitCount,
  splitCountNum,
  splitTokenId,
  splitErgPerBox,
  splitErgPerBoxNano,
  splitAmountNano,
  splitSourceTokens,
  splitTotalDisplay,
  splitIsValid,
  error,
  getTokenName,
  onSplitTypeChange,
  onSplitAmountChange,
  onSplitCountChange,
  onSplitTokenIdChange,
  onSplitErgPerBoxChange,
  onPreview,
}: UtxoSplitPanelProps) {
  if (!selectedSplitBox) {
    return (
      <div className="utxo-split-empty">
        <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
          <rect x="3" y="3" width="7" height="7" />
          <rect x="14" y="3" width="7" height="7" />
          <rect x="3" y="14" width="7" height="7" />
          <rect x="14" y="14" width="7" height="7" />
        </svg>
        <p className="utxo-split-empty-title">Select a box to split</p>
        <p className="utxo-split-empty-hint">Click one UTXO card on the board</p>
      </div>
    )
  }

  return (
    <>
      <div className="utxo-focus-card utxo-split-source-card">
        <code className="mono utxo-focus-id">
          {selectedSplitBox.boxId.slice(0, 10)}…{selectedSplitBox.boxId.slice(-8)}
        </code>
        <span className="mono">{formatErg(ergNano(selectedSplitBox))} ERG</span>
        {selectedSplitBox.assets.length > 0 && (
          <span className="utxo-focus-tokens">
            {selectedSplitBox.assets.length} token type
            {selectedSplitBox.assets.length !== 1 ? 's' : ''}
          </span>
        )}
      </div>

      <div className="utxo-split-type-toggle">
        <button
          type="button"
          className={`utxo-split-type-btn ${splitType === 'erg' ? 'active' : ''}`}
          onClick={() => onSplitTypeChange('erg')}
        >
          Split ERG
        </button>
        <button
          type="button"
          className={`utxo-split-type-btn ${splitType === 'token' ? 'active' : ''}`}
          onClick={() => onSplitTypeChange('token')}
          disabled={splitSourceTokens.length === 0}
        >
          Split Token
        </button>
      </div>

      <div className="utxo-split-form">
        {splitType === 'token' && (
          <div className="utxo-split-field">
            <label>Token</label>
            <select
              value={splitTokenId}
              onChange={e => onSplitTokenIdChange(e.target.value)}
              className="utxo-split-select"
            >
              <option value="">Select token...</option>
              {splitSourceTokens.map(t => (
                <option key={t.token_id} value={t.token_id}>
                  {getTokenName(t.token_id, t.name)} ({formatTokenAmount(t.amount, t.decimals)})
                </option>
              ))}
            </select>
          </div>
        )}

        <div className="utxo-split-field">
          <label>{splitType === 'erg' ? 'ERG per box' : 'Tokens per box'}</label>
          <input
            type="text"
            inputMode="decimal"
            value={splitAmount}
            onChange={e => onSplitAmountChange(e.target.value)}
            placeholder={splitType === 'erg' ? '1.0' : '100'}
            className="utxo-split-input"
          />
        </div>

        <div className="utxo-split-field">
          <label>Number of boxes (1–30)</label>
          <div className="utxo-split-alloc">
            <input
              type="range"
              min={1}
              max={30}
              value={Math.min(30, Math.max(1, splitCountNum || 1))}
              onChange={e => onSplitCountChange(e.target.value)}
              className="utxo-split-range"
              aria-label="Split box count"
            />
            <input
              type="text"
              inputMode="numeric"
              value={splitCount}
              onChange={e => onSplitCountChange(e.target.value.replace(/\D/g, ''))}
              placeholder="5"
              className="utxo-split-input utxo-split-count"
            />
          </div>
        </div>

        {splitType === 'token' && (
          <div className="utxo-split-field">
            <label>ERG per box</label>
            <input
              type="text"
              inputMode="decimal"
              value={splitErgPerBox}
              onChange={e => onSplitErgPerBoxChange(e.target.value)}
              placeholder="0.001"
              className="utxo-split-input"
            />
          </div>
        )}

        {splitCountNum > 0 && splitAmount && (
          <div className="utxo-confirm-summary">
            <div className="utxo-confirm-row">
              <span>Total</span>
              <span>{splitTotalDisplay || '—'}</span>
            </div>
            {splitType === 'token' && (
              <div className="utxo-confirm-row">
                <span>ERG locked</span>
                <span>{formatErg(splitErgPerBoxNano * splitCountNum)} ERG</span>
              </div>
            )}
            <div className="utxo-confirm-row">
              <span>Miner Fee</span>
              <span>{formatErg(TX_FEE_NANO)} ERG</span>
            </div>
            <div className="utxo-confirm-row">
              <span>Citadel fee</span>
              <span>{formatErg(DEV_FEE_NANO)} ERG</span>
            </div>
            <p className="utxo-muted">Includes {formatErg(DEV_FEE_NANO)} ERG Citadel fee</p>
          </div>
        )}
      </div>

      <div className="utxo-split-preview utxo-split-preview--compact">
        <div className="utxo-split-flow">
          <div className="utxo-split-stage">
            <div className="utxo-split-preview-label">Before</div>
            <div className="utxo-ghost-tile utxo-ghost-source kind-large">
              <span className="utxo-ghost-caption">source</span>
              <span className="mono utxo-ghost-value">
                {formatErg(ergNano(selectedSplitBox), 0, 2)}
              </span>
              <span className="utxo-ghost-sub">
                {splitCountNum > 0 ? `${splitCountNum}× →` : 'set count'}
              </span>
            </div>
          </div>

          <div className="utxo-split-arrow" aria-hidden>→</div>

          <div className="utxo-split-stage utxo-split-stage-after">
            <div className="utxo-split-preview-label">After · {splitCountNum || 0} boxes</div>
            {splitCountNum > 0 && splitIsValid ? (
              <div className="utxo-split-ghosts">
                {Array.from({ length: Math.min(splitCountNum, 30) }, (_, i) => (
                  <div
                    key={i}
                    className={`utxo-ghost-tile${splitType === 'token' ? ' token' : ''}`}
                    style={{ animationDelay: `${i * 30}ms` }}
                  >
                    <span className="mono">
                      {splitType === 'erg' ? formatErg(splitAmountNano, 0, 2) : splitAmount}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <p className="utxo-split-preview-hint">
                {splitCountNum > 0
                  ? 'Adjust amount to fit this box'
                  : 'Set amount & count to preview'}
              </p>
            )}
          </div>
        </div>
      </div>

      {error && <div className="message error">{error}</div>}

      <button
        type="button"
        className="utxo-submit-btn"
        onClick={onPreview}
        disabled={!splitIsValid}
      >
        {!selectedSplitBox
          ? 'Select a box to split'
          : !splitIsValid
            ? 'Set valid split options'
            : 'Review Split'}
      </button>
    </>
  )
}
