import { useState, useEffect, useCallback } from 'react'
import { scanCircularArbs } from '../api/arb'
import type { CircularArbSnapshot, CircularArb } from '../api/arb'
import type { RouteHop } from '../api/router'
import './ArbScannerTab.css'

interface ArbScannerTabProps {
  walletAddress: string | null
}

function formatErg(nano: number): string {
  return (nano / 1e9).toLocaleString(undefined, {
    minimumFractionDigits: 4,
    maximumFractionDigits: 9,
  })
}

function formatErgSigned(nano: number): string {
  const prefix = nano >= 0 ? '+' : ''
  return prefix + formatErg(Math.abs(nano))
}

function formatTokenAmount(raw: number, decimals: number): string {
  const value = raw / Math.pow(10, decimals)
  return value.toLocaleString(undefined, {
    minimumFractionDigits: Math.min(decimals, 4),
    maximumFractionDigits: decimals,
  })
}

function tokenName(hop: RouteHop, which: 'in' | 'out'): string {
  if (which === 'in') return hop.token_in_name || hop.token_in.slice(0, 8)
  return hop.token_out_name || hop.token_out.slice(0, 8)
}

function impactClass(impact: number): string {
  if (impact < 3) return 'impact-low'
  if (impact < 10) return 'impact-medium'
  return 'impact-high'
}

export function ArbScannerTab({ walletAddress: _walletAddress }: ArbScannerTabProps) {
  const [snapshot, setSnapshot] = useState<CircularArbSnapshot | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const doScan = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const result = await scanCircularArbs(4)
      setSnapshot(result)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    doScan()
  }, [doScan])

  return (
    <div className="arb-scanner-tab">
      <div className="arb-scanner-header">
        <div>
          <h2>Arb Scanner</h2>
          <p className="arb-scanner-desc">
            Scan for circular arbitrage opportunities across all DEX pools
          </p>
        </div>
        <button
          className="arb-scanner-refresh"
          onClick={doScan}
          disabled={loading}
        >
          {loading ? 'Scanning...' : 'Refresh'}
        </button>
      </div>

      {error && <div className="message error">{error}</div>}

      {loading && !snapshot && (
        <div className="empty-state">
          <div className="spinner" />
          <p>Scanning pools for arb opportunities...</p>
        </div>
      )}

      {snapshot && (
        <>
          {snapshot.windows.length > 0 ? (
            <>
              <div className="arb-scanner-summary">
                <span className="arb-scanner-count">
                  {snapshot.windows.length} opportunit{snapshot.windows.length === 1 ? 'y' : 'ies'} found
                </span>
                <span className="arb-scanner-total">
                  Total net profit: {formatErgSigned(snapshot.total_net_profit_nano)} ERG
                </span>
                <span className="arb-scanner-time">
                  Scanned in {snapshot.scan_time_ms}ms
                </span>
              </div>

              <div className="arb-scanner-cards">
                {snapshot.windows.map((arb, idx) => (
                  <ArbCard key={idx} arb={arb} />
                ))}
              </div>
            </>
          ) : (
            <div className="arb-scanner-empty">
              <p>No profitable arbs found.</p>
              <p className="arb-scanner-empty-hint">
                Circular arbs appear when pool prices diverge from each other.
                Check back after large trades move prices.
              </p>
            </div>
          )}
        </>
      )}
    </div>
  )
}

function ArbCard({ arb }: { arb: CircularArb }) {
  return (
    <div className="arb-card">
      <div className="arb-card-header">
        <span className="arb-card-path">{arb.path_label}</span>
        <span className="arb-card-hops">{arb.hops} hop{arb.hops > 1 ? 's' : ''}</span>
        <span className="arb-card-profit-badge">
          {arb.profit_pct >= 0 ? '+' : ''}{arb.profit_pct.toFixed(2)}%
        </span>
      </div>

      <div className="arb-card-amounts">
        <div className="arb-card-amount">
          <span className="arb-card-label">Input</span>
          <span className="arb-card-value">{formatErg(arb.optimal_input_nano)} ERG</span>
        </div>
        <div className="arb-card-amount">
          <span className="arb-card-label">Output</span>
          <span className="arb-card-value">{formatErg(arb.output_nano)} ERG</span>
        </div>
      </div>

      {/* Per-hop breakdown */}
      <div className="arb-card-hops-detail">
        {arb.route.hops.map((hop, idx) => (
          <div key={idx} className="arb-hop">
            <div className="arb-hop-header">
              <span className="arb-hop-index">Hop {idx + 1}</span>
              <span className="arb-hop-pool" title={hop.pool_id}>
                {hop.pool_display_name || `${tokenName(hop, 'in')}/${tokenName(hop, 'out')}`}
              </span>
              <span className="arb-hop-pool-id">{hop.pool_id.slice(0, 8)}</span>
              <span className={`arb-hop-impact ${impactClass(hop.price_impact)}`}>
                {hop.price_impact.toFixed(1)}%
              </span>
            </div>
            <div className="arb-hop-swap">
              <span className="arb-hop-amount">
                {formatTokenAmount(hop.input_amount, hop.token_in_decimals)} {tokenName(hop, 'in')}
              </span>
              <span className="arb-hop-arrow">&rarr;</span>
              <span className="arb-hop-amount">
                {formatTokenAmount(hop.output_amount, hop.token_out_decimals)} {tokenName(hop, 'out')}
              </span>
              <span className="arb-hop-fee">fee: {hop.fee_num}/{hop.fee_denom}</span>
            </div>
          </div>
        ))}
      </div>

      <div className="arb-card-breakdown">
        <div className="arb-card-detail">
          <span className="arb-card-label">Gross</span>
          <span className="arb-card-value profit">{formatErgSigned(arb.gross_profit_nano)} ERG</span>
        </div>
        <div className="arb-card-detail">
          <span className="arb-card-label">Fees ({arb.hops} tx)</span>
          <span className="arb-card-value fee">-{formatErg(arb.tx_fee_nano)} ERG</span>
        </div>
        <div className="arb-card-detail">
          <span className="arb-card-label">Net</span>
          <span className="arb-card-value net">{formatErgSigned(arb.net_profit_nano)} ERG</span>
        </div>
      </div>
    </div>
  )
}
