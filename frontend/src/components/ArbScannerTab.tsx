import { useState, useEffect, useCallback } from 'react'
import { scanCircularArbs } from '../api/arb'
import type { CircularArbSnapshot, CircularArb } from '../api/arb'
import './ArbScannerTab.css'

interface ArbScannerTabProps {
  walletAddress: string | null
}

function formatErg(nano: number): string {
  return (nano / 1e9).toLocaleString(undefined, {
    minimumFractionDigits: 4,
    maximumFractionDigits: 4,
  })
}

function formatErgSigned(nano: number): string {
  const prefix = nano >= 0 ? '+' : ''
  return prefix + formatErg(Math.abs(nano))
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

      <div className="arb-card-breakdown">
        <div className="arb-card-detail">
          <span className="arb-card-label">Gross</span>
          <span className="arb-card-value profit">{formatErgSigned(arb.gross_profit_nano)} ERG</span>
        </div>
        <div className="arb-card-detail">
          <span className="arb-card-label">Fees</span>
          <span className="arb-card-value fee">-{formatErg(arb.tx_fee_nano)} ERG</span>
        </div>
        <div className="arb-card-detail">
          <span className="arb-card-label">Net</span>
          <span className="arb-card-value net">{formatErgSigned(arb.net_profit_nano)} ERG</span>
        </div>
      </div>

      <div className="arb-card-footer">
        <span className={`arb-card-impact ${impactClass(arb.price_impact)}`}>
          Impact: {arb.price_impact.toFixed(1)}%
        </span>
      </div>
    </div>
  )
}
