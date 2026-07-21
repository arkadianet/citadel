import { useState, useCallback, useEffect, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  buildSplitChains, startArbLegSign, submitArbChain,
  type SplitChainBuild, type ArbChainSubmitResponse,
} from '../api/arb'
import { getTxStatus } from '../api/types'
import type { SplitRouteDetail } from '../api/router'
import { formatTokenAmount } from '../utils/format'

interface SplitExecuteModalProps {
  isOpen: boolean
  onClose: () => void
  split: SplitRouteDetail
  onSuccess: () => void
}

type Step = 'building' | 'review' | 'signing' | 'submitting' | 'done' | 'error'

/**
 * Executes a quoted split as pre-built 0-conf chained direct swaps:
 * build all allocation legs from one pool snapshot → sign each leg in
 * Nautilus (sign-only) → submit all legs in order.
 */
export function SplitExecuteModal({ isOpen, onClose, split, onSuccess }: SplitExecuteModalProps) {
  const [step, setStep] = useState<Step>('building')
  const [error, setError] = useState<string | null>(null)
  const [build, setBuild] = useState<SplitChainBuild | null>(null)
  const [signingLeg, setSigningLeg] = useState(0)
  const [requestIds, setRequestIds] = useState<string[]>([])
  const [submitResult, setSubmitResult] = useState<ArbChainSubmitResponse | null>(null)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const firstAlloc = split.allocations[0]
  const firstHop = firstAlloc.route.hops[0]
  const lastHop = firstAlloc.route.hops[firstAlloc.route.hops.length - 1]
  const sourceLabel = firstHop.token_in_name || (firstHop.token_in === 'ERG' ? 'ERG' : firstHop.token_in.slice(0, 8))
  const targetLabel = lastHop.token_out_name || (lastHop.token_out === 'ERG' ? 'ERG' : lastHop.token_out.slice(0, 8))
  const targetDecimals = lastHop.token_out_decimals ?? 0
  const totalLegsQuoted = split.allocations.reduce((n, a) => n + a.route.hops.length, 0)

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [])

  useEffect(() => () => stopPolling(), [stopPolling])

  const doBuild = useCallback(async () => {
    setStep('building')
    setError(null)
    try {
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const utxos = await invoke<object[]>('get_user_utxos')
      const allocations = split.allocations.map(a => {
        const hop0 = a.route.hops[0]
        return {
          poolIds: a.route.hops.map(h => h.pool_id),
          sourceToken: hop0.token_in === 'ERG' ? null : hop0.token_in,
          inputAmount: a.input_amount,
        }
      })
      // Abort if pools moved enough that built output is >1% worse than quote.
      const minTotalOutput = Math.floor(split.total_output * 0.99)
      const result = await buildSplitChains(
        allocations,
        utxos,
        nodeStatus.chain_height,
        minTotalOutput,
      )
      setBuild(result)
      setStep('review')
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
  }, [split])

  useEffect(() => {
    if (isOpen) doBuild()
  }, [isOpen, doBuild])

  const doSubmit = useCallback(async (ids: string[]) => {
    setStep('submitting')
    try {
      const result = await submitArbChain(ids)
      setSubmitResult(result)
      setStep('done')
      if (result.failedLeg === null) onSuccess()
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
  }, [onSuccess])

  const signLeg = useCallback(async (legIndex: number, priorRequestIds: string[]) => {
    if (!build) return
    setSigningLeg(legIndex)
    setStep('signing')
    try {
      const leg = build.legs[legIndex]
      const message = `Split leg ${legIndex + 1}/${build.legs.length}: ${leg.summary.input_amount} ${leg.summary.input_token} -> ${leg.summary.output_amount} ${leg.summary.output_token} (NOT broadcast until all legs signed)`
      const sign = await startArbLegSign(leg.unsignedTx, message)
      const ids = [...priorRequestIds, sign.requestId]
      setRequestIds(ids)
      await invoke('open_nautilus', { nautilusUrl: sign.nautilusUrl })

      stopPolling()
      pollRef.current = setInterval(async () => {
        try {
          const status = await getTxStatus(sign.requestId)
          if (status.status === 'signed') {
            stopPolling()
            if (legIndex + 1 < build.legs.length) {
              await signLeg(legIndex + 1, ids)
            } else {
              await doSubmit(ids)
            }
          } else if (status.status === 'expired' || status.status === 'failed') {
            stopPolling()
            setError(status.error || 'Signing request failed')
            setStep('error')
          }
        } catch {
          // transient poll error -- keep polling
        }
      }, 1500)
    } catch (e) {
      setError(String(e))
      setStep('error')
    }
  }, [build, stopPolling, doSubmit])

  const handleClose = () => {
    stopPolling()
    onClose()
  }

  if (!isOpen) return null

  // Map flat legs back to allocation groups for the review UI.
  const allocLegRanges: { start: number; end: number }[] = []
  if (build) {
    let cursor = 0
    for (const a of build.allocations) {
      allocLegRanges.push({ start: cursor, end: cursor + a.legCount })
      cursor += a.legCount
    }
  }

  return (
    <div className="modal-overlay" onClick={handleClose}>
      <div className="modal arb-execute-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h3>Split Swap: {sourceLabel} &rarr; {targetLabel}</h3>
          <button className="close-btn" onClick={handleClose}>&times;</button>
        </div>

        <div className="modal-content">
          {step === 'building' && (
            <div className="swap-preview-loading">
              <div className="spinner-small" />
              <p>
                Pre-building {split.allocations.length} allocations
                ({totalLegsQuoted} legs) from fresh pool state...
              </p>
            </div>
          )}

          {step === 'review' && build && (
            <>
              <div className="preview-section">
                <div className="preview-row highlight">
                  <span>You receive (exact, gross)</span>
                  <span>{formatTokenAmount(build.totalOutput, targetDecimals)} {targetLabel}</span>
                </div>
                <div className="preview-row">
                  <span>Quoted net (after tx fees)</span>
                  <span>{formatTokenAmount(split.net_output, targetDecimals)} {targetLabel}</span>
                </div>
                <div className="preview-row">
                  <span>Tx fees ({build.legs.length} hops × 0.0011 ERG)</span>
                  <span>−{formatTokenAmount(split.total_miner_fees, 9)} ERG</span>
                </div>
              </div>

              {build.allocations.map((alloc, aIdx) => {
                const range = allocLegRanges[aIdx]
                const legs = build.legs.slice(range.start, range.end)
                return (
                  <div key={aIdx} className="arb-exec-legs">
                    <div className="preview-row">
                      <span>Allocation {aIdx + 1}</span>
                      <span>
                        {formatTokenAmount(alloc.outputAmount, targetDecimals)} {targetLabel}
                      </span>
                    </div>
                    {legs.map((leg, idx) => (
                      <div key={leg.txId} className="arb-exec-leg">
                        <span className="arb-exec-leg-index">Leg {range.start + idx + 1}</span>
                        <span className="arb-exec-leg-swap">
                          {leg.summary.input_amount} {leg.summary.input_token} &rarr;{' '}
                          {leg.summary.output_amount} {leg.summary.output_token}
                        </span>
                      </div>
                    ))}
                  </div>
                )
              })}

              <div className="warning-box">
                You will sign {build.legs.length} transactions in Nautilus.
                Nothing broadcasts until every leg is signed. Allocations use
                disjoint pools so amounts are exact for this snapshot — if
                someone moves a pool first, that leg fails wholesale.
              </div>

              <div className="button-group">
                <button className="btn btn-secondary" onClick={handleClose}>Cancel</button>
                <button className="btn btn-primary" onClick={() => signLeg(0, [])}>
                  Sign {build.legs.length} legs in Nautilus
                </button>
              </div>
            </>
          )}

          {step === 'signing' && build && (
            <div className="mint-signing-step">
              <div className="arb-exec-stepper">
                {build.legs.map((_, idx) => (
                  <span
                    key={idx}
                    className={`arb-exec-step ${idx < signingLeg ? 'done' : idx === signingLeg ? 'active' : ''}`}
                  >
                    {idx < signingLeg ? '✓' : idx + 1}
                  </span>
                ))}
              </div>
              <div className="spinner-small" />
              <p>Sign leg {signingLeg + 1} of {build.legs.length} in Nautilus...</p>
              <p className="arb-exec-hint">The Nautilus window opened in your browser. Nothing broadcasts yet.</p>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={handleClose}>Abort (nothing broadcast)</button>
              </div>
            </div>
          )}

          {step === 'submitting' && (
            <div className="swap-preview-loading">
              <div className="spinner-small" />
              <p>All legs signed. Broadcasting chain in order...</p>
            </div>
          )}

          {step === 'done' && submitResult && (
            <>
              {submitResult.failedLeg === null ? (
                <div className="preview-section">
                  <div className="preview-row highlight">
                    <span>Split submitted</span>
                    <span>{submitResult.txIds.length} txs</span>
                  </div>
                  {submitResult.txIds.map((txId, idx) => (
                    <div key={txId} className="preview-row">
                      <span>Leg {idx + 1}</span>
                      <span className="arb-exec-txid" title={txId}>{txId.slice(0, 16)}...</span>
                    </div>
                  ))}
                </div>
              ) : (
                <>
                  <div className="message error">
                    {submitResult.error} — {submitResult.txIds.length} of {requestIds.length} legs
                    landed. You may be holding an intermediate token; re-quote and
                    swap it onward or back.
                  </div>
                  {submitResult.txIds.map((txId, idx) => (
                    <div key={txId} className="preview-row">
                      <span>Leg {idx + 1} (landed)</span>
                      <span className="arb-exec-txid" title={txId}>{txId.slice(0, 16)}...</span>
                    </div>
                  ))}
                </>
              )}
              <div className="button-group">
                <button className="btn btn-primary" onClick={handleClose}>Close</button>
              </div>
            </>
          )}

          {step === 'error' && (
            <>
              <div className="message error">{error}</div>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={handleClose}>Close</button>
                <button className="btn btn-primary" onClick={doBuild}>Rebuild</button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
