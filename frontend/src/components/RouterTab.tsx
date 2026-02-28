import { useState, useEffect, useRef, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  findSwapRoutes, findSwapRoutesByOutput, compareSigusdOptions,
  getSigusdArbSnapshot,
  type RouteQuote, type DepthTiers as DepthTiersType,
  type AcquisitionComparison, type SplitRouteDetail, type RouteHop,
  type OracleArbSnapshot,
} from '../api/router'
import {
  buildDirectSwapTx, startSwapSign, getSwapTxStatus,
} from '../api/amm'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import { formatTokenAmount } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import './RouterTab.css'

const SIGUSD_TOKEN_ID = '03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04'
const ERG_TOKEN_ID = 'ERG'

interface RouterTabProps {
  walletBalance: {
    erg_nano: number
    tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>
  } | null
  ergUsdPrice?: number
  canMintSigusd?: boolean
  reserveRatioPct?: number
  walletAddress?: string | null
  explorerUrl?: string
}

function formatErg(nanoErg: number): string {
  const erg = nanoErg / 1e9
  if (erg >= 1000) return erg.toLocaleString(undefined, { maximumFractionDigits: 2 })
  if (erg >= 1) return erg.toLocaleString(undefined, { maximumFractionDigits: 4 })
  return erg.toLocaleString(undefined, { maximumFractionDigits: 6 })
}

function formatSigusd(rawAmount: number): string {
  const val = rawAmount / 100
  return val.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })
}

function formatHopAmount(amount: number, decimals: number, name: string | null | undefined): string {
  return `${formatTokenAmount(amount, decimals)} ${name || ''}`
}

function maxInputForImpact(reservesIn: number, impactPct: number): number {
  const impact = impactPct / 100
  return Math.floor(reservesIn * impact / (1 - impact))
}

function impactClass(impact: number): string {
  if (impact > 10) return 'impact-severe'
  if (impact > 3) return 'impact-high'
  if (impact > 1) return 'impact-medium'
  return 'impact-low'
}

const HOP_IMPACT_TIERS = [0.5, 1, 2, 5]

