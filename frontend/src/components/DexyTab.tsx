import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import { DexyMintModal } from './DexyMintModal'
import { DexySwapModal } from './DexySwapModal'
import { getDexyActivity, type ProtocolInteraction } from '../api/protocolActivity'
import {
  previewLpDeposit,
  previewLpRedeem,
  buildLpDepositTx,
  buildLpRedeemTx,
  type LpPreviewResponse,
} from '../api/dexySwap'
import { formatErg } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import type { TxStatusResponse } from '../api/types'
import { useExplorerNav } from '../contexts/ExplorerNavContext'
import './DexyTab.css'

interface DexyState {
  variant: string
  bank_erg_nano: number
  dexy_in_bank: number
  bank_box_id: string
  dexy_token_id: string
  free_mint_available: number
  free_mint_reset_height: number
  current_height: number
  oracle_rate_nano: number
  oracle_box_id: string
  lp_erg_reserves: number
  lp_dexy_reserves: number
  lp_box_id: string
  lp_rate_nano: number
  can_mint: boolean
  rate_difference_pct: number
  dexy_circulating: number
  lp_token_reserves: number
  lp_circulating: number
  can_redeem_lp: boolean
}

interface WalletBalance {
  address: string
  erg_nano: number
  erg_formatted: string
  sigusd_amount: number
  sigusd_formatted: string
  sigrsv_amount: number
  tokens: Array<{
    token_id: string
    amount: number
    name: string | null
    decimals: number
  }>
}

interface DexyTabProps {
  isConnected: boolean
  capabilityTier?: string
  walletAddress: string | null
  walletBalance: WalletBalance | null
  ergUsdPrice?: number
  explorerUrl: string
}

type DexyVariant = 'gold' | 'usd'

const DEXY_TOKEN_IDS: Record<DexyVariant, string> = {
  gold: '6122f7289e7bb2df2de273e09d4b2756cda6aeb0f40438dc9d257688f45183ad',
  usd: 'a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669',
}

const DEXY_TOKEN_ID_SET = new Set(Object.values(DEXY_TOKEN_IDS))

const LP_TOKEN_IDS: Record<DexyVariant, string> = {
  gold: 'cf74432b2d3ab8a1a934b6326a1004e1a19aec7b357c57209018c4aa35226246',
  usd: '804a66426283b8281240df8f9de783651986f20ad6391a71b26b9e7d6faad099',
}

const TROY_OZ_IN_MG = 31103.5

interface TokenChange {
  token_id: string
  amount: number
  name: string | null
  decimals: number
}

interface RecentTx {
  tx_id: string
  inclusion_height: number
  num_confirmations: number
  timestamp: number
  erg_change_nano: number
  token_changes: TokenChange[]
}

function pollLpTxStatus(requestId: string): Promise<TxStatusResponse> {
  return invoke<TxStatusResponse>('get_mint_tx_status', { requestId })
}

const TOKEN_ICONS: Record<string, string> = {
  DexyGold: '/icons/dexygold.svg',
  USE: '/icons/use.svg',
}

const TOKEN_DECIMALS: Record<string, number> = {
  DexyGold: 0,
  USE: 3,
}

