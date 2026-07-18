import { useState, useCallback, useEffect, useMemo, type Dispatch, type SetStateAction } from 'react'
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
import { PageHeader, Card, CardHeader, CardBody, CardFooter, EmptyState, Button, Modal, Input } from './ui'
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
  //
  // Two INDEPENDENT flows can be active at once — a live redemption just signed in
  // this session (the modal) and a resumed step 2 from an older tx id (the card
  // below) — so each gets its own state rather than sharing one, to guarantee a
  // resumed ready/done proxy can never be mistaken for, or clobber, a freshly
  // started recovery's step-2 state (or vice versa).
  type Step2Status =
    | 'idle'
    | 'resolving'
    | 'checking'
    | 'ready'
    | 'submitting'
    | 'done'
    | 'error'
  interface Step2FlowState {
    status: Step2Status
    error: string | null
    proxyCheck: PaideiaProxyCheck | null
    txId: string | null
  }
  const idleStep2: Step2FlowState = { status: 'idle', error: null, proxyCheck: null, txId: null }
  const [activeStep2, setActiveStep2] = useState<Step2FlowState>(idleStep2)
  const [resumeStep2, setResumeStep2] = useState<Step2FlowState>(idleStep2)

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
    setActiveStep2(idleStep2)
    flow.reset()
  }, [flow])

  // Resolve the confirmed proxy box from a step-1 tx id, then dry-run both spend
  // paths. Takes an explicit txId + its own setter (rather than reading/writing
  // shared component state) so the live post-signing flow and a cold resume never
  // interfere with each other.
  const runStep2Check = useCallback(
    async (txId: string, setState: Dispatch<SetStateAction<Step2FlowState>>) => {
      if (!txId) return
      setState(s => ({ ...s, status: 'resolving', error: null }))
      try {
        const boxId = await paideiaProxyBoxId(txId)
        setState(s => ({ ...s, status: 'checking' }))
        const check = await checkPaideiaProxy(boxId)
        setState(s => ({ ...s, status: 'ready', proxyCheck: check }))
      } catch (e) {
        setState(s => ({ ...s, status: 'error', error: String(e) }))
      }
    },
    [],
  )

  const runStep2Submit = useCallback(
    async (
      which: 'executor' | 'refund',
      state: Step2FlowState,
      setState: Dispatch<SetStateAction<Step2FlowState>>,
    ) => {
      if (!state.proxyCheck) return
      setState(s => ({ ...s, status: 'submitting', error: null }))
      try {
        const txId = await submitPaideiaProxyTx(state.proxyCheck.proxyBoxId, which)
        setState(s => ({ ...s, status: 'done', txId }))
      } catch (e) {
        setState(s => ({ ...s, status: 'error', error: String(e) }))
      }
    },
    [],
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
          {resumeStep2.status === 'idle' && (
            <div className="recovery-lookup">
              <Input
                type="text"
                className="recovery-lookup-input"
                placeholder="Step-1 proxy-creation transaction ID (64 hex chars)..."
                value={resumeTxIdInput}
                onChange={e => setResumeTxIdInput(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') runStep2Check(resumeTxIdInput.trim(), setResumeStep2) }}
                spellCheck={false}
              />
              <Button
                onClick={() => runStep2Check(resumeTxIdInput.trim(), setResumeStep2)}
                disabled={!resumeTxIdInput.trim()}
              >
                Check
              </Button>
            </div>
          )}

          {(resumeStep2.status === 'resolving' || resumeStep2.status === 'checking') && (
            <div className="recovery-centered">
              <div className="spinner-small" />
              <span>
                {resumeStep2.status === 'resolving'
                  ? 'Resolving the proxy box from that transaction...'
                  : 'Dry-running payout and refund against the node...'}
              </span>
            </div>
          )}

          {(resumeStep2.status === 'ready' || resumeStep2.status === 'submitting') && resumeStep2.proxyCheck && (
            <div className="recovery-step2">
              <div className="recovery-stat">
                <span className="recovery-stat-label">Reward payout</span>
                <span className="recovery-stat-value">
                  {resumeStep2.proxyCheck.executor.valid ? '✓ validates' : '✗ not runnable yet'}
                </span>
              </div>
              {!resumeStep2.proxyCheck.executor.valid && (
                <div className="recovery-scan-detail">{resumeStep2.proxyCheck.executor.message}</div>
              )}
              <div className="recovery-stat">
                <span className="recovery-stat-label">Refund (safety net)</span>
                <span className="recovery-stat-value">
                  {resumeStep2.proxyCheck.refund.valid ? '✓ validates' : '✗ ' + resumeStep2.proxyCheck.refund.message}
                </span>
              </div>
              <div className="recovery-step2-actions">
                <Button
                  variant="primary"
                  disabled={!resumeStep2.proxyCheck.executor.valid || resumeStep2.status === 'submitting'}
                  onClick={() => runStep2Submit('executor', resumeStep2, setResumeStep2)}
                >
                  {resumeStep2.status === 'submitting' ? 'Submitting...' : 'Execute payout'}
                </Button>
                <Button
                  disabled={!resumeStep2.proxyCheck.refund.valid || resumeStep2.status === 'submitting'}
                  onClick={() => runStep2Submit('refund', resumeStep2, setResumeStep2)}
                >
                  Refund key instead
                </Button>
              </div>
            </div>
          )}

          {resumeStep2.status === 'done' && (
            <div className="success-step">
              <div className="success-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                </svg>
              </div>
              <h3>Submitted</h3>
              {resumeStep2.txId && <TxSuccess txId={resumeStep2.txId} explorerUrl={explorerUrl} />}
              <Button
                variant="primary"
                onClick={() => {
                  setResumeStep2(idleStep2)
                  setResumeTxIdInput('')
                  handleScan()
                }}
              >
                Done
              </Button>
            </div>
          )}

          {resumeStep2.status === 'error' && (
            <>
              <p className="error-message">{resumeStep2.error}</p>
              <Button onClick={() => runStep2Check(resumeTxIdInput.trim(), setResumeStep2)}>
                Retry
              </Button>
              <Button
                variant="ghost"
                onClick={() => setResumeStep2(idleStep2)}
              >
                Start over
              </Button>
            </>
          )}
        </CardBody>
      </Card>

      <div className="recovery-lookup">
        <Input
          type="text"
          className="recovery-lookup-input"
          placeholder="Paste stake key token ID (64 hex chars) to check its StakeBox..."
          value={lookupInput}
          onChange={e => setLookupInput(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') handleLookup() }}
          spellCheck={false}
        />
        <Button
          onClick={handleLookup}
          loading={lookingUp}
          disabled={!lookupInput.trim()}
        >
          {lookingUp ? 'Looking up...' : 'Lookup'}
        </Button>
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
                <Button
                  variant="primary"
                  className="recovery-redeem-btn"
                  onClick={() => handleRedeem(stake)}
                  disabled={redeemStep !== 'idle'}
                >
                  Redeem
                </Button>
              </CardFooter>
            </Card>
          ))}
        </div>
      )}

      {activeKey && redeemStep !== 'idle' && (
        <Modal
          open={true}
          onClose={closeRedeem}
          title={`Recover ${activeKey.rewardAmountDisplay} ${activeKey.rewardTokenName}`}
        >
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
                  <Button onClick={flow.handleBackToChoice}>Back</Button>
                </div>
              )}

              {redeemStep === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
                <div className="recovery-centered">
                  <p>Scan with your Ergo wallet</p>
                  <div className="qr-container">
                    <QRCodeSVG value={flow.qrUrl} size={200} />
                  </div>
                  <Button onClick={flow.handleBackToChoice}>Back</Button>
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
                  <Button variant="primary" onClick={() => { closeRedeem(); handleScan() }}>
                    Done
                  </Button>
                </div>
              )}

              {redeemStep === 'success' && flow.txId && activeKey?.protocol === 'Paideia' && (
                <div className="success-step">
                  {activeStep2.status === 'done' ? (
                    <>
                      <div className="success-icon">
                        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                          <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                        </svg>
                      </div>
                      <h3>Payout submitted</h3>
                      {activeStep2.txId && <TxSuccess txId={activeStep2.txId} explorerUrl={explorerUrl} />}
                      <Button variant="primary" onClick={() => { closeRedeem(); handleScan() }}>
                        Done
                      </Button>
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

                      {activeStep2.status === 'idle' && (
                        <Button variant="primary" onClick={() => runStep2Check(flow.txId!, setActiveStep2)}>
                          Verify payout &amp; refund (dry-run)
                        </Button>
                      )}
                      {(activeStep2.status === 'resolving' || activeStep2.status === 'checking') && (
                        <div className="recovery-centered">
                          <div className="spinner-small" />
                          <span>
                            {activeStep2.status === 'resolving'
                              ? 'Waiting for the proxy box to confirm...'
                              : 'Dry-running payout and refund against the node...'}
                          </span>
                        </div>
                      )}

                      {(activeStep2.status === 'ready' || activeStep2.status === 'submitting') && activeStep2.proxyCheck && (
                        <div className="recovery-step2">
                          <div className="recovery-stat">
                            <span className="recovery-stat-label">Reward payout</span>
                            <span className="recovery-stat-value">
                              {activeStep2.proxyCheck.executor.valid ? '✓ validates' : '✗ not runnable yet'}
                            </span>
                          </div>
                          {!activeStep2.proxyCheck.executor.valid && (
                            <div className="recovery-scan-detail">{activeStep2.proxyCheck.executor.message}</div>
                          )}
                          <div className="recovery-stat">
                            <span className="recovery-stat-label">Refund (safety net)</span>
                            <span className="recovery-stat-value">
                              {activeStep2.proxyCheck.refund.valid ? '✓ validates' : '✗ ' + activeStep2.proxyCheck.refund.message}
                            </span>
                          </div>
                          <div className="recovery-step2-actions">
                            <Button
                              variant="primary"
                              disabled={!activeStep2.proxyCheck.executor.valid || activeStep2.status === 'submitting'}
                              onClick={() => runStep2Submit('executor', activeStep2, setActiveStep2)}
                            >
                              {activeStep2.status === 'submitting' ? 'Submitting...' : `Execute payout (${activeKey.rewardAmountDisplay} ${activeKey.rewardTokenName})`}
                            </Button>
                            <Button
                              disabled={!activeStep2.proxyCheck.refund.valid || activeStep2.status === 'submitting'}
                              onClick={() => runStep2Submit('refund', activeStep2, setActiveStep2)}
                            >
                              Refund key instead
                            </Button>
                          </div>
                        </div>
                      )}

                      {activeStep2.status === 'error' && (
                        <>
                          <p className="error-message">{activeStep2.error}</p>
                          <Button onClick={() => runStep2Check(flow.txId!, setActiveStep2)}>
                            Retry
                          </Button>
                        </>
                      )}
                      <Button variant="ghost" onClick={() => { closeRedeem(); handleScan() }}>
                        Close (finish payout later)
                      </Button>
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
                  <Button onClick={closeRedeem}>Close</Button>
                </div>
              )}
            </div>
        </Modal>
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