export function RouterTab({
  walletBalance, ergUsdPrice, canMintSigusd, reserveRatioPct,
  walletAddress, explorerUrl,
}: RouterTabProps) {
  const [mode, setMode] = useState<'have-erg' | 'want-sigusd'>('have-erg')
  const [inputValue, setInputValue] = useState('')
  const [routes, setRoutes] = useState<RouteQuote[]>([])
  const [depthTiers, setDepthTiers] = useState<DepthTiersType[]>([])
  const [crossProtocol, setCrossProtocol] = useState<AcquisitionComparison | null>(null)
  const [split, setSplit] = useState<SplitRouteDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [showDepth, setShowDepth] = useState(false)
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Arb snapshot (page-load, no input needed)
  const [arbSnapshot, setArbSnapshot] = useState<OracleArbSnapshot | null>(null)
  const [arbLoading, setArbLoading] = useState(false)

  // Execution state
  const [execRoute, setExecRoute] = useState<RouteQuote | null>(null)
  const [execStep, setExecStep] = useState<'idle' | 'building' | 'signing' | 'success' | 'error'>('idle')
  const [execError, setExecError] = useState<string | null>(null)
  const [execTxId, setExecTxId] = useState<string | null>(null)

  const flow = useTransactionFlow({
    pollStatus: getSwapTxStatus,
    isOpen: execRoute !== null && execStep === 'signing',
    onSuccess: (txId) => { setExecTxId(txId ?? null); setExecStep('success') },
    onError: (err) => { setExecError(err); setExecStep('error') },
    watchParams: { protocol: 'AMM', operation: 'router-swap', description: 'Router swap' },
  })

  const fetchRoutes = useCallback(async () => {
    const val = parseFloat(inputValue)
    if (!val || val <= 0) {
      setRoutes([])
      setDepthTiers([])
      setCrossProtocol(null)
      setSplit(null)
      setError(null)
      return
    }

    setLoading(true)
    setError(null)

    try {
      if (mode === 'have-erg') {
        const nanoErg = Math.round(val * 1e9)
        const [routesResult, crossResult] = await Promise.all([
          findSwapRoutes(ERG_TOKEN_ID, SIGUSD_TOKEN_ID, nanoErg, 3, 5, 0.5, ergUsdPrice),
          compareSigusdOptions(nanoErg),
        ])
        setRoutes(routesResult.routes)
        setDepthTiers(routesResult.depth_tiers)
        setCrossProtocol(crossResult)
        setSplit(routesResult.split)
      } else {
        const rawSigusd = Math.round(val * 100)
        const routesResult = await findSwapRoutesByOutput(
          ERG_TOKEN_ID, SIGUSD_TOKEN_ID, rawSigusd, 3, 5, 0.5,
        )
        setRoutes(routesResult.routes)
        setDepthTiers(routesResult.depth_tiers)
        setSplit(routesResult.split)

        if (routesResult.routes.length > 0) {
          const bestInput = routesResult.routes[0].route.total_input
          try {
            const cp = await compareSigusdOptions(bestInput)
            setCrossProtocol(cp)
          } catch {
            setCrossProtocol(null)
          }
        } else {
          setCrossProtocol(null)
        }
      }
    } catch (e) {
      console.error('Router fetch failed:', e)
      setError(String(e))
      setRoutes([])
      setDepthTiers([])
      setCrossProtocol(null)
      setSplit(null)
    } finally {
      setLoading(false)
    }
  }, [inputValue, mode, ergUsdPrice])

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)

    if (!inputValue || parseFloat(inputValue) <= 0) {
      setRoutes([])
      setDepthTiers([])
      setCrossProtocol(null)
      setSplit(null)
      setError(null)
      return
    }

    debounceRef.current = setTimeout(fetchRoutes, 400)
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current) }
  }, [fetchRoutes])

  // Fetch arb snapshot on mount (or when oracle rate changes)
  useEffect(() => {
    if (!ergUsdPrice || ergUsdPrice <= 0) {
      setArbSnapshot(null)
      return
    }

    let cancelled = false
    setArbLoading(true)

    getSigusdArbSnapshot(ergUsdPrice)
      .then((snap) => { if (!cancelled) setArbSnapshot(snap) })
      .catch((e) => {
        console.error('Arb snapshot fetch failed:', e)
        if (!cancelled) setArbSnapshot(null)
      })
      .finally(() => { if (!cancelled) setArbLoading(false) })

    return () => { cancelled = true }
  }, [ergUsdPrice])

  const handleModeSwitch = (newMode: 'have-erg' | 'want-sigusd') => {
    setMode(newMode)
    setInputValue('')
    setRoutes([])
    setDepthTiers([])
    setCrossProtocol(null)
    setSplit(null)
    setError(null)
  }

  const handleExecuteRoute = async (rq: RouteQuote) => {
    if (!walletAddress || rq.route.hops.length !== 1) return
    const hop = rq.route.hops[0]

    setExecRoute(rq)
    setExecStep('building')
    setExecError(null)
    setExecTxId(null)

    try {
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const utxos = await invoke<object[]>('get_user_utxos')

      const inputType = hop.token_in === ERG_TOKEN_ID ? 'erg' : 'token'
      const tokenId = hop.token_in === ERG_TOKEN_ID ? undefined : hop.token_in

      const buildResult = await buildDirectSwapTx(
        hop.pool_id,
        inputType,
        hop.input_amount,
        tokenId,
        rq.min_output,
        walletAddress,
        utxos,
        nodeStatus.chain_height,
      )

      const inLabel = hop.token_in_name || 'ERG'
      const outLabel = hop.token_out_name || 'SigUSD'
      const message = `Router: ${formatTokenAmount(hop.input_amount, hop.token_in_decimals)} ${inLabel} → ${formatTokenAmount(hop.output_amount, hop.token_out_decimals)} ${outLabel}`

      const signResult = await startSwapSign(buildResult.unsigned_tx, message)
      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setExecStep('signing')
    } catch (e) {
      setExecError(String(e))
      setExecStep('error')
    }
  }

  const closeExecModal = () => {
    setExecRoute(null)
    setExecStep('idle')
    setExecError(null)
    setExecTxId(null)
  }

  const ergBalance = walletBalance ? walletBalance.erg_nano : 0
  const sigusdToken = walletBalance?.tokens.find(t => t.token_id === SIGUSD_TOKEN_ID)
  const sigusdBalance = sigusdToken ? sigusdToken.amount : 0
  const oracleRate = ergUsdPrice && ergUsdPrice > 0 ? ergUsdPrice : null

  return (
    <div className="router-tab">
      <div className="router-header">
        <h2 className="router-title">SigUSD Router</h2>
        <div className="router-subtitle">
          Find the cheapest path to acquire SigUSD across DEX routes and protocol minting
        </div>
      </div>

      {/* Market Context */}
      <div className="router-context">
        {oracleRate != null && (
          <div className="router-context-item">
            <span className="router-context-label">Oracle ERG/USD</span>
            <span className="router-context-value">${oracleRate.toFixed(2)}</span>
          </div>
        )}
        <div className="router-context-item">
          <span className="router-context-label">SigUSD Minting</span>
          <span className={`router-context-value ${canMintSigusd ? 'mint-available' : 'mint-blocked'}`}>
            {canMintSigusd
              ? 'Available'
              : `Blocked (RR ${Math.round(reserveRatioPct ?? 0)}%)`
            }
          </span>
        </div>
        {walletBalance && (
          <>
            <div className="router-context-item">
              <span className="router-context-label">ERG Balance</span>
              <span className="router-context-value">{formatErg(ergBalance)}</span>
            </div>
            <div className="router-context-item">
              <span className="router-context-label">SigUSD Balance</span>
              <span className="router-context-value">{formatSigusd(sigusdBalance)}</span>
            </div>
          </>
        )}
      </div>

      {/* Mode Toggle + Input */}
      <div className="router-input-panel">
        <div className="router-mode-toggle">
          <button
            className={`router-mode-btn ${mode === 'have-erg' ? 'active' : ''}`}
            onClick={() => handleModeSwitch('have-erg')}
          >
            I have ERG
          </button>
          <button
            className={`router-mode-btn ${mode === 'want-sigusd' ? 'active' : ''}`}
            onClick={() => handleModeSwitch('want-sigusd')}
          >
            I want SigUSD
          </button>
        </div>

        <div className="router-input-field">
          <input
            type="number"
            value={inputValue}
            onChange={e => setInputValue(e.target.value)}
            placeholder={mode === 'have-erg' ? 'ERG amount' : 'SigUSD amount'}
            min="0"
            step="any"
          />
          <span className="router-input-suffix">
            {mode === 'have-erg' ? 'ERG' : 'SigUSD'}
          </span>
          {mode === 'have-erg' && walletBalance && (
            <button
              className="router-max-btn"
              onClick={() => setInputValue(
                (Math.max(0, ergBalance - 10_000_000) / 1e9).toString()
              )}
            >
              MAX
            </button>
          )}
        </div>
      </div>

      {/* Error */}
      {error && (
        <div className="router-error">{error}</div>
      )}

      {/* Loading */}
      {loading && (
        <div className="router-loading">Finding best routes...</div>
      )}

      {/* Results */}
      {!loading && routes.length > 0 && (
        <div className="router-results">
          <SummaryBanner routes={routes} split={split} mode={mode} oracleRate={oracleRate} />

          {split && split.improvement_pct > 0.5 && (
            <SplitSuggestion
              split={split}
              oracleRate={oracleRate}
              canExecute={!!walletAddress && split.allocations.every(a => a.route.hops.length === 1)}
              onExecute={() => {/* TODO: sequential split execution */}}
            />
          )}

          <div className="router-routes-header">
            Routes ({routes.length})
          </div>

          {routes.map((rq, idx) => (
            <RouteCard
              key={idx}
              rq={rq}
              isBest={idx === 0}
              bestOutput={routes[0].route.total_output}
              bestInput={routes[0].route.total_input}
              mode={mode}
              oracleRate={oracleRate}
              canExecute={!!walletAddress && rq.route.hops.length === 1}
              onExecute={() => handleExecuteRoute(rq)}
            />
          ))}

          {crossProtocol && crossProtocol.options.length > 0 && (
            <div className="router-cross-section">
              <div className="router-section-title">Protocol Comparison</div>
              {crossProtocol.options.map((opt, idx) => (
                <div key={idx} className="router-cross-row">
                  <div className="router-cross-left">
                    <span className="router-cross-protocol">{opt.protocol}</span>
                    <span className="router-cross-desc">{opt.description}</span>
                  </div>
                  <div className={`router-cross-right ${
                    !opt.available ? 'unavailable' :
                    crossProtocol.best_index === idx ? 'best' : ''
                  }`}>
                    {opt.available ? (
                      <>
                        <span className="router-cross-output">{formatSigusd(opt.output_amount)}</span>
                        <span className="router-cross-unit">SigUSD</span>
                        <span className="router-cross-impact">{opt.impact_or_fee_pct.toFixed(1)}%</span>
                      </>
                    ) : (
                      <span className="router-cross-reason">{opt.unavailable_reason || 'Unavailable'}</span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}

          {depthTiers.length > 0 && (
            <div className="router-depth-section">
              <div
                className="router-section-title clickable"
                onClick={() => setShowDepth(!showDepth)}
              >
                Liquidity Depth {showDepth ? '\u25BE' : '\u25B8'}
              </div>
              {showDepth && depthTiers.map((tier, idx) => (
                <DepthTierRow key={idx} tier={tier} />
              ))}
            </div>
          )}
        </div>
      )}

      {!loading && !error && inputValue && parseFloat(inputValue) > 0 && routes.length === 0 && (
        <div className="router-empty">No routes found for this amount.</div>
      )}

      {!inputValue && !loading && (
        <>
          {arbLoading && (
            <div className="router-loading">Scanning pools for opportunities...</div>
          )}

          {!arbLoading && arbSnapshot && arbSnapshot.windows.length > 0 && (
            <ArbSnapshotPanel
              snapshot={arbSnapshot}
              oracleRate={oracleRate!}
              onRouteClick={(ergNano) => {
                setMode('have-erg')
                setInputValue((ergNano / 1e9).toString())
              }}
            />
          )}

          {!arbLoading && (!arbSnapshot || arbSnapshot.windows.length === 0) && (
            <div className="router-help">
              {oracleRate
                ? 'No pools currently offer rates above oracle. Enter an amount above to discover routes.'
                : 'Enter an amount above to discover the best routes for acquiring SigUSD. The router searches across all DEX liquidity pools including multi-hop paths through intermediate tokens, and compares against SigmaUSD protocol minting.'}
            </div>
          )}
        </>
      )}

      {/* Execution Modal */}
      {execRoute && (
        <div className="modal-overlay" onClick={closeExecModal}>
          <div className="modal router-execute-modal" onClick={e => e.stopPropagation()}>
            {execStep === 'building' && (
              <div className="router-exec-status">Building transaction...</div>
            )}

            {execStep === 'signing' && (
              <div className="router-exec-signing">
                <h3>Sign Transaction</h3>
                <p className="router-exec-desc">
                  {execRoute.route.hops[0].token_in_name || 'ERG'} &rarr;{' '}
                  {execRoute.route.hops[0].token_out_name || 'SigUSD'} via pool{' '}
                  {execRoute.route.hops[0].pool_id.slice(0, 8)}
                </p>

                {flow.nautilusUrl && (
                  <button
                    className="router-nautilus-btn"
                    onClick={() => window.open(flow.nautilusUrl!, '_blank')}
                  >
                    Sign with Nautilus
                  </button>
                )}

                {flow.qrUrl && (
                  <div className="router-qr-section">
                    <p className="router-qr-label">Or scan with ErgoPay wallet:</p>
                    <QRCodeSVG value={flow.qrUrl} size={180} bgColor="transparent" fgColor="#e2e8f0" />
                  </div>
                )}

                <div className="router-exec-waiting">Waiting for signature...</div>
              </div>
            )}

            {execStep === 'success' && (
              <div className="router-exec-success">
                <TxSuccess
                  txId={execTxId || ''}
                  explorerUrl={explorerUrl || 'https://sigmaspace.io'}
                />
                <button className="router-exec-close-btn" onClick={closeExecModal}>Close</button>
              </div>
            )}

            {execStep === 'error' && (
              <div className="router-exec-error">
                <h3>Error</h3>
                <p>{execError}</p>
                <button className="router-exec-close-btn" onClick={closeExecModal}>Close</button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Summary Banner
// ---------------------------------------------------------------------------

function SummaryBanner({ routes, split, mode, oracleRate }: {
  routes: RouteQuote[]
  split: SplitRouteDetail | null
  mode: 'have-erg' | 'want-sigusd'
  oracleRate: number | null
}) {
  const best = routes[0].route
  const numericRate = best.total_input > 0 ? (best.total_output / 100) / (best.total_input / 1e9) : 0

  const hasBetterSplit = split && split.improvement_pct > 0.5
  const splitNumericRate = hasBetterSplit && split.total_input > 0
    ? (split.total_output / 100) / (split.total_input / 1e9) : null

  const bestRate = splitNumericRate != null && splitNumericRate > numericRate ? splitNumericRate : numericRate
  const rateClass = oracleRate != null ? (bestRate >= oracleRate ? 'rate-good' : 'rate-bad') : ''

  return (
    <div className="router-summary">
      <div className="router-summary-top">
        <span className="router-summary-label">{mode === 'have-erg' ? 'Best rate' : 'Cheapest'}</span>
        <span className={`router-summary-rate ${rateClass}`}>
          {bestRate.toFixed(4)} SigUSD/ERG
        </span>
        {oracleRate != null && (
          <span className="router-summary-oracle">oracle: {oracleRate.toFixed(4)}</span>
        )}
      </div>
      <div className="router-summary-bottom">
        <span className="router-summary-detail">
          {formatErg(best.total_input)} ERG &rarr; {formatSigusd(best.total_output)} SigUSD
          {' '}({best.total_price_impact.toFixed(2)}% impact)
        </span>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Split Suggestion
// ---------------------------------------------------------------------------

function SplitSuggestion({ split, oracleRate, canExecute, onExecute }: {
  split: SplitRouteDetail
  oracleRate: number | null
  canExecute: boolean
  onExecute: () => void
}) {
  const splitRate = split.total_input > 0
    ? (split.total_output / 100) / (split.total_input / 1e9) : 0
  const rateClass = oracleRate != null ? (splitRate >= oracleRate ? 'rate-good' : 'rate-bad') : ''

  return (
    <div className="router-split-card">
      <div className="router-split-header">
        <span className="router-badge-split">SPLIT</span>
        <span className="router-split-improvement">
          +{split.improvement_pct.toFixed(1)}% more output
        </span>
      </div>

      <div className="router-split-summary">
        <span className="router-split-total">
          {formatErg(split.total_input)} ERG &rarr; {formatSigusd(split.total_output)} SigUSD
        </span>
        <span className={`router-split-rate ${rateClass}`}>
          {splitRate.toFixed(4)} SigUSD/ERG
        </span>
      </div>

      <div className="router-split-allocations">
        {split.allocations.map((alloc, idx) => {
          const pct = (alloc.fraction * 100).toFixed(1)
          const hops = alloc.route.hops
          const path = hops.map((h, i) => {
            const parts: string[] = []
            if (i === 0) parts.push(h.token_in_name || 'ERG')
            parts.push(h.token_out_name || h.token_out.slice(0, 6))
            return parts.join(' \u2192 ')
          }).join(' \u2192 ')

          return (
            <div key={idx} className="router-split-alloc">
              <div className="router-split-alloc-bar" style={{ width: `${pct}%` }} />
              <div className="router-split-alloc-info">
                <span className="router-split-alloc-pct">{pct}%</span>
                <span className="router-split-alloc-amount">{formatErg(alloc.input_amount)} ERG</span>
                <span className="router-split-alloc-path">
                  {path}
                  <span className="router-hop-pool-tag">{hops[0].pool_id.slice(0, 6)}</span>
                </span>
                <span className="router-split-alloc-output">&rarr; {formatSigusd(alloc.output_amount)}</span>
              </div>
            </div>
          )
        })}
      </div>

      {canExecute && (
        <button className="router-execute-btn" onClick={onExecute}>Execute Split</button>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Route Card
// ---------------------------------------------------------------------------

function RouteCard({ rq, isBest, bestOutput, bestInput, mode, oracleRate, canExecute, onExecute }: {
  rq: RouteQuote
  isBest: boolean
  bestOutput: number
  bestInput: number
  mode: 'have-erg' | 'want-sigusd'
  oracleRate: number | null
  canExecute: boolean
  onExecute: () => void
}) {
  const route = rq.route
  const effectiveRate = route.total_input > 0
    ? (route.total_output / 100) / (route.total_input / 1e9) : 0
  const rateClass = oracleRate != null
    ? (effectiveRate >= oracleRate ? 'rate-good' : 'rate-bad') : ''

  let diffLabel = ''
  if (!isBest) {
    if (mode === 'have-erg' && bestOutput > 0) {
      const pct = ((bestOutput - route.total_output) / bestOutput * 100).toFixed(1)
      diffLabel = `${pct}% less output`
    } else if (mode === 'want-sigusd' && bestInput > 0) {
      const pct = ((route.total_input - bestInput) / bestInput * 100).toFixed(1)
      diffLabel = `${pct}% more expensive`
    }
  }

  return (
    <div className={`router-route-card ${isBest ? 'best' : ''}`}>
      <div className="router-route-header">
        <div className="router-route-badges">
          {isBest && <span className="router-badge-best">BEST</span>}
          {!isBest && diffLabel && (
            <span className="router-badge-diff">{diffLabel}</span>
          )}
          <span className="router-badge-hops">{route.hops.length} hop{route.hops.length > 1 ? 's' : ''}</span>
          <span className={`router-badge-impact ${impactClass(route.total_price_impact)}`}>
            {route.total_price_impact.toFixed(1)}%
          </span>
        </div>
        <div className="router-route-amounts">
          <span className="router-route-input">{formatErg(route.total_input)} ERG</span>
          <span className="router-route-arrow">&rarr;</span>
          <span className="router-route-output">{formatSigusd(route.total_output)} SigUSD</span>
        </div>
      </div>

      {/* Effective rate */}
      <div className="router-route-rate">
        <span className={`router-rate-value ${rateClass}`}>
          {effectiveRate.toFixed(4)} SigUSD/ERG
        </span>
        {oracleRate != null && effectiveRate < oracleRate && (
          <span className="router-rate-premium">
            {((1 - effectiveRate / oracleRate) * 100).toFixed(1)}% worse than oracle
          </span>
        )}
      </div>

      {/* Hop chain with pool IDs */}
      <div className="router-hop-chain">
        {route.hops.map((hop, hIdx) => (
          <span key={hIdx} className="router-hop-segment">
            {hIdx === 0 && (
              <span className="router-hop-token">{hop.token_in_name || 'ERG'}</span>
            )}
            <span className="router-hop-arrow-sm">&rarr;</span>
            <span className="router-hop-token">{hop.token_out_name || hop.token_out.slice(0, 8)}</span>
            <span className="router-hop-pool-id" title={hop.pool_id}>{hop.pool_id.slice(0, 6)}</span>
          </span>
        ))}
      </div>

      {/* Per-hop details */}
      <div className="router-hop-details">
        {route.hops.map((hop, hIdx) => (
          <HopDetail key={hIdx} hop={hop} />
        ))}
      </div>

      {/* Route stats */}
      <div className="router-route-stats">
        <span>Impact: <strong>{route.total_price_impact.toFixed(2)}%</strong></span>
        <span>Fees: <strong>{formatErg(route.total_fees)}</strong></span>
      </div>

      {/* Execute / Multi-hop notice */}
      {canExecute && (
        <button className="router-execute-btn" onClick={onExecute}>Execute Swap</button>
      )}
      {route.hops.length > 1 && (
        <span className="router-multihop-notice">Multi-hop (view only)</span>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Hop Detail
// ---------------------------------------------------------------------------

function HopDetail({ hop }: { hop: RouteHop }) {
  return (
    <div className="router-hop-detail">
      <div className="router-hop-detail-top">
        <span className="router-hop-label">
          {hop.pool_display_name || `${hop.token_in_name || hop.token_in.slice(0, 6)} / ${hop.token_out_name || hop.token_out.slice(0, 6)}`}
          <span className="router-hop-pool-tag">{hop.pool_id.slice(0, 6)}</span>
        </span>
        <span className="router-hop-impact">
          {hop.price_impact.toFixed(2)}% impact
        </span>
      </div>
      <div className="router-hop-detail-amounts">
        <span className="router-hop-in">
          {formatHopAmount(hop.input_amount, hop.token_in_decimals, hop.token_in_name)}
        </span>
        <span className="router-hop-out">
          &rarr; {formatHopAmount(hop.output_amount, hop.token_out_decimals, hop.token_out_name)}
        </span>
      </div>
      <div className="router-hop-depth">
        <span className="router-hop-depth-header">Depth ({hop.token_in_name || 'input'}):</span>
        {HOP_IMPACT_TIERS.map((tier) => {
          const maxInput = maxInputForImpact(hop.reserves_in, tier)
          return (
            <div key={tier} className="router-hop-depth-cell">
              <span className="router-hop-depth-tier">{tier}%</span>
              <span className="router-hop-depth-max">
                {formatTokenAmount(maxInput, hop.token_in_decimals)}
              </span>
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Depth Tier Row
// ---------------------------------------------------------------------------

const DEPTH_LABELS = ['0.5%', '1%', '2%', '5%', '10%']

function DepthTierRow({ tier }: { tier: DepthTiersType }) {
  return (
    <div className="router-depth-row">
      <div className="router-depth-pool">
        Pool {tier.pool_id.slice(0, 8)}...
      </div>
      <div className="router-depth-grid">
        {tier.tiers.map(([impact, maxInput], idx) => (
          <div key={idx} className="router-depth-cell">
            <span className="router-depth-label">{DEPTH_LABELS[idx] || `${impact}%`}</span>
            <span className="router-depth-value">{formatErg(maxInput)} ERG</span>
          </div>
        ))}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Arb Snapshot Panel (page-load, no input required)
// ---------------------------------------------------------------------------

function ArbSnapshotPanel({ snapshot, oracleRate, onRouteClick }: {
  snapshot: OracleArbSnapshot
  oracleRate: number
  onRouteClick: (ergNano: number) => void
}) {
  const totalSigusdUsd = snapshot.total_sigusd_below_oracle_raw / 100

  return (
    <div className="arb-snapshot">
      <div className="arb-snapshot-header">
        <span className="arb-snapshot-title">Above-Oracle Rates</span>
        <span className="arb-snapshot-oracle">Oracle: {oracleRate.toFixed(4)} SigUSD/ERG</span>
      </div>

      <div className="arb-snapshot-totals">
        <div className="arb-snapshot-total-item">
          <span className="arb-snapshot-total-label">SigUSD available</span>
          <span className="arb-snapshot-total-value">
            {totalSigusdUsd.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
          </span>
        </div>
        <div className="arb-snapshot-total-item">
          <span className="arb-snapshot-total-label">Total ERG cost</span>
          <span className="arb-snapshot-total-value">{formatErg(snapshot.total_erg_needed_nano)}</span>
        </div>
      </div>

      <div className="arb-snapshot-pools">
        {snapshot.windows.map((w) => (
          <ArbWindowCard
            key={w.path_label}
            window={w}
            onClick={() => onRouteClick(w.max_erg_input_nano)}
          />
        ))}
      </div>

      <div className="arb-snapshot-hint">
        Click an opportunity to auto-fill the amount and see full route details.
      </div>
    </div>
  )
}

function ArbWindowCard({ window: w, onClick }: {
  window: import('../api/router').OracleArbWindow
  onClick: () => void
}) {
  return (
    <div className="arb-window-card clickable" onClick={onClick}>
      <div className="arb-window-header">
        <span className="arb-window-pool">{w.path_label}</span>
        <span className="arb-window-hops">{w.hops} hop{w.hops > 1 ? 's' : ''}</span>
        <span className="arb-window-discount">+{w.discount_pct.toFixed(1)}% vs oracle</span>
      </div>
      <div className="arb-window-details">
        <div className="arb-window-detail">
          <span className="arb-window-detail-label">Spot rate</span>
          <span className="arb-window-detail-value">
            {w.spot_rate_usd_per_erg.toFixed(4)} SigUSD/ERG
          </span>
        </div>
        <div className="arb-window-detail">
          <span className="arb-window-detail-label">Swap up to</span>
          <span className="arb-window-detail-value">{formatErg(w.max_erg_input_nano)} ERG</span>
        </div>
        <div className="arb-window-detail">
          <span className="arb-window-detail-label">You receive</span>
          <span className="arb-window-detail-value">
            {w.sigusd_output_at_max_usd.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} SigUSD
          </span>
        </div>
        <div className="arb-window-detail">
          <span className="arb-window-detail-label">Rate at max</span>
          <span className="arb-window-detail-value">
            {w.rate_at_max.toFixed(4)} SigUSD/ERG
          </span>
        </div>
        <div className="arb-window-detail">
          <span className="arb-window-detail-label">Impact at max</span>
          <span className={`arb-window-detail-value ${impactClass(w.price_impact_at_max)}`}>
            {w.price_impact_at_max.toFixed(1)}%
          </span>
        </div>
      </div>
      <div className="arb-window-action">Route {formatErg(w.max_erg_input_nano)} ERG &rarr;</div>
    </div>
  )
}