function formatTimeAgo(timestampMs: number): string {
  const now = Date.now()
  const diff = now - timestampMs
  const minutes = Math.floor(diff / 60000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  return new Date(timestampMs).toLocaleDateString()
}

export function DexyTab({
  isConnected,
  capabilityTier,
  walletAddress,
  walletBalance,
  ergUsdPrice,
  explorerUrl,
}: DexyTabProps) {
  const { navigateToExplorer } = useExplorerNav()
  const [goldState, setGoldState] = useState<DexyState | null>(null)
  const [usdState, setUsdState] = useState<DexyState | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [mintModalOpen, setMintModalOpen] = useState(false)
  const [swapModalOpen, setSwapModalOpen] = useState(false)
  const [selectedVariant, setSelectedVariant] = useState<DexyVariant>('gold')
  const [activity, setActivity] = useState<ProtocolInteraction[]>([])
  const [activityLoading, setActivityLoading] = useState(false)
  const [userTxs, setUserTxs] = useState<RecentTx[]>([])
  const [userTxsLoading, setUserTxsLoading] = useState(false)

  // Sub-tab state
  const [subTab, setSubTab] = useState<'overview' | 'liquidity'>('overview')

  // LP liquidity state
  const [depositErg, setDepositErg] = useState('')
  const [depositDexy, setDepositDexy] = useState('')
  const [depositPreview, setDepositPreview] = useState<LpPreviewResponse | null>(null)
  const [redeemLp, setRedeemLp] = useState('')
  const [redeemErg, setRedeemErg] = useState('')
  const [redeemDexy, setRedeemDexy] = useState('')
  const [redeemPreview, setRedeemPreview] = useState<LpPreviewResponse | null>(null)
  const [lpTxStep, setLpTxStep] = useState<'idle' | 'signing' | 'success' | 'error'>('idle')
  const [lpTxError, setLpTxError] = useState<string | null>(null)
  const [lpTxLoading, setLpTxLoading] = useState(false)

  const lpFlow = useTransactionFlow({
    pollStatus: pollLpTxStatus,
    isOpen: subTab === 'liquidity',
    onSuccess: () => {
      setLpTxStep('success')
      fetchAllStates()
    },
    onError: (err) => { setLpTxError(err); setLpTxStep('error') },
    watchParams: { protocol: 'Dexy', operation: 'liquidity', description: `Dexy LP operation` },
  })

  const fetchDexyState = useCallback(async (variant: DexyVariant) => {
    try {
      const state = await invoke<DexyState>('get_dexy_state', { variant })
      return state
    } catch (e) {
      console.error(`Failed to fetch Dexy ${variant} state:`, e)
      throw e
    }
  }, [])

  const fetchAllStates = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') {
      return
    }

    setLoading(true)
    setError(null)

    try {
      const [gold, usd] = await Promise.all([
        fetchDexyState('gold'),
        fetchDexyState('usd'),
      ])
      setGoldState(gold)
      setUsdState(usd)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [isConnected, capabilityTier, fetchDexyState])

  const fetchDexyActivity = useCallback(async () => {
    if (!isConnected || capabilityTier === 'Basic') return
    setActivityLoading(true)
    try {
      const data = await getDexyActivity(10)
      setActivity(data)
    } catch (e) {
      console.error('Failed to fetch Dexy activity:', e)
      setActivity([])
    } finally {
      setActivityLoading(false)
    }
  }, [isConnected, capabilityTier])

  useEffect(() => {
    fetchAllStates()
    const interval = setInterval(fetchAllStates, 30000)
    return () => clearInterval(interval)
  }, [fetchAllStates])

  useEffect(() => {
    let cancelled = false
    if (!isConnected || capabilityTier === 'Basic') {
      setActivity([])
      return
    }
    setActivityLoading(true)
    getDexyActivity(10)
      .then(data => { if (!cancelled) setActivity(data) })
      .catch(e => {
        console.error('Failed to fetch Dexy activity:', e)
        if (!cancelled) setActivity([])
      })
      .finally(() => { if (!cancelled) setActivityLoading(false) })
    return () => { cancelled = true }
  }, [isConnected, capabilityTier])

  // Fetch user's recent transactions, filtered to Dexy-related
  const fetchUserDexyTxs = useCallback(async () => {
    if (!isConnected || !walletBalance) {
      setUserTxs([])
      return
    }
    setUserTxsLoading(true)
    try {
      const res = await invoke<{ transactions: RecentTx[] }>('get_recent_transactions', { limit: 20 })
      const dexyTxs = res.transactions.filter(tx =>
        tx.token_changes.some(tc => DEXY_TOKEN_ID_SET.has(tc.token_id))
      )
      setUserTxs(dexyTxs.slice(0, 10))
    } catch (e) {
      console.error('Failed to fetch user Dexy transactions:', e)
      setUserTxs([])
    } finally {
      setUserTxsLoading(false)
    }
  }, [isConnected, walletBalance])

  useEffect(() => {
    let cancelled = false
    if (!isConnected || !walletBalance) {
      setUserTxs([])
      return
    }
    setUserTxsLoading(true)
    invoke<{ transactions: RecentTx[] }>('get_recent_transactions', { limit: 20 })
      .then(res => {
        if (cancelled) return
        const dexyTxs = res.transactions.filter(tx =>
          tx.token_changes.some(tc => DEXY_TOKEN_ID_SET.has(tc.token_id))
        )
        setUserTxs(dexyTxs.slice(0, 10))
      })
      .catch(e => {
        console.error('Failed to fetch user Dexy transactions:', e)
        if (!cancelled) setUserTxs([])
      })
      .finally(() => { if (!cancelled) setUserTxsLoading(false) })
    return () => { cancelled = true }
  }, [isConnected, walletBalance])

  // LP deposit preview (debounced)
  useEffect(() => {
    const ergNano = Math.floor(parseFloat(depositErg || '0') * 1e9)
    const dexyRaw = selectedVariant === 'usd'
      ? Math.floor(parseFloat(depositDexy || '0') * 1000)
      : Math.floor(parseFloat(depositDexy || '0'))

    if (ergNano <= 0 || dexyRaw <= 0) {
      setDepositPreview(null)
      return
    }

    const timer = setTimeout(async () => {
      try {
        const preview = await previewLpDeposit(selectedVariant, ergNano, dexyRaw)
        setDepositPreview(preview)
      } catch (_e) {
        setDepositPreview(null)
      }
    }, 300)
    return () => clearTimeout(timer)
  }, [depositErg, depositDexy, selectedVariant])

  // LP redeem preview (debounced)
  useEffect(() => {
    const lpRaw = Math.floor(parseFloat(redeemLp || '0'))
    if (lpRaw <= 0) {
      setRedeemPreview(null)
      return
    }

    const timer = setTimeout(async () => {
      try {
        const preview = await previewLpRedeem(selectedVariant, lpRaw)
        setRedeemPreview(preview)
      } catch (_e) {
        setRedeemPreview(null)
      }
    }, 300)
    return () => clearTimeout(timer)
  }, [redeemLp, selectedVariant])

  // Reset LP form inputs when variant changes
  useEffect(() => {
    setDepositErg('')
    setDepositDexy('')
    setDepositPreview(null)
    setRedeemLp('')
    setRedeemErg('')
    setRedeemDexy('')
    setRedeemPreview(null)
    setLpTxStep('idle')
    setLpTxError(null)
  }, [selectedVariant])

  const handleDeposit = async () => {
    if (!depositPreview?.can_execute || !walletAddress) return

    const ergNano = Math.floor(parseFloat(depositErg || '0') * 1e9)
    const dexyRaw = selectedVariant === 'usd'
      ? Math.floor(parseFloat(depositDexy || '0') * 1000)
      : Math.floor(parseFloat(depositDexy || '0'))

    setLpTxLoading(true)
    setLpTxError(null)

    try {
      const utxos = await invoke<unknown[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const buildResult = await buildLpDepositTx(
        selectedVariant,
        ergNano,
        dexyRaw,
        walletAddress,
        utxos as object[],
        nodeStatus.chain_height,
      )

      const signResult = await invoke<{
        request_id: string
        ergopay_url: string
        nautilus_url: string
      }>('start_mint_sign', {
        request: {
          unsigned_tx: buildResult.unsigned_tx,
          message: `Add liquidity: ${depositErg} ERG + ${depositDexy} ${selectedVariant === 'usd' ? 'USE' : 'DexyGold'}`,
        },
      })

      lpFlow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setLpTxStep('signing')
    } catch (e) {
      setLpTxError(String(e))
      setLpTxStep('error')
    } finally {
      setLpTxLoading(false)
    }
  }

  const handleRedeem = async () => {
    if (!redeemPreview?.can_execute || !walletAddress) return

    const lpRaw = Math.floor(parseFloat(redeemLp || '0'))

    setLpTxLoading(true)
    setLpTxError(null)

    try {
      const utxos = await invoke<unknown[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      const buildResult = await buildLpRedeemTx(
        selectedVariant,
        lpRaw,
        walletAddress,
        utxos as object[],
        nodeStatus.chain_height,
      )

      const signResult = await invoke<{
        request_id: string
        ergopay_url: string
        nautilus_url: string
      }>('start_mint_sign', {
        request: {
          unsigned_tx: buildResult.unsigned_tx,
          message: `Remove liquidity: ${redeemErg} ERG + ${redeemDexy} ${selectedVariant === 'usd' ? 'USE' : 'DexyGold'}`,
        },
      })

      lpFlow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setLpTxStep('signing')
    } catch (e) {
      setLpTxError(String(e))
      setLpTxStep('error')
    } finally {
      setLpTxLoading(false)
    }
  }

  const openMintModal = (variant: DexyVariant) => {
    setSelectedVariant(variant)
    setMintModalOpen(true)
  }

  const openSwapModal = (variant: DexyVariant) => {
    setSelectedVariant(variant)
    setSwapModalOpen(true)
  }

  const getDexyBalance = (variant: DexyVariant): number => {
    if (!walletBalance) return 0
    const tokenId = DEXY_TOKEN_IDS[variant]
    const token = walletBalance.tokens.find(t => t.token_id === tokenId)
    return token?.amount || 0
  }

  // Holdings calculations
  const ergBalance = walletBalance ? walletBalance.erg_nano / 1e9 : 0
  const ergUsd = ergBalance * (ergUsdPrice || 0)

  const goldBalance = getDexyBalance('gold')
  const goldUsdPerUnit = goldState && ergUsdPrice
    ? (goldState.oracle_rate_nano / 1e9) * ergUsdPrice
    : 0
  const goldUsd = goldBalance * goldUsdPerUnit

  const useBalance = getDexyBalance('usd') / 1e3 // USE has 3 decimals
  const useUsdPerUnit = usdState && ergUsdPrice
    ? (usdState.oracle_rate_nano / 1e9) * 1e3 * ergUsdPrice // rate is per raw unit, 1e3 raw = 1 USE
    : 0
  const useUsd = useBalance * useUsdPerUnit

  // LP token balances and estimated values
  // LP tokens represent a share of BOTH the ERG and token reserves in the pool.
  const goldLpBalance = walletBalance ? walletBalance.tokens.find(t => t.token_id === LP_TOKEN_IDS['gold'])?.amount ?? 0 : 0
  const useLpBalance = walletBalance ? walletBalance.tokens.find(t => t.token_id === LP_TOKEN_IDS['usd'])?.amount ?? 0 : 0

  const goldLpErgShare = goldState && goldState.lp_circulating > 0
    ? goldLpBalance * goldState.lp_erg_reserves / goldState.lp_circulating * 0.98 / 1e9
    : 0
  const goldLpTokenShare = goldState && goldState.lp_circulating > 0
    ? goldLpBalance * goldState.lp_dexy_reserves / goldState.lp_circulating * 0.98
    : 0
  const goldLpTokenUsd = goldLpTokenShare * goldUsdPerUnit
  const goldLpErgUsd = goldLpErgShare * (ergUsdPrice || 0)
  const goldLpTotalUsd = goldLpErgUsd + goldLpTokenUsd

  const useLpErgShare = usdState && usdState.lp_circulating > 0
    ? useLpBalance * usdState.lp_erg_reserves / usdState.lp_circulating * 0.98 / 1e9
    : 0
  const useLpTokenShare = usdState && usdState.lp_circulating > 0
    ? useLpBalance * usdState.lp_dexy_reserves / usdState.lp_circulating * 0.98 / 1e3
    : 0
  const useLpTokenUsd = useLpTokenShare * useUsdPerUnit
  const useLpErgUsd = useLpErgShare * (ergUsdPrice || 0)
  const useLpTotalUsd = useLpErgUsd + useLpTokenUsd

  const lpTotalUsd = goldLpTotalUsd + useLpTotalUsd

  const totalHoldingsUsd = ergUsd + goldUsd + useUsd + lpTotalUsd

  if (!isConnected) {
    return (
      <div className="dexy-tab">
        <div className="empty-state">
          <p>Connect to a node first</p>
        </div>
      </div>
    )
  }

  if (capabilityTier === 'Basic') {
    return (
      <div className="dexy-tab">
        <div className="message error">
          Dexy requires an indexed node with extraIndex enabled.
        </div>
      </div>
    )
  }

  if (loading && !goldState && !usdState) {
    return (
      <div className="dexy-tab">
        <div className="empty-state">
          <div className="spinner" />
          <p>Loading Dexy protocol state...</p>
        </div>
      </div>
    )
  }

  if (error && !goldState && !usdState) {
    return (
      <div className="dexy-tab">
        <div className="message error">{error}</div>
      </div>
    )
  }

  return (
    <div className="dexy-tab">
      {/* Protocol Header */}
      <div className="dexy-header">
        <div className="dexy-header-row">
          <div className="dexy-icon-stack">
            <span className="icon-gold">
              <img src="/icons/dexygold.svg" alt="DexyGold" />
            </span>
            <span className="icon-usd">
              <img src="/icons/use.svg" alt="USE" />
            </span>
          </div>
          <div>
            <h2>Dexy Protocol</h2>
            <p className="dexy-description">Oracle-pegged stablecoins with LP dynamics</p>
          </div>
        </div>
      </div>

      {/* Protocol Info Bar */}
      <div className="dexy-info-bar">
        <div className="dexy-info-item">
          <span className="dexy-info-label">Variants:</span>
          <span className="dexy-info-value">DexyGold, USE</span>
        </div>
        <div className="dexy-info-divider" />
        <div className="dexy-info-item">
          <span className="dexy-info-label">Actions:</span>
          <span className="dexy-info-value">Mint, LP Swap, Liquidity</span>
        </div>
      </div>

      {/* Sub-tab Navigation */}
      <div className="dexy-sub-tabs">
        <button
          className={`dexy-sub-tab ${subTab === 'overview' ? 'active' : ''}`}
          onClick={() => setSubTab('overview')}
        >
          Overview
        </button>
        <button
          className={`dexy-sub-tab ${subTab === 'liquidity' ? 'active' : ''}`}
          onClick={() => setSubTab('liquidity')}
        >
          Liquidity
        </button>
      </div>

      {subTab === 'overview' && (<>
      {/* Asset Cards */}
      <div className="token-cards-grid">
        <DexyAssetCard
          state={goldState}
          variant="gold"
          tokenName="DexyGold"
          decimals={0}
          walletAddress={walletAddress}
          walletBalance={walletBalance}
          onMint={() => openMintModal('gold')}
          onSwap={() => openSwapModal('gold')}
          ergUsdPrice={ergUsdPrice}
        />
        <DexyAssetCard
          state={usdState}
          variant="usd"
          tokenName="USE"
          decimals={3}
          walletAddress={walletAddress}
          walletBalance={walletBalance}
          onMint={() => openMintModal('usd')}
          onSwap={() => openSwapModal('usd')}
        />
      </div>

      {/* Your Holdings */}
      {walletAddress && walletBalance ? (
        <div className="dexy-holdings-section">
          <div className="dexy-holdings-header">
            <h3>Your Holdings</h3>
            {ergUsdPrice && (
              <span className="dexy-holdings-total">Total: ${totalHoldingsUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</span>
            )}
          </div>
          <div className="dexy-holdings-grid">
            <div className="dexy-holding-card orange">
              <div className="dexy-holding-header">
                <div className="dexy-holding-icon" style={{ background: 'rgba(249, 115, 22, 0.3)', color: '#fb923c' }}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" width="14" height="14">
                    <path d="M20.24 12.24a6 6 0 0 0-8.49-8.49L5 10.5V19h8.5z" />
                  </svg>
                </div>
                <span className="dexy-holding-name">ERG</span>
              </div>
              <div className="dexy-holding-amount">{ergBalance.toLocaleString(undefined, { minimumFractionDigits: 4, maximumFractionDigits: 4 })}</div>
              {ergUsdPrice && <div className="dexy-holding-usd">${ergUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>}
            </div>
            <div className="dexy-holding-card amber">
              <div className="dexy-holding-header">
                <div className="dexy-holding-icon-wrap gold">
                  <img src="/icons/dexygold.svg" alt="DexyGold" />
                </div>
                <span className="dexy-holding-name">DexyGold</span>
              </div>
              <div className="dexy-holding-amount">{goldBalance.toLocaleString()}</div>
              {ergUsdPrice && <div className="dexy-holding-usd">${goldUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>}
            </div>
            <div className="dexy-holding-card emerald">
              <div className="dexy-holding-header">
                <div className="dexy-holding-icon-wrap usd">
                  <img src="/icons/use.svg" alt="USE" />
                </div>
                <span className="dexy-holding-name">USE</span>
              </div>
              <div className="dexy-holding-amount">{useBalance.toLocaleString(undefined, { minimumFractionDigits: 3, maximumFractionDigits: 3 })}</div>
              {ergUsdPrice && <div className="dexy-holding-usd">${useUsd.toLocaleString(undefined, { maximumFractionDigits: 2 })}</div>}
            </div>
            {(goldLpBalance > 0 || useLpBalance > 0) && (<>
              {goldLpBalance > 0 && (
                <div className="dexy-holding-card amber">
                  <div className="dexy-holding-header">
                    <div className="dexy-holding-icon-wrap gold">
                      <img src="/icons/dexygold.svg" alt="DexyGold LP" />
                    </div>
                    <span className="dexy-holding-name">Gold LP</span>
                  </div>
                  <div className="dexy-holding-amount">{goldLpBalance.toLocaleString()} LP</div>
                  <div className="dexy-holding-usd" style={{ color: 'var(--slate-400)' }}>
                    ~{goldLpErgShare.toFixed(2)} ERG + {goldLpTokenShare.toLocaleString(undefined, { maximumFractionDigits: 2 })} DexyGold
                    {ergUsdPrice ? ` ($${goldLpTotalUsd.toFixed(2)})` : ''}
                  </div>
                </div>
              )}
              {useLpBalance > 0 && (
                <div className="dexy-holding-card emerald">
                  <div className="dexy-holding-header">
                    <div className="dexy-holding-icon-wrap usd">
                      <img src="/icons/use.svg" alt="USE LP" />
                    </div>
                    <span className="dexy-holding-name">USE LP</span>
                  </div>
                  <div className="dexy-holding-amount">{useLpBalance.toLocaleString()} LP</div>
                  <div className="dexy-holding-usd" style={{ color: 'var(--slate-400)' }}>
                    ~{useLpErgShare.toFixed(2)} ERG + {useLpTokenShare.toLocaleString(undefined, { maximumFractionDigits: 3 })} USE
                    {ergUsdPrice ? ` ($${useLpTotalUsd.toFixed(2)})` : ''}
                  </div>
                </div>
              )}
            </>)}
          </div>
        </div>
      ) : !walletAddress && (
        <div className="dexy-wallet-section">
          <div className="wallet-notice">
            <div className="wallet-notice-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="2" y="5" width="20" height="14" rx="2" />
                <path d="M2 10h20" />
              </svg>
            </div>
            <h3>Wallet Not Connected</h3>
            <p>Connect your wallet using the button in the header to mint Dexy tokens</p>
          </div>
        </div>
      )}

      {/* Activity Feeds — side by side */}
      <div className="dexy-activity-grid">
        {/* Your Dexy Activity */}
        <div className="dexy-activity-section">
          <h3 className="dexy-section-header">Your Dexy Activity</h3>
          <div className="dexy-activity-card">
            {!walletAddress ? (
              <div className="dexy-activity-empty">Connect wallet to see your activity</div>
            ) : userTxsLoading ? (
              <div className="dexy-activity-loading">
                <div className="spinner-small" />
                <span>Loading...</span>
              </div>
            ) : userTxs.length === 0 ? (
              <div className="dexy-activity-empty">No recent Dexy transactions</div>
            ) : (
              <div className="dexy-activity-list">
                {userTxs.map(tx => {
                  const dexyChanges = tx.token_changes.filter(tc => DEXY_TOKEN_ID_SET.has(tc.token_id))
                  const ergChange = tx.erg_change_nano / 1e9
                  const isReceive = tx.erg_change_nano > 0
                  return (
                    <div
                      key={tx.tx_id}
                      className="dexy-activity-row"
                      onClick={() => navigateToExplorer({ page: 'transaction', id: tx.tx_id })}
                      role="button"
                      tabIndex={0}
                    >
                      <div className={`activity-op-icon ${isReceive ? 'mint' : 'redeem'}`}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                          {isReceive
                            ? <path d="M12 5v14M5 12l7 7 7-7" />
                            : <path d="M12 19V5M5 12l7-7 7 7" />
                          }
                        </svg>
                      </div>
                      <div className="activity-info">
                        <span className="activity-op">{tx.tx_id.slice(0, 8)}...{tx.tx_id.slice(-6)}</span>
                      </div>
                      <div className="activity-amounts">
                        {dexyChanges.map(tc => {
                          const amt = tc.amount / Math.pow(10, tc.decimals)
                          const isPos = tc.amount > 0
                          return (
                            <span key={tc.token_id} className={`activity-token-amt ${isPos ? 'positive' : 'negative'}`}>
                              {isPos ? '+' : ''}{amt.toLocaleString(undefined, { maximumFractionDigits: tc.decimals })} {tc.name ?? tc.token_id.slice(0, 6)}
                            </span>
                          )
                        })}
                        <span className="activity-erg-amt">
                          {isReceive ? '+' : ''}{ergChange.toLocaleString(undefined, { maximumFractionDigits: 4 })} ERG
                        </span>
                      </div>
                      <span className="activity-time">
                        {tx.timestamp > 0 ? formatTimeAgo(tx.timestamp) : `#${tx.inclusion_height}`}
                      </span>
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        </div>

        {/* Recent Protocol Activity */}
        <div className="dexy-activity-section">
          <h3 className="dexy-section-header">Recent Protocol Activity</h3>
          <div className="dexy-activity-card">
            {activityLoading ? (
              <div className="dexy-activity-loading">
                <div className="spinner-small" />
                <span>Loading activity...</span>
              </div>
            ) : activity.length === 0 ? (
              <div className="dexy-activity-empty">No recent Dexy protocol activity</div>
            ) : (
              <div className="dexy-activity-list">
                {activity.map((item, idx) => {
                  const op = item.operation
                  const opLabel = op === 'mint' ? 'Mint'
                    : op === 'redeem' ? 'Redeem'
                    : op === 'swap' ? 'Swap'
                    : op === 'lp_deposit' ? 'Add Liquidity'
                    : op === 'lp_redeem' ? 'Remove Liquidity'
                    : item.operation
                  const opClass = op === 'mint' || op === 'lp_deposit' ? 'mint'
                    : op === 'swap' ? 'swap' : 'redeem'
                  const ergAbs = Math.abs(item.erg_change_nano) / 1e9
                  const icon = TOKEN_ICONS[item.token]
                  return (
                    <div
                      key={`${item.tx_id}-${idx}`}
                      className="dexy-activity-row"
                      onClick={() => navigateToExplorer({ page: 'transaction', id: item.tx_id })}
                      role="button"
                      tabIndex={0}
                    >
                      <div className={`activity-op-icon ${opClass}`}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                          {op === 'swap'
                            ? <path d="M7 16V4m0 0L3 8m4-4l4 4M17 8v12m0 0l4-4m-4 4l-4-4" />
                            : op === 'mint' || op === 'lp_deposit'
                              ? <path d="M12 19V5M5 12l7-7 7 7" />
                              : <path d="M12 5v14M5 12l7 7 7-7" />
                          }
                        </svg>
                      </div>
                      <div className="activity-info">
                        <div className="activity-label">
                          {icon && (
                            <span className={`activity-token-icon-wrap ${item.token === 'DexyGold' ? 'gold' : 'usd'}`}>
                              <img src={icon} alt="" />
                            </span>
                          )}
                          <span className="activity-op">{opLabel}</span>
                          <span className="activity-token">{item.token}</span>
                        </div>
                        <span className="activity-protocol">{item.protocol}</span>
                      </div>
                      <div className="activity-amounts">
                        {item.token_amount_change > 0 && (() => {
                          const decimals = TOKEN_DECIMALS[item.token] ?? 0
                          const amt = decimals > 0
                            ? (item.token_amount_change / Math.pow(10, decimals)).toLocaleString(undefined, { maximumFractionDigits: decimals })
                            : item.token_amount_change.toLocaleString()
                          const isPositive = op === 'mint' || op === 'lp_deposit'
                          return (
                            <span className={`activity-token-amt ${isPositive ? 'positive' : 'negative'}`}>
                              {amt} {item.token}
                            </span>
                          )
                        })()}
                        {ergAbs > 0 && (
                          <span className="activity-erg-amt">
                            {ergAbs.toLocaleString(undefined, { maximumFractionDigits: 2 })} ERG
                          </span>
                        )}
                      </div>
                      <span className="activity-time">
                        {item.timestamp > 0 ? formatTimeAgo(item.timestamp) : `#${item.height}`}
                      </span>
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        </div>
      </div>
      </>)}

      {subTab === 'liquidity' && (() => {
        const state = selectedVariant === 'gold' ? goldState : usdState
        const tokenName = selectedVariant === 'usd' ? 'USE' : 'DexyGold'
        const tokenDecimals = selectedVariant === 'usd' ? 3 : 0
        const lpTokenId = LP_TOKEN_IDS[selectedVariant]
        const userLpToken = walletBalance?.tokens.find(t => t.token_id === lpTokenId)
        const userLpBalance = userLpToken?.amount ?? 0
        const circulatingLp = state?.lp_circulating ?? 0
        const poolSharePct = circulatingLp > 0 ? (userLpBalance / circulatingLp) * 100 : 0
        // Value after 2% redemption fee
        const ergValue = circulatingLp > 0 && state ? Math.floor(userLpBalance * state.lp_erg_reserves / circulatingLp * 0.98) : 0
        const dexyValue = circulatingLp > 0 && state ? Math.floor(userLpBalance * state.lp_dexy_reserves / circulatingLp * 0.98) : 0

        const formatDexyAmount = (rawAmount: number): string => {
          if (tokenDecimals === 0) return rawAmount.toLocaleString()
          const divisor = Math.pow(10, tokenDecimals)
          return (rawAmount / divisor).toLocaleString(undefined, {
            minimumFractionDigits: tokenDecimals,
            maximumFractionDigits: tokenDecimals,
          })
        }

        return (
          <>
            {/* Variant Selector for Liquidity */}
            <div className="dexy-variant-selector">
              <button
                className={`dexy-variant-btn ${selectedVariant === 'gold' ? 'active gold' : ''}`}
                onClick={() => setSelectedVariant('gold')}
              >
                <img src="/icons/dexygold.svg" alt="DexyGold" className="dexy-variant-icon" />
                DexyGold
              </button>
              <button
                className={`dexy-variant-btn ${selectedVariant === 'usd' ? 'active usd' : ''}`}
                onClick={() => setSelectedVariant('usd')}
              >
                <img src="/icons/use.svg" alt="USE" className="dexy-variant-icon" />
                USE
              </button>
            </div>

            {lpTxStep === 'signing' && (
              <div className="dexy-lp-section">
                <h3>Sign Transaction</h3>
                <div className="mint-signing-step">
                  {lpFlow.signMethod === 'choose' && (
                    <div className="sign-method-choice">
                      <p>Choose signing method:</p>
                      <div className="sign-methods">
                        <button className="sign-method-btn" onClick={lpFlow.handleNautilusSign}>
                          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <rect x="2" y="3" width="20" height="14" rx="2" />
                            <path d="M8 21h8" />
                            <path d="M12 17v4" />
                          </svg>
                          <span>Nautilus</span>
                          <small>Browser Extension</small>
                        </button>
                        <button className="sign-method-btn" onClick={lpFlow.handleMobileSign}>
                          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <rect x="5" y="2" width="14" height="20" rx="2" />
                            <line x1="12" y1="18" x2="12.01" y2="18" />
                          </svg>
                          <span>Mobile</span>
                          <small>Scan QR Code</small>
                        </button>
                      </div>
                    </div>
                  )}
                  {lpFlow.signMethod === 'nautilus' && (
                    <div className="nautilus-waiting">
                      <div className="waiting-icon">
                        <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--primary)" strokeWidth="1.5">
                          <rect x="2" y="3" width="20" height="14" rx="2" />
                          <path d="M8 21h8" />
                          <path d="M12 17v4" />
                        </svg>
                      </div>
                      <p>Approve in Nautilus</p>
                      <div className="waiting-spinner" />
                      <button className="btn btn-secondary" onClick={lpFlow.handleBackToChoice}>Back</button>
                    </div>
                  )}
                  {lpFlow.signMethod === 'mobile' && lpFlow.qrUrl && (
                    <div className="qr-signing">
                      <p>Scan with Ergo Mobile Wallet</p>
                      <div className="qr-container">
                        <QRCodeSVG value={lpFlow.qrUrl} size={200} level="M" includeMargin bgColor="white" fgColor="black" />
                      </div>
                      <div className="waiting-spinner" />
                      <button className="btn btn-secondary" onClick={lpFlow.handleBackToChoice}>Back</button>
                    </div>
                  )}
                </div>
              </div>
            )}

            {lpTxStep === 'success' && (
              <div className="dexy-lp-section">
                <div className="mint-success-step">
                  <div className="success-icon">
                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--success)" strokeWidth="2">
                      <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                      <polyline points="22 4 12 14.01 9 11.01" />
                    </svg>
                  </div>
                  <h3>Transaction Submitted!</h3>
                  {lpFlow.txId && <TxSuccess txId={lpFlow.txId} explorerUrl={explorerUrl} />}
                  <button className="btn btn-primary" onClick={() => {
                    setLpTxStep('idle')
                    setDepositErg('')
                    setDepositDexy('')
                    setDepositPreview(null)
                    setRedeemLp('')
                    setRedeemPreview(null)
                    fetchAllStates()
                  }}>
                    Done
                  </button>
                </div>
              </div>
            )}

            {lpTxStep === 'error' && (
              <div className="dexy-lp-section">
                <div className="mint-error-step">
                  <div className="error-icon">
                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--error)" strokeWidth="2">
                      <circle cx="12" cy="12" r="10" />
                      <line x1="15" y1="9" x2="9" y2="15" />
                      <line x1="9" y1="9" x2="15" y2="15" />
                    </svg>
                  </div>
                  <h3>Transaction Failed</h3>
                  <p className="error-message">{lpTxError}</p>
                  <button className="btn btn-primary" onClick={() => setLpTxStep('idle')}>
                    Try Again
                  </button>
                </div>
              </div>
            )}

            {lpTxStep === 'idle' && (<>
              {/* Pool Liquidity */}
              <div className="dexy-lp-position">
                <h3>Pool Liquidity</h3>
                {state ? (
                  <div className="dexy-lp-stats">
                    <div className="dexy-lp-stat">
                      <span className="label">ERG Reserves</span>
                      <span className="value">{formatErg(state.lp_erg_reserves)}</span>
                    </div>
                    <div className="dexy-lp-stat">
                      <span className="label">{tokenName} Reserves</span>
                      <span className="value">{formatDexyAmount(state.lp_dexy_reserves)}</span>
                    </div>
                    <div className="dexy-lp-stat">
                      <span className="label">LP Rate</span>
                      <span className="value">{(state.lp_rate_nano / 1e9).toFixed(4)} ERG/{tokenName}</span>
                    </div>
                    <div className="dexy-lp-stat">
                      <span className="label">LP Circulating</span>
                      <span className="value">{circulatingLp.toLocaleString()}</span>
                    </div>
                  </div>
                ) : (
                  <p className="dexy-lp-empty">Loading pool data...</p>
                )}
              </div>

              {/* LP Position Display */}
              <div className="dexy-lp-position">
                <h3>Your LP Position</h3>
                {!walletAddress ? (
                  <p className="dexy-lp-empty">Connect wallet to see your LP position</p>
                ) : userLpBalance > 0 ? (
                  <div className="dexy-lp-stats">
                    <div className="dexy-lp-stat">
                      <span className="label">LP Tokens</span>
                      <span className="value">{userLpBalance.toLocaleString()}</span>
                    </div>
                    <div className="dexy-lp-stat">
                      <span className="label">Pool Share</span>
                      <span className="value">{poolSharePct.toFixed(4)}%</span>
                    </div>
                    <div className="dexy-lp-stat">
                      <span className="label">Value (ERG)</span>
                      <span className="value">{formatErg(ergValue)}</span>
                    </div>
                    <div className="dexy-lp-stat">
                      <span className="label">Value ({tokenName})</span>
                      <span className="value">{formatDexyAmount(dexyValue)}</span>
                    </div>
                  </div>
                ) : (
                  <p className="dexy-lp-empty">No LP tokens held</p>
                )}
              </div>

              {/* Deposit Section */}
              <div className="dexy-lp-section">
                <h3>Add Liquidity</h3>
                <div className="dexy-lp-form">
                  <div className="dexy-lp-input-group">
                    <label>ERG Amount {walletBalance ? <span style={{ color: 'var(--slate-500)', fontWeight: 400 }}>[available: {(walletBalance.erg_nano / 1e9).toFixed(4)}]</span> : null}</label>
                    <input
                      type="number"
                      value={depositErg}
                      onChange={e => {
                        const val = e.target.value
                        setDepositErg(val)
                        if (state && state.lp_erg_reserves > 0 && state.lp_dexy_reserves > 0) {
                          const ergVal = parseFloat(val || '0')
                          if (ergVal > 0) {
                            const ergNano = ergVal * 1e9
                            const dexyRaw = ergNano * state.lp_dexy_reserves / state.lp_erg_reserves
                            setDepositDexy(tokenDecimals > 0
                              ? (dexyRaw / Math.pow(10, tokenDecimals)).toFixed(tokenDecimals)
                              : Math.floor(dexyRaw).toString())
                          } else {
                            setDepositDexy('')
                          }
                        }
                      }}
                      placeholder="0.0"
                      min="0"
                      step="0.1"
                    />
                  </div>
                  <div className="dexy-lp-input-group">
                    <label>{tokenName} Amount {(() => {
                      const tok = walletBalance?.tokens.find(t => t.token_id === (selectedVariant === 'gold' ? DEXY_TOKEN_IDS['gold'] : DEXY_TOKEN_IDS['usd']))
                      if (!tok) return null
                      const display = tokenDecimals > 0 ? (tok.amount / Math.pow(10, tokenDecimals)).toFixed(tokenDecimals) : tok.amount.toLocaleString()
                      return <span style={{ color: 'var(--slate-500)', fontWeight: 400 }}>[available: {display}]</span>
                    })()}</label>
                    <input
                      type="number"
                      value={depositDexy}
                      onChange={e => {
                        const val = e.target.value
                        setDepositDexy(val)
                        if (state && state.lp_erg_reserves > 0 && state.lp_dexy_reserves > 0) {
                          const dexyVal = parseFloat(val || '0')
                          if (dexyVal > 0) {
                            const dexyRaw = dexyVal * Math.pow(10, tokenDecimals)
                            const ergNano = dexyRaw * state.lp_erg_reserves / state.lp_dexy_reserves
                            setDepositErg((ergNano / 1e9).toFixed(4))
                          } else {
                            setDepositErg('')
                          }
                        }
                      }}
                      placeholder="0.0"
                      min="0"
                      step={tokenDecimals === 0 ? '1' : '0.001'}
                    />
                  </div>
                  {depositPreview && depositPreview.can_execute && (
                    <div className="dexy-lp-preview">
                      <div className="preview-row">
                        <span>LP Tokens to receive:</span>
                        <span>{Number(depositPreview.lp_tokens).toLocaleString()}</span>
                      </div>
                      <div className="preview-row">
                        <span>ERG consumed:</span>
                        <span>{(Number(depositPreview.erg_amount) / 1e9).toFixed(4)} ERG</span>
                      </div>
                      <div className="preview-row">
                        <span>{tokenName} consumed:</span>
                        <span>{formatDexyAmount(Number(depositPreview.dexy_amount))}</span>
                      </div>
                    </div>
                  )}
                  {depositPreview && !depositPreview.can_execute && depositPreview.error && (
                    <div className="dexy-lp-error">{depositPreview.error}</div>
                  )}
                  <button
                    className="dexy-action-btn"
                    disabled={!depositPreview?.can_execute || !isConnected || !walletAddress || lpTxLoading}
                    onClick={handleDeposit}
                  >
                    {lpTxLoading ? 'Building...' : 'Add Liquidity'}
                  </button>
                </div>
              </div>

              {/* Redeem Section */}
              <div className="dexy-lp-section">
                <h3>Remove Liquidity</h3>
                {state && !state.can_redeem_lp && (
                  <div className="dexy-lp-warning">
                    Redemption locked: LP rate is below 98% of oracle rate (depeg protection)
                  </div>
                )}
                <div className="dexy-lp-form">
                  <div className="dexy-lp-input-group">
                    <label>LP Tokens to redeem {userLpBalance > 0 ? <span style={{ color: 'var(--slate-500)', fontWeight: 400 }}>[available: {userLpBalance.toLocaleString()}]</span> : null}</label>
                    <input
                      type="number"
                      value={redeemLp}
                      onChange={e => {
                        const val = e.target.value
                        setRedeemLp(val)
                        if (state && circulatingLp > 0) {
                          const lp = parseFloat(val || '0')
                          if (lp > 0) {
                            const ergOut = Math.floor(lp * state.lp_erg_reserves / circulatingLp * 98 / 100)
                            const dexyOut = Math.floor(lp * state.lp_dexy_reserves / circulatingLp * 98 / 100)
                            setRedeemErg((ergOut / 1e9).toFixed(4))
                            setRedeemDexy(tokenDecimals > 0
                              ? (dexyOut / Math.pow(10, tokenDecimals)).toFixed(tokenDecimals)
                              : Math.floor(dexyOut).toString())
                          } else {
                            setRedeemErg('')
                            setRedeemDexy('')
                          }
                        }
                      }}
                      placeholder="0"
                      min="0"
                      step="1"
                    />
                    {userLpBalance > 0 && (
                      <button className="dexy-max-btn" onClick={() => {
                        const lp = userLpBalance
                        setRedeemLp(String(lp))
                        if (state && circulatingLp > 0) {
                          const ergOut = Math.floor(lp * state.lp_erg_reserves / circulatingLp * 98 / 100)
                          const dexyOut = Math.floor(lp * state.lp_dexy_reserves / circulatingLp * 98 / 100)
                          setRedeemErg((ergOut / 1e9).toFixed(4))
                          setRedeemDexy(tokenDecimals > 0
                            ? (dexyOut / Math.pow(10, tokenDecimals)).toFixed(tokenDecimals)
                            : Math.floor(dexyOut).toString())
                        }
                      }}>
                        MAX
                      </button>
                    )}
                  </div>
                  <div className="dexy-lp-input-group">
                    <label>ERG to receive</label>
                    <input
                      type="number"
                      value={redeemErg}
                      onChange={e => {
                        const val = e.target.value
                        setRedeemErg(val)
                        if (state && circulatingLp > 0 && state.lp_erg_reserves > 0) {
                          const ergVal = parseFloat(val || '0')
                          if (ergVal > 0) {
                            const ergNano = ergVal * 1e9
                            const lpNeeded = Math.ceil(ergNano * circulatingLp * 100 / (state.lp_erg_reserves * 98))
                            const dexyOut = Math.floor(lpNeeded * state.lp_dexy_reserves / circulatingLp * 98 / 100)
                            setRedeemLp(String(lpNeeded))
                            setRedeemDexy(tokenDecimals > 0
                              ? (dexyOut / Math.pow(10, tokenDecimals)).toFixed(tokenDecimals)
                              : Math.floor(dexyOut).toString())
                          } else {
                            setRedeemLp('')
                            setRedeemDexy('')
                          }
                        }
                      }}
                      placeholder="0.0"
                      min="0"
                      step="0.1"
                    />
                  </div>
                  <div className="dexy-lp-input-group">
                    <label>{tokenName} to receive</label>
                    <input
                      type="number"
                      value={redeemDexy}
                      onChange={e => {
                        const val = e.target.value
                        setRedeemDexy(val)
                        if (state && circulatingLp > 0 && state.lp_dexy_reserves > 0) {
                          const dexyVal = parseFloat(val || '0')
                          if (dexyVal > 0) {
                            const dexyRaw = dexyVal * Math.pow(10, tokenDecimals)
                            const lpNeeded = Math.ceil(dexyRaw * circulatingLp * 100 / (state.lp_dexy_reserves * 98))
                            const ergOut = Math.floor(lpNeeded * state.lp_erg_reserves / circulatingLp * 98 / 100)
                            setRedeemLp(String(lpNeeded))
                            setRedeemErg((ergOut / 1e9).toFixed(4))
                          } else {
                            setRedeemLp('')
                            setRedeemErg('')
                          }
                        }
                      }}
                      placeholder="0.0"
                      min="0"
                      step={tokenDecimals === 0 ? '1' : '0.001'}
                    />
                  </div>
                  {redeemPreview && redeemPreview.can_execute && (
                    <div className="dexy-lp-preview">
                      <div className="preview-row">
                        <span>ERG to receive (confirmed):</span>
                        <span>{(Number(redeemPreview.erg_amount) / 1e9).toFixed(4)} ERG</span>
                      </div>
                      <div className="preview-row">
                        <span>{tokenName} to receive (confirmed):</span>
                        <span>{formatDexyAmount(Number(redeemPreview.dexy_amount))}</span>
                      </div>
                      <div className="preview-row muted">
                        <span>Redemption fee:</span>
                        <span>2%</span>
                      </div>
                    </div>
                  )}
                  {redeemPreview && !redeemPreview.can_execute && redeemPreview.error && (
                    <div className="dexy-lp-error">{redeemPreview.error}</div>
                  )}
                  <button
                    className="dexy-action-btn"
                    disabled={!redeemPreview?.can_execute || !isConnected || !walletAddress || userLpBalance === 0 || lpTxLoading}
                    onClick={handleRedeem}
                  >
                    {lpTxLoading ? 'Building...' : 'Remove Liquidity'}
                  </button>
                </div>
              </div>
            </>)}
          </>
        )
      })()}

      {/* Mint Modal */}
      {mintModalOpen && walletAddress && walletBalance && (
        <DexyMintModal
          isOpen={mintModalOpen}
          onClose={() => setMintModalOpen(false)}
          variant={selectedVariant}
          state={selectedVariant === 'gold' ? goldState : usdState}
          walletAddress={walletAddress}
          ergBalance={walletBalance.erg_nano}
          explorerUrl={explorerUrl}
          onSuccess={() => {
            fetchAllStates()
            fetchDexyActivity()
            fetchUserDexyTxs()
          }}
        />
      )}

      {/* Swap Modal */}
      {swapModalOpen && walletAddress && walletBalance && (
        <DexySwapModal
          isOpen={swapModalOpen}
          onClose={() => setSwapModalOpen(false)}
          variant={selectedVariant}
          state={selectedVariant === 'gold' ? goldState : usdState}
          walletAddress={walletAddress}
          ergBalance={walletBalance.erg_nano}
          dexyBalance={getDexyBalance(selectedVariant)}
          explorerUrl={explorerUrl}
          onSuccess={() => {
            fetchAllStates()
            fetchDexyActivity()
            fetchUserDexyTxs()
          }}
        />
      )}
    </div>
  )
}

// Asset card matching SigmaUSD's token-card structure
function DexyAssetCard({
  state,
  variant,
  tokenName,
  decimals,
  walletAddress,
  walletBalance,
  onMint,
  onSwap,
  ergUsdPrice,
}: {
  state: DexyState | null
  variant: DexyVariant
  tokenName: string
  decimals: number
  walletAddress: string | null
  walletBalance: WalletBalance | null
  onMint: () => void
  onSwap: () => void
  ergUsdPrice?: number
}) {
  const formatRate = (rateNano: number) => {
    const tokenMultiplier = Math.pow(10, decimals)
    const ergPerDisplayUnit = (rateNano / 1e9) * tokenMultiplier
    return ergPerDisplayUnit.toFixed(4)
  }

  const formatAmount = (amount: number, dec: number) => {
    const divisor = Math.pow(10, dec)
    return (amount / divisor).toLocaleString(undefined, {
      minimumFractionDigits: dec,
      maximumFractionDigits: dec,
    })
  }

  const description = variant === 'gold'
    ? '1 DexyGold = 1mg of gold'
    : 'USD-pegged stablecoin'

  const goldUsdPerOz = variant === 'gold' && state && ergUsdPrice
    ? (state.oracle_rate_nano / 1e9) * TROY_OZ_IN_MG * ergUsdPrice
    : null

  const iconSrc = variant === 'gold' ? '/icons/dexygold.svg' : '/icons/use.svg'
  const iconAlt = variant === 'gold' ? 'DexyGold' : 'USE'
  const colorClass = variant === 'gold' ? 'amber' : 'emerald'

  const userBalance = walletBalance?.tokens.find(t => t.token_id === DEXY_TOKEN_IDS[variant])?.amount || 0

  if (!state) {
    return (
      <div className={`token-card ${colorClass}`}>
        <div className="token-card-header">
          <div className="token-header-content">
            <div className="token-header-left">
              <div className={`dexy-token-icon-wrap ${variant}`}>
                <img src={iconSrc} alt={iconAlt} />
              </div>
              <div className="token-info">
                <h3>{tokenName}</h3>
                <p>{description}</p>
              </div>
            </div>
            <span className="token-ticker">{tokenName.toUpperCase()}</span>
          </div>
        </div>
        <div className="token-card-body">
          <div className="asset-loading">
            <div className="spinner-small" />
            <span>Loading...</span>
          </div>
        </div>
      </div>
    )
  }

  // FreeMint is the only mint type the app can build.
  // can_mint only checks the rate condition; we also need free_mint_available > 0.
  const freeMintReady = state.can_mint && state.free_mint_available > 0
  const canMint = walletAddress && freeMintReady

  // Estimate time until FreeMint resets (~2 min per block on Ergo)
  const blocksUntilReset = state.free_mint_reset_height - state.current_height
  const freeMintResetsIn = blocksUntilReset > 0
    ? blocksUntilReset <= 30 ? `~${blocksUntilReset * 2}m`
      : `~${Math.round(blocksUntilReset * 2 / 60)}h`
    : null

  return (
    <div className={`token-card ${colorClass}`}>
      <div className="token-card-header">
        <div className="token-header-content">
          <div className="token-header-left">
            <div className={`dexy-token-icon-wrap ${variant}`}>
              <img src={iconSrc} alt={iconAlt} />
            </div>
            <div className="token-info">
              <h3>{tokenName}</h3>
              <p>{description}</p>
            </div>
          </div>
          <span className="token-ticker">{tokenName.toUpperCase()}</span>
        </div>
      </div>

      <div className="token-card-body">
        <div className="token-stats">
          <div className="token-stat">
            <span className="token-stat-label">Oracle Rate</span>
            <span className="token-stat-value">{formatRate(state.oracle_rate_nano)} ERG</span>
          </div>
          <div className="token-stat">
            <span className="token-stat-label">LP Rate</span>
            <span className="token-stat-value">{formatRate(state.lp_rate_nano)} ERG</span>
          </div>
          <div className="token-stat">
            <span className="token-stat-label">Rate Diff</span>
            <span className={`token-stat-value ${state.rate_difference_pct > 0 ? 'positive' : state.rate_difference_pct < 0 ? 'negative' : ''}`}>
              {state.rate_difference_pct > 0 ? '+' : ''}{state.rate_difference_pct.toFixed(2)}%
            </span>
          </div>
          <div className="token-stat">
            <span className="token-stat-label">Circulating</span>
            <span className="token-stat-value">{formatAmount(state.dexy_circulating, decimals)}</span>
          </div>
          {goldUsdPerOz !== null && (
            <div className="token-stat">
              <span className="token-stat-label">Gold (USD/oz)</span>
              <span className="token-stat-value">${goldUsdPerOz.toLocaleString(undefined, { maximumFractionDigits: 2 })}</span>
            </div>
          )}
        </div>

        {walletAddress && userBalance > 0 && (
          <div className="wallet-balance-box">
            <div className="wallet-balance-row">
              <span className="wallet-balance-label">Your Balance</span>
              <div className="wallet-balance-value">
                <span className="wallet-balance-amount">{formatAmount(userBalance, decimals)}</span>
                <span className="wallet-balance-ticker">{tokenName.toUpperCase()}</span>
              </div>
            </div>
          </div>
        )}

        <div className="token-actions">
          <button
            className={`action-btn ${canMint ? `primary ${colorClass}` : ''}`}
            disabled={!canMint}
            onClick={onMint}
            title={
              !walletAddress ? 'Connect wallet first'
                : !state.can_mint ? 'Minting unavailable (rate condition not met)'
                : state.free_mint_available <= 0 ? `FreeMint exhausted${freeMintResetsIn ? ` (resets in ${freeMintResetsIn})` : ''}`
                : `Mint ${tokenName}`
            }
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 4v16m8-8H4" />
            </svg>
            Mint
          </button>
          <button
            className={`action-btn ${walletAddress ? 'secondary' : ''}`}
            disabled={!walletAddress}
            onClick={onSwap}
            title={!walletAddress ? 'Connect wallet first' : `Swap via LP pool`}
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M7 16V4m0 0L3 8m4-4l4 4M17 8v12m0 0l4-4m-4 4l-4-4" />
            </svg>
            Swap
          </button>
        </div>

        <div className="status-badges">
          {freeMintReady ? (
            <span className="status-badge available">
              <span className="dot" />
              Mint Available
            </span>
          ) : state.can_mint && state.free_mint_available <= 0 ? (
            <span className="status-badge exhausted">
              <span className="dot" />
              FreeMint Exhausted{freeMintResetsIn && ` (resets ${freeMintResetsIn})`}
            </span>
          ) : (
            <span className="status-badge unavailable">
              <span className="dot" />
              Mint Unavailable
            </span>
          )}
          {(() => {
            // Compare cost per token: FreeMint (oracle rate + 0.5% bank fee) vs LP Swap (lp rate + 0.3% LP fee)
            const mintRate = state.oracle_rate_nano * 1.005
            const swapEffective = state.lp_rate_nano * 1.003
            const mintBetter = state.can_mint && mintRate < swapEffective
            const savingPct = Math.abs(mintRate - swapEffective) / Math.max(mintRate, swapEffective) * 100
            if (!mintBetter) {
              return (
                <span className="status-badge best-path swap-best">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                    <polyline points="20 6 9 17 4 12" />
                  </svg>
                  Best: LP Swap
                  {savingPct > 0.1 && <span className="saving-pct">({savingPct.toFixed(1)}% cheaper)</span>}
                </span>
              )
            }
            return (
              <span className={`status-badge best-path mint-best${!freeMintReady ? ' dimmed' : ''}`}>
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
                {freeMintReady ? 'Best: Mint' : 'Mint cheaper, but exhausted'}
                {savingPct > 0.1 && <span className="saving-pct">({savingPct.toFixed(1)}%)</span>}
              </span>
            )
          })()}
        </div>
      </div>
    </div>
  )
}
