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
import { Tabs, EmptyState } from './ui'
import './UtxoManagementTab.css'

interface UtxoManagementTabProps {
  isConnected: boolean
  walletAddress: string | null
  walletBalance: {
    erg_nano: number
    tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>
  } | null
  explorerUrl: string
  /** When nested under Wallet, hide the page header and tighten padding. */
  embedded?: boolean
}

type SubTab = 'consolidate' | 'split'
type Step = 'select' | 'confirm' | 'building' | 'signing' | 'success' | 'error'
type SignMethod = 'choose' | 'mobile' | 'nautilus'
type SplitType = 'erg' | 'token'
type BoardFilter = 'all' | 'dust' | 'tokens' | 'large'

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

const DUST_NANO = 10_000_000 // 0.01 ERG
const LARGE_NANO = 1_000_000_000 // 1 ERG

function ergNano(box: UtxoBox): number {
  return parseInt(box.value || '0', 10) || 0
}

/** Log-scaled visual tier 1–5 for mosaic sizing. */
function valueTier(nano: number, maxNano: number): 1 | 2 | 3 | 4 | 5 {
  if (maxNano <= 0) return 1
  const ratio = Math.log10(Math.max(nano, 1)) / Math.log10(Math.max(maxNano, 10))
  if (ratio < 0.25) return 1
  if (ratio < 0.45) return 2
  if (ratio < 0.65) return 3
  if (ratio < 0.85) return 4
  return 5
}

function boxKind(box: UtxoBox): 'dust' | 'token' | 'erg' | 'large' {
  const nano = ergNano(box)
  if (box.assets.length > 0) return 'token'
  if (nano < DUST_NANO) return 'dust'
  if (nano >= LARGE_NANO) return 'large'
  return 'erg'
}

