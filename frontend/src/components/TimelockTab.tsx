import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  fetchMewLockState,
  getLockDurations,
  buildLockTx,
  buildUnlockTx,
  formatUnlockStatus,
  calculateFeePreview,
  type MewLockBox,
  type MewLockState,
  type LockDuration,
} from '../api/mewlock'
import { formatErg, blocksToTime, truncateAddress } from '../utils/format'
import { startSign, getTxStatus } from '../api/types'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import { EmptyState } from './ui'
import './TimelockTab.css'

interface WalletBalance {
  address: string
  erg_nano: number
  erg_formatted: string
  tokens: Array<{
    token_id: string
    amount: number
    name: string | null
    decimals: number
  }>
}

interface TimelockTabProps {
  isConnected: boolean
  capabilityTier?: string
  walletAddress: string | null
  walletBalance: WalletBalance | null
  explorerUrl: string
}

type Filter = 'all' | 'mine' | 'unlockable'
type SortKey = 'newest' | 'value' | 'unlock'

export function TimelockTab({
  isConnected,
  capabilityTier,
  walletAddress,
  walletBalance,
  explorerUrl,
}: TimelockTabProps) {
  const [state, setState] = useState<MewLockState | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [filter, setFilter] = useState<Filter>('all')
  const [sortKey, setSortKey] = useState<SortKey>('newest')

  // Create lock modal
  const [showCreateModal, setShowCreateModal] = useState(false)

  // Unlock modal
  const [unlockTarget, setUnlockTarget] = useState<MewLockBox | null>(null)

  const fetchState = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') return
    setLoading(true)
    setError(null)
    try {
      const result = await fetchMewLockState(walletAddress ?? undefined)
      setState(result)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [isConnected, capabilityTier, walletAddress])

  useEffect(() => {
    fetchState()
  }, [fetchState])

  // Filter and sort locks
  const filteredLocks = (state?.locks ?? [])
    .filter(lock => {
      if (filter === 'mine') return lock.isOwn
      if (filter === 'unlockable') return lock.isUnlockable
      return true
    })
    .sort((a, b) => {
      switch (sortKey) {
        case 'value': return b.ergValue - a.ergValue
        case 'unlock': return a.blocksRemaining - b.blocksRemaining
        default: return b.creationHeight - a.creationHeight
      }
    })

  if (!isConnected) {
    return (
      <div className="timelock-tab">
        <div className="tl-empty-wrap">
          <EmptyState title="Node Not Connected" description="Connect to a node to view timelocks." />
        </div>
      </div>
    )
  }

  if (capabilityTier === 'Basic') {
    return (
      <div className="timelock-tab">
        <div className="tl-empty-wrap">
          <EmptyState title="Indexed Node Required" description="MewLock requires an indexed node (Full or Extra tier)." />
        </div>
      </div>
    )
  }

  return (
    <div className="timelock-tab">
      <header className="tl-header">
        <div className="tl-header-left">
          <div className="tl-icon" aria-hidden>
            <img src="/icons/mew.png" alt="" />
          </div>
          <div>
            <h1 className="tl-title">MewLock Timelocks</h1>
            <p className="tl-subtitle">Lock ERG and tokens until a future block height</p>
          </div>
        </div>
        <div className="tl-header-meta">
          {state && (
            <>
              <span className="tl-meta-chip">Total <strong className="mono">{state.totalLocks}</strong></span>
              <span className="tl-meta-chip">Mine <strong className="mono">{state.ownLocks}</strong></span>
              <span className="tl-meta-chip mono">Height {state.currentHeight.toLocaleString()}</span>
            </>
          )}
          <button
            className="tl-create-btn"
            onClick={() => setShowCreateModal(true)}
            disabled={!walletAddress}
          >
            + Create Lock
          </button>
        </div>
      </header>

      <div className="tl-controls">
        <div className="tl-filter-tabs" role="tablist" aria-label="Filter locks">
          {([
            ['all', 'All Locks'],
            ['mine', 'My Locks'],
            ['unlockable', 'Unlockable'],
          ] as [Filter, string][]).map(([key, label]) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={filter === key}
              className={`tl-filter-tab ${filter === key ? 'active' : ''}`}
              onClick={() => setFilter(key)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="tl-sort-group">
          <span className="tl-sort-label">Sort</span>
          {([
            ['newest', 'Newest'],
            ['value', 'Value'],
            ['unlock', 'Unlock Time'],
          ] as [SortKey, string][]).map(([key, label]) => (
            <button
              key={key}
              type="button"
              className={`tl-sort-btn ${sortKey === key ? 'active' : ''}`}
              onClick={() => setSortKey(key)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {error && <div className="tl-error">{error}</div>}

      <div className="tl-body">
        {loading && !state ? (
          <div className="tl-state">
            <span className="spinner-small" />
            Loading timelocks…
          </div>
        ) : state && filteredLocks.length === 0 ? (
          <EmptyState
            title="No Timelocks"
            description={
              filter === 'all'
                ? 'No timelocks found on chain.'
                : filter === 'mine'
                ? 'You have no timelocks.'
                : 'No unlockable locks found.'
            }
          />
        ) : (
          <div className="tl-grid">
            {filteredLocks.map(lock => (
              <LockCard
                key={lock.boxId}
                lock={lock}
                onUnlock={() => setUnlockTarget(lock)}
              />
            ))}
          </div>
        )}
      </div>

      {/* Create Lock Modal */}
      {showCreateModal && walletAddress && walletBalance && (
        <CreateLockModal
          onClose={() => setShowCreateModal(false)}
          walletAddress={walletAddress}
          walletBalance={walletBalance}
          currentHeight={state?.currentHeight ?? 0}
          explorerUrl={explorerUrl}
          onSuccess={fetchState}
        />
      )}

      {/* Unlock Modal */}
      {unlockTarget && walletAddress && walletBalance && (
        <UnlockModal
          lock={unlockTarget}
          onClose={() => setUnlockTarget(null)}
          walletAddress={walletAddress}
          walletBalance={walletBalance}
          currentHeight={state?.currentHeight ?? 0}
          explorerUrl={explorerUrl}
          onSuccess={fetchState}
        />
      )}
    </div>
  )
}

// =============================================================================
// Lock Card
// =============================================================================

function LockCard({
  lock,
  onUnlock,
}: {
  lock: MewLockBox
  onUnlock: () => void
}) {
  const isLocked = lock.blocksRemaining > 0

  return (
    <div className={`tl-card ${lock.isOwn ? 'own' : ''} ${lock.isUnlockable ? 'unlockable' : ''}`}>
      <div className="tl-card-head">
        <div className="tl-card-head-left">
          <span className="tl-lock-name">
            {lock.lockName || `Lock #${lock.boxId.slice(0, 8)}`}
          </span>
          {lock.isOwn && <span className="tl-chip tl-chip--own">Yours</span>}
        </div>
        <span className={`tl-status ${isLocked ? 'locked' : 'unlockable'}`}>
          {isLocked ? 'Locked' : 'Unlockable'}
        </span>
      </div>

      <div className="tl-card-body">
        <div className="tl-row">
          <span className="tl-row-label">ERG Value</span>
          <span className="tl-row-value tl-row-value--accent mono">{formatErg(lock.ergValue)} ERG</span>
        </div>

        {lock.tokens.length > 0 && (
          <div className="tl-row">
            <span className="tl-row-label">Tokens</span>
            <span className="tl-row-value mono">
              {lock.tokens.map(t =>
                `${t.amount.toLocaleString()} ${t.name || t.tokenId.slice(0, 8) + '…'}`
              ).join(', ')}
            </span>
          </div>
        )}

        <div className="tl-row">
          <span className="tl-row-label">Unlock Height</span>
          <span className="tl-row-value mono">{lock.unlockHeight.toLocaleString()}</span>
        </div>

        <div className="tl-row">
          <span className="tl-row-label">Status</span>
          <span className={`tl-row-value ${isLocked ? '' : 'tl-row-value--success'}`}>
            {formatUnlockStatus(lock.blocksRemaining)}
          </span>
        </div>

        <div className="tl-row">
          <span className="tl-row-label">Owner</span>
          <span className="tl-row-value tl-row-value--faint mono">{truncateAddress(lock.depositorAddress)}</span>
        </div>

        {lock.lockDescription && (
          <div className="tl-row">
            <span className="tl-row-label">Description</span>
            <span className="tl-row-value tl-row-value--faint mono">
              {lock.lockDescription.length > 60
                ? lock.lockDescription.slice(0, 60) + '…'
                : lock.lockDescription}
            </span>
          </div>
        )}
      </div>

      {lock.isUnlockable && (
        <div className="tl-card-actions">
          <button className="tl-action-btn primary" onClick={onUnlock}>
            Unlock
          </button>
        </div>
      )}
    </div>
  )
}

// =============================================================================
// Create Lock Modal
// =============================================================================

function CreateLockModal({
  onClose,
  walletAddress,
  walletBalance,
  currentHeight,
  explorerUrl,
  onSuccess,
}: {
  onClose: () => void
  walletAddress: string
  walletBalance: WalletBalance
  currentHeight: number
  explorerUrl: string
  onSuccess: () => void
}) {
  const [step, setStep] = useState<'input' | 'building' | 'signing' | 'success' | 'error'>('input')
  const [durations, setDurations] = useState<LockDuration[]>([])
  const [selectedDuration, setSelectedDuration] = useState<number | null>(null)
  const [customBlocks, setCustomBlocks] = useState('')
  const [ergAmount, setErgAmount] = useState('')
  const [lockName, setLockName] = useState('')
  const [lockDescription, setLockDescription] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [buildLoading, setBuildLoading] = useState(false)

  const flow = useTransactionFlow({
    pollStatus: getTxStatus,
    isOpen: true,
    onSuccess: () => { setStep('success'); onSuccess() },
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'MewLock', operation: 'lock', description: 'Create timelock' },
  })

  useEffect(() => {
    getLockDurations().then(setDurations).catch(console.error)
  }, [])

  const unlockHeight = selectedDuration !== null
    ? currentHeight + selectedDuration
    : customBlocks
    ? currentHeight + parseInt(customBlocks, 10)
    : 0

  const ergNano = Math.floor(parseFloat(ergAmount || '0') * 1_000_000_000)
  const feePreview = calculateFeePreview(ergNano)

  const canSubmit = ergNano > 0 && unlockHeight > currentHeight

  const handleSubmit = async () => {
    if (!canSubmit) return
    setBuildLoading(true)
    setError(null)

    try {
      const userUtxos = await invoke<object[]>('get_user_utxos')
      const ergoTree = await invoke<string>('validate_ergo_address', { address: walletAddress })

      const tx = await buildLockTx(
        ergoTree,
        ergNano.toString(),
        '[]',
        unlockHeight,
        Math.floor(Date.now() / 1000).toString(),
        lockName || null,
        lockDescription || null,
        userUtxos,
        currentHeight,
      )

      const signResult = await startSign(tx, 'Lock assets in MewLock timelock')
      setStep('signing')
      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setBuildLoading(false)
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-card" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Create Timelock</h2>
          <button className="modal-close" onClick={onClose}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-body">
          {step === 'input' && (
            <div className="tl-modal-form">
              {/* Duration Selection */}
              <div className="form-group">
                <label className="form-label">Lock Duration</label>
                <div className="tl-duration-grid">
                  {durations.map(d => (
                    <button
                      key={d.blocks}
                      className={`tl-duration-btn ${selectedDuration === d.blocks ? 'active' : ''}`}
                      onClick={() => { setSelectedDuration(d.blocks); setCustomBlocks('') }}
                    >
                      {d.label}
                    </button>
                  ))}
                  <button
                    className={`tl-duration-btn ${selectedDuration === null && customBlocks ? 'active' : ''}`}
                    onClick={() => setSelectedDuration(null)}
                  >
                    Custom
                  </button>
                </div>
                {selectedDuration === null && (
                  <input
                    type="number"
                    className="input"
                    placeholder="Blocks (e.g. 43200 for ~60 days)"
                    value={customBlocks}
                    onChange={e => setCustomBlocks(e.target.value)}
                    style={{ marginTop: 8 }}
                  />
                )}
              </div>

              {/* Unlock Height Preview */}
              {unlockHeight > 0 && (
                <div className="tl-fee-preview">
                  Unlock at block <span>{unlockHeight.toLocaleString()}</span>
                  {' '}(~{blocksToTime(unlockHeight - currentHeight)} from now)
                </div>
              )}

              {/* ERG Amount */}
              <div className="form-group">
                <label className="form-label">
                  ERG Amount
                  <span className="tl-balance-hint">Balance: {walletBalance.erg_formatted} ERG</span>
                </label>
                <input
                  type="number"
                  className="input"
                  placeholder="0.00"
                  value={ergAmount}
                  onChange={e => setErgAmount(e.target.value)}
                  min="0"
                  step="0.01"
                />
              </div>

              {/* Fee Preview */}
              {ergNano > 0 && (
                <div className="tl-fee-preview">
                  Withdrawal fee: <span>{formatErg(feePreview)} ERG</span> (3%)
                </div>
              )}

              {/* Lock Name */}
              <div className="form-group">
                <label className="form-label">Lock Name (optional)</label>
                <input
                  type="text"
                  className="input"
                  placeholder="My savings lock"
                  value={lockName}
                  onChange={e => setLockName(e.target.value)}
                  maxLength={64}
                />
              </div>

              {/* Lock Description */}
              <div className="form-group">
                <label className="form-label">Description (optional)</label>
                <input
                  type="text"
                  className="input"
                  placeholder="Locking until..."
                  value={lockDescription}
                  onChange={e => setLockDescription(e.target.value)}
                  maxLength={128}
                />
              </div>

              {error && <div className="tl-error">{error}</div>}

              <button
                className="btn btn-primary tl-submit-btn"
                onClick={handleSubmit}
                disabled={!canSubmit || buildLoading}
              >
                {buildLoading ? 'Building...' : 'Create Lock'}
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'choose' && (
            <div className="tl-sign-choice">
              <p>Choose signing method</p>
              <button className="btn btn-primary" onClick={flow.handleNautilusSign}>
                Sign with Nautilus
              </button>
              <button className="btn btn-secondary" onClick={flow.handleMobileSign}>
                Scan QR Code (ErgoPay)
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'nautilus' && (
            <div className="tl-sign-wait">
              <p>Waiting for Nautilus confirmation...</p>
              <div className="spinner-small" />
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                Back
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
            <div className="tl-sign-qr">
              <p>Scan with ErgoPay wallet</p>
              <QRCodeSVG value={flow.qrUrl} size={200} bgColor="transparent" fgColor="#e2e8f0" />
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                Back
              </button>
            </div>
          )}

          {step === 'success' && flow.txId && (
            <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />
          )}

          {step === 'error' && (
            <div className="tl-sign-wait">
              <div className="tl-error">{error}</div>
              <button className="btn btn-secondary" onClick={() => { setStep('input'); setError(null) }}>
                Try Again
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

// =============================================================================
// Unlock Modal
// =============================================================================

function UnlockModal({
  lock,
  onClose,
  walletAddress,
  currentHeight,
  explorerUrl,
  onSuccess,
}: {
  lock: MewLockBox
  onClose: () => void
  walletAddress: string
  walletBalance: WalletBalance
  currentHeight: number
  explorerUrl: string
  onSuccess: () => void
}) {
  const [step, setStep] = useState<'confirm' | 'building' | 'signing' | 'success' | 'error'>('confirm')
  const [error, setError] = useState<string | null>(null)
  const [buildLoading, setBuildLoading] = useState(false)

  const flow = useTransactionFlow({
    pollStatus: getTxStatus,
    isOpen: true,
    onSuccess: () => { setStep('success'); onSuccess() },
    onError: (err) => { setError(err); setStep('error') },
    watchParams: { protocol: 'MewLock', operation: 'unlock', description: 'Unlock timelock' },
  })

  const ergFee = calculateFeePreview(lock.ergValue)
  const ergReceive = lock.ergValue - ergFee

  const handleUnlock = async () => {
    setBuildLoading(true)
    setError(null)

    try {
      const userUtxos = await invoke<object[]>('get_user_utxos')
      const ergoTree = await invoke<string>('validate_ergo_address', { address: walletAddress })

      const tx = await buildUnlockTx(
        lock.boxId,
        ergoTree,
        userUtxos,
        currentHeight,
      )

      const signResult = await startSign(tx, 'Unlock MewLock timelock')
      setStep('signing')
      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
    } catch (e) {
      setError(String(e))
      setStep('error')
    } finally {
      setBuildLoading(false)
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-card" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Unlock Timelock</h2>
          <button className="modal-close" onClick={onClose}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="modal-body">
          {step === 'confirm' && (
            <div className="tl-confirm-form">
              {lock.lockName && (
                <div className="tl-row">
                  <span className="tl-row-label">Lock</span>
                  <span className="tl-row-value">{lock.lockName}</span>
                </div>
              )}
              <div className="tl-row">
                <span className="tl-row-label">Locked ERG</span>
                <span className="tl-row-value mono">{formatErg(lock.ergValue)} ERG</span>
              </div>
              <div className="tl-row">
                <span className="tl-row-label">Fee (3%)</span>
                <span className="tl-row-value tl-row-value--danger mono">
                  -{formatErg(ergFee)} ERG
                </span>
              </div>
              <div className="tl-row">
                <span className="tl-row-label">You Receive</span>
                <span className="tl-row-value tl-row-value--success mono">~{formatErg(ergReceive)} ERG</span>
              </div>

              {lock.tokens.length > 0 && (
                <>
                  <div className="tl-divider" />
                  {lock.tokens.map(t => {
                    const tokenFee = t.amount > 34 ? Math.floor((t.amount * 3000) / 100000) : 0
                    return (
                      <div key={t.tokenId} className="tl-row">
                        <span className="tl-row-label">{t.name || t.tokenId.slice(0, 8) + '…'}</span>
                        <span className="tl-row-value mono">
                          {(t.amount - tokenFee).toLocaleString()}
                          {tokenFee > 0 && (
                            <span className="tl-row-value--faint"> (-{tokenFee} fee)</span>
                          )}
                        </span>
                      </div>
                    )
                  })}
                </>
              )}

              {error && <div className="tl-error">{error}</div>}

              <button
                className="btn btn-primary tl-submit-btn"
                onClick={handleUnlock}
                disabled={buildLoading}
              >
                {buildLoading ? 'Building...' : 'Confirm Unlock'}
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'choose' && (
            <div className="tl-sign-choice">
              <p>Choose signing method</p>
              <button className="btn btn-primary" onClick={flow.handleNautilusSign}>
                Sign with Nautilus
              </button>
              <button className="btn btn-secondary" onClick={flow.handleMobileSign}>
                Scan QR Code (ErgoPay)
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'nautilus' && (
            <div className="tl-sign-wait">
              <p>Waiting for Nautilus confirmation...</p>
              <div className="spinner-small" />
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                Back
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
            <div className="tl-sign-qr">
              <p>Scan with ErgoPay wallet</p>
              <QRCodeSVG value={flow.qrUrl} size={200} bgColor="transparent" fgColor="#e2e8f0" />
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>
                Back
              </button>
            </div>
          )}

          {step === 'success' && flow.txId && (
            <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />
          )}

          {step === 'error' && (
            <div className="tl-sign-wait">
              <div className="tl-error">{error}</div>
              <button className="btn btn-secondary" onClick={() => { setStep('confirm'); setError(null) }}>
                Try Again
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
