import React from 'react'
import './DexyPathCard.css'

export interface MintPath {
  name: string
  available: boolean
  reason?: string
  erg_per_token?: number
  tokens_per_erg?: number
  effective_rate?: number
  max_tokens?: number
  remaining_today?: number
  fee_percent: number
  is_best_rate: boolean
}

interface DexyPathCardProps {
  path: MintPath
  selected: boolean
  onSelect: () => void
  tokenName: string
  decimals: number
}

export const DexyPathCard = React.memo(function DexyPathCard({
  path,
  selected,
  onSelect,
  tokenName,
  decimals,
}: DexyPathCardProps) {
  const formatTokens = (amount: number) => {
    const divisor = Math.pow(10, decimals)
    return (amount / divisor).toLocaleString(undefined, {
      maximumFractionDigits: decimals,
    })
  }

  const formatRate = (rate: number) => {
    return rate.toFixed(4)
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (path.available && (e.key === 'Enter' || e.key === ' ')) {
      e.preventDefault()
      onSelect()
    }
  }

  const ariaLabel = path.available
    ? `${path.name} path, ${path.erg_per_token?.toFixed(4)} ERG per ${tokenName}${path.is_best_rate ? ', best rate' : ''}`
    : `${path.name} path, unavailable: ${path.reason}`

  return (
    <div
      className={`dexy-path-card ${selected ? 'selected' : ''} ${!path.available ? 'disabled' : ''}`}
      onClick={path.available ? onSelect : undefined}
      onKeyDown={handleKeyDown}
      role="button"
      tabIndex={path.available ? 0 : -1}
      aria-selected={selected}
      aria-disabled={!path.available}
      aria-label={ariaLabel}
    >
      <div className="path-header">
        <span className="path-name">{path.name}</span>
        {path.is_best_rate && path.available && <span className="best-rate-badge">Best Rate</span>}
      </div>

      {path.available ? (
        <div className="path-content">
          <div className="path-rate">
            <span className="rate-value">{formatRate(path.erg_per_token || 0)}</span>
            <span className="rate-label">ERG / {tokenName}</span>
          </div>

          {path.effective_rate && path.effective_rate !== path.erg_per_token && (
            <div className="path-effective-rate">
              <span className="effective-label">After fee:</span>
              <span className="effective-value">{formatRate(path.effective_rate)} ERG</span>
            </div>
          )}

          {path.remaining_today !== undefined && (
            <div className="path-limit">
              <span className="limit-label">Available today:</span>
              <span className="limit-value">{formatTokens(path.remaining_today)}</span>
            </div>
          )}

          {path.max_tokens !== undefined && path.remaining_today === undefined && (
            <div className="path-limit">
              <span className="limit-label">Max available:</span>
              <span className="limit-value">{formatTokens(path.max_tokens)}</span>
            </div>
          )}

          {path.fee_percent > 0 && (
            <div className="path-fee">
              <span className="fee-label">Fee:</span>
              <span className="fee-value">{path.fee_percent}%</span>
            </div>
          )}
        </div>
      ) : (
        <div className="path-unavailable">
          <span className="unavailable-reason">{path.reason || 'Unavailable'}</span>
        </div>
      )}

      <div className="path-action">
        {path.available ? (
          selected ? (
            <span className="selected-indicator">Selected</span>
          ) : (
            <span className="select-prompt">Select</span>
          )
        ) : (
          <span className="disabled-indicator">-</span>
        )}
      </div>
    </div>
  )
})
