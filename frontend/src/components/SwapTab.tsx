import { useState, useEffect, useCallback, useRef } from 'react'
import {
  getAmmPools, getAmmQuote, getPoolDisplayName, formatTokenAmount, formatErg,
  type AmmPool, type SwapQuote,
} from '../api/amm'
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

export function SwapTab({ isConnected, walletAddress, walletBalance, explorerUrl }: SwapTabProps) {
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
    </div>
  )
}
