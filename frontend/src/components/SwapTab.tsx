import { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  getAmmPools, getAmmQuote, getPoolDisplayName, formatTokenAmount, formatErg,
  buildAmmLpDepositTx, buildAmmLpDepositOrder,
  buildAmmLpRedeemTx, buildAmmLpRedeemOrder,
  startSwapSign, getSwapTxStatus,
  previewPoolCreate, buildPoolBootstrapTx, buildPoolCreateTx,
  type AmmPool, type SwapQuote, type AmmLpBuildResponse,
  type PoolCreatePreviewResponse,
} from '../api/amm'
import { useTransactionFlow } from '../hooks/useTransactionFlow'
import { SwapModal } from './SwapModal'
import { OrderHistory } from './OrderHistory'

interface SwapTabProps {
  isConnected: boolean
  walletAddress: string | null
  walletBalance: {
    erg_nano: number
    tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>
  } | null
  explorerUrl: string
  ergUsdPrice?: number
  canMintSigusd?: boolean
  reserveRatioPct?: number
}

// =============================================================================
// Token Icons
// =============================================================================

/** Map known token names (lowercase) to icon paths */
const TOKEN_ICON_MAP: Record<string, string> = {
  // Core tokens
  erg: '/icons/ergo.svg',
  sigusd: '/icons/sigmausd.svg',
  sigrsv: '/icons/sigrsv.svg',
  rsn: '/icons/rosen.svg',
  rsada: '/icons/rsada.svg',
  spf: '/icons/spf.svg',
  rsbtc: '/icons/rsbtc.svg',
  quacks: '/icons/quacks.svg',
  // Bridge tokens
  rseth: '/icons/rseth.svg',
  rsbnb: '/icons/rsbnb.svg',
  rsdoge: '/icons/rsdoge.png',
  rsdis: '/icons/rsdis.png',
  // DeFi / ecosystem tokens
  ergopad: '/icons/ergopad.svg',
  neta: '/icons/neta.svg',
  paideia: '/icons/paideia.svg',
  exle: '/icons/exle.svg',
  epos: '/icons/epos.svg',
  flux: '/icons/flux.svg',
  terahertz: '/icons/terahertz.svg',
  gort: '/icons/gort.png',
  gluon: '/icons/gluon.png',
  // Dexy tokens
  dexygold: '/icons/dexygold.svg',
  use: '/icons/use.svg',
  // Meme / community tokens
  erdoge: '/icons/erdoge.svg',
  ermoon: '/icons/ermoon.svg',
  kushti: '/icons/kushti.svg',
  comet: '/icons/comet.png',
  aht: '/icons/aht.svg',
  burn: '/icons/burn.svg',
  getblok: '/icons/getblock.svg',
  hodlerg: '/icons/hodlerg3.svg',
  hodlerg3: '/icons/hodlerg3.svg',
  ergold: '/icons/ergold.svg',
  egio: '/icons/egio.svg',
  woodennickels: '/icons/woodennickels.svg',
  migoreng: '/icons/migoreng.svg',
  greasycex: '/icons/greasycex.svg',
  bober: '/icons/bober.png',
  bulls: '/icons/bulls.png',
  buns: '/icons/buns.png',
  cypx: '/icons/cypx.png',
  ergonaut: '/icons/ergonaut.png',
  ergone: '/icons/ergone.png',
  gauc: '/icons/gauc.png',
  gau: '/icons/gau.png',
  gif: '/icons/gif.png',
  ketchup: '/icons/ketchup.png',
  love: '/icons/love.png',
  lunadog: '/icons/lunadog.png',
  lykos: '/icons/lykos.png',
  mew: '/icons/mew.png',
  mustard: '/icons/mustard.png',
  oink: '/icons/oink.png',
  obsidian: '/icons/obsidian.png',
  pandav: '/icons/pandav.png',
  peperg: '/icons/peperg.png',
  php: '/icons/php.png',
  proxie: '/icons/proxie.png',
  walrus: '/icons/walrus.png',
  auctioncoin: '/icons/auctioncoin.png',
}

/** Deterministic color from token name for fallback circles */
function tokenColor(name: string): string {
  let hash = 0
  for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash)
  const hue = ((hash % 360) + 360) % 360
  return `hsl(${hue}, 55%, 55%)`
}

function TokenIcon({ name, size = 18 }: { name: string; size?: number }) {
  const icon = TOKEN_ICON_MAP[name.toLowerCase()]
  if (icon) {
    return <img src={icon} alt={name} className="token-icon" style={{ width: size, height: size }} />
  }
  return (
    <span
      className="token-icon-fallback"
      style={{ width: size, height: size, background: tokenColor(name), fontSize: size * 0.55 }}
    >
      {name.charAt(0).toUpperCase()}
    </span>
  )
}

function PoolPairIcons({ pool }: { pool: AmmPool }) {
  const nameX = pool.pool_type === 'N2T' ? 'ERG' : (pool.token_x?.name || 'X')
  const nameY = pool.token_y.name || 'Y'
  return (
    <span className="pool-pair-icons">
      <TokenIcon name={nameX} size={18} />
      <TokenIcon name={nameY} size={18} />
    </span>
  )
}

// =============================================================================
// Helper Functions
// =============================================================================

function getInputType(pool: AmmPool, side: 'x' | 'y'): 'erg' | 'token' {
  if (pool.pool_type === 'N2T') {
    return side === 'x' ? 'erg' : 'token'
  }
  // T2T: both sides are tokens
  return 'token'
}

function getInputTokenId(pool: AmmPool, side: 'x' | 'y'): string | undefined {
  if (pool.pool_type === 'N2T') {
    return side === 'x' ? undefined : pool.token_y.token_id
  }
  // T2T
  return side === 'x' ? pool.token_x?.token_id : pool.token_y.token_id
}

function getInputLabel(pool: AmmPool, side: 'x' | 'y'): string {
  if (pool.pool_type === 'N2T') {
    return side === 'x' ? 'ERG' : (pool.token_y.name || pool.token_y.token_id.slice(0, 8))
  }
  if (side === 'x') {
    return pool.token_x?.name || pool.token_x?.token_id.slice(0, 8) || 'Token X'
  }
  return pool.token_y.name || pool.token_y.token_id.slice(0, 8)
}

function getOutputLabel(pool: AmmPool, side: 'x' | 'y'): string {
  // Output is the opposite side
  return getInputLabel(pool, side === 'x' ? 'y' : 'x')
}

function getInputDecimals(pool: AmmPool, side: 'x' | 'y'): number {
  if (pool.pool_type === 'N2T') {
    return side === 'x' ? 9 : (pool.token_y.decimals ?? 0)
  }
  if (side === 'x') {
    return pool.token_x?.decimals ?? 0
  }
  return pool.token_y.decimals ?? 0
}

function parseInputAmount(input: string, pool: AmmPool, side: 'x' | 'y'): number {
  const value = parseFloat(input)
  if (isNaN(value) || value <= 0) return 0
  const decimals = getInputDecimals(pool, side)
  return Math.round(value * Math.pow(10, decimals))
}

function formatForInput(amount: number, decimals: number): string {
  if (decimals === 0) return amount.toString()
  return (amount / Math.pow(10, decimals)).toString()
}

// =============================================================================
// SwapTab Component
// =============================================================================

