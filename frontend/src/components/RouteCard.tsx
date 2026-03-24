import { useState } from 'react'
import { TokenIcon } from './tokenIcons'
import { formatTokenAmount } from '../utils/format'
import type { RouteQuote, RouteHop } from '../api/router'

// =============================================================================
// Helpers
// =============================================================================

function impactClass(impact: number): string {
  if (impact < 1) return 'impact-low'
  if (impact < 5) return 'impact-medium'
  if (impact < 10) return 'impact-high'
  return 'impact-severe'
}

function formatHopAmount(amount: number, decimals: number, name: string | null): string {
  return `${formatTokenAmount(amount, decimals)} ${name ?? '?'}`
}

function truncateId(id: string): string {
  if (id.length <= 12) return id
  return `${id.slice(0, 6)}…${id.slice(-6)}`
}

// =============================================================================
// HopDetail (internal)
// =============================================================================

interface HopDetailProps {
  hop: RouteHop
  index: number
}

function HopDetail({ hop, index }: HopDetailProps) {
  const feePercent = hop.fee_denom > 0 ? ((hop.fee_denom - hop.fee_num) / hop.fee_denom) * 100 : 0

  return (
    <div className="smart-hop-detail">
      <div className="smart-hop-detail-top">
        <span className="smart-hop-label">Hop {index + 1}</span>
        <span className={`smart-hop-impact ${impactClass(hop.price_impact)}`}>
          {hop.price_impact.toFixed(2)}% impact
        </span>
        <span className="smart-hop-pool-tag">
          {hop.pool_display_name ?? truncateId(hop.pool_id)}
          {' '}
          <span className="smart-hop-pool-id">({truncateId(hop.pool_id)})</span>
        </span>
      </div>
      <div className="smart-hop-detail-amounts">
        <span>{formatHopAmount(hop.input_amount, hop.token_in_decimals, hop.token_in_name)}</span>
        <span className="smart-route-arrow">→</span>
        <span>{formatHopAmount(hop.output_amount, hop.token_out_decimals, hop.token_out_name)}</span>
      </div>
      <div className="smart-hop-detail-meta">
        <span>Fee: {feePercent.toFixed(2)}% ({formatHopAmount(hop.fee_amount, hop.token_in_decimals, hop.token_in_name)})</span>
        <span>Reserves in: {formatTokenAmount(hop.reserves_in, hop.token_in_decimals)}</span>
        <span>Reserves out: {formatTokenAmount(hop.reserves_out, hop.token_out_decimals)}</span>
      </div>
    </div>
  )
}

// =============================================================================
// RouteCard
// =============================================================================

export interface RouteCardProps {
  routeQuote: RouteQuote
  isBest: boolean
  isSelected: boolean
  onSelect: () => void
  compact?: boolean
}

export function RouteCard({ routeQuote, isBest, isSelected, onSelect, compact = false }: RouteCardProps) {
  const [expanded, setExpanded] = useState(false)

  const { route } = routeQuote
  const hops = route.hops
  const lastHop = hops[hops.length - 1]

  // Build path: first token_in_name, then each hop's token_out_name
  const pathTokens: Array<{ name: string | null }> = [
    { name: hops[0]?.token_in_name ?? null },
    ...hops.map(h => ({ name: h.token_out_name ?? null })),
  ]

  if (compact) {
    const pathLabel = pathTokens.map(t => t.name ?? '?').join(' → ')
    return (
      <button
        className={`smart-route-compact${isSelected ? ' selected' : ''}`}
        onClick={onSelect}
        type="button"
      >
        <span className="smart-route-compact-path">{pathLabel}</span>
        <span className="smart-route-compact-output">
          {formatTokenAmount(route.total_output, lastHop?.token_out_decimals ?? 0)} {lastHop?.token_out_name ?? ''}
        </span>
        <span className={`smart-route-compact-impact ${impactClass(route.total_price_impact)}`}>
          {route.total_price_impact.toFixed(2)}%
        </span>
      </button>
    )
  }

  return (
    <div
      className={`smart-route-card${isBest ? ' best' : ''}${isSelected ? ' selected' : ''}`}
      onClick={onSelect}
      role="button"
      tabIndex={0}
      onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') onSelect() }}
    >
      <div className="smart-route-header">
        <div className="smart-route-badges">
          {isBest && <span className="smart-badge-best">BEST</span>}
          <span className="smart-badge-hops">{hops.length} {hops.length === 1 ? 'hop' : 'hops'}</span>
        </div>
      </div>

      {/* Path visualization */}
      <div className="smart-route-path">
        {pathTokens.map((token, i) => (
          <span key={i} className="smart-route-path-item">
            {i > 0 && <span className="smart-route-arrow">→</span>}
            <TokenIcon name={token.name ?? ''} size={16} />
            <span>{token.name ?? '?'}</span>
          </span>
        ))}
      </div>

      {/* Output amount */}
      <div className="smart-route-output">
        <span className="smart-route-output-amount">
          {formatTokenAmount(route.total_output, lastHop?.token_out_decimals ?? 0)}
          {' '}
          {lastHop?.token_out_name ?? ''}
        </span>
      </div>

      {/* Metrics row */}
      <div className="smart-route-metrics">
        <span>Rate: {route.effective_rate.toFixed(6)}</span>
        <span className={impactClass(route.total_price_impact)}>
          Impact: {route.total_price_impact.toFixed(2)}%
        </span>
        <span>
          Fees: {formatTokenAmount(route.total_fees, hops[0]?.token_in_decimals ?? 0)} {hops[0]?.token_in_name ?? ''}
        </span>
      </div>

      {/* Expand/collapse toggle */}
      <button
        className="smart-route-expand-btn"
        onClick={e => { e.stopPropagation(); setExpanded(v => !v) }}
        type="button"
      >
        {expanded ? '▾ Hide details' : '▸ Route details'}
      </button>

      {/* Per-hop details */}
      {expanded && (
        <div className="smart-route-details">
          {hops.map((hop, i) => (
            <HopDetail key={hop.pool_id + i} hop={hop} index={i} />
          ))}
        </div>
      )}
    </div>
  )
}
