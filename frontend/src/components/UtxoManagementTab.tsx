import { useState, useEffect, useMemo, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  buildConsolidateTx,
  buildSplitTx,
  startUtxoMgmtSign,
  getUtxoMgmtTxStatus,
} from '../api/utxoManagement'
import type { ConsolidateBuildResponse, SplitBuildResponse } from '../api/utxoManagement'
import { getCachedTokenInfo } from '../api/tokenCache'
import { TX_FEE_NANO, MIN_BOX_VALUE_NANO } from '../constants'
import { formatErg, formatTokenAmount } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import './UtxoManagementTab.css'

interface UtxoManagementTabProps {
  isConnected: boolean
  walletAddress: string | null
  walletBalance: {
    erg_nano: number
    tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>
  } | null
  explorerUrl: string
}

type SubTab = 'consolidate' | 'split'
type Step = 'select' | 'confirm' | 'building' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'
type SplitType = 'erg' | 'token'

interface UtxoBox {
  boxId: string
  value: string
  ergoTree: string
  assets: Array<{ tokenId: string; amount: string }>
  creationHeight: number
  transactionId: string
  index: number
  additionalRegisters: Record<string, string>
  extension: Record<string, string>
}

export function UtxoManagementTab({
  isConnected,
  walletAddress,
  walletBalance,
  explorerUrl,
}: UtxoManagementTabProps) {
  const [subTab, setSubTab] = useState<SubTab>('consolidate')
  const [step, setStep] = useState<Step>('select')
  const [signMethod, setSignMethod] = useState<SignMethod>('choose')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [requestId, setRequestId] = useState<string | null>(null)
  const [txId, setTxId] = useState<string | null>(null)

  // Consolidate state
  const [utxos, setUtxos] = useState<UtxoBox[]>([])
  const [loadingUtxos, setLoadingUtxos] = useState(false)
  const [selectedBoxIds, setSelectedBoxIds] = useState<Set<string>>(new Set())
  const [consolidateSummary, setConsolidateSummary] = useState<ConsolidateBuildResponse | null>(null)

  // Split state
  const [splitType, setSplitType] = useState<SplitType>('erg')
  const [splitAmount, setSplitAmount] = useState('')
  const [splitCount, setSplitCount] = useState('')
  const [splitTokenId, setSplitTokenId] = useState('')
  const [splitErgPerBox, setSplitErgPerBox] = useState('0.001')
  const [splitSummary, setSplitSummary] = useState<SplitBuildResponse | null>(null)

  // Token name resolution
  const [resolvedNames, setResolvedNames] = useState<Map<string, string>>(new Map())

  const tokens = walletBalance?.tokens ?? []

  // Resolve unknown token names
  useEffect(() => {
    const unknown = tokens.filter(t => !t.name)
    if (unknown.length === 0) return
    let cancelled = false
    for (const t of unknown) {
      getCachedTokenInfo(t.token_id)
        .then(info => {
          if (cancelled) return
          if (info.name) {
            setResolvedNames(prev => {
              const next = new Map(prev)
              next.set(t.token_id, info.name!)
              return next
            })
          }
        })
        .catch(() => {})
    }
    return () => { cancelled = true }
  }, [tokens])

  const getTokenName = useCallback(
    (tokenId: string, name: string | null): string =>
      name || resolvedNames.get(tokenId) || tokenId.slice(0, 8) + '...',
    [resolvedNames],
  )

  // Fetch UTXOs for consolidate tab
  const fetchUtxos = useCallback(async () => {
    if (!walletAddress) return
    setLoadingUtxos(true)
    try {
      const raw = await invoke<UtxoBox[]>('get_user_utxos')
      setUtxos(raw)
    } catch (e) {
      console.error('Failed to fetch UTXOs:', e)
    } finally {
      setLoadingUtxos(false)
    }
  }, [walletAddress])

  useEffect(() => {
    if (subTab === 'consolidate' && step === 'select') {
      fetchUtxos()
    }
  }, [subTab, step, fetchUtxos])

  // Reset on wallet change
  useEffect(() => {
    handleReset()
  }, [walletAddress])

  // Poll for tx status
  useEffect(() => {
    if (step !== 'signing' || !requestId) return
    let isPolling = false
    const poll = async () => {
      if (isPolling) return
      isPolling = true
      try {
        const status = await getUtxoMgmtTxStatus(requestId)
        if (status.status === 'submitted' && status.tx_id) {
          setTxId(status.tx_id)
          setStep('success')
        } else if (status.status === 'failed' || status.status === 'expired') {
          setError(status.error || 'Transaction failed')
          setStep('error')
        }
      } catch (e) {
        console.error('Poll error:', e)
      } finally {
        isPolling = false
      }
    }
    const interval = setInterval(poll, 2000)
    return () => clearInterval(interval)
  }, [step, requestId])

  // -------------------------------------------------------------------------
  // Consolidate handlers
  // -------------------------------------------------------------------------

  const toggleUtxo = (boxId: string) => {
    setSelectedBoxIds(prev => {
      const next = new Set(prev)
      if (next.has(boxId)) next.delete(boxId)
      else next.add(boxId)
      return next
    })
  }

  const selectAllUtxos = () => {
    setSelectedBoxIds(new Set(utxos.map(u => u.boxId)))
  }

  const deselectAllUtxos = () => {
    setSelectedBoxIds(new Set())
  }

  const selectedUtxosSummary = useMemo(() => {
    const selected = utxos.filter(u => selectedBoxIds.has(u.boxId))
    const totalErg = selected.reduce((sum, u) => sum + parseInt(u.value || '0', 10), 0)
    const tokenSet = new Set<string>()
    for (const u of selected) {
      for (const a of u.assets) tokenSet.add(a.tokenId)
    }
    return { count: selected.length, totalErg, tokenCount: tokenSet.size }
  }, [utxos, selectedBoxIds])

  const handleConsolidate = async () => {
    setLoading(true)
    setError(null)
    setStep('building')
    try {
      const selectedBoxes = utxos.filter(u => selectedBoxIds.has(u.boxId))
      const userErgoTree = selectedBoxes[0]?.ergoTree
      if (!userErgoTree) throw new Error('Cannot determine user ErgoTree')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const result = await buildConsolidateTx(
        selectedBoxes as unknown as object[],
        userErgoTree,
        nodeStatus.chain_height,
      )
      setConsolidateSummary(result)

      const message = `Consolidate ${selectedBoxes.length} UTXOs`
      const signResult = await startUtxoMgmtSign(result.unsignedTx, message)

      setRequestId(signResult.request_id)
      setQrUrl(signResult.ergopay_url)
      setNautilusUrl(signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  // -------------------------------------------------------------------------
  // Split handlers
  // -------------------------------------------------------------------------

  const splitAmountNano = useMemo(() => {
    if (splitType === 'erg') {
      const parsed = parseFloat(splitAmount.replace(/,/g, ''))
      return isNaN(parsed) ? 0 : Math.floor(parsed * 1e9)
    }
    return parseInt(splitAmount.replace(/,/g, ''), 10) || 0
  }, [splitType, splitAmount])

  const splitCountNum = useMemo(() => {
    return parseInt(splitCount, 10) || 0
  }, [splitCount])

  const splitErgPerBoxNano = useMemo(() => {
    const parsed = parseFloat(splitErgPerBox.replace(/,/g, ''))
    return isNaN(parsed) ? 0 : Math.floor(parsed * 1e9)
  }, [splitErgPerBox])

  const splitIsValid = useMemo(() => {
    if (splitCountNum < 1 || splitCountNum > 30) return false
    if (splitType === 'erg') {
      return splitAmountNano >= MIN_BOX_VALUE_NANO
    } else {
      return splitAmountNano > 0 && splitTokenId !== '' && splitErgPerBoxNano >= MIN_BOX_VALUE_NANO
    }
  }, [splitType, splitAmountNano, splitCountNum, splitTokenId, splitErgPerBoxNano])

  const splitTotalDisplay = useMemo(() => {
    if (splitType === 'erg') {
      return formatErg(splitAmountNano * splitCountNum) + ' ERG'
    }
    const token = tokens.find(t => t.token_id === splitTokenId)
    if (!token) return ''
    return formatTokenAmount(splitAmountNano * splitCountNum, token.decimals)
  }, [splitType, splitAmountNano, splitCountNum, splitTokenId, tokens])

  const handleSplit = async () => {
    setLoading(true)
    setError(null)
    setStep('building')
    try {
      const allUtxos = await invoke<object[]>('get_user_utxos')
      if (!allUtxos?.length) throw new Error('No UTXOs available')

      const first = allUtxos[0] as { ergo_tree?: string; ergoTree?: string }
      const userErgoTree = first.ergo_tree || first.ergoTree
      if (!userErgoTree) throw new Error('Cannot determine user ErgoTree')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      let amountStr: string
      if (splitType === 'erg') {
        amountStr = splitAmountNano.toString()
      } else {
        // For tokens, compute raw amount based on decimals
        const token = tokens.find(t => t.token_id === splitTokenId)
        const decimals = token?.decimals ?? 0
        const parsed = parseFloat(splitAmount.replace(/,/g, ''))
        const raw = isNaN(parsed) ? 0 : Math.floor(parsed * Math.pow(10, decimals))
        amountStr = raw.toString()
      }

      const result = await buildSplitTx(
        allUtxos,
        userErgoTree,
        nodeStatus.chain_height,
        splitType,
        amountStr,
        splitCountNum,
        splitType === 'token' ? splitTokenId : undefined,
        splitType === 'token' ? splitErgPerBoxNano : undefined,
      )
      setSplitSummary(result)

      const noun = splitType === 'erg' ? 'ERG' : 'token'
      const message = `Split into ${splitCountNum} ${noun} boxes`
      const signResult = await startUtxoMgmtSign(result.unsignedTx, message)

      setRequestId(signResult.request_id)
      setQrUrl(signResult.ergopay_url)
      setNautilusUrl(signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  // -------------------------------------------------------------------------
  // Common handlers
  // -------------------------------------------------------------------------

  const handleNautilusSign = async () => {
    if (!nautilusUrl) return
    setSignMethod('nautilus')
    try {
      await invoke('open_nautilus', { nautilusUrl })
    } catch (e) {
      setError(String(e))
    }
  }

  const handleReset = () => {
    setStep('select')
    setError(null)
    setQrUrl(null)
    setNautilusUrl(null)
    setRequestId(null)
    setTxId(null)
    setSignMethod('choose')
    setConsolidateSummary(null)
    setSplitSummary(null)
    setSelectedBoxIds(new Set())
    setSplitAmount('')
    setSplitCount('')
    setSplitTokenId('')
    setSplitErgPerBox('0.001')
  }

  // -------------------------------------------------------------------------
  // Empty states
  // -------------------------------------------------------------------------

  if (!isConnected || !walletAddress) {
    return (
      <div className="utxo-tab">
        <div className="utxo-header">
          <h2>UTXO Management</h2>
          <p className="utxo-description">Consolidate or split your boxes for better UTXO hygiene.</p>
        </div>
        <div className="message warning">
          {!isConnected ? 'Connect to a node to manage UTXOs.' : 'Connect your wallet to manage UTXOs.'}
        </div>
      </div>
    )
  }

  // -------------------------------------------------------------------------
  // Building step
  // -------------------------------------------------------------------------

  if (step === 'building') {
    return (
      <div className="utxo-tab">
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="swap-preview-loading">
                <div className="spinner-small" />
                <span>Building transaction...</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // -------------------------------------------------------------------------
  // Signing — choose method
  // -------------------------------------------------------------------------

  if (step === 'signing' && signMethod === 'choose') {
    return (
      <div className="utxo-tab">
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                {consolidateSummary && (
                  <div className="utxo-confirm-summary" style={{ marginBottom: 'var(--space-md)' }}>
                    <div className="utxo-confirm-row">
                      <span>Consolidating</span>
                      <span>{consolidateSummary.inputCount} boxes into 1</span>
                    </div>
                    <div className="utxo-confirm-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(consolidateSummary.minerFee)} ERG</span>
                    </div>
                  </div>
                )}
                {splitSummary && (
                  <div className="utxo-confirm-summary" style={{ marginBottom: 'var(--space-md)' }}>
                    <div className="utxo-confirm-row">
                      <span>Splitting into</span>
                      <span>{splitSummary.splitCount} boxes</span>
                    </div>
                    <div className="utxo-confirm-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(splitSummary.minerFee)} ERG</span>
                    </div>
                  </div>
                )}
                <p>Choose your signing method</p>
                <div className="wallet-options">
                  <button className="wallet-option" onClick={handleNautilusSign}>
                    <div className="wallet-option-icon">
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="2" y="3" width="20" height="14" rx="2" />
                        <path d="M8 21h8" /><path d="M12 17v4" />
                      </svg>
                    </div>
                    <div className="wallet-option-info">
                      <span className="wallet-option-name">Nautilus Extension</span>
                      <span className="wallet-option-desc">Sign with browser extension</span>
                    </div>
                  </button>
                  <button className="wallet-option" onClick={() => setSignMethod('mobile')}>
                    <div className="wallet-option-icon">
                      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <rect x="5" y="2" width="14" height="20" rx="2" />
                        <line x1="12" y1="18" x2="12.01" y2="18" />
                      </svg>
                    </div>
                    <div className="wallet-option-info">
                      <span className="wallet-option-name">Mobile Wallet</span>
                      <span className="wallet-option-desc">Scan QR code with Ergo Wallet</span>
                    </div>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Signing — Nautilus waiting
  if (step === 'signing' && signMethod === 'nautilus') {
    return (
      <div className="utxo-tab">
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                <p>Approve the transaction in Nautilus</p>
                <div className="nautilus-waiting">
                  <div className="nautilus-icon">
                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="1.5">
                      <rect x="2" y="3" width="20" height="14" rx="2" />
                      <path d="M8 21h8" /><path d="M12 17v4" />
                    </svg>
                  </div>
                  <p className="signing-hint">Waiting for Nautilus approval...</p>
                </div>
                <div className="button-group">
                  <button className="btn btn-secondary" onClick={() => setSignMethod('choose')}>Back</button>
                  <button className="btn btn-primary" onClick={handleNautilusSign}>Open Nautilus Again</button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Signing — QR code
  if (step === 'signing' && signMethod === 'mobile' && qrUrl) {
    return (
      <div className="utxo-tab">
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="mint-signing-step">
                <p>Scan with your Ergo wallet to sign</p>
                <div className="qr-container">
                  <QRCodeSVG value={qrUrl} size={200} />
                </div>
                <p className="signing-hint">Waiting for signature...</p>
                <button className="btn btn-secondary" onClick={() => setSignMethod('choose')}>Back</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Success
  if (step === 'success') {
    const action = subTab === 'consolidate' ? 'Consolidated' : 'Split'
    return (
      <div className="utxo-tab">
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="success-step">
                <div className="success-icon">
                  <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                  </svg>
                </div>
                <h3>UTXOs {action}!</h3>
                {txId && <TxSuccess txId={txId} explorerUrl={explorerUrl} />}
                <button className="btn btn-primary" onClick={handleReset}>Done</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Error
  if (step === 'error') {
    return (
      <div className="utxo-tab">
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="error-step">
                <div className="error-icon">
                  <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--red-500)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" /><path d="M15 9l-6 6M9 9l6 6" />
                  </svg>
                </div>
                <h3>Transaction Failed</h3>
                <p className="error-message">{error}</p>
                <div className="button-group">
                  <button className="btn btn-secondary" onClick={handleReset}>Start Over</button>
                  <button className="btn btn-primary" onClick={() => { setStep('select'); setError(null) }}>
                    Try Again
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // -------------------------------------------------------------------------
  // Confirm step (consolidate or split)
  // -------------------------------------------------------------------------

  if (step === 'confirm') {
    if (subTab === 'consolidate') {
      return (
        <div className="utxo-tab">
          <div className="utxo-header">
            <h2>Confirm Consolidation</h2>
          </div>
          <div className="utxo-centered-card">
            <div className="card">
              <div className="card-content">
                <div className="utxo-confirm-summary">
                  <div className="utxo-confirm-row">
                    <span>Inputs</span>
                    <span>{selectedUtxosSummary.count} boxes</span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>Total ERG</span>
                    <span>{formatErg(selectedUtxosSummary.totalErg)} ERG</span>
                  </div>
                  {selectedUtxosSummary.tokenCount > 0 && (
                    <div className="utxo-confirm-row">
                      <span>Token types</span>
                      <span>{selectedUtxosSummary.tokenCount}</span>
                    </div>
                  )}
                  <div className="utxo-confirm-row">
                    <span>Miner Fee</span>
                    <span>~0.0011 ERG</span>
                  </div>
                  <div className="utxo-confirm-row utxo-highlight-row">
                    <span>Result</span>
                    <span>1 output box</span>
                  </div>
                </div>

                <div className="utxo-info-box" style={{ marginTop: 'var(--space-md)' }}>
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-400)" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" />
                    <line x1="12" y1="16" x2="12" y2="12" />
                    <line x1="12" y1="8" x2="12.01" y2="8" />
                  </svg>
                  <p>This will merge {selectedUtxosSummary.count} boxes into a single UTXO containing all ERG and tokens.</p>
                </div>

                <div className="button-group" style={{ marginTop: 'var(--space-md)' }}>
                  <button className="btn btn-secondary" onClick={() => setStep('select')}>Back</button>
                  <button className="btn btn-primary utxo-action-btn" onClick={handleConsolidate} disabled={loading}>
                    {loading ? 'Building...' : 'Consolidate'}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )
    }

    // Split confirm
    return (
      <div className="utxo-tab">
        <div className="utxo-header">
          <h2>Confirm Split</h2>
        </div>
        <div className="utxo-centered-card">
          <div className="card">
            <div className="card-content">
              <div className="utxo-confirm-summary">
                <div className="utxo-confirm-row">
                  <span>Mode</span>
                  <span>{splitType === 'erg' ? 'ERG Split' : 'Token Split'}</span>
                </div>
                <div className="utxo-confirm-row">
                  <span>Boxes</span>
                  <span>{splitCountNum}</span>
                </div>
                <div className="utxo-confirm-row">
                  <span>Per box</span>
                  <span>
                    {splitType === 'erg'
                      ? `${splitAmount} ERG`
                      : (() => {
                          const token = tokens.find(t => t.token_id === splitTokenId)
                          return `${splitAmount} ${token ? getTokenName(token.token_id, token.name) : 'tokens'}`
                        })()}
                  </span>
                </div>
                <div className="utxo-confirm-row utxo-highlight-row">
                  <span>Total</span>
                  <span>{splitTotalDisplay}</span>
                </div>
                <div className="utxo-confirm-row">
                  <span>Miner Fee</span>
                  <span>~0.0011 ERG</span>
                </div>
              </div>

              <div className="button-group" style={{ marginTop: 'var(--space-md)' }}>
                <button className="btn btn-secondary" onClick={() => setStep('select')}>Back</button>
                <button className="btn btn-primary utxo-action-btn" onClick={handleSplit} disabled={loading}>
                  {loading ? 'Building...' : 'Split'}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // -------------------------------------------------------------------------
  // Main select step
  // -------------------------------------------------------------------------

  return (
    <div className="utxo-tab">
      <div className="utxo-header">
        <h2>UTXO Management</h2>
        <p className="utxo-description">Consolidate or split your boxes for better UTXO hygiene.</p>
      </div>

      {/* Sub-tab toggle */}
      <div className="utxo-subtab-bar">
        <button
          className={`utxo-subtab ${subTab === 'consolidate' ? 'active' : ''}`}
          onClick={() => { setSubTab('consolidate'); handleReset() }}
        >
          Consolidate
        </button>
        <button
          className={`utxo-subtab ${subTab === 'split' ? 'active' : ''}`}
          onClick={() => { setSubTab('split'); handleReset() }}
        >
          Split
        </button>
      </div>

      {/* ================================================================= */}
      {/* Consolidate sub-tab                                               */}
      {/* ================================================================= */}
      {subTab === 'consolidate' && (
        <div className="utxo-layout">
          {/* UTXO list panel */}
          <div className="utxo-list-panel">
            <div className="utxo-list-toolbar">
              <div className="utxo-toolbar-actions">
                <button className="utxo-toolbar-btn" onClick={selectAllUtxos}>Select All</button>
                <button className="utxo-toolbar-btn" onClick={deselectAllUtxos}>Deselect All</button>
              </div>
              {selectedBoxIds.size > 0 && (
                <span className="utxo-select-badge">{selectedBoxIds.size}</span>
              )}
            </div>

            <div className="utxo-list">
              {loadingUtxos ? (
                <div className="utxo-list-empty">
                  <div className="spinner-small" />
                  <span>Loading UTXOs...</span>
                </div>
              ) : utxos.length === 0 ? (
                <div className="utxo-list-empty">
                  <span>No UTXOs found</span>
                </div>
              ) : (
                utxos.map(u => {
                  const selected = selectedBoxIds.has(u.boxId)
                  const ergValue = parseInt(u.value || '0', 10)
                  return (
                    <button
                      key={u.boxId}
                      className={`utxo-list-item${selected ? ' selected' : ''}`}
                      onClick={() => toggleUtxo(u.boxId)}
                    >
                      <div className="utxo-item-check">
                        {selected ? (
                          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-400)" strokeWidth="3">
                            <polyline points="20 6 9 17 4 12" />
                          </svg>
                        ) : (
                          <div className="utxo-item-checkbox" />
                        )}
                      </div>
                      <div className="utxo-item-info">
                        <span className="utxo-item-id">{u.boxId.slice(0, 12)}...{u.boxId.slice(-6)}</span>
                        <span className="utxo-item-meta">
                          {u.assets.length > 0 && (
                            <span className="utxo-item-token-badge">{u.assets.length} token{u.assets.length !== 1 ? 's' : ''}</span>
                          )}
                        </span>
                      </div>
                      <span className="utxo-item-erg">{formatErg(ergValue)} ERG</span>
                    </button>
                  )
                })
              )}
            </div>

            {utxos.length > 0 && (
              <div className="utxo-list-count">
                {utxos.length} box{utxos.length !== 1 ? 'es' : ''}
              </div>
            )}
          </div>

          {/* Summary panel */}
          <div className="utxo-summary-panel">
            {selectedBoxIds.size === 0 ? (
              <div className="utxo-summary-empty">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                  <rect x="3" y="3" width="7" height="7" rx="1" />
                  <rect x="14" y="3" width="7" height="7" rx="1" />
                  <rect x="3" y="14" width="7" height="7" rx="1" />
                  <path d="M14 17h7M17.5 14v7" />
                </svg>
                <span>Select boxes to consolidate</span>
                <span className="utxo-summary-hint">Pick 2 or more UTXOs to merge into one</span>
              </div>
            ) : (
              <>
                <div className="utxo-confirm-summary">
                  <div className="utxo-confirm-row">
                    <span>Selected</span>
                    <span>{selectedUtxosSummary.count} boxes</span>
                  </div>
                  <div className="utxo-confirm-row">
                    <span>Total ERG</span>
                    <span>{formatErg(selectedUtxosSummary.totalErg)} ERG</span>
                  </div>
                  {selectedUtxosSummary.tokenCount > 0 && (
                    <div className="utxo-confirm-row">
                      <span>Token types</span>
                      <span>{selectedUtxosSummary.tokenCount}</span>
                    </div>
                  )}
                  <div className="utxo-confirm-row">
                    <span>Miner Fee</span>
                    <span>~0.0011 ERG</span>
                  </div>
                  <div className="utxo-confirm-row utxo-highlight-row">
                    <span>Output</span>
                    <span>{formatErg(selectedUtxosSummary.totalErg - TX_FEE_NANO)} ERG</span>
                  </div>
                </div>

                {error && <div className="message error">{error}</div>}

                <div className="utxo-summary-footer">
                  <button
                    className="utxo-submit-btn"
                    onClick={() => setStep('confirm')}
                    disabled={selectedBoxIds.size < 2}
                  >
                    {selectedBoxIds.size < 2 ? 'Select at least 2 boxes' : 'Review Consolidation'}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* ================================================================= */}
      {/* Split sub-tab                                                     */}
      {/* ================================================================= */}
      {subTab === 'split' && (
        <div className="utxo-split-panel">
          {/* Split type toggle */}
          <div className="utxo-split-type-toggle">
            <button
              className={`utxo-split-type-btn ${splitType === 'erg' ? 'active' : ''}`}
              onClick={() => { setSplitType('erg'); setSplitAmount(''); setSplitCount('') }}
            >
              Split ERG
            </button>
            <button
              className={`utxo-split-type-btn ${splitType === 'token' ? 'active' : ''}`}
              onClick={() => { setSplitType('token'); setSplitAmount(''); setSplitCount('') }}
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
                  onChange={e => setSplitTokenId(e.target.value)}
                  className="utxo-split-select"
                >
                  <option value="">Select token...</option>
                  {tokens.map(t => (
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
                onChange={e => setSplitAmount(e.target.value)}
                placeholder={splitType === 'erg' ? '1.0' : '100'}
                className="utxo-split-input"
              />
            </div>

            <div className="utxo-split-field">
              <label>Number of boxes (1-30)</label>
              <input
                type="text"
                inputMode="numeric"
                value={splitCount}
                onChange={e => setSplitCount(e.target.value.replace(/\D/g, ''))}
                placeholder="5"
                className="utxo-split-input"
              />
            </div>

            {splitType === 'token' && (
              <div className="utxo-split-field">
                <label>ERG per box</label>
                <input
                  type="text"
                  inputMode="decimal"
                  value={splitErgPerBox}
                  onChange={e => setSplitErgPerBox(e.target.value)}
                  placeholder="0.001"
                  className="utxo-split-input"
                />
              </div>
            )}

            {/* Live preview */}
            {splitCountNum > 0 && splitAmountNano > 0 && (
              <div className="utxo-confirm-summary" style={{ marginTop: 'var(--space-sm)' }}>
                <div className="utxo-confirm-row">
                  <span>Total</span>
                  <span>{splitTotalDisplay}</span>
                </div>
                {splitType === 'token' && (
                  <div className="utxo-confirm-row">
                    <span>ERG locked</span>
                    <span>{formatErg(splitErgPerBoxNano * splitCountNum)} ERG</span>
                  </div>
                )}
                <div className="utxo-confirm-row">
                  <span>Miner Fee</span>
                  <span>~0.0011 ERG</span>
                </div>
              </div>
            )}

            {error && <div className="message error" style={{ marginTop: 'var(--space-sm)' }}>{error}</div>}

            <button
              className="utxo-submit-btn"
              style={{ marginTop: 'var(--space-md)' }}
              onClick={() => setStep('confirm')}
              disabled={!splitIsValid}
            >
              Review Split
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