export function SwapTab({ isConnected, walletAddress, walletBalance, explorerUrl, ergUsdPrice, canMintSigusd, reserveRatioPct }: SwapTabProps) {
  const [pools, setPools] = useState<AmmPool[]>([])
  const [filteredPools, setFilteredPools] = useState<AmmPool[]>([])
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedPool, setSelectedPool] = useState<AmmPool | null>(null)
  const [inputAmount, setInputAmount] = useState('')
  const [inputSide, setInputSide] = useState<'x' | 'y'>('x')
  const [quote, setQuote] = useState<SwapQuote | null>(null)
  const [quoteLoading, setQuoteLoading] = useState(false)
  const [quoteError, setQuoteError] = useState<string | null>(null)
  const [slippage, setSlippage] = useState(0.5)
  const [nitro, setNitro] = useState(1.2)
  const [swapMode, setSwapMode] = useState<'proxy' | 'direct'>('proxy')
  const [showSwapModal, setShowSwapModal] = useState(false)
  const [poolsLoading, setPoolsLoading] = useState(false)
  const [poolsError, setPoolsError] = useState<string | null>(null)
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Liquidity view state
  const [view, setView] = useState<'swap' | 'liquidity'>('swap')
  const [lpMode, setLpMode] = useState<'deposit' | 'redeem'>('deposit')
  const [lpPool, setLpPool] = useState<AmmPool | null>(null)
  const [depositErgInput, setDepositErgInput] = useState('')
  const [depositTokenInput, setDepositTokenInput] = useState('')
  const [depositLpOutput, setDepositLpOutput] = useState('')
  const [redeemLpInput, setRedeemLpInput] = useState('')
  const [redeemErgOutput, setRedeemErgOutput] = useState('')
  const [redeemTokenOutput, setRedeemTokenOutput] = useState('')
  const [lpSwapMode, setLpSwapMode] = useState<'proxy' | 'direct'>('proxy')
  const [lpTxStep, setLpTxStep] = useState<'idle' | 'building' | 'signing' | 'success' | 'error'>('idle')
  const [lpTxLoading, setLpTxLoading] = useState(false)
  const [lpTxError, setLpTxError] = useState<string | null>(null)

  // Pool creation state
  const [showCreatePool, setShowCreatePool] = useState(false)
  const [createPoolType, setCreatePoolType] = useState<'N2T' | 'T2T'>('N2T')
  const [createXTokenId, setCreateXTokenId] = useState('')
  const [createXAmount, setCreateXAmount] = useState('')
  const [createYTokenId, setCreateYTokenId] = useState('')
  const [createYAmount, setCreateYAmount] = useState('')
  const [createFeePercent, setCreateFeePercent] = useState('0.3')
  const [createPreview, setCreatePreview] = useState<PoolCreatePreviewResponse | null>(null)
  const [createLoading, setCreateLoading] = useState(false)
  const [createError, setCreateError] = useState('')
  const [createTxStep, setCreateTxStep] = useState<'idle' | 'signing_bootstrap' | 'signing_create' | 'done'>('idle')

  // SigUSD divergence
  const SIGUSD_TOKEN_ID = '03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04'

  const sigusdDivergence = useMemo(() => {
    if (!ergUsdPrice || ergUsdPrice <= 0) return null
    const sigusdPool = pools
      .filter(p => p.pool_type === 'N2T' && p.token_y.token_id === SIGUSD_TOKEN_ID)
      .sort((a, b) => (b.erg_reserves ?? 0) - (a.erg_reserves ?? 0))[0]
    if (!sigusdPool || !sigusdPool.erg_reserves) return null

    const dexErgUsd = (sigusdPool.token_y.amount / 100) / (sigusdPool.erg_reserves / 1e9)
    const pct = ((dexErgUsd - ergUsdPrice) / ergUsdPrice) * 100
    if (Math.abs(pct) < 3) return null

    return { pool: sigusdPool, dexPrice: dexErgUsd, oraclePrice: ergUsdPrice, pct }
  }, [pools, ergUsdPrice])

  // Fetch pools
  const fetchPools = useCallback(async () => {
    if (!isConnected) return
    setPoolsLoading(true)
    try {
      const response = await getAmmPools()
      setPools(response.pools)
      setPoolsError(null)
    } catch (e) {
      console.error('Failed to fetch AMM pools:', e)
      setPoolsError(String(e))
    } finally {
      setPoolsLoading(false)
    }
  }, [isConnected])

  useEffect(() => {
    fetchPools()
    const interval = setInterval(fetchPools, 30000)
    return () => clearInterval(interval)
  }, [fetchPools])

  // Build set of user's token IDs for pool matching
  const userTokenIds = walletBalance
    ? new Set(walletBalance.tokens.map(t => t.token_id))
    : new Set<string>()

  const isUserPool = useCallback((pool: AmmPool): boolean => {
    if (userTokenIds.size === 0) return false
    if (pool.token_y && userTokenIds.has(pool.token_y.token_id)) return true
    if (pool.token_x && userTokenIds.has(pool.token_x.token_id)) return true
    return false
  }, [userTokenIds])

  // Filter pools by search, then pin user-token pools to top
  useEffect(() => {
    let result = pools
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase()
      result = pools.filter(p => {
        const name = getPoolDisplayName(p).toLowerCase()
        return name.includes(q) || p.pool_id.toLowerCase().includes(q)
      })
    }

    if (userTokenIds.size > 0) {
      const userPools = result.filter(p => isUserPool(p))
      const otherPools = result.filter(p => !isUserPool(p))
      setFilteredPools([...userPools, ...otherPools])
    } else {
      setFilteredPools(result)
    }
  }, [pools, searchQuery, walletBalance])

  // Auto-select first pool
  useEffect(() => {
    if (pools.length > 0 && !selectedPool) {
      setSelectedPool(pools[0])
    }
  }, [pools, selectedPool])

  // Fetch quote with debounce
  useEffect(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current)
    }

    if (!selectedPool || !inputAmount || parseFloat(inputAmount) <= 0) {
      setQuote(null)
      setQuoteError(null)
      return
    }

    const rawAmount = parseInputAmount(inputAmount, selectedPool, inputSide)
    if (rawAmount <= 0) {
      setQuote(null)
      return
    }

    debounceRef.current = setTimeout(async () => {
      setQuoteLoading(true)
      setQuoteError(null)
      try {
        const inputType = getInputType(selectedPool, inputSide)
        const tokenId = getInputTokenId(selectedPool, inputSide)
        const result = await getAmmQuote(selectedPool.pool_id, inputType, rawAmount, tokenId)
        setQuote(result)
      } catch (e) {
        console.error('Failed to get quote:', e)
        setQuoteError(String(e))
        setQuote(null)
      } finally {
        setQuoteLoading(false)
      }
    }, 300)

    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current)
      }
    }
  }, [selectedPool, inputAmount, inputSide])

  const handlePoolSelect = (pool: AmmPool) => {
    setSelectedPool(pool)
    setInputAmount('')
    setQuote(null)
    setQuoteError(null)
    setInputSide('x')
  }

  const handleFlip = () => {
    setInputSide(prev => prev === 'x' ? 'y' : 'x')
    setInputAmount('')
    setQuote(null)
    setQuoteError(null)
  }

  const handleMax = () => {
    if (!selectedPool || !walletBalance) return
    const inputType = getInputType(selectedPool, inputSide)
    const decimals = getInputDecimals(selectedPool, inputSide)

    if (inputType === 'erg') {
      // Leave some ERG for fees
      const available = Math.max(0, walletBalance.erg_nano - 10_000_000) // 0.01 ERG buffer
      setInputAmount(formatForInput(available, decimals))
    } else {
      const tokenId = getInputTokenId(selectedPool, inputSide)
      const token = walletBalance.tokens.find(t => t.token_id === tokenId)
      if (token) {
        setInputAmount(formatForInput(token.amount, decimals))
      }
    }
  }

  const handleSwapClick = () => {
    if (!selectedPool || !quote || !walletAddress) return
    setShowSwapModal(true)
  }

  const canSwap = selectedPool && quote && walletAddress && !quoteLoading && !quoteError

  // =========================================================================
  // Liquidity: useTransactionFlow hook
  // =========================================================================

  const lpFlow = useTransactionFlow({
    pollStatus: getSwapTxStatus,
    isOpen: lpTxStep === 'signing',
    onSuccess: (txId) => {
      void txId
      setLpTxStep('success')
    },
    onError: (err) => {
      setLpTxError(err)
      setLpTxStep('error')
    },
    watchParams: { protocol: 'amm', operation: lpMode === 'deposit' ? 'lp_deposit' : 'lp_redeem', description: 'LP operation' },
  })

  // =========================================================================
  // Liquidity: helper functions
  // =========================================================================

  const getUserLpBalance = useCallback((pool: AmmPool): number => {
    if (!walletBalance) return 0
    const lpToken = walletBalance.tokens.find(t => t.token_id === pool.lp_token_id)
    return lpToken?.amount ?? 0
  }, [walletBalance])

  const handleDepositErgChange = useCallback((val: string) => {
    setDepositErgInput(val)
    if (!lpPool || !lpPool.erg_reserves || lpPool.token_y.amount === 0) return
    const ergVal = parseFloat(val || '0')
    if (ergVal > 0) {
      const ergNano = Math.floor(ergVal * 1e9)
      const tokenNeeded = Math.floor(ergNano * lpPool.token_y.amount / lpPool.erg_reserves!)
      const lpReward = Math.floor(ergNano * lpPool.lp_circulating / lpPool.erg_reserves!)
      const tokenDecimals = lpPool.token_y.decimals ?? 0
      setDepositTokenInput(tokenDecimals > 0
        ? (tokenNeeded / Math.pow(10, tokenDecimals)).toFixed(tokenDecimals)
        : tokenNeeded.toString())
      setDepositLpOutput(lpReward.toLocaleString())
    } else {
      setDepositTokenInput('')
      setDepositLpOutput('')
    }
  }, [lpPool])

  const handleDepositTokenChange = useCallback((val: string) => {
    setDepositTokenInput(val)
    if (!lpPool || !lpPool.erg_reserves || lpPool.erg_reserves === 0) return
    const tokenDecimals = lpPool.token_y.decimals ?? 0
    const tokenVal = parseFloat(val || '0')
    if (tokenVal > 0) {
      const tokenRaw = Math.floor(tokenVal * Math.pow(10, tokenDecimals))
      const ergNeeded = Math.floor(tokenRaw * lpPool.erg_reserves! / lpPool.token_y.amount)
      const lpReward = Math.floor(tokenRaw * lpPool.lp_circulating / lpPool.token_y.amount)
      setDepositErgInput((ergNeeded / 1e9).toFixed(4))
      setDepositLpOutput(lpReward.toLocaleString())
    } else {
      setDepositErgInput('')
      setDepositLpOutput('')
    }
  }, [lpPool])

  const handleRedeemLpChange = useCallback((val: string) => {
    setRedeemLpInput(val)
    if (!lpPool || !lpPool.erg_reserves || lpPool.lp_circulating === 0) return
    const lpVal = parseFloat(val || '0')
    if (lpVal > 0) {
      const lpAmount = Math.floor(lpVal)
      const ergOut = Math.floor(lpAmount * lpPool.erg_reserves! / lpPool.lp_circulating)
      const tokenOut = Math.floor(lpAmount * lpPool.token_y.amount / lpPool.lp_circulating)
      const tokenDecimals = lpPool.token_y.decimals ?? 0
      setRedeemErgOutput((ergOut / 1e9).toFixed(4))
      setRedeemTokenOutput(tokenDecimals > 0
        ? (tokenOut / Math.pow(10, tokenDecimals)).toFixed(tokenDecimals)
        : tokenOut.toString())
    } else {
      setRedeemErgOutput('')
      setRedeemTokenOutput('')
    }
  }, [lpPool])

  const handleLpPoolSelect = useCallback((pool: AmmPool) => {
    setLpPool(pool)
    setDepositErgInput('')
    setDepositTokenInput('')
    setDepositLpOutput('')
    setRedeemLpInput('')
    setRedeemErgOutput('')
    setRedeemTokenOutput('')
    setLpTxStep('idle')
    setLpTxError(null)
  }, [])

  const handleLpDeposit = useCallback(async () => {
    if (!lpPool || !walletAddress) return
    const ergNano = Math.floor(parseFloat(depositErgInput || '0') * 1e9)
    const tokenDecimals = lpPool.token_y.decimals ?? 0
    const tokenRaw = Math.floor(parseFloat(depositTokenInput || '0') * Math.pow(10, tokenDecimals))
    if (ergNano <= 0 || tokenRaw <= 0) return

    setLpTxLoading(true)
    setLpTxError(null)
    try {
      const utxos = await invoke<unknown[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      let buildResult: AmmLpBuildResponse
      if (lpSwapMode === 'direct') {
        buildResult = await buildAmmLpDepositTx(
          lpPool.pool_id, ergNano, tokenRaw, walletAddress, utxos as object[], nodeStatus.chain_height
        )
      } else {
        buildResult = await buildAmmLpDepositOrder(
          lpPool.pool_id, ergNano, tokenRaw, walletAddress, utxos as object[], nodeStatus.chain_height
        )
      }

      const tokenName = lpPool.token_y.name || 'Token'
      const signResult = await startSwapSign(
        buildResult.unsignedTx,
        `Add liquidity: ${depositErgInput} ERG + ${depositTokenInput} ${tokenName}`
      )
      lpFlow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setLpTxStep('signing')
    } catch (e) {
      setLpTxError(String(e))
      setLpTxStep('error')
    } finally {
      setLpTxLoading(false)
    }
  }, [lpPool, walletAddress, depositErgInput, depositTokenInput, lpSwapMode, lpFlow])

  const handleLpRedeem = useCallback(async () => {
    if (!lpPool || !walletAddress) return
    const lpRaw = Math.floor(parseFloat(redeemLpInput || '0'))
    if (lpRaw <= 0) return

    setLpTxLoading(true)
    setLpTxError(null)
    try {
      const utxos = await invoke<unknown[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      let buildResult: AmmLpBuildResponse
      if (lpSwapMode === 'direct') {
        buildResult = await buildAmmLpRedeemTx(
          lpPool.pool_id, lpRaw, walletAddress, utxos as object[], nodeStatus.chain_height
        )
      } else {
        buildResult = await buildAmmLpRedeemOrder(
          lpPool.pool_id, lpRaw, walletAddress, utxos as object[], nodeStatus.chain_height
        )
      }

      const signResult = await startSwapSign(
        buildResult.unsignedTx,
        `Remove liquidity: ${redeemLpInput} LP tokens`
      )
      lpFlow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setLpTxStep('signing')
    } catch (e) {
      setLpTxError(String(e))
      setLpTxStep('error')
    } finally {
      setLpTxLoading(false)
    }
  }, [lpPool, walletAddress, redeemLpInput, lpSwapMode, lpFlow])

  // =========================================================================
  // Pool Creation: Preview effect
  // =========================================================================

  useEffect(() => {
    if (!showCreatePool) return
    const xAmt = parseFloat(createXAmount)
    const yAmt = parseFloat(createYAmount)
    const fee = parseFloat(createFeePercent)
    if (!xAmt || !yAmt || isNaN(fee) || fee <= 0 || fee >= 100) {
      setCreatePreview(null)
      return
    }
    if (createPoolType === 'T2T' && !createXTokenId) {
      setCreatePreview(null)
      return
    }
    if (!createYTokenId) {
      setCreatePreview(null)
      return
    }

    const timer = setTimeout(async () => {
      try {
        const xRaw = createPoolType === 'N2T'
          ? Math.round(xAmt * 1e9)  // ERG -> nanoERG
          : Math.round(xAmt)
        const yRaw = Math.round(yAmt)

        const preview = await previewPoolCreate(
          createPoolType,
          createPoolType === 'T2T' ? createXTokenId : undefined,
          xRaw, createYTokenId, yRaw, fee,
        )
        setCreatePreview(preview)
        setCreateError('')
      } catch (e: unknown) {
        setCreateError(String(e))
        setCreatePreview(null)
      }
    }, 500)
    return () => clearTimeout(timer)
  }, [showCreatePool, createPoolType, createXAmount, createYAmount, createXTokenId, createYTokenId, createFeePercent])

  // =========================================================================
  // Pool Creation: Handler (two-step signing flow)
  // =========================================================================

  const handleCreatePool = async () => {
    if (!createPreview || !walletAddress || !walletBalance) return
    setCreateLoading(true)
    setCreateError('')
    setCreateTxStep('signing_bootstrap')

    try {
      const xRaw = createPoolType === 'N2T'
        ? Math.round(parseFloat(createXAmount) * 1e9)
        : Math.round(parseFloat(createXAmount))
      const yRaw = Math.round(parseFloat(createYAmount))
      const fee = parseFloat(createFeePercent)

      // Get UTXOs and height
      const utxosResult = await invoke<object[]>('get_user_utxos')
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')

      // Build TX0 (bootstrap)
      const tx0 = await buildPoolBootstrapTx(
        createPoolType,
        createPoolType === 'T2T' ? createXTokenId : undefined,
        xRaw, createYTokenId, yRaw, fee,
        utxosResult, nodeStatus.chain_height,
      )

      // Sign TX0
      const sign0 = await startSwapSign(tx0.unsignedTx, 'Create pool: mint LP tokens')
      let status0 = await getSwapTxStatus(sign0.request_id)
      while (status0.status === 'pending') {
        await new Promise(r => setTimeout(r, 1500))
        status0 = await getSwapTxStatus(sign0.request_id)
      }
      if (status0.status !== 'success') {
        throw new Error(status0.error || 'Bootstrap signing failed')
      }

      setCreateTxStep('signing_create')

      // Construct bootstrap box for TX1
      // The bootstrap box is TX0's first output, with box_id = LP token ID
      const tx0Output = tx0.unsignedTx as Record<string, unknown>
      const outputs = tx0Output.outputs as Array<Record<string, unknown>>
      const tx0Summary = tx0.summary as Record<string, unknown>
      const bootstrapBox = {
        boxId: tx0Summary.lp_token_id as string,
        transactionId: status0.tx_id,
        index: 0,
        value: outputs[0].value,
        ergoTree: outputs[0].ergoTree,
        assets: outputs[0].assets,
        creationHeight: outputs[0].creationHeight,
        additionalRegisters: outputs[0].additionalRegisters || {},
        extension: {},
      }

      // Build TX1 (pool creation)
      const tx1 = await buildPoolCreateTx(
        bootstrapBox,
        createPoolType,
        createPoolType === 'T2T' ? createXTokenId : undefined,
        xRaw, createYTokenId, yRaw,
        createPreview.fee_num,
        tx0Summary.lp_token_id as string,
        createPreview.lp_share,
        nodeStatus.chain_height,
      )

      // Sign TX1
      const sign1 = await startSwapSign(tx1.unsignedTx, 'Create pool: deploy pool')
      let status1 = await getSwapTxStatus(sign1.request_id)
      while (status1.status === 'pending') {
        await new Promise(r => setTimeout(r, 1500))
        status1 = await getSwapTxStatus(sign1.request_id)
      }
      if (status1.status !== 'success') {
        throw new Error(status1.error || 'Pool creation signing failed')
      }

      setCreateTxStep('done')
    } catch (e: unknown) {
      setCreateError(String(e))
      setCreateTxStep('idle')
    } finally {
      setCreateLoading(false)
    }
  }

  const resetCreatePoolForm = () => {
    setShowCreatePool(false)
    setCreateXAmount('')
    setCreateYAmount('')
    setCreateXTokenId('')
    setCreateYTokenId('')
    setCreateFeePercent('0.3')
    setCreatePreview(null)
    setCreateError('')
    setCreateTxStep('idle')
    setCreateLoading(false)
  }

  if (!isConnected) {
    return (
      <div className="swap-tab">
        <div className="empty-state">
          <p>Connect to a node first</p>
        </div>
      </div>
    )
  }

  return (
    <div className="swap-tab">
      {/* Protocol Header */}
      <div className="swap-header">
        <div className="swap-header-row">
          <div
            className="protocol-app-icon"
            style={{ background: '#8b5cf6', width: 40, height: 40, borderRadius: 12, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '1.25rem', color: 'white', fontWeight: 700 }}
          >
            X
          </div>
          <div>
            <h2>DEX Swap</h2>
            <p className="swap-description">Swap tokens via Spectrum AMM pools</p>
          </div>
        </div>
      </div>

      {/* Info Bar */}
      <div className="protocol-info-bar">
        <div className="info-item">
          <span className="info-label">Protocol:</span>
          <span className="info-value">Spectrum AMM</span>
        </div>
        <div className="info-divider" />
        <div className="info-item">
          <span className="info-label">Pools:</span>
          <span className="info-value">{pools.length}</span>
        </div>
        <div className="info-divider" />
        <div className="info-item">
          <span className="info-label">Types:</span>
          <span className="info-value">N2T, T2T</span>
        </div>
        <div className="info-status">
          <span className="dot" />
          <span className="info-label">Live</span>
        </div>
      </div>

      {/* View Toggle */}
      <div className="slippage-row" style={{ justifyContent: 'center' }}>
        <div className="slippage-options">
          <button
            className={`slippage-btn ${view === 'swap' ? 'active' : ''}`}
            onClick={() => setView('swap')}
          >
            Swap
          </button>
          <button
            className={`slippage-btn ${view === 'liquidity' ? 'active' : ''}`}
            onClick={() => setView('liquidity')}
          >
            Liquidity
          </button>
        </div>
      </div>

      {view === 'swap' ? (<>
      {/* Main Layout */}
      <div className="swap-layout">
        {/* Pool List Panel */}
        <div className="pool-list-panel">
          <div className="pool-search">
            <input
              type="text"
              placeholder="Search pools..."
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              className="pool-search-input"
            />
          </div>
          <div className="pool-list">
            {poolsLoading && pools.length === 0 && (
              <div className="pool-list-empty">
                <div className="spinner-small" />
                <span>Loading pools...</span>
              </div>
            )}
            {poolsError && pools.length === 0 && (
              <div className="pool-list-empty">
                <span className="text-danger">Failed to load pools</span>
              </div>
            )}
            {filteredPools.map(pool => (
              <button
                key={pool.pool_id}
                className={`pool-list-item ${selectedPool?.pool_id === pool.pool_id ? 'selected' : ''}`}
                onClick={() => handlePoolSelect(pool)}
              >
                <div className="pool-item-info">
                  {isUserPool(pool) && <span className="wallet-dot" title="You hold this token" />}
                  <PoolPairIcons pool={pool} />
                  <span className="pool-name">{getPoolDisplayName(pool)}</span>
                  <span className="pool-type-badge">{pool.pool_type}</span>
                </div>
                <div className="pool-item-meta">
                  <span className="pool-fee">{pool.fee_percent}% fee</span>
                </div>
                {sigusdDivergence && pool.pool_id === sigusdDivergence.pool.pool_id && (
                  <span style={{
                    fontSize: 'var(--text-xxs, 10px)',
                    padding: '1px 5px',
                    borderRadius: 4,
                    background: Math.abs(sigusdDivergence.pct) > 10 ? 'rgba(239, 68, 68, 0.2)' : 'rgba(245, 158, 11, 0.2)',
                    color: Math.abs(sigusdDivergence.pct) > 10 ? 'var(--red-400)' : 'var(--amber-400)',
                    whiteSpace: 'nowrap',
                  }}>
                    {sigusdDivergence.pct > 0 ? '+' : ''}{sigusdDivergence.pct.toFixed(1)}%
                  </span>
                )}
              </button>
            ))}
            {filteredPools.length === 0 && pools.length > 0 && (
              <div className="pool-list-empty">
                <span>No pools match your search</span>
              </div>
            )}
          </div>
        </div>

        {/* Swap Form Panel */}
        <div className="swap-form-panel">
          {selectedPool ? (
            <>
              <div className="swap-form-header">
                <div className="swap-form-header-left">
                  <PoolPairIcons pool={selectedPool} />
                  <h3>{getPoolDisplayName(selectedPool)}</h3>
                </div>
                <span className="pool-type-badge">{selectedPool.pool_type}</span>
              </div>

              {/* Pool Reserves */}
              <div className="swap-reserves">
                <div className="reserve-item">
                  <span className="reserve-label">
                    <TokenIcon name={getInputLabel(selectedPool, 'x')} size={14} />
                    {getInputLabel(selectedPool, 'x')} Reserves
                  </span>
                  <span className="reserve-value">
                    {selectedPool.pool_type === 'N2T'
                      ? formatErg(selectedPool.erg_reserves ?? 0)
                      : formatTokenAmount(selectedPool.token_x?.amount ?? 0, selectedPool.token_x?.decimals ?? 0)}
                  </span>
                </div>
                <div className="reserve-item">
                  <span className="reserve-label">
                    <TokenIcon name={getInputLabel(selectedPool, 'y')} size={14} />
                    {getInputLabel(selectedPool, 'y')} Reserves
                  </span>
                  <span className="reserve-value">
                    {formatTokenAmount(selectedPool.token_y.amount, selectedPool.token_y.decimals ?? 0)}
                  </span>
                </div>
              </div>

              {sigusdDivergence && selectedPool?.pool_id === sigusdDivergence.pool.pool_id && (
                <div style={{
                  padding: '6px 10px',
                  marginBottom: 'var(--space-sm)',
                  borderRadius: 6,
                  fontSize: 'var(--text-xs)',
                  background: Math.abs(sigusdDivergence.pct) > 10 ? 'rgba(239, 68, 68, 0.12)' : 'rgba(245, 158, 11, 0.12)',
                  color: Math.abs(sigusdDivergence.pct) > 10 ? 'var(--red-400)' : 'var(--amber-400)',
                  display: 'flex',
                  justifyContent: 'space-between',
                  gap: 8,
                }}>
                  <span>Oracle: ${sigusdDivergence.oraclePrice.toFixed(2)} | DEX: ${sigusdDivergence.dexPrice.toFixed(2)} ({sigusdDivergence.pct > 0 ? '+' : ''}{sigusdDivergence.pct.toFixed(1)}%)</span>
                  <span style={{ fontWeight: 500 }}>
                    {canMintSigusd ? 'Arb available' : `Arb blocked (RR ${Math.round(reserveRatioPct ?? 0)}%)`}
                  </span>
                </div>
              )}

              {/* Input Field */}
              <div className="swap-input-section">
                <div className="swap-field">
                  <div className="swap-field-header">
                    <span className="swap-field-label">You Pay</span>
                    <span className="swap-field-balance">
                      {walletBalance && (
                        <>
                          Balance: {getInputType(selectedPool, inputSide) === 'erg'
                            ? formatErg(walletBalance.erg_nano)
                            : (() => {
                              const tokenId = getInputTokenId(selectedPool, inputSide)
                              const token = walletBalance.tokens.find(t => t.token_id === tokenId)
                              return token ? formatTokenAmount(token.amount, token.decimals) : '0'
                            })()
                          }
                        </>
                      )}
                    </span>
                  </div>
                  <div className="swap-field-input">
                    <input
                      type="number"
                      value={inputAmount}
                      onChange={e => setInputAmount(e.target.value)}
                      placeholder="0.00"
                      min="0"
                    />
                    <button
                      className="max-btn"
                      onClick={handleMax}
                      disabled={!walletBalance}
                    >
                      MAX
                    </button>
                    <span className="swap-field-token">
                      <TokenIcon name={getInputLabel(selectedPool, inputSide)} size={16} />
                      {getInputLabel(selectedPool, inputSide)}
                    </span>
                  </div>
                </div>
              </div>

              {/* Flip Button */}
              <div className="swap-flip-row">
                <button className="flip-btn" onClick={handleFlip} title="Swap direction">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M7 16V4m0 0L3 8m4-4l4 4M17 8v12m0 0l4-4m-4 4l-4-4" />
                  </svg>
                </button>
              </div>

              {/* Output Display */}
              <div className="output-display">
                <div className="output-header">
                  <span className="swap-field-label">You Receive</span>
                  <span className="swap-field-token">
                    <TokenIcon name={getOutputLabel(selectedPool, inputSide)} size={16} />
                    {getOutputLabel(selectedPool, inputSide)}
                  </span>
                </div>
                <div className="output-amount">
                  {quoteLoading ? (
                    <span className="quote-loading">Calculating...</span>
                  ) : quote ? (
                    formatTokenAmount(quote.output.amount, quote.output.decimals ?? 0)
                  ) : (
                    <span className="quote-placeholder">---</span>
                  )}
                </div>
              </div>

              {/* Quote Error */}
              {quoteError && (
                <div className="message error" style={{ marginTop: 8, padding: '8px 12px', fontSize: '0.8rem' }}>
                  {quoteError}
                </div>
              )}

              {/* Quote Details */}
              {quote && (
                <div className="quote-details">
                  <div className="quote-row">
                    <span>Price Impact</span>
                    <span className={quote.price_impact > 3 ? 'text-danger' : quote.price_impact > 1 ? 'text-warning' : ''}>
                      {quote.price_impact.toFixed(2)}%
                    </span>
                  </div>
                  <div className="quote-row">
                    <span>Pool Fee</span>
                    <span>{quote.fee_amount.toLocaleString()} ({selectedPool.fee_percent}%)</span>
                  </div>
                  <div className="quote-row">
                    <span>Rate</span>
                    <span>1 {getInputLabel(selectedPool, inputSide)} = {quote.effective_rate.toFixed(6)} {getOutputLabel(selectedPool, inputSide)}</span>
                  </div>
                  {swapMode === 'proxy' && (
                    <div className="quote-row">
                      <span>Min. Output ({slippage}% slippage)</span>
                      <span>{formatTokenAmount(Math.floor(quote.output.amount * (1 - slippage / 100)), quote.output.decimals ?? 0)}</span>
                    </div>
                  )}
                </div>
              )}

              {/* Swap Mode Toggle */}
              <div className="slippage-row">
                <span className="slippage-label">Swap Mode</span>
                <div className="slippage-options">
                  <button
                    className={`slippage-btn ${swapMode === 'proxy' ? 'active' : ''}`}
                    onClick={() => setSwapMode('proxy')}
                    title="Creates a proxy box that batcher bots execute against the pool. Supports slippage protection and refunds."
                  >
                    Proxy
                  </button>
                  <button
                    className={`slippage-btn ${swapMode === 'direct' ? 'active' : ''}`}
                    onClick={() => setSwapMode('direct')}
                    title="Swaps directly with the pool in a single transaction. Faster and cheaper, but may fail if the pool state changes."
                  >
                    Direct
                  </button>
                </div>
              </div>

              {/* Slippage Selector - only for proxy mode */}
              {swapMode === 'proxy' && (
                <div className="slippage-row">
                  <span className="slippage-label">Slippage Tolerance</span>
                  <div className="slippage-options">
                    {[0.1, 0.5, 1, 3].map(s => (
                      <button
                        key={s}
                        className={`slippage-btn ${slippage === s ? 'active' : ''}`}
                        onClick={() => setSlippage(s)}
                      >
                        {s}%
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {/* Nitro (Execution Fee) Selector - only for proxy mode */}
              {swapMode === 'proxy' && (
                <div className="slippage-row">
                  <span className="slippage-label" title="Higher nitro = higher execution fee = bots prioritize your swap">
                    Execution Fee (Nitro)
                  </span>
                  <div className="slippage-options">
                    {[1, 1.2, 1.5, 2].map(n => (
                      <button
                        key={n}
                        className={`slippage-btn ${nitro === n ? 'active' : ''}`}
                        onClick={() => setNitro(n)}
                      >
                        {n === 1 ? 'Min' : `${n}x`}
                      </button>
                    ))}
                  </div>
                  <span className="nitro-fee-display">
                    {formatErg(Math.round(2_000_000 * nitro))} ERG
                  </span>
                </div>
              )}

              {/* Swap Button */}
              <button
                className="btn btn-primary swap-confirm-btn"
                disabled={!canSwap}
                onClick={handleSwapClick}
              >
                {!walletAddress
                  ? 'Connect Wallet'
                  : !selectedPool
                    ? 'Select a Pool'
                    : quoteLoading
                      ? 'Getting Quote...'
                      : quoteError
                        ? 'Quote Error'
                        : !quote
                          ? 'Enter an Amount'
                          : swapMode === 'direct'
                            ? 'Direct Swap'
                            : 'Swap'}
              </button>
            </>
          ) : (
            <div className="swap-form-empty">
              <p>Select a pool to start swapping</p>
            </div>
          )}
        </div>
      </div>

      {/* Order History */}
      <OrderHistory walletAddress={walletAddress} explorerUrl={explorerUrl} />

      {/* Swap Modal */}
      {showSwapModal && selectedPool && quote && walletAddress && (
        <SwapModal
          isOpen={showSwapModal}
          onClose={() => setShowSwapModal(false)}
          pool={selectedPool}
          quote={quote}
          inputAmount={inputAmount}
          inputSide={inputSide}
          slippage={slippage}
          nitro={nitro}
          swapMode={swapMode}
          walletAddress={walletAddress}
          explorerUrl={explorerUrl}
          onSuccess={() => { setShowSwapModal(false); fetchPools() }}
        />
      )}
      </>) : (
      /* ================================================================= */
      /* Liquidity UI                                                      */
      /* ================================================================= */
      <>
      {showCreatePool ? (
        <div className="swap-form-panel" style={{ maxWidth: 520, margin: '0 auto' }}>
          <div className="swap-form-header">
            <h3>Create New Pool</h3>
            <button className="btn btn-secondary" style={{ fontSize: 'var(--text-xs)' }} onClick={resetCreatePoolForm}>
              Back to Liquidity
            </button>
          </div>

          {/* Pool Type Toggle */}
          <div className="slippage-row">
            <span className="slippage-label">Pool Type</span>
            <div className="slippage-options">
              <button
                className={`slippage-btn ${createPoolType === 'N2T' ? 'active' : ''}`}
                onClick={() => setCreatePoolType('N2T')}
              >
                ERG / Token
              </button>
              <button
                className={`slippage-btn ${createPoolType === 'T2T' ? 'active' : ''}`}
                onClick={() => setCreatePoolType('T2T')}
              >
                Token / Token
              </button>
            </div>
          </div>

          {/* Token X */}
          <div className="swap-input-section">
            <div className="swap-field">
              <div className="swap-field-header">
                <span className="swap-field-label">{createPoolType === 'N2T' ? 'ERG Amount' : 'Token X'}</span>
                {createPoolType === 'N2T' && walletBalance && (
                  <span className="swap-field-balance">Balance: {formatErg(walletBalance.erg_nano)}</span>
                )}
              </div>
              {createPoolType === 'T2T' && (
                <select
                  value={createXTokenId}
                  onChange={e => setCreateXTokenId(e.target.value)}
                  className="pool-search-input"
                  style={{ marginBottom: 'var(--space-xs)' }}
                >
                  <option value="">Select token X...</option>
                  {walletBalance?.tokens.map(t => (
                    <option key={t.token_id} value={t.token_id}>
                      {t.name || t.token_id.slice(0, 8)} ({t.amount})
                    </option>
                  ))}
                </select>
              )}
              <div className="swap-field-input">
                <input
                  type="number"
                  value={createXAmount}
                  onChange={e => setCreateXAmount(e.target.value)}
                  placeholder={createPoolType === 'N2T' ? '0.0' : '0'}
                  min="0"
                />
                <span className="swap-field-token">
                  {createPoolType === 'N2T' ? (
                    <><TokenIcon name="ERG" size={16} /> ERG</>
                  ) : (
                    (() => {
                      const t = walletBalance?.tokens.find(tok => tok.token_id === createXTokenId)
                      const name = t?.name || (createXTokenId ? createXTokenId.slice(0, 8) : 'Token X')
                      return <><TokenIcon name={name} size={16} /> {name}</>
                    })()
                  )}
                </span>
              </div>
            </div>
          </div>

          {/* Token Y */}
          <div className="swap-input-section">
            <div className="swap-field">
              <div className="swap-field-header">
                <span className="swap-field-label">Token Y</span>
              </div>
              <select
                value={createYTokenId}
                onChange={e => setCreateYTokenId(e.target.value)}
                className="pool-search-input"
                style={{ marginBottom: 'var(--space-xs)' }}
              >
                <option value="">Select token Y...</option>
                {walletBalance?.tokens.map(t => (
                  <option key={t.token_id} value={t.token_id}>
                    {t.name || t.token_id.slice(0, 8)} ({t.amount})
                  </option>
                ))}
              </select>
              <div className="swap-field-input">
                <input
                  type="number"
                  value={createYAmount}
                  onChange={e => setCreateYAmount(e.target.value)}
                  placeholder="0"
                  min="0"
                />
                <span className="swap-field-token">
                  {(() => {
                    const t = walletBalance?.tokens.find(tok => tok.token_id === createYTokenId)
                    const name = t?.name || (createYTokenId ? createYTokenId.slice(0, 8) : 'Token Y')
                    return <><TokenIcon name={name} size={16} /> {name}</>
                  })()}
                </span>
              </div>
            </div>
          </div>

          {/* Fee Selector */}
          <div className="slippage-row">
            <span className="slippage-label">Fee</span>
            <div className="slippage-options">
              {['0.3', '1', '2', '3'].map(f => (
                <button
                  key={f}
                  className={`slippage-btn ${createFeePercent === f ? 'active' : ''}`}
                  onClick={() => setCreateFeePercent(f)}
                >
                  {f}%
                </button>
              ))}
            </div>
            <input
              type="number"
              value={createFeePercent}
              onChange={e => setCreateFeePercent(e.target.value)}
              style={{ width: 60, marginLeft: 8 }}
              className="pool-search-input"
              min="0.01"
              max="99"
              step="0.1"
            />
            <span style={{ color: 'var(--slate-400)', marginLeft: 4 }}>%</span>
          </div>

          {/* Preview */}
          {createPreview && (
            <div className="swap-reserves">
              <div className="reserve-item">
                <span className="reserve-label">LP Tokens</span>
                <span className="reserve-value">{createPreview.lp_share.toLocaleString()}</span>
              </div>
              <div className="reserve-item">
                <span className="reserve-label">Pool Share</span>
                <span className="reserve-value">100%</span>
              </div>
              <div className="reserve-item">
                <span className="reserve-label">Total ERG Cost</span>
                <span className="reserve-value">{(createPreview.total_erg_cost_nano / 1e9).toFixed(4)} ERG</span>
              </div>
            </div>
          )}

          {/* Error display */}
          {createError && (
            <div className="message error" style={{ marginTop: 8, padding: '8px 12px', fontSize: '0.8rem' }}>
              {createError}
            </div>
          )}

          {/* Progress during signing */}
          {createTxStep === 'signing_bootstrap' && (
            <p style={{ color: 'var(--slate-400)', textAlign: 'center', marginTop: 'var(--space-sm)' }}>
              Step 1/2: Signing bootstrap transaction...
            </p>
          )}
          {createTxStep === 'signing_create' && (
            <p style={{ color: 'var(--slate-400)', textAlign: 'center', marginTop: 'var(--space-sm)' }}>
              Step 2/2: Signing pool creation...
            </p>
          )}
          {createTxStep === 'done' && (
            <p style={{ color: 'var(--emerald-400)', textAlign: 'center', marginTop: 'var(--space-sm)', fontWeight: 600 }}>
              Pool created successfully!
            </p>
          )}

          {/* Create button */}
          <button
            className="btn btn-primary swap-confirm-btn"
            disabled={!walletAddress || createLoading || !createPreview || createTxStep !== 'idle'}
            onClick={handleCreatePool}
          >
            {createLoading
              ? (createTxStep === 'signing_bootstrap' ? 'Step 1/2: Bootstrap...' : 'Step 2/2: Creating...')
              : createTxStep === 'done'
                ? 'Done'
                : 'Create Pool'}
          </button>
        </div>
      ) : (<>
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-sm)' }}>
          <button className="btn btn-secondary" onClick={() => setShowCreatePool(true)}>
            + Create Pool
          </button>
        </div>
        <div className="swap-layout">
        {/* Pool List Panel (N2T only) */}
        <div className="pool-list-panel">
          <div className="pool-search">
            <input
              type="text"
              placeholder="Search pools..."
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              className="pool-search-input"
            />
          </div>
          <div className="pool-list">
            {poolsLoading && pools.length === 0 && (
              <div className="pool-list-empty">
                <div className="spinner-small" />
                <span>Loading pools...</span>
              </div>
            )}
            {filteredPools.filter(p => p.pool_type === 'N2T').map(pool => (
              <button
                key={pool.pool_id}
                className={`pool-list-item ${lpPool?.pool_id === pool.pool_id ? 'selected' : ''}`}
                onClick={() => handleLpPoolSelect(pool)}
              >
                <div className="pool-item-info">
                  {getUserLpBalance(pool) > 0 && <span className="wallet-dot" title="You hold LP tokens" />}
                  <PoolPairIcons pool={pool} />
                  <span className="pool-name">{getPoolDisplayName(pool)}</span>
                </div>
                <div className="pool-item-meta">
                  {getUserLpBalance(pool) > 0 && (
                    <span className="pool-fee">LP: {getUserLpBalance(pool).toLocaleString()}</span>
                  )}
                </div>
              </button>
            ))}
            {filteredPools.filter(p => p.pool_type === 'N2T').length === 0 && pools.length > 0 && (
              <div className="pool-list-empty">
                <span>No N2T pools match your search</span>
              </div>
            )}
          </div>
        </div>

        {/* LP Form Panel */}
        <div className="swap-form-panel">
          {lpPool ? (
            <>
              <div className="swap-form-header">
                <div className="swap-form-header-left">
                  <PoolPairIcons pool={lpPool} />
                  <h3>{getPoolDisplayName(lpPool)}</h3>
                </div>
                <span className="pool-type-badge">N2T</span>
              </div>

              {/* Pool Reserves */}
              <div className="swap-reserves">
                <div className="reserve-item">
                  <span className="reserve-label"><TokenIcon name="ERG" size={14} /> ERG Reserves</span>
                  <span className="reserve-value">{formatErg(lpPool.erg_reserves ?? 0)}</span>
                </div>
                <div className="reserve-item">
                  <span className="reserve-label"><TokenIcon name={lpPool.token_y.name || 'Token'} size={14} /> {lpPool.token_y.name || 'Token'} Reserves</span>
                  <span className="reserve-value">{formatTokenAmount(lpPool.token_y.amount, lpPool.token_y.decimals ?? 0)}</span>
                </div>
                <div className="reserve-item">
                  <span className="reserve-label">LP Circulating</span>
                  <span className="reserve-value">{lpPool.lp_circulating.toLocaleString()}</span>
                </div>
                {getUserLpBalance(lpPool) > 0 && (
                  <div className="reserve-item">
                    <span className="reserve-label">Your LP Balance</span>
                    <span className="reserve-value">{getUserLpBalance(lpPool).toLocaleString()}</span>
                  </div>
                )}
              </div>

              {/* LP signing flow */}
              {lpTxStep === 'signing' && (
                <div className="swap-input-section">
                  <div style={{ textAlign: 'center', padding: 'var(--space-md)' }}>
                    {lpFlow.signMethod === 'choose' && (
                      <div>
                        <p style={{ color: 'var(--slate-400)', marginBottom: 'var(--space-sm)' }}>Choose signing method:</p>
                        <div style={{ display: 'flex', gap: 'var(--space-sm)', justifyContent: 'center' }}>
                          <button className="btn btn-primary" onClick={lpFlow.handleNautilusSign}>Nautilus</button>
                          <button className="btn btn-secondary" onClick={lpFlow.handleMobileSign}>Mobile (QR)</button>
                        </div>
                      </div>
                    )}
                    {lpFlow.signMethod === 'nautilus' && (
                      <div>
                        <div className="spinner-small" style={{ margin: '0 auto var(--space-sm)' }} />
                        <p style={{ color: 'var(--slate-400)' }}>Waiting for Nautilus...</p>
                        <button className="btn btn-secondary" onClick={lpFlow.handleBackToChoice} style={{ marginTop: 'var(--space-sm)' }}>Back</button>
                      </div>
                    )}
                    {lpFlow.signMethod === 'mobile' && lpFlow.qrUrl && (
                      <div>
                        <p style={{ color: 'var(--slate-400)', marginBottom: 'var(--space-sm)' }}>Scan QR code with Ergo Mobile Wallet:</p>
                        <div style={{ background: 'white', display: 'inline-block', padding: 8, borderRadius: 8 }}>
                          <QRCodeSVG value={lpFlow.qrUrl} size={200} level="M" includeMargin bgColor="white" fgColor="black" />
                        </div>
                        <button className="btn btn-secondary" onClick={lpFlow.handleBackToChoice} style={{ marginTop: 'var(--space-sm)', display: 'block', margin: 'var(--space-sm) auto 0' }}>Back</button>
                      </div>
                    )}
                  </div>
                </div>
              )}

              {lpTxStep === 'success' && (
                <div className="swap-input-section" style={{ textAlign: 'center', padding: 'var(--space-md)' }}>
                  <p style={{ color: 'var(--emerald-400)', fontWeight: 600, fontSize: 'var(--text-lg)' }}>Transaction Submitted!</p>
                  {lpFlow.txId && (
                    <a href={`${explorerUrl}/en/transactions/${lpFlow.txId}`} target="_blank" rel="noopener noreferrer" style={{ color: 'var(--slate-400)', fontSize: 'var(--text-xs)' }}>
                      View on Explorer
                    </a>
                  )}
                  <button className="btn btn-primary" onClick={() => { setLpTxStep('idle'); fetchPools() }} style={{ marginTop: 'var(--space-sm)' }}>Done</button>
                </div>
              )}

              {lpTxStep === 'error' && (
                <div className="swap-input-section" style={{ textAlign: 'center', padding: 'var(--space-md)' }}>
                  <p style={{ color: 'var(--red-400)', fontWeight: 600 }}>Transaction Failed</p>
                  <p style={{ color: 'var(--slate-500)', fontSize: 'var(--text-xs)', marginTop: 4 }}>{lpTxError}</p>
                  <button className="btn btn-primary" onClick={() => setLpTxStep('idle')} style={{ marginTop: 'var(--space-sm)' }}>Try Again</button>
                </div>
              )}

              {lpTxStep === 'idle' && (<>
                {/* Deposit/Redeem Toggle */}
                <div className="slippage-row">
                  <span className="slippage-label">Operation</span>
                  <div className="slippage-options">
                    <button className={`slippage-btn ${lpMode === 'deposit' ? 'active' : ''}`} onClick={() => setLpMode('deposit')}>Deposit</button>
                    <button className={`slippage-btn ${lpMode === 'redeem' ? 'active' : ''}`} onClick={() => setLpMode('redeem')}>Redeem</button>
                  </div>
                </div>

                {lpMode === 'deposit' ? (
                  <>
                    {/* Deposit Form */}
                    <div className="swap-input-section">
                      <div className="swap-field">
                        <div className="swap-field-header">
                          <span className="swap-field-label">ERG Amount</span>
                          <span className="swap-field-balance">
                            {walletBalance && <>Balance: {formatErg(walletBalance.erg_nano)}</>}
                          </span>
                        </div>
                        <div className="swap-field-input">
                          <input type="number" value={depositErgInput} onChange={e => handleDepositErgChange(e.target.value)} placeholder="0.0" min="0" step="0.1" />
                          <button className="max-btn" onClick={() => {
                            if (!walletBalance) return
                            const available = Math.max(0, walletBalance.erg_nano - 10_000_000)
                            handleDepositErgChange((available / 1e9).toFixed(4))
                          }} disabled={!walletBalance}>MAX</button>
                          <span className="swap-field-token"><TokenIcon name="ERG" size={16} /> ERG</span>
                        </div>
                      </div>
                    </div>

                    <div className="swap-input-section">
                      <div className="swap-field">
                        <div className="swap-field-header">
                          <span className="swap-field-label">{lpPool.token_y.name || 'Token'} Amount</span>
                          <span className="swap-field-balance">
                            {walletBalance && (() => {
                              const token = walletBalance.tokens.find(t => t.token_id === lpPool.token_y.token_id)
                              return token ? <>Balance: {formatTokenAmount(token.amount, token.decimals)}</> : null
                            })()}
                          </span>
                        </div>
                        <div className="swap-field-input">
                          <input type="number" value={depositTokenInput} onChange={e => handleDepositTokenChange(e.target.value)} placeholder="0.0" min="0" step={lpPool.token_y.decimals === 0 ? '1' : '0.001'} />
                          <button className="max-btn" onClick={() => {
                            if (!walletBalance) return
                            const token = walletBalance.tokens.find(t => t.token_id === lpPool.token_y.token_id)
                            if (token) {
                              const tokenDecimals = lpPool.token_y.decimals ?? 0
                              handleDepositTokenChange(tokenDecimals > 0 ? (token.amount / Math.pow(10, tokenDecimals)).toFixed(tokenDecimals) : token.amount.toString())
                            }
                          }} disabled={!walletBalance}>MAX</button>
                          <span className="swap-field-token"><TokenIcon name={lpPool.token_y.name || 'Token'} size={16} /> {lpPool.token_y.name || 'Token'}</span>
                        </div>
                      </div>
                    </div>

                    {/* LP Output (read-only) */}
                    <div className="output-display">
                      <div className="output-header">
                        <span className="swap-field-label">LP Tokens to Receive</span>
                      </div>
                      <div className="output-amount">
                        {depositLpOutput || <span className="quote-placeholder">---</span>}
                      </div>
                    </div>
                  </>
                ) : (
                  <>
                    {/* Redeem Form */}
                    <div className="swap-input-section">
                      <div className="swap-field">
                        <div className="swap-field-header">
                          <span className="swap-field-label">LP Tokens to Redeem</span>
                          <span className="swap-field-balance">
                            {getUserLpBalance(lpPool) > 0 && <>Balance: {getUserLpBalance(lpPool).toLocaleString()}</>}
                          </span>
                        </div>
                        <div className="swap-field-input">
                          <input type="number" value={redeemLpInput} onChange={e => handleRedeemLpChange(e.target.value)} placeholder="0" min="0" step="1" />
                          <button className="max-btn" onClick={() => {
                            const balance = getUserLpBalance(lpPool)
                            if (balance > 0) handleRedeemLpChange(String(balance))
                          }} disabled={getUserLpBalance(lpPool) === 0}>MAX</button>
                          <span className="swap-field-token">LP</span>
                        </div>
                      </div>
                    </div>

                    {/* ERG Output (read-only) */}
                    <div className="output-display">
                      <div className="output-header">
                        <span className="swap-field-label">ERG to Receive</span>
                        <span className="swap-field-token"><TokenIcon name="ERG" size={16} /> ERG</span>
                      </div>
                      <div className="output-amount">
                        {redeemErgOutput ? `${redeemErgOutput} ERG` : <span className="quote-placeholder">---</span>}
                      </div>
                    </div>

                    <div className="output-display">
                      <div className="output-header">
                        <span className="swap-field-label">{lpPool.token_y.name || 'Token'} to Receive</span>
                        <span className="swap-field-token"><TokenIcon name={lpPool.token_y.name || 'Token'} size={16} /> {lpPool.token_y.name || 'Token'}</span>
                      </div>
                      <div className="output-amount">
                        {redeemTokenOutput ? `${redeemTokenOutput} ${lpPool.token_y.name || 'Token'}` : <span className="quote-placeholder">---</span>}
                      </div>
                    </div>
                  </>
                )}

                {/* Direct vs Proxy Toggle */}
                <div className="slippage-row">
                  <span className="slippage-label">Execution Mode</span>
                  <div className="slippage-options">
                    <button
                      className={`slippage-btn ${lpSwapMode === 'proxy' ? 'active' : ''}`}
                      onClick={() => setLpSwapMode('proxy')}
                      title="Creates a proxy box for bot execution. More reliable under contention."
                    >Proxy</button>
                    <button
                      className={`slippage-btn ${lpSwapMode === 'direct' ? 'active' : ''}`}
                      onClick={() => setLpSwapMode('direct')}
                      title="Spends pool box directly. No bot fee, but may fail if pool state changes."
                    >Direct</button>
                  </div>
                </div>

                {/* Action Button */}
                <button
                  className="btn btn-primary swap-confirm-btn"
                  disabled={
                    !walletAddress || lpTxLoading ||
                    (lpMode === 'deposit' ? (!depositErgInput || !depositTokenInput || parseFloat(depositErgInput) <= 0 || parseFloat(depositTokenInput) <= 0) : (!redeemLpInput || parseFloat(redeemLpInput) <= 0))
                  }
                  onClick={lpMode === 'deposit' ? handleLpDeposit : handleLpRedeem}
                >
                  {!walletAddress
                    ? 'Connect Wallet'
                    : lpTxLoading
                      ? 'Building...'
                      : lpMode === 'deposit'
                        ? (lpSwapMode === 'direct' ? 'Direct Deposit' : 'Deposit Liquidity')
                        : (lpSwapMode === 'direct' ? 'Direct Redeem' : 'Redeem Liquidity')}
                </button>
              </>)}
            </>
          ) : (
            <div className="swap-form-empty">
              <p>Select a pool to manage liquidity</p>
            </div>
          )}
        </div>
      </div>
      </>)}
      </>
      )}
    </div>
  )
}
