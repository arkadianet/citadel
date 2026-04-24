import { useState, useCallback, useEffect, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  type RecoverableStake,
  type RecoveryScan,
  scanErgopadRecoverableStakes,
  buildErgopadRecoveryTx,
  previewErgopadRecovery,
} from '../api/ergopadRecovery'
import { startSign, getTxStatus } from '../api/types'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import { PageHeader, Card, CardHeader, CardBody, CardFooter, EmptyState } from './ui'
import { TxSuccess } from './TxSuccess'
import './ErgopadRecoveryTab.css'

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

interface ErgopadRecoveryTabProps {
  isConnected: boolean
  capabilityTier?: string
  walletAddress: string | null
  walletBalance: WalletBalance | null
  explorerUrl: string
}

type RedeemStep = 'idle' | 'building' | 'signing' | 'success' | 'error'

export function ErgopadRecoveryTab({
  isConnected,
  capabilityTier,
  walletAddress,
  walletBalance,
  explorerUrl,
}: ErgopadRecoveryTabProps) {
  const [scan, setScan] = useState<RecoveryScan | null>(null)
  const [scanning, setScanning] = useState(false)
  const [scanError, setScanError] = useState<string | null>(null)

  // Ad-hoc lookup: paste any stake-key token ID to see the pending stake (even
  // for keys not in this wallet — useful for checking a key before buying it).
  const [lookupInput, setLookupInput] = useState('')
  const [lookupResult, setLookupResult] = useState<RecoverableStake | null>(null)
  const [lookupError, setLookupError] = useState<string | null>(null)
  const [lookingUp, setLookingUp] = useState(false)

  const [activeKey, setActiveKey] = useState<RecoverableStake | null>(null)
  const [redeemStep, setRedeemStep] = useState<RedeemStep>('idle')
  const [redeemError, setRedeemError] = useState<string | null>(null)

  const flow = useTransactionFlow({
    pollStatus: getTxStatus,
    isOpen: redeemStep !== 'idle',
    onSuccess: () => setRedeemStep('success'),
    onError: (err) => {
      setRedeemError(err)
      setRedeemStep('error')
    },
    watchParams: {
      protocol: 'ErgopadRecovery',
      operation: 'redeem',
      description: 'Ergopad v1 stake recovery',
    },
  })

  const candidateTokenIds = useMemo(() => {
    if (!walletBalance) return [] as string[]
    // Any singleton token in the wallet is a candidate stake-key NFT. We intentionally
    // don't filter by `t.name === 'ergopad Stake Key'` because some node configurations
    // don't populate token names, and the scan cost of including extra singletons is zero.
    return walletBalance.tokens
      .filter(t => t.amount === 1 && t.decimals === 0)
      .map(t => t.token_id)
  }, [walletBalance])

  const handleLookup = useCallback(async () => {
    const id = lookupInput.trim().toLowerCase()
    if (!/^[0-9a-f]{64}$/.test(id)) {
      setLookupError('Enter a 64-character hex token ID')
      setLookupResult(null)
      return
    }
    setLookingUp(true)
    setLookupError(null)
    setLookupResult(null)
    try {
      const stake = await previewErgopadRecovery(id)
      setLookupResult(stake)
    } catch (e) {
      setLookupError(String(e))
    } finally {
      setLookingUp(false)
    }
  }, [lookupInput])

  const handleScan = useCallback(async () => {
    if (!walletBalance) return
    setScanning(true)
    setScanError(null)
    try {
      const result = await scanErgopadRecoverableStakes(candidateTokenIds)
      setScan(result)
    } catch (e) {
      setScanError(String(e))
    } finally {
      setScanning(false)
    }
  }, [walletBalance, candidateTokenIds])

  // Auto-scan whenever the candidate set changes. Keyed on a stable signature
  // (sorted-joined IDs) so we don't refire when React hands us a new array
  // reference with the same contents.
  const candidatesKey = useMemo(
    () => [...candidateTokenIds].sort().join(','),
    [candidateTokenIds],
  )
  useEffect(() => {
    if (candidatesKey.length === 0) {
      setScan(null)
      return
    }
    handleScan()
    // handleScan intentionally not in deps — candidatesKey already reflects its
    // only material input, and we want exactly one scan per candidate-set change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [candidatesKey])

  const closeRedeem = useCallback(() => {
    setActiveKey(null)
    setRedeemStep('idle')
    setRedeemError(null)
    flow.reset()
  }, [flow])

  const handleRedeem = useCallback(async (stake: RecoverableStake) => {
    setActiveKey(stake)
    setRedeemStep('building')
    setRedeemError(null)
    try {
      const utxos = await invoke<Array<{ ergo_tree?: string; ergoTree?: string }>>('get_user_utxos')
      if (!utxos?.length) throw new Error('No UTXOs available')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const unsignedTx = await buildErgopadRecoveryTx(
        stake.stakeKeyId,
        utxos as object[],
        nodeStatus.chain_height,
      )

      const signResult = await startSign(
        unsignedTx,
        `Recover ${stake.ergopadAmountDisplay} ERGOPAD from stake key ${stake.stakeKeyId.slice(0, 8)}...`,
      )

      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setRedeemStep('signing')
    } catch (e) {
      setRedeemError(String(e))
      setRedeemStep('error')
    }
  }, [flow])

  if (!isConnected || capabilityTier === 'Basic') {
    return (
      <div className="ergopad-recovery-tab">
        <PageHeader
          icon={<RecoveryIcon />}
          title="Ergopad Recovery"
          subtitle="Unstake from the v1 Ergopad contracts using abandoned stake key NFTs"
        />
        <EmptyState
          title="Node Required"
          description="Connect to an indexed node to scan for recoverable stakes."
        />
      </div>
    )
  }

  if (!walletAddress) {
    return (
      <div className="ergopad-recovery-tab">
        <PageHeader
          icon={<RecoveryIcon />}
          title="Ergopad Recovery"
          subtitle="Unstake from the v1 Ergopad contracts using abandoned stake key NFTs"
        />
        <EmptyState
          title="Connect Wallet"
          description="Connect a wallet that holds `ergopad Stake Key` NFTs to begin."
        />
      </div>
    )
  }

  const totalRecoverable = scan?.stakes.reduce((sum, s) => sum + s.ergopadAmountRaw, 0) ?? 0
  const totalDisplay = (totalRecoverable / 100).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })

  return (
    <div className="ergopad-recovery-tab">
      <PageHeader
        icon={<RecoveryIcon />}
        title="Ergopad Recovery"
        subtitle="Unstake from the v1 Ergopad contracts using abandoned stake key NFTs you hold."
      />

      <div className="recovery-actions">
        {scanning ? (
          <span className="recovery-summary">
            <span className="spinner-small" /> Scanning {candidateTokenIds.length} candidate{candidateTokenIds.length === 1 ? '' : 's'}...
          </span>
        ) : scan ? (
          <span className="recovery-summary">
            {scan.stakes.length} recoverable · {totalDisplay} ERGOPAD
          </span>
        ) : candidateTokenIds.length === 0 ? (
          <span className="recovery-summary">No stake-key candidates in wallet.</span>
        ) : null}
      </div>

      <div className="recovery-lookup">
        <input
          type="text"
          className="recovery-lookup-input"
          placeholder="Paste stake key token ID (64 hex chars) to check its StakeBox..."
          value={lookupInput}
          onChange={e => setLookupInput(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') handleLookup() }}
          spellCheck={false}
        />
        <button
          className="btn btn-secondary"
          onClick={handleLookup}
          disabled={lookingUp || !lookupInput.trim()}
        >
          {lookingUp ? (<><span className="spinner-small" /> Looking up...</>) : 'Lookup'}
        </button>
      </div>

      {lookupError && <div className="message error">{lookupError}</div>}

      {lookupResult && (
        <Card className="recovery-card recovery-lookup-result" surface="display">
          <CardHeader className="recovery-card-header">
            <div>
              <h3>{lookupResult.ergopadAmountDisplay} ERGOPAD</h3>
              <span className="recovery-key-id">
                {lookupResult.stakeKeyId.slice(0, 10)}...{lookupResult.stakeKeyId.slice(-6)}
              </span>
            </div>
            <span className="recovery-lookup-badge">Lookup</span>
          </CardHeader>
          <CardBody>
            <div className="recovery-stat">
              <span className="recovery-stat-label">Staked</span>
              <span className="recovery-stat-value">
                {new Date(lookupResult.stakeTimeMs).toISOString().slice(0, 10)}
              </span>
            </div>
            <div className="recovery-stat">
              <span className="recovery-stat-label">Checkpoint</span>
              <span className="recovery-stat-value">{lookupResult.checkpoint}</span>
            </div>
            <div className="recovery-stat">
              <span className="recovery-stat-label">Stake box</span>
              <span className="recovery-stat-value mono">
                {lookupResult.stakeBoxId.slice(0, 8)}...
              </span>
            </div>
            <div className="recovery-scan-detail" style={{ marginTop: 8 }}>
              This is a read-only lookup. Redeeming requires the stake key NFT in your wallet.
            </div>
          </CardBody>
        </Card>
      )}

      {scanError && <div className="message error">{scanError}</div>}

      {scan && scan.stakes.length === 0 && !scanError && (
        <div className="message warning">
          <div>No matching StakeBoxes found.</div>
          <div className="recovery-scan-detail">
            Checked {scan.candidatesChecked} wallet singleton{scan.candidatesChecked === 1 ? '' : 's'} against {scan.boxesScanned.toLocaleString()} unspent StakeBoxes ({scan.pagesFetched} page{scan.pagesFetched === 1 ? '' : 's'}).
            {scan.hitPageLimit && ' Hit the page limit — the stake P2S has more boxes than scanned.'}
            {!scan.hitPageLimit && ' The P2S was fully scanned.'}
            {' Either the stakes were already redeemed, or your keys are for a different contract version.'}
          </div>
        </div>
      )}

      {scan && scan.stakes.length > 0 && (
        <div className="recovery-grid view-grid">
          {scan.stakes.map(stake => (
            <Card key={stake.stakeKeyId} className="recovery-card" surface="display">
              <CardHeader className="recovery-card-header">
                <div>
                  <h3>{stake.ergopadAmountDisplay} ERGOPAD</h3>
                  <span className="recovery-key-id">
                    {stake.stakeKeyId.slice(0, 10)}...{stake.stakeKeyId.slice(-6)}
                  </span>
                </div>
              </CardHeader>
              <CardBody>
                <div className="recovery-stat">
                  <span className="recovery-stat-label">Staked</span>
                  <span className="recovery-stat-value">
                    {new Date(stake.stakeTimeMs).toISOString().slice(0, 10)}
                  </span>
                </div>
                <div className="recovery-stat">
                  <span className="recovery-stat-label">Checkpoint</span>
                  <span className="recovery-stat-value">{stake.checkpoint}</span>
                </div>
                <div className="recovery-stat">
                  <span className="recovery-stat-label">Stake box</span>
                  <span className="recovery-stat-value mono">
                    {stake.stakeBoxId.slice(0, 8)}...
                  </span>
                </div>
              </CardBody>
              <CardFooter>
                <button
                  className="btn btn-primary recovery-redeem-btn"
                  onClick={() => handleRedeem(stake)}
                  disabled={redeemStep !== 'idle'}
                >
                  Redeem
                </button>
              </CardFooter>
            </Card>
          ))}
        </div>
      )}

      {activeKey && redeemStep !== 'idle' && (
        <div className="modal-overlay" onClick={closeRedeem}>
          <div className="recovery-modal" onClick={e => e.stopPropagation()}>
            <div className="recovery-modal-header">
              <h2>Recover {activeKey.ergopadAmountDisplay} ERGOPAD</h2>
              <button className="close-btn" onClick={closeRedeem}>
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>

            <div className="recovery-modal-content">
              {redeemStep === 'building' && (
                <div className="recovery-centered">
                  <div className="spinner-small" />
                  <span>Building transaction...</span>
                </div>
              )}

              {redeemStep === 'signing' && flow.signMethod === 'choose' && (
                <>
                  <p>Choose your signing method</p>
                  <div className="wallet-options">
                    <button className="wallet-option" onClick={flow.handleNautilusSign}>
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
                    <button className="wallet-option" onClick={flow.handleMobileSign}>
                      <div className="wallet-option-icon">
                        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <rect x="5" y="2" width="14" height="20" rx="2" />
                          <line x1="12" y1="18" x2="12.01" y2="18" />
                        </svg>
                      </div>
                      <div className="wallet-option-info">
                        <span className="wallet-option-name">Mobile Wallet</span>
                        <span className="wallet-option-desc">Scan QR with Ergo Wallet</span>
                      </div>
                    </button>
                  </div>
                </>
              )}

              {redeemStep === 'signing' && flow.signMethod === 'nautilus' && (
                <div className="recovery-centered">
                  <p>Approve in Nautilus...</p>
                  <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
                </div>
              )}

              {redeemStep === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
                <div className="recovery-centered">
                  <p>Scan with your Ergo wallet</p>
                  <div className="qr-container">
                    <QRCodeSVG value={flow.qrUrl} size={200} />
                  </div>
                  <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
                </div>
              )}

              {redeemStep === 'success' && flow.txId && (
                <div className="success-step">
                  <div className="success-icon">
                    <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                      <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                    </svg>
                  </div>
                  <h3>Recovery submitted</h3>
                  <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />
                  <button className="btn btn-primary" onClick={() => { closeRedeem(); handleScan() }}>
                    Done
                  </button>
                </div>
              )}

              {redeemStep === 'error' && (
                <div className="error-step">
                  <div className="error-icon">
                    <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--red-500)" strokeWidth="2">
                      <circle cx="12" cy="12" r="10" /><path d="M15 9l-6 6M9 9l6 6" />
                    </svg>
                  </div>
                  <h3>Recovery failed</h3>
                  <p className="error-message">{redeemError}</p>
                  <button className="btn btn-secondary" onClick={closeRedeem}>Close</button>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function RecoveryIcon() {
  return (
    <div className="recovery-icon">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="20" height="20">
        <path d="M21 12a9 9 0 11-3.6-7.2" />
        <polyline points="21 4 21 10 15 10" />
      </svg>
    </div>
  )
}
