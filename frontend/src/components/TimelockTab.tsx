import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  fetchMewLockState,
  getLockDurations,
  buildLockTx,
  buildUnlockTx,
  startMewLockSign,
  getMewLockTxStatus,
  formatErg,
  blocksToTime,
  formatUnlockStatus,
  truncateAddress,
  calculateFeePreview,
  type MewLockBox,
  type MewLockState,
  type LockDuration,
} from '../api/mewlock'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
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
        <div className="timelock-notice">Connect to a node to view timelocks</div>
      </div>
    )
  }

  if (capabilityTier === 'Basic') {
    return (
      <div className="timelock-tab">
        <div className="timelock-notice">
          MewLock requires an indexed node (Full or Extra tier)
        </div>
      </div>
    )
  }

  return (
    <div className="timelock-tab">
      {/* Header */}
      <div className="timelock-header">
        <div className="timelock-header-row">
          <div className="timelock-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="10" />
              <path d="M12 6v6l4 2" />
            </svg>
          </div>
          <div>
            <h2>MewLock Timelocks</h2>
            <p className="timelock-description">Lock ERG and tokens until a future block height</p>
          </div>
          <button
            className="timelock-create-btn"
            onClick={() => setShowCreateModal(true)}
            disabled={!walletAddress}
          >
            + Create Lock
          </button>
        </div>
      </div>

      {/* Info Bar */}
      {state && (
        <div className="timelock-info-bar">
          <div className="timelock-info-item">
            <span className="timelock-info-label">Total Locks</span>
            <span className="timelock-info-value">{state.totalLocks}</span>
          </div>
          <div className="timelock-info-divider" />
          <div className="timelock-info-item">
            <span className="timelock-info-label">My Locks</span>
            <span className="timelock-info-value">{state.ownLocks}</span>
          </div>
          <div className="timelock-info-divider" />
          <div className="timelock-info-item">
            <span className="timelock-info-label">Height</span>
            <span className="timelock-info-value">{state.currentHeight.toLocaleString()}</span>
          </div>
        </div>
      )}

      {/* Filter Bar */}
      <div className="timelock-tab-bar">
        {(['all', 'mine', 'unlockable'] as Filter[]).map(f => (
          <button
            key={f}
            className={`timelock-tab-btn ${filter === f ? 'active' : ''}`}
            onClick={() => setFilter(f)}
          >
            {f === 'all' ? 'All Locks' : f === 'mine' ? 'My Locks' : 'Unlockable'}
          </button>
        ))}
        <div className="timelock-controls">
          <button
            className="timelock-refresh-btn"
            onClick={fetchState}
            disabled={loading}
          >
            {loading ? 'Loading...' : 'Refresh'}
          </button>
        </div>
      </div>

      {/* Sort Bar */}
      <div className="timelock-sort-bar">
        <span className="timelock-sort-label">Sort:</span>
        {([
          ['newest', 'Newest'],
          ['value', 'Value'],
          ['unlock', 'Unlock Time'],
        ] as [SortKey, string][]).map(([key, label]) => (
          <button
            key={key}
            className={`timelock-sort-btn ${sortKey === key ? 'active' : ''}`}
            onClick={() => setSortKey(key)}
          >
            {label}
          </button>
        ))}
      </div>

      {/* Error */}
      {error && <div className="timelock-error">{error}</div>}

      {/* Loading */}
      {loading && !state && (
        <div className="timelock-loading">
          <span className="spinner-small" />
          Loading timelocks...
        </div>
      )}

      {/* Empty State */}
      {state && filteredLocks.length === 0 && !loading && (
        <div className="empty-state">
          {filter === 'all'
            ? 'No timelocks found on chain'
            : filter === 'mine'
            ? 'You have no timelocks'
            : 'No unlockable locks found'}
        </div>
      )}

      {/* Card Grid */}
      {filteredLocks.length > 0 && (
        <div className="timelock-grid">
          {filteredLocks.map(lock => (
            <LockCard
              key={lock.boxId}
              lock={lock}
              onUnlock={() => setUnlockTarget(lock)}
              explorerUrl={explorerUrl}
            />
          ))}
        </div>
      )}

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
  explorerUrl: _explorerUrl,
}: {
  lock: MewLockBox
  onUnlock: () => void
  explorerUrl: string
}) {
  void _explorerUrl
  const isLocked = lock.blocksRemaining > 0

  return (
    <div className={`timelock-card ${lock.isOwn ? 'own' : ''} ${lock.isUnlockable ? 'unlockable' : ''}`}>
      <div className="timelock-card-header">
        <div className="timelock-card-header-left">
          {lock.lockName && (
            <span className="timelock-lock-name">{lock.lockName}</span>
          )}
          {!lock.lockName && (
            <span className="timelock-lock-name" style={{ color: 'var(--slate-500)' }}>
              Lock #{lock.boxId.slice(0, 8)}
            </span>
          )}
          {lock.isOwn && <span className="timelock-own-badge">Your Lock</span>}
        </div>
        <span className={`timelock-status-badge ${isLocked ? 'locked' : 'unlockable'}`}>
          {isLocked ? 'Locked' : 'Unlockable'}
        </span>
      </div>

      <div className="timelock-card-body">
        <div className="timelock-row">
          <span className="timelock-row-label">ERG Value</span>
          <span className="timelock-row-value highlight">{formatErg(lock.ergValue)} ERG</span>
        </div>

        {lock.tokens.length > 0 && (
          <div className="timelock-row">
            <span className="timelock-row-label">Tokens</span>
            <span className="timelock-row-value">
              {lock.tokens.map(t =>
                `${t.amount.toLocaleString()} ${t.name || t.tokenId.slice(0, 8) + '...'}`
              ).join(', ')}
            </span>
          </div>
        )}

        <div className="timelock-row">
          <span className="timelock-row-label">Unlock Height</span>
          <span className="timelock-row-value">{lock.unlockHeight.toLocaleString()}</span>
        </div>

        <div className="timelock-row">
          <span className="timelock-row-label">Status</span>
          <span className={`timelock-row-value ${isLocked ? '' : 'success'}`}>
            {formatUnlockStatus(lock.blocksRemaining)}
          </span>
        </div>

        <div className="timelock-row">
          <span className="timelock-row-label">Owner</span>
          <span className="timelock-row-value mono">{truncateAddress(lock.depositorAddress)}</span>
        </div>

        {lock.lockDescription && (
          <div className="timelock-row">
            <span className="timelock-row-label">Description</span>
            <span className="timelock-row-value mono" style={{ fontSize: '11px' }}>
              {lock.lockDescription.length > 60
                ? lock.lockDescription.slice(0, 60) + '...'
                : lock.lockDescription}
            </span>
          </div>
        )}
      </div>

      {lock.isUnlockable && (
        <div className="timelock-card-actions">
          <button className="timelock-action-btn primary" onClick={onUnlock}>
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
    pollStatus: getMewLockTxStatus,
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

      const signResult = await startMewLockSign(tx, 'Lock assets in MewLock timelock')
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
      <div className="modal-card" onClick={e => e.stopPropagation()} style={{ maxWidth: 520 }}>
        <div className="modal-header">
          <h2>Create Timelock</h2>
          <button className="modal-close" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div style={{ padding: '16px 24px' }}>
          {step === 'input' && (
            <div className="timelock-modal-form">
              {/* Duration Selection */}
              <div className="form-group">
                <label className="form-label">Lock Duration</label>
                <div className="timelock-duration-grid">
                  {durations.map(d => (
                    <button
                      key={d.blocks}
                      className={`timelock-duration-btn ${selectedDuration === d.blocks ? 'active' : ''}`}
                      onClick={() => { setSelectedDuration(d.blocks); setCustomBlocks('') }}
                    >
                      {d.label}
                    </button>
                  ))}
                  <button
                    className={`timelock-duration-btn ${selectedDuration === null && customBlocks ? 'active' : ''}`}
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
                <div className="timelock-fee-preview">
                  Unlock at block <span>{unlockHeight.toLocaleString()}</span>
                  {' '}(~{blocksToTime(unlockHeight - currentHeight)} from now)
                </div>
              )}

              {/* ERG Amount */}
              <div className="form-group">
                <label className="form-label">
                  ERG Amount
                  <span style={{ float: 'right', color: 'var(--slate-500)', fontWeight: 400 }}>
                    Balance: {walletBalance.erg_formatted} ERG
                  </span>
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
                <div className="timelock-fee-preview">
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

              {error && <div className="timelock-error">{error}</div>}

              <button
                className="btn btn-primary"
                onClick={handleSubmit}
                disabled={!canSubmit || buildLoading}
                style={{ width: '100%' }}
              >
                {buildLoading ? 'Building...' : 'Create Lock'}
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'choose' && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12, alignItems: 'center', padding: '24px 0' }}>
              <p style={{ color: 'var(--slate-400)', textAlign: 'center' }}>Choose signing method</p>
              <button className="btn btn-primary" onClick={flow.handleNautilusSign} style={{ width: '100%' }}>
                Sign with Nautilus
              </button>
              <button className="btn btn-secondary" onClick={flow.handleMobileSign} style={{ width: '100%' }}>
                Scan QR Code (ErgoPay)
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'nautilus' && (
            <div style={{ textAlign: 'center', padding: '24px 0' }}>
              <p style={{ color: 'var(--slate-400)' }}>Waiting for Nautilus confirmation...</p>
              <div className="spinner-small" style={{ margin: '16px auto' }} />
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice} style={{ marginTop: 12 }}>
                Back
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
            <div style={{ textAlign: 'center', padding: '16px 0' }}>
              <p style={{ color: 'var(--slate-400)', marginBottom: 12 }}>Scan with ErgoPay wallet</p>
              <QRCodeSVG value={flow.qrUrl} size={200} bgColor="transparent" fgColor="#e2e8f0" />
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice} style={{ marginTop: 12 }}>
                Back
              </button>
            </div>
          )}

          {step === 'success' && flow.txId && (
            <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />
          )}

          {step === 'error' && (
            <div style={{ textAlign: 'center', padding: '24px 0' }}>
              <div className="timelock-error" style={{ marginBottom: 12 }}>{error}</div>
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
  walletBalance: _walletBalance,
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
  void _walletBalance
  const [step, setStep] = useState<'confirm' | 'building' | 'signing' | 'success' | 'error'>('confirm')
  const [error, setError] = useState<string | null>(null)
  const [buildLoading, setBuildLoading] = useState(false)

  const flow = useTransactionFlow({
    pollStatus: getMewLockTxStatus,
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

      // Fetch the lock box as EIP-12 input
      const lockBox = await invoke<object>('get_box_by_id', { boxId: lock.boxId })

      const tx = await buildUnlockTx(
        JSON.stringify(lockBox),
        ergoTree,
        userUtxos,
        currentHeight,
      )

      const signResult = await startMewLockSign(tx, 'Unlock MewLock timelock')
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
      <div className="modal-card" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
        <div className="modal-header">
          <h2>Unlock Timelock</h2>
          <button className="modal-close" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div style={{ padding: '16px 24px' }}>
          {step === 'confirm' && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              {lock.lockName && (
                <div className="timelock-row">
                  <span className="timelock-row-label">Lock</span>
                  <span className="timelock-row-value">{lock.lockName}</span>
                </div>
              )}
              <div className="timelock-row">
                <span className="timelock-row-label">Locked ERG</span>
                <span className="timelock-row-value">{formatErg(lock.ergValue)} ERG</span>
              </div>
              <div className="timelock-row">
                <span className="timelock-row-label">Fee (3%)</span>
                <span className="timelock-row-value" style={{ color: 'var(--red-400)' }}>
                  -{formatErg(ergFee)} ERG
                </span>
              </div>
              <div className="timelock-row">
                <span className="timelock-row-label">You Receive</span>
                <span className="timelock-row-value success">~{formatErg(ergReceive)} ERG</span>
              </div>

              {lock.tokens.length > 0 && (
                <>
                  <div style={{ borderTop: '1px solid rgba(51,65,85,0.5)', margin: '4px 0' }} />
                  {lock.tokens.map(t => {
                    const tokenFee = t.amount > 34 ? Math.floor((t.amount * 3000) / 100000) : 0
                    return (
                      <div key={t.tokenId} className="timelock-row">
                        <span className="timelock-row-label">{t.name || t.tokenId.slice(0, 8) + '...'}</span>
                        <span className="timelock-row-value">
                          {(t.amount - tokenFee).toLocaleString()}
                          {tokenFee > 0 && (
                            <span style={{ color: 'var(--slate-500)', fontSize: '11px' }}> (-{tokenFee} fee)</span>
                          )}
                        </span>
                      </div>
                    )
                  })}
                </>
              )}

              {error && <div className="timelock-error">{error}</div>}

              <button
                className="btn btn-primary"
                onClick={handleUnlock}
                disabled={buildLoading}
                style={{ width: '100%', marginTop: 8 }}
              >
                {buildLoading ? 'Building...' : 'Confirm Unlock'}
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'choose' && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12, alignItems: 'center', padding: '24px 0' }}>
              <p style={{ color: 'var(--slate-400)', textAlign: 'center' }}>Choose signing method</p>
              <button className="btn btn-primary" onClick={flow.handleNautilusSign} style={{ width: '100%' }}>
                Sign with Nautilus
              </button>
              <button className="btn btn-secondary" onClick={flow.handleMobileSign} style={{ width: '100%' }}>
                Scan QR Code (ErgoPay)
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'nautilus' && (
            <div style={{ textAlign: 'center', padding: '24px 0' }}>
              <p style={{ color: 'var(--slate-400)' }}>Waiting for Nautilus confirmation...</p>
              <div className="spinner-small" style={{ margin: '16px auto' }} />
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice} style={{ marginTop: 12 }}>
                Back
              </button>
            </div>
          )}

          {step === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
            <div style={{ textAlign: 'center', padding: '16px 0' }}>
              <p style={{ color: 'var(--slate-400)', marginBottom: 12 }}>Scan with ErgoPay wallet</p>
              <QRCodeSVG value={flow.qrUrl} size={200} bgColor="transparent" fgColor="#e2e8f0" />
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice} style={{ marginTop: 12 }}>
                Back
              </button>
            </div>
          )}

          {step === 'success' && flow.txId && (
            <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />
          )}

          {step === 'error' && (
            <div style={{ textAlign: 'center', padding: '24px 0' }}>
              <div className="timelock-error" style={{ marginBottom: 12 }}>{error}</div>
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
