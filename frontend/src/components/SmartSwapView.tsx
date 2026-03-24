import { useState, useEffect, useRef, useMemo, useCallback } from 'react'
import { type AmmPool } from '../api/amm'
import {
  findSwapRoutes,
  type RoutesResponse,
  type RouteQuote,
  type SplitRouteDetail,
} from '../api/router'
import { TokenSelector, type TokenEntry } from './TokenSelector'
import { RouteList } from './RouteList'
import { formatTokenAmount } from '../utils/format'
import './SmartSwap.css'

// =============================================================================
// Constants
// =============================================================================

const ERG_TOKEN_ID = 'ERG'
const ERG_DECIMALS = 9
const ERG_RESERVE_NANO = 10_000_000 // 0.01 ERG

// =============================================================================
// Props
// =============================================================================

interface WalletToken {
  token_id: string
  amount: number
  name: string | null
  decimals: number
}

interface WalletBalance {
  erg_nano: number
  tokens: WalletToken[]
}

export interface SmartSwapViewProps {
  isConnected: boolean
  walletAddress: string | null
  walletBalance: WalletBalance | null
  explorerUrl: string
  pools: AmmPool[]
}

// =============================================================================
// Token list building
// =============================================================================

/**
 * Build the list of tokens for source or target mode.
 *
 * Source mode: ERG + wallet tokens with balance > 0, ERG first then alpha.
 * Target mode: ERG + all unique tokens from pool graph.
 *   Wallet-held tokens pinned to top (sorted by name), non-held below (sorted by name).
 */
function buildTokenList(
  pools: AmmPool[],
  walletBalance: WalletBalance | null,
  mode: 'source' | 'target',
): TokenEntry[] {
  // Build a map of token metadata from pool data (preferred over wallet data)
  const poolTokenMeta = new Map<string, { name: string | null; decimals: number }>()
  for (const pool of pools) {
    if (pool.token_y) {
      poolTokenMeta.set(pool.token_y.token_id, {
        name: pool.token_y.name ?? null,
        decimals: pool.token_y.decimals ?? 0,
      })
    }
    if (pool.token_x) {
      poolTokenMeta.set(pool.token_x.token_id, {
        name: pool.token_x.name ?? null,
        decimals: pool.token_x.decimals ?? 0,
      })
    }
  }

  const ergEntry: TokenEntry = {
    token_id: ERG_TOKEN_ID,
    name: 'ERG',
    decimals: ERG_DECIMALS,
    balance: walletBalance?.erg_nano,
  }

  if (mode === 'source') {
    const walletTokens: TokenEntry[] = (walletBalance?.tokens ?? [])
      .filter((t) => t.amount > 0)
      .map((t) => {
        const meta = poolTokenMeta.get(t.token_id)
        return {
          token_id: t.token_id,
          name: meta?.name ?? t.name ?? null,
          decimals: meta?.decimals ?? t.decimals,
          balance: t.amount,
        }
      })
      .sort((a, b) => {
        const na = (a.name ?? a.token_id).toLowerCase()
        const nb = (b.name ?? b.token_id).toLowerCase()
        return na.localeCompare(nb)
      })

    return [ergEntry, ...walletTokens]
  }

  // Target mode: collect all unique tokens from pool graph
  const allPoolTokenIds = new Set<string>()
  for (const pool of pools) {
    allPoolTokenIds.add(pool.token_y.token_id)
    if (pool.token_x) allPoolTokenIds.add(pool.token_x.token_id)
  }

  // Build a map of wallet-held tokens by id for quick lookup
  const walletHeld = new Map<string, WalletToken>()
  for (const t of walletBalance?.tokens ?? []) {
    walletHeld.set(t.token_id, t)
  }

  const heldTokens: TokenEntry[] = []
  const nonHeldTokens: TokenEntry[] = []

  for (const tokenId of allPoolTokenIds) {
    const meta = poolTokenMeta.get(tokenId)
    const walletToken = walletHeld.get(tokenId)

    const entry: TokenEntry = {
      token_id: tokenId,
      name: meta?.name ?? walletToken?.name ?? null,
      decimals: meta?.decimals ?? walletToken?.decimals ?? 0,
      balance: walletToken?.amount,
    }

    if (walletToken !== undefined) {
      heldTokens.push(entry)
    } else {
      nonHeldTokens.push(entry)
    }
  }

  const sortByName = (a: TokenEntry, b: TokenEntry) => {
    const na = (a.name ?? a.token_id).toLowerCase()
    const nb = (b.name ?? b.token_id).toLowerCase()
    return na.localeCompare(nb)
  }

  heldTokens.sort(sortByName)
  nonHeldTokens.sort(sortByName)

  return [ergEntry, ...heldTokens, ...nonHeldTokens]
}

