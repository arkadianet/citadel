import { useState, useCallback, useEffect, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  type RecoverableStake,
  type RecoveryScan,
  type PaideiaProxyCheck,
  scanRecoverableStakes,
  buildRecoveryTx,
  previewRecovery,
  paideiaProxyBoxId,
  checkPaideiaProxy,
  submitPaideiaProxyTx,
} from '../api/stakeRecovery'
import { startSign, getTxStatus } from '../api/types'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import { PageHeader, Card, CardHeader, CardBody, CardFooter, EmptyState } from './ui'
import { TxSuccess } from './TxSuccess'
import './StakeRecoveryTab.css'

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

interface StakeRecoveryTabProps {
  isConnected: boolean
  capabilityTier?: string
  walletAddress: string | null
  walletBalance: WalletBalance | null
  explorerUrl: string
}

type RedeemStep = 'idle' | 'building' | 'signing' | 'success' | 'error'

export function StakeRecoveryTab({
  isConnected,
  capabilityTier,
  walletAddress,
  walletBalance,
  explorerUrl,
}: StakeRecoveryTabProps) {
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

  // Paideia step 2: after the step-1 proxy is confirmed, the reward payout and the
  // refund are both permissionless (no signature). We dry-run both against the exact
  // proxy box before broadcasting either.
  type Step2Status =
    | 'idle'
    | 'resolving'
    | 'checking'
    | 'ready'
    | 'submitting'
    | 'done'
    | 'error'
  const [step2Status, setStep2Status] = useState<Step2Status>('idle')
  const [step2Error, setStep2Error] = useState<string | null>(null)
  const [proxyCheck, setProxyCheck] = useState<PaideiaProxyCheck | null>(null)
  const [step2TxId, setStep2TxId] = useState<string | null>(null)

  // Resume a Paideia step 2 from a step-1 tx id when the original signing session
  // (browser tab / app restart) is gone. Step 2 is fully re-derivable on-chain from
  // just the step-1 tx id — nothing about it depends on in-memory session state.
  const [resumeTxIdInput, setResumeTxIdInput] = useState('')

  const flow = useTransactionFlow({
    pollStatus: getTxStatus,
    isOpen: redeemStep !== 'idle',
    onSuccess: () => setRedeemStep('success'),
    onError: (err) => {
      setRedeemError(err)
      setRedeemStep('error')
    },
    watchParams: {
      protocol: 'StakeRecovery',
      operation: 'redeem',
      description: 'v1 stake recovery',
    },
  })

  const candidateTokenIds = useMemo(() => {
    if (!walletBalance) return [] as string[]
    // Any singleton token in the wallet is a candidate stake-key NFT. We intentionally
    // don't filter by name because some node configurations don't populate token
    // names, and the scan cost of including extra singletons is zero.
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
      const stake = await previewRecovery(id)
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
      const result = await scanRecoverableStakes(candidateTokenIds)
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
    setStep2Status('idle')
    setStep2Error(null)
    setProxyCheck(null)
    setStep2TxId(null)
    flow.reset()
  }, [flow])

  // Resolve the confirmed proxy box from a step-1 tx id, then dry-run both spend
  // paths. Takes an explicit txId (rather than reading component state) so it works
  // identically for the live post-signing flow and for a cold resume.
  const handleStep2Check = useCallback(async (txId: string) => {
    if (!txId) return
    setStep2Status('resolving')
    setStep2Error(null)
    try {
      const boxId = await paideiaProxyBoxId(txId)
      setStep2Status('checking')
      const check = await checkPaideiaProxy(boxId)
      setProxyCheck(check)
      setStep2Status('ready')
    } catch (e) {
      setStep2Error(String(e))
      setStep2Status('error')
    }
  }, [])

  const handleStep2Submit = useCallback(
    async (which: 'executor' | 'refund') => {
      if (!proxyCheck) return
      setStep2Status('submitting')
      setStep2Error(null)
      try {
        const txId = await submitPaideiaProxyTx(proxyCheck.proxyBoxId, which)
        setStep2TxId(txId)
        setStep2Status('done')
      } catch (e) {
        setStep2Error(String(e))
        setStep2Status('error')
      }
    },
    [proxyCheck],
  )

  const handleRedeem = useCallback(async (stake: RecoverableStake) => {
    setActiveKey(stake)
    setRedeemStep('building')
    setRedeemError(null)
    try {
      const utxos = await invoke<Array<{ ergo_tree?: string; ergoTree?: string }>>('get_user_utxos')
      if (!utxos?.length) throw new Error('No UTXOs available')

      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const unsignedTx = await buildRecoveryTx(
        stake.stakeKeyId,
        utxos as object[],
        nodeStatus.chain_height,
      )

      const signResult = await startSign(
        unsignedTx,
        `Recover ${stake.rewardAmountDisplay} ${stake.rewardTokenName} from ${stake.protocol} stake key ${stake.stakeKeyId.slice(0, 8)}...`,
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
      <div className="stake-recovery-tab">
        <PageHeader
          icon={<RecoveryIcon />}
          title="Stake Recovery"
          subtitle="Unstake from v1 Ergopad, EGIO and Paideia staking contracts using abandoned stake key NFTs"
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
      <div className="stake-recovery-tab">
        <PageHeader
          icon={<RecoveryIcon />}
          title="Stake Recovery"
          subtitle="Unstake from v1 Ergopad, EGIO and Paideia staking contracts using abandoned stake key NFTs"
        />
        <EmptyState
          title="Connect Wallet"
          description="Connect a wallet that holds v1 `Stake Key` NFTs to begin."
        />
      </div>
    )
  }

  const recoverableCount = scan?.stakes.length ?? 0

  return (
    <div className="stake-recovery-tab">
      <PageHeader
        icon={<RecoveryIcon />}
        title="Stake Recovery"
        subtitle="Unstake from v1 Ergopad, EGIO and Paideia staking contracts using abandoned stake key NFTs you hold."
      />

      <div className="recovery-actions">
        {scanning ? (
          <span className="recovery-summary">
            <span className="spinner-small" /> Scanning {candidateTokenIds.length} candidate{candidateTokenIds.length === 1 ? '' : 's'}...
          </span>
        ) : scan ? (
          <span className="recovery-summary">
            {recoverableCount} recoverable stake{recoverableCount === 1 ? '' : 's'}
          </span>
        ) : candidateTokenIds.length === 0 ? (
          <span className="recovery-summary">No stake-key candidates in wallet.</span>
        ) : null}
      </div>

      <Card className="recovery-card recovery-resume" surface="display">
        <CardHeader className="recovery-card-header">
          <div>
            <h3>Resume a Paideia unstake</h3>
            <span className="recovery-key-id">
              Already signed step 1 in a previous session? Paste that transaction ID to
              finish step 2 — nothing here depends on the original browser session.
            </span>
          </div>
        </CardHeader>
        <CardBody>
          {step2Status === 'idle' && (
            <div className="recovery-lookup">
              <input
                type="text"
                className="recovery-lookup-input"
                placeholder="Step-1 proxy-creation transaction ID (64 hex chars)..."
                value={resumeTxIdInput}
                onChange={e => setResumeTxIdInput(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') handleStep2Check(resumeTxIdInput.trim()) }}
                spellCheck={false}
              />
              <button
                className="btn btn-secondary"
                onClick={() => handleStep2Check(resumeTxIdInput.trim())}
                disabled={!resumeTxIdInput.trim()}
              >
                Check
              </button>
            </div>
          )}

          {(step2Status === 'resolving' || step2Status === 'checking') && (
            <div className="recovery-centered">
              <div className="spinner-small" />
              <span>
                {step2Status === 'resolving'
                  ? 'Resolving the proxy box from that transaction...'
                  : 'Dry-running payout and refund against the node...'}
              </span>
            </div>
          )}

          {(step2Status === 'ready' || step2Status === 'submitting') && proxyCheck && (
            <div className="recovery-step2">
              <div className="recovery-stat">
                <span className="recovery-stat-label">Reward payout</span>
                <span className="recovery-stat-value">
                  {proxyCheck.executor.valid ? '✓ validates' : '✗ not runnable yet'}
                </span>
              </div>
              {!proxyCheck.executor.valid && (
                <div className="recovery-scan-detail">{proxyCheck.executor.message}</div>
              )}
              <div className="recovery-stat">
                <span className="recovery-stat-label">Refund (safety net)</span>
                <span className="recovery-stat-value">
                  {proxyCheck.refund.valid ? '✓ validates' : '✗ ' + proxyCheck.refund.message}
                </span>
              </div>
              <div className="recovery-step2-actions">
                <button
                  className="btn btn-primary"
                  disabled={!proxyCheck.executor.valid || step2Status === 'submitting'}
                  onClick={() => handleStep2Submit('executor')}
                >
                  {step2Status === 'submitting' ? 'Submitting...' : 'Execute payout'}
                </button>
                <button
                  className="btn btn-secondary"
                  disabled={!proxyCheck.refund.valid || step2Status === 'submitting'}
                  onClick={() => handleStep2Submit('refund')}
                >
                  Refund key instead
                </button>
              </div>
            </div>
          )}

          {step2Status === 'done' && (
            <div className="success-step">
              <div className="success-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                </svg>
              </div>
              <h3>Submitted</h3>
              {step2TxId && <TxSuccess txId={step2TxId} explorerUrl={explorerUrl} />}
              <button
                className="btn btn-primary"
                onClick={() => {
                  setStep2Status('idle')
                  setProxyCheck(null)
                  setStep2TxId(null)
                  setResumeTxIdInput('')
                  handleScan()
                }}
              >
                Done
              </button>
            </div>
          )}

          {step2Status === 'error' && (
            <>
              <p className="error-message">{step2Error}</p>
              <button className="btn btn-secondary" onClick={() => handleStep2Check(resumeTxIdInput.trim())}>
                Retry
              </button>
              <button
                className="btn btn-ghost"
                onClick={() => { setStep2Status('idle'); setStep2Error(null) }}
              >
                Start over
              </button>
            </>
          )}
        </CardBody>
      </Card>

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
              <h3>{lookupResult.rewardAmountDisplay} {lookupResult.rewardTokenName}</h3>
              <span className="recovery-key-id">
                {lookupResult.protocol} · {lookupResult.stakeKeyId.slice(0, 10)}...{lookupResult.stakeKeyId.slice(-6)}
              </span>
            </div>
            <span className="recovery-lookup-badge">Lookup</span>
          </CardHeader>
          <CardBody>
            <div className="recovery-stat">
              <span className="recovery-stat-label">Protocol</span>
              <span className="recovery-stat-value">{lookupResult.protocol}</span>
            </div>
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
            Checked {scan.candidatesChecked} wallet singleton{scan.candidatesChecked === 1 ? '' : 's'} against {scan.boxesScanned.toLocaleString()} unspent StakeBoxes ({scan.pagesFetched} page{scan.pagesFetched === 1 ? '' : 's'}) across {scan.states.length} live protocol{scan.states.length === 1 ? '' : 's'}.
            {scan.hitPageLimit && ' Hit the page limit — a stake P2S has more boxes than scanned.'}
            {!scan.hitPageLimit && ' The P2S sets were fully scanned.'}
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
                  <h3>{stake.rewardAmountDisplay} {stake.rewardTokenName}</h3>
                  <span className="recovery-key-id">
                    {stake.protocol} · {stake.stakeKeyId.slice(0, 10)}...{stake.stakeKeyId.slice(-6)}
                  </span>
                </div>
                <span className="recovery-lookup-badge">{stake.protocol}</span>
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
              <h2>Recover {activeKey.rewardAmountDisplay} {activeKey.rewardTokenName}</h2>
              <button className="close-btn" onClick={closeRedeem}>
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>

            <div className="recovery-modal-content">
              {activeKey.protocol === 'Paideia' && redeemStep !== 'success' && (
                <p className="recovery-note">
                  Paideia uses a two-step unstake. This first transaction creates a single-use
                  unstake <strong>proxy box</strong> that holds your stake key and names your
                  wallet as the payout recipient. It is the only step that needs your signature.
                  Afterwards this app runs step 2 for you — a permissionless payout of{' '}
                  {activeKey.rewardAmountDisplay} {activeKey.rewardTokenName} to your address.
                  If for any reason the payout can't run, the same box has a permissionless
                  <strong> refund</strong> path that returns your key and ERG to your own
                  address — so signing this is safe either way.
                </p>
              )}
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

              {redeemStep === 'success' && flow.txId && activeKey?.protocol !== 'Paideia' && (
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

              {redeemStep === 'success' && flow.txId && activeKey?.protocol === 'Paideia' && (
                <div className="success-step">
                  {step2Status === 'done' ? (
                    <>
                      <div className="success-icon">
                        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                          <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                        </svg>
                      </div>
                      <h3>Payout submitted</h3>
                      {step2TxId && <TxSuccess txId={step2TxId} explorerUrl={explorerUrl} />}
                      <button className="btn btn-primary" onClick={() => { closeRedeem(); handleScan() }}>
                        Done
                      </button>
                    </>
                  ) : (
                    <>
                      <h3>Step 1 of 2 complete: proxy created</h3>
                      <p className="recovery-note">
                        Your stake key is now in a single-use unstake proxy box. Step 2 (the
                        reward payout) and the refund are both <strong>permissionless</strong> —
                        they need no signature, so this app can execute them directly once the
                        proxy is confirmed on-chain. Nothing is at risk: if the payout can't run,
                        the refund returns your key and ERG to your own address.
                      </p>
                      <div className="recovery-scan-detail">Step 1 proxy tx:</div>
                      <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />

                      {step2Status === 'idle' && (
                        <button className="btn btn-primary" onClick={() => handleStep2Check(flow.txId!)}>
                          Verify payout &amp; refund (dry-run)
                        </button>
                      )}
                      {(step2Status === 'resolving' || step2Status === 'checking') && (
                        <div className="recovery-centered">
                          <div className="spinner-small" />
                          <span>
                            {step2Status === 'resolving'
                              ? 'Waiting for the proxy box to confirm...'
                              : 'Dry-running payout and refund against the node...'}
                          </span>
                        </div>
                      )}

                      {(step2Status === 'ready' || step2Status === 'submitting') && proxyCheck && (
                        <div className="recovery-step2">
                          <div className="recovery-stat">
                            <span className="recovery-stat-label">Reward payout</span>
                            <span className="recovery-stat-value">
                              {proxyCheck.executor.valid ? '✓ validates' : '✗ not runnable yet'}
                            </span>
                          </div>
                          {!proxyCheck.executor.valid && (
                            <div className="recovery-scan-detail">{proxyCheck.executor.message}</div>
                          )}
                          <div className="recovery-stat">
                            <span className="recovery-stat-label">Refund (safety net)</span>
                            <span className="recovery-stat-value">
                              {proxyCheck.refund.valid ? '✓ validates' : '✗ ' + proxyCheck.refund.message}
                            </span>
                          </div>
                          <div className="recovery-step2-actions">
                            <button
                              className="btn btn-primary"
                              disabled={!proxyCheck.executor.valid || step2Status === 'submitting'}
                              onClick={() => handleStep2Submit('executor')}
                            >
                              {step2Status === 'submitting' ? 'Submitting...' : `Execute payout (${activeKey.rewardAmountDisplay} ${activeKey.rewardTokenName})`}
                            </button>
                            <button
                              className="btn btn-secondary"
                              disabled={!proxyCheck.refund.valid || step2Status === 'submitting'}
                              onClick={() => handleStep2Submit('refund')}
                            >
                              Refund key instead
                            </button>
                          </div>
                        </div>
                      )}

                      {step2Status === 'error' && (
                        <>
                          <p className="error-message">{step2Error}</p>
                          <button className="btn btn-secondary" onClick={() => handleStep2Check(flow.txId!)}>
                            Retry
                          </button>
                        </>
                      )}
                      <button className="btn btn-ghost" onClick={() => { closeRedeem(); handleScan() }}>
                        Close (finish payout later)
                      </button>
                    </>
                  )}
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