export function UtxoManagementTab({
  isConnected,
  walletAddress,
  walletBalance,
  explorerUrl,
  embedded = false,
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

  const [utxos, setUtxos] = useState<UtxoBox[]>([])
  const [loadingUtxos, setLoadingUtxos] = useState(false)
  const [selectedBoxIds, setSelectedBoxIds] = useState<Set<string>>(new Set())
  const [focusedBoxId, setFocusedBoxId] = useState<string | null>(null)
  const [boardFilter, setBoardFilter] = useState<BoardFilter>('all')
  const [consolidateSummary, setConsolidateSummary] = useState<ConsolidateBuildResponse | null>(null)

  const [splitType, setSplitType] = useState<SplitType>('erg')
  const [splitAmount, setSplitAmount] = useState('')
  const [splitCount, setSplitCount] = useState('')
  const [splitTokenId, setSplitTokenId] = useState('')
  const [splitErgPerBox, setSplitErgPerBox] = useState('0.001')
  const [splitSummary, setSplitSummary] = useState<SplitBuildResponse | null>(null)

  const [resolvedNames, setResolvedNames] = useState<Map<string, string>>(new Map())

  const tokens = walletBalance?.tokens ?? []
  const rootClass = embedded ? 'utxo-tab utxo-tab--embedded' : 'utxo-tab'

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

  useEffect(() => {
    handleReset()
  }, [walletAddress])

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

  const maxNano = useMemo(
    () => utxos.reduce((m, u) => Math.max(m, ergNano(u)), 0),
    [utxos],
  )

  const filteredUtxos = useMemo(() => {
    return utxos.filter(u => {
      const kind = boxKind(u)
      if (boardFilter === 'all') return true
      if (boardFilter === 'dust') return kind === 'dust'
      if (boardFilter === 'tokens') return kind === 'token'
      if (boardFilter === 'large') return kind === 'large' || ergNano(u) >= LARGE_NANO
      return true
    })
  }, [utxos, boardFilter])

  const boardStats = useMemo(() => {
    const dust = utxos.filter(u => boxKind(u) === 'dust').length
    const withTokens = utxos.filter(u => u.assets.length > 0).length
    const totalErg = utxos.reduce((s, u) => s + ergNano(u), 0)
    return { dust, withTokens, totalErg, count: utxos.length }
  }, [utxos])

  const toggleUtxo = (boxId: string) => {
    setFocusedBoxId(boxId)
    setSelectedBoxIds(prev => {
      const next = new Set(prev)
      if (next.has(boxId)) next.delete(boxId)
      else next.add(boxId)
      return next
    })
  }

  const selectVisible = () => {
    setSelectedBoxIds(new Set(filteredUtxos.map(u => u.boxId)))
  }

  const deselectAllUtxos = () => {
    setSelectedBoxIds(new Set())
    setFocusedBoxId(null)
  }

  const selectDust = () => {
    setSelectedBoxIds(new Set(utxos.filter(u => boxKind(u) === 'dust').map(u => u.boxId)))
  }

  const selectedUtxosSummary = useMemo(() => {
    const selected = utxos.filter(u => selectedBoxIds.has(u.boxId))
    const totalErg = selected.reduce((sum, u) => sum + ergNano(u), 0)
    const tokenSet = new Set<string>()
    for (const u of selected) {
      for (const a of u.assets) tokenSet.add(a.tokenId)
    }
    return { count: selected.length, totalErg, tokenCount: tokenSet.size }
  }, [utxos, selectedBoxIds])

  const focusedBox = useMemo(
    () => (focusedBoxId ? utxos.find(u => u.boxId === focusedBoxId) ?? null : null),
    [focusedBoxId, utxos],
  )

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

  const splitAmountNano = useMemo(() => {
    if (splitType === 'erg') {
      const parsed = parseFloat(splitAmount.replace(/,/g, ''))
      return isNaN(parsed) ? 0 : Math.floor(parsed * 1e9)
    }
    return parseInt(splitAmount.replace(/,/g, ''), 10) || 0
  }, [splitType, splitAmount])

  const splitCountNum = useMemo(() => parseInt(splitCount, 10) || 0, [splitCount])

  const splitErgPerBoxNano = useMemo(() => {
    const parsed = parseFloat(splitErgPerBox.replace(/,/g, ''))
    return isNaN(parsed) ? 0 : Math.floor(parsed * 1e9)
  }, [splitErgPerBox])

  const splitIsValid = useMemo(() => {
    if (splitCountNum < 1 || splitCountNum > 30) return false
    if (splitType === 'erg') {
      return splitAmountNano >= MIN_BOX_VALUE_NANO
    }
    return splitAmountNano > 0 && splitTokenId !== '' && splitErgPerBoxNano >= MIN_BOX_VALUE_NANO
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
    setFocusedBoxId(null)
    setSplitAmount('')
    setSplitCount('')
    setSplitTokenId('')
    setSplitErgPerBox('0.001')
  }

  const pageHeader = embedded ? null : (
    <header className="utxo-header-main">
      <div className="utxo-header-left">
        <div className="utxo-icon" aria-hidden>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="18" height="18">
            <rect x="3" y="3" width="7" height="7" />
            <rect x="14" y="3" width="7" height="7" />
            <rect x="3" y="14" width="7" height="7" />
            <rect x="14" y="14" width="7" height="7" />
          </svg>
        </div>
        <div>
          <h1 className="utxo-title">UTXO Management</h1>
          <p className="utxo-subtitle">Consolidate or split your boxes for better UTXO hygiene</p>
        </div>
      </div>
    </header>
  )

  if (!isConnected || !walletAddress) {
    return (
      <div className={rootClass}>
        {pageHeader}
        <EmptyState
          title={!isConnected ? 'Node Required' : 'Wallet Required'}
          description={!isConnected ? 'Connect to a node to manage UTXOs.' : 'Connect your wallet to manage UTXOs.'}
        />
      </div>
    )
  }

  if (step === 'building') {
    return (
      <div className={rootClass}>
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

  if (step === 'signing' && signMethod === 'choose') {
    return (
      <div className={rootClass}>
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

  if (step === 'signing' && signMethod === 'nautilus') {
    return (
      <div className={rootClass}>
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

  if (step === 'signing' && signMethod === 'mobile' && qrUrl) {
    return (
      <div className={rootClass}>
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

  if (step === 'success') {
    const action = subTab === 'consolidate' ? 'Consolidated' : 'Split'
    return (
      <div className={rootClass}>
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

  if (step === 'error') {
    return (
      <div className={rootClass}>
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

  if (step === 'confirm') {
    if (subTab === 'consolidate') {
      return (
        <div className={rootClass}>
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

    return (
      <div className={rootClass}>
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

  return (
    <div className={rootClass}>
      {pageHeader}

      <Tabs
        tabs={[
          { id: 'consolidate', label: 'Consolidate' },
          { id: 'split', label: 'Split' },
        ]}
        activeId={subTab}
        onChange={(id) => { setSubTab(id as SubTab); handleReset() }}
        size="compact"
      />

      {subTab === 'consolidate' && (
        <div className="utxo-board-stage">
          <div className="utxo-board-toolbar">
            <div className="utxo-board-filters" role="group" aria-label="Filter boxes">
              {([
                ['all', 'All'],
                ['dust', 'Dust'],
                ['tokens', 'Tokens'],
                ['large', 'Large'],
              ] as const).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  className={`utxo-filter-chip${boardFilter === id ? ' active' : ''}`}
                  onClick={() => setBoardFilter(id)}
                >
                  {label}
                </button>
              ))}
            </div>
            <div className="utxo-toolbar-actions">
              <button type="button" className="utxo-toolbar-btn" onClick={selectVisible}>Select visible</button>
              <button type="button" className="utxo-toolbar-btn" onClick={selectDust} disabled={boardStats.dust === 0}>
                Select dust
              </button>
              <button type="button" className="utxo-toolbar-btn" onClick={deselectAllUtxos}>Clear</button>
              <button type="button" className="utxo-toolbar-btn" onClick={fetchUtxos} disabled={loadingUtxos}>
                Refresh
              </button>
            </div>
          </div>

          <div className="utxo-board-legend" aria-hidden>
            <span><i className="utxo-swatch dust" /> Dust</span>
            <span><i className="utxo-swatch erg" /> ERG</span>
            <span><i className="utxo-swatch token" /> Tokens</span>
            <span><i className="utxo-swatch large" /> Large</span>
            <span className="utxo-board-stat mono">{boardStats.count} boxes · {formatErg(boardStats.totalErg)} ERG</span>
          </div>

          <div className="utxo-board-canvas" role="listbox" aria-multiselectable aria-label="UTXO boxes">
            {loadingUtxos ? (
              <div className="utxo-board-empty">
                <div className="spinner-small" />
                <span>Loading UTXOs…</span>
              </div>
            ) : filteredUtxos.length === 0 ? (
              <div className="utxo-board-empty">
                <span>{utxos.length === 0 ? 'No UTXOs found' : 'No boxes match this filter'}</span>
              </div>
            ) : (
              <div className="utxo-mosaic">
                {filteredUtxos.map((u, i) => {
                  const nano = ergNano(u)
                  const tier = valueTier(nano, maxNano)
                  const kind = boxKind(u)
                  const selected = selectedBoxIds.has(u.boxId)
                  const focused = focusedBoxId === u.boxId
                  return (
                    <button
                      key={u.boxId}
                      type="button"
                      role="option"
                      aria-selected={selected}
                      className={`utxo-tile tier-${tier} kind-${kind}${selected ? ' selected' : ''}${focused ? ' focused' : ''}`}
                      style={{ animationDelay: `${Math.min(i, 24) * 18}ms` }}
                      onClick={() => toggleUtxo(u.boxId)}
                      title={`${u.boxId}\n${formatErg(nano)} ERG${u.assets.length ? ` · ${u.assets.length} token(s)` : ''}`}
                    >
                      <span className="utxo-tile-erg mono">{formatErg(nano, 0, nano < DUST_NANO ? 4 : 2)}</span>
                      {u.assets.length > 0 && (
                        <span className="utxo-tile-tokens">{u.assets.length}</span>
                      )}
                      {selected && (
                        <span className="utxo-tile-check" aria-hidden>
                          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
                            <polyline points="20 6 9 17 4 12" />
                          </svg>
                        </span>
                      )}
                    </button>
                  )
                })}
              </div>
            )}
          </div>

          <div className="utxo-board-dock">
            {focusedBox && (
              <div className="utxo-focus-card">
                <code className="mono utxo-focus-id">
                  {focusedBox.boxId.slice(0, 10)}…{focusedBox.boxId.slice(-8)}
                </code>
                <span className="mono">{formatErg(ergNano(focusedBox))} ERG</span>
                {focusedBox.assets.length > 0 && (
                  <span className="utxo-focus-tokens">{focusedBox.assets.length} token type{focusedBox.assets.length !== 1 ? 's' : ''}</span>
                )}
              </div>
            )}
            <div className="utxo-dock-main">
              <div className="utxo-dock-stats">
                {selectedBoxIds.size === 0 ? (
                  <span className="utxo-dock-hint">Click boxes to select · size ≈ ERG value</span>
                ) : (
                  <>
                    <span className="utxo-select-badge">{selectedUtxosSummary.count}</span>
                    <span className="mono">{formatErg(selectedUtxosSummary.totalErg)} ERG</span>
                    {selectedUtxosSummary.tokenCount > 0 && (
                      <span>{selectedUtxosSummary.tokenCount} token type{selectedUtxosSummary.tokenCount !== 1 ? 's' : ''}</span>
                    )}
                    <span className="utxo-dock-out mono">
                      → {formatErg(Math.max(0, selectedUtxosSummary.totalErg - TX_FEE_NANO))} ERG
                    </span>
                  </>
                )}
              </div>
              <button
                type="button"
                className="utxo-submit-btn utxo-dock-action"
                onClick={() => setStep('confirm')}
                disabled={selectedBoxIds.size < 2}
              >
                {selectedBoxIds.size < 2 ? 'Select ≥2 boxes' : 'Review consolidation'}
              </button>
            </div>
            {error && <div className="message error">{error}</div>}
          </div>
        </div>
      )}

      {subTab === 'split' && (
        <div className="utxo-split-layout">
          <div className="utxo-split-panel">
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
                <label>Number of boxes (1–30)</label>
                <div className="utxo-split-alloc">
                  <input
                    type="range"
                    min={1}
                    max={30}
                    value={Math.min(30, Math.max(1, splitCountNum || 1))}
                    onChange={e => setSplitCount(e.target.value)}
                    className="utxo-split-range"
                    aria-label="Split box count"
                  />
                  <input
                    type="text"
                    inputMode="numeric"
                    value={splitCount}
                    onChange={e => setSplitCount(e.target.value.replace(/\D/g, ''))}
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
                    onChange={e => setSplitErgPerBox(e.target.value)}
                    placeholder="0.001"
                    className="utxo-split-input"
                  />
                </div>
              )}

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

          <div className="utxo-split-preview">
            <div className="utxo-split-flow">
              <div className="utxo-split-stage">
                <div className="utxo-split-preview-label">Before</div>
                {splitAmountNano > 0 && splitCountNum > 0 ? (
                  <div className="utxo-ghost-tile utxo-ghost-source kind-large">
                    <span className="utxo-ghost-caption">source</span>
                    <span className="mono utxo-ghost-value">
                      {splitType === 'erg'
                        ? formatErg(splitAmountNano * splitCountNum, 0, 2)
                        : splitTotalDisplay}
                    </span>
                    <span className="utxo-ghost-sub">
                      {splitCountNum}× →
                    </span>
                  </div>
                ) : (
                  <p className="utxo-split-preview-hint">Set amount &amp; count</p>
                )}
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
                  <p className="utxo-split-preview-hint">Adjust allocation to preview resulting boxes</p>
                )}
              </div>
            </div>
            <div className="utxo-board-legend utxo-split-legend" aria-hidden>
              <span><i className="utxo-swatch large" /> Source total</span>
              <span><i className="utxo-swatch erg" /> Output boxes</span>
              {splitType === 'token' && <span><i className="utxo-swatch token" /> Token split</span>}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