// =============================================================================
// SmartSwapView
// =============================================================================

export function SmartSwapView({
  isConnected,
  walletAddress,
  walletBalance,
  explorerUrl: _explorerUrl,
  pools,
}: SmartSwapViewProps) {
  // Token selections
  const [sourceToken, setSourceToken] = useState<TokenEntry | null>(null)
  const [targetToken, setTargetToken] = useState<TokenEntry | null>(null)

  // Input
  const [inputAmount, setInputAmount] = useState<string>('')

  // Slippage
  const [slippage, setSlippage] = useState<number>(0.5)
  const [showSlippage, setShowSlippage] = useState<boolean>(false)
  const [customSlippage, setCustomSlippage] = useState<string>('')

  // Route state
  const [routes, setRoutes] = useState<RouteQuote[]>([])
  const [split, setSplit] = useState<SplitRouteDetail | null>(null)
  const [selectedRouteIndex, setSelectedRouteIndex] = useState<number>(0)
  const [useSplit, setUseSplit] = useState<boolean>(false)
  const [routeLoading, setRouteLoading] = useState<boolean>(false)
  const [routeError, setRouteError] = useState<string | null>(null)
  const [routeStale, setRouteStale] = useState<boolean>(false)

  // Swap modal
  const [showSwapModal, setShowSwapModal] = useState<boolean>(false)

  // Debounce ref for amount changes
  const amountDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // =============================================================================
  // Token lists
  // =============================================================================

  const sourceTokenList = useMemo(
    () => buildTokenList(pools, walletBalance, 'source'),
    [pools, walletBalance],
  )

  const targetTokenList = useMemo(
    () => buildTokenList(pools, walletBalance, 'target'),
    [pools, walletBalance],
  )

  // =============================================================================
  // Computed: raw input
  // =============================================================================

  const rawInput = useMemo<number>(() => {
    if (!sourceToken || !inputAmount.trim()) return 0
    const parsed = parseFloat(inputAmount)
    if (isNaN(parsed) || parsed <= 0) return 0
    return Math.floor(parsed * Math.pow(10, sourceToken.decimals))
  }, [inputAmount, sourceToken])

  // =============================================================================
  // Computed: balance
  // =============================================================================

  const sourceBalance: number | undefined = useMemo(() => {
    if (!sourceToken) return undefined
    if (sourceToken.token_id === ERG_TOKEN_ID) return walletBalance?.erg_nano
    return walletBalance?.tokens.find((t) => t.token_id === sourceToken.token_id)?.amount
  }, [sourceToken, walletBalance])

  const insufficientBalance = useMemo<boolean>(() => {
    if (!isConnected || sourceBalance === undefined) return false
    return rawInput > 0 && rawInput > sourceBalance
  }, [isConnected, rawInput, sourceBalance])

  // =============================================================================
  // Route finding
  // =============================================================================

  const findRoutes = useCallback(async () => {
    if (!sourceToken || !targetToken || rawInput <= 0) return

    if (routes.length > 0) setRouteStale(true)
    setRouteLoading(true)
    setRouteError(null)

    try {
      const result: RoutesResponse = await findSwapRoutes(
        sourceToken.token_id,
        targetToken.token_id,
        rawInput,
        4,
        5,
        slippage,
      )
      setRoutes(result.routes)
      setSplit(result.split)
      setSelectedRouteIndex(0)
      setUseSplit(false)
      setRouteStale(false)
    } catch (err) {
      setRouteError(err instanceof Error ? err.message : String(err))
      setRoutes([])
      setSplit(null)
      setRouteStale(false)
    } finally {
      setRouteLoading(false)
    }
  }, [sourceToken, targetToken, rawInput, slippage, routes.length])

  // Amount change — debounced 500ms
  useEffect(() => {
    if (!sourceToken || !targetToken || rawInput <= 0) return

    if (amountDebounceRef.current) clearTimeout(amountDebounceRef.current)
    amountDebounceRef.current = setTimeout(() => {
      findRoutes()
    }, 500)

    return () => {
      if (amountDebounceRef.current) clearTimeout(amountDebounceRef.current)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rawInput])

  // Token change — immediate (no debounce)
  useEffect(() => {
    if (!sourceToken || !targetToken || rawInput <= 0) return
    if (amountDebounceRef.current) clearTimeout(amountDebounceRef.current)
    findRoutes()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceToken?.token_id, targetToken?.token_id])

  // =============================================================================
  // Max button
  // =============================================================================

  function handleMax() {
    if (!sourceToken || sourceBalance === undefined) return
    let maxRaw = sourceBalance
    if (sourceToken.token_id === ERG_TOKEN_ID) {
      maxRaw = Math.max(0, sourceBalance - ERG_RESERVE_NANO)
    }
    const displayVal = maxRaw / Math.pow(10, sourceToken.decimals)
    setInputAmount(displayVal.toString())
  }

  // =============================================================================
  // Execution logic
  // =============================================================================

  const selectedRoute = useSplit ? null : (routes[selectedRouteIndex] ?? null)

  const canExecute =
    selectedRoute !== null &&
    selectedRoute.route.hops.length === 1 &&
    selectedRoute.route.hops[0].pool_type === 'N2T' &&
    walletAddress !== null &&
    !insufficientBalance

  const executionBlockReason: string | null = (() => {
    if (!selectedRoute) return useSplit ? 'Split execution not yet supported' : null
    if (!walletAddress) return 'Connect wallet'
    if (insufficientBalance) return 'Insufficient balance'
    if (selectedRoute.route.hops.length > 1) return 'Multi-hop execution coming soon'
    if (selectedRoute.route.hops[0].pool_type !== 'N2T') return 'T2T direct swap not yet supported'
    return null
  })()

  // =============================================================================
  // Slippage helpers
  // =============================================================================

  const SLIPPAGE_PRESETS = [0.1, 0.5, 1.0]

  function handleSlippagePreset(value: number) {
    setSlippage(value)
    setCustomSlippage('')
  }

  function handleCustomSlippageChange(e: React.ChangeEvent<HTMLInputElement>) {
    const raw = e.target.value
    setCustomSlippage(raw)
    const parsed = parseFloat(raw)
    if (!isNaN(parsed) && parsed > 0 && parsed <= 50) {
      setSlippage(parsed)
    }
  }

  // =============================================================================
  // Token handlers — clear routes when either side changes
  // =============================================================================

  function handleSelectSource(token: TokenEntry) {
    setSourceToken(token)
    setRoutes([])
    setSplit(null)
    setRouteError(null)
    setRouteStale(false)
  }

  function handleSelectTarget(token: TokenEntry) {
    setTargetToken(token)
    setRoutes([])
    setSplit(null)
    setRouteError(null)
    setRouteStale(false)
  }

  // =============================================================================
  // Render
  // =============================================================================

  const sourceBalanceDisplay =
    sourceBalance !== undefined && sourceToken
      ? formatTokenAmount(sourceBalance, sourceToken.decimals)
      : null

  return (
    <div className="smart-swap-view">
      {/* Input row */}
      <div className="smart-swap-input-row">
        <div className="smart-swap-token-col">
          <label className="smart-swap-label">From</label>
          <TokenSelector
            tokens={sourceTokenList}
            selected={sourceToken}
            onSelect={handleSelectSource}
            placeholder="Select token"
            disabled={!isConnected}
          />
        </div>

        <div className="smart-swap-amount-col">
          <div className="smart-swap-amount-header">
            <label className="smart-swap-label">Amount</label>
            {sourceBalanceDisplay !== null && (
              <span className="smart-swap-balance">
                Balance: {sourceBalanceDisplay}
              </span>
            )}
          </div>
          <div className="smart-swap-amount-input-row">
            <input
              className="smart-swap-amount-input"
              type="number"
              min="0"
              placeholder="0.00"
              value={inputAmount}
              onChange={(e) => setInputAmount(e.target.value)}
              disabled={!isConnected || !sourceToken}
            />
            <button
              type="button"
              className="smart-swap-max-btn"
              onClick={handleMax}
              disabled={!isConnected || !sourceToken || sourceBalance === undefined}
            >
              Max
            </button>
            <button
              type="button"
              className={`smart-swap-slippage-btn${showSlippage ? ' active' : ''}`}
              onClick={() => setShowSlippage((v) => !v)}
              title="Slippage settings"
            >
              ⚙
            </button>
          </div>
        </div>
      </div>

      {/* Slippage popover */}
      {showSlippage && (
        <div className="smart-swap-slippage-popover">
          <div className="smart-swap-slippage-label">Slippage tolerance</div>
          <div className="smart-swap-slippage-presets">
            {SLIPPAGE_PRESETS.map((preset) => (
              <button
                key={preset}
                type="button"
                className={`smart-swap-slippage-preset${slippage === preset && !customSlippage ? ' active' : ''}`}
                onClick={() => handleSlippagePreset(preset)}
              >
                {preset}%
              </button>
            ))}
          </div>
          <div className="smart-swap-slippage-custom">
            <input
              type="number"
              min="0.01"
              max="50"
              step="0.1"
              placeholder="Custom %"
              value={customSlippage}
              onChange={handleCustomSlippageChange}
              className="smart-swap-slippage-custom-input"
            />
          </div>
          <div className="smart-swap-slippage-current">Current: {slippage}%</div>
        </div>
      )}

      {/* Target row */}
      <div className="smart-swap-target-row">
        <label className="smart-swap-label">To</label>
        <TokenSelector
          tokens={targetTokenList}
          selected={targetToken}
          onSelect={handleSelectTarget}
          placeholder="Select token"
          disabled={!isConnected}
        />
      </div>

      {/* Insufficient balance warning */}
      {insufficientBalance && (
        <div className="smart-swap-warning">
          Insufficient balance
        </div>
      )}

      {/* Not connected info */}
      {!isConnected && (
        <div className="smart-swap-not-connected">
          Connect your wallet to swap
        </div>
      )}

      {/* Routes section */}
      <div className={`smart-swap-routes${routeStale && routeLoading ? ' stale' : ''}`}>
        {routeLoading && routes.length === 0 && (
          <div className="smart-swap-spinner">
            <span className="smart-swap-spinner-icon">⟳</span> Finding routes…
          </div>
        )}

        {routeLoading && routes.length > 0 && (
          <div className="smart-swap-loading-overlay">Updating routes…</div>
        )}

        {routeError && !routeLoading && (
          <div className="smart-swap-error">
            <span>{routeError}</span>
            <button
              type="button"
              className="smart-swap-retry-btn"
              onClick={() => findRoutes()}
            >
              Retry
            </button>
          </div>
        )}

        {!routeLoading && !routeError && routes.length === 0 && sourceToken && targetToken && rawInput > 0 && (
          <div className="smart-swap-no-routes">No routes found for this pair</div>
        )}

        {routes.length > 0 && (
          <RouteList
            routes={routes}
            split={split}
            selectedIndex={selectedRouteIndex}
            onSelectRoute={setSelectedRouteIndex}
            useSplit={useSplit}
            onToggleSplit={setUseSplit}
          />
        )}
      </div>

      {/* Confirm button */}
      <button
        type="button"
        className="smart-swap-confirm-btn"
        disabled={!canExecute}
        onClick={() => setShowSwapModal(true)}
        title={executionBlockReason ?? undefined}
      >
        {executionBlockReason ?? 'Swap'}
      </button>

      {/* SmartSwapModal placeholder — wired in Task 6 */}
      {showSwapModal && selectedRoute && walletAddress && (
        <div className="smart-swap-modal-placeholder">
          {/* SmartSwapModal will be inserted here in Task 6 */}
        </div>
      )}
    </div>
  )
}
