# Smart Swap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make token-to-token smart routing the default swap experience in SwapTab, with wallet-aware token selectors, auto-debounced multi-hop route finding, and 1-hop direct swap execution.

**Architecture:** SmartSwapView becomes the default mode in SwapTab, alongside the existing Pool Swap mode accessed via a segmented control. Smart routing reuses the existing `findSwapRoutes` backend (BFS pathfinding + split optimization in `router.rs`) and `buildDirectSwapTx` for 1-hop N2T execution. New frontend components: TokenSelector (reusable dropdown), RouteCard/RouteList (extracted from RouterTab patterns), and SmartSwapView (orchestrates the flow).

**Tech Stack:** React + TypeScript, Tauri IPC (`@tauri-apps/api/core`), existing `router.ts` / `amm.ts` API wrappers, CSS custom properties (dark theme)

**Spec:** `docs/superpowers/specs/2026-03-25-smart-swap-design.md`

---

## File Structure

### New Files
| File | Responsibility |
|------|---------------|
| `frontend/src/components/TokenSelector.tsx` | Reusable token dropdown — source mode (wallet-only) and target mode (all pool tokens). Search, balance display, icon rendering. |
| `frontend/src/components/SmartSwapView.tsx` | Main smart swap UI — token selectors, amount input, slippage config, route finding trigger, route display, swap button. |
| `frontend/src/components/RouteCard.tsx` | Single route display — path arrows, output, rate, impact, fees, expandable per-hop detail. |
| `frontend/src/components/RouteList.tsx` | Best route + collapsed alternatives + split suggestion callout. |
| `frontend/src/components/SmartSwap.css` | Styles for all smart swap components. |
| `frontend/src/components/SmartSwapModal.tsx` | Swap confirmation modal for smart swap (direct swap execution via route data). |

### Modified Files
| File | Change |
|------|--------|
| `frontend/src/components/SwapTab.tsx` | Extract `TOKEN_ICON_MAP`, `tokenColor`, `TokenIcon`, `PoolPairIcons` to shared location. Add mode toggle state (`'smart' \| 'pool'`). Render SmartSwapView when mode is `'smart'` (default). Pass existing props through. |

### Untouched Files (reused as-is)
| File | Usage |
|------|-------|
| `frontend/src/api/router.ts` | `findSwapRoutes()`, `RoutesResponse`, `RouteQuote`, `RouteHop`, `SplitRouteDetail` types |
| `frontend/src/api/amm.ts` | `getAmmPools()`, `AmmPool`, `buildDirectSwapTx()`, `previewDirectSwap()`, `startSwapSign()`, `getSwapTxStatus()` |
| `frontend/src/hooks/useTransactionFlow.ts` | Transaction signing flow |
| `frontend/src/utils/format.ts` | `formatTokenAmount`, `formatErg` |

---

## Task 1: Extract Token Icons to Shared Module

**Files:**
- Create: `frontend/src/components/tokenIcons.tsx`
- Modify: `frontend/src/components/SwapTab.tsx:35-135`

This task moves `TOKEN_ICON_MAP`, `tokenColor()`, `TokenIcon`, and `PoolPairIcons` out of SwapTab into a shared module so both SwapTab and SmartSwapView can use them.

- [ ] **Step 1: Create `tokenIcons.tsx`**

Copy the icon map and components from SwapTab into a new shared file. The content is lines 35-135 of SwapTab.tsx.

```tsx
// frontend/src/components/tokenIcons.tsx
import type { AmmPool } from '../api/amm'

/** Map known token names (lowercase) to icon paths */
export const TOKEN_ICON_MAP: Record<string, string> = {
  // (copy entire map from SwapTab.tsx lines 36-101)
  erg: '/icons/ergo.svg',
  sigusd: '/icons/sigmausd.svg',
  // ... rest of the map unchanged ...
}

/** Deterministic color from token name for fallback circles */
export function tokenColor(name: string): string {
  let hash = 0
  for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash)
  const hue = ((hash % 360) + 360) % 360
  return `hsl(${hue}, 55%, 55%)`
}

export function TokenIcon({ name, size = 18 }: { name: string; size?: number }) {
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

export function PoolPairIcons({ pool }: { pool: AmmPool }) {
  const nameX = pool.pool_type === 'N2T' ? 'ERG' : (pool.token_x?.name || 'X')
  const nameY = pool.token_y.name || 'Y'
  return (
    <span className="pool-pair-icons">
      <TokenIcon name={nameX} size={18} />
      <TokenIcon name={nameY} size={18} />
    </span>
  )
}
```

- [ ] **Step 2: Update SwapTab imports**

In `SwapTab.tsx`, delete lines 35-135 (the `TOKEN_ICON_MAP`, `tokenColor`, `TokenIcon`, `PoolPairIcons` definitions) and add an import:

```tsx
import { TOKEN_ICON_MAP, tokenColor, TokenIcon, PoolPairIcons } from './tokenIcons'
```

- [ ] **Step 3: Verify SwapTab still compiles**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/tokenIcons.tsx frontend/src/components/SwapTab.tsx
git commit -m "refactor: extract token icons to shared module"
```

---

## Task 2: Build TokenSelector Component

**Files:**
- Create: `frontend/src/components/TokenSelector.tsx`

Reusable dropdown for selecting tokens. Two modes: `source` (wallet tokens only, ERG synthetic) and `target` (all pool tokens, wallet pinned).

- [ ] **Step 1: Create TokenSelector component**

```tsx
// frontend/src/components/TokenSelector.tsx
import { useState, useRef, useEffect, useMemo } from 'react'
import { TokenIcon } from './tokenIcons'
import { formatTokenAmount } from '../utils/format'

export interface TokenEntry {
  token_id: string
  name: string | null
  decimals: number
  balance?: number  // raw units, present if user holds it
}

interface TokenSelectorProps {
  tokens: TokenEntry[]
  selected: TokenEntry | null
  onSelect: (token: TokenEntry) => void
  placeholder?: string
  disabled?: boolean
}

export function TokenSelector({ tokens, selected, onSelect, placeholder = 'Select token', disabled = false }: TokenSelectorProps) {
  const [open, setOpen] = useState(false)
  const [search, setSearch] = useState('')
  const ref = useRef<HTMLDivElement>(null)

  // Close on outside click
  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [])

  const filtered = useMemo(() => {
    if (!search) return tokens
    const q = search.toLowerCase()
    return tokens.filter(t =>
      (t.name && t.name.toLowerCase().includes(q)) ||
      t.token_id.toLowerCase().includes(q)
    )
  }, [tokens, search])

  const displayName = (t: TokenEntry) => t.name || t.token_id.slice(0, 8)

  return (
    <div className="token-selector" ref={ref}>
      <button
        className="token-selector-trigger"
        onClick={() => !disabled && setOpen(!open)}
        disabled={disabled}
      >
        {selected ? (
          <>
            <TokenIcon name={displayName(selected)} size={20} />
            <span className="token-selector-name">{displayName(selected)}</span>
          </>
        ) : (
          <span className="token-selector-placeholder">{placeholder}</span>
        )}
        <span className="token-selector-arrow">▾</span>
      </button>

      {open && (
        <div className="token-selector-dropdown">
          <input
            className="token-selector-search"
            placeholder="Search tokens..."
            value={search}
            onChange={e => setSearch(e.target.value)}
            autoFocus
          />
          <div className="token-selector-list">
            {filtered.length === 0 && (
              <div className="token-selector-empty">No tokens found</div>
            )}
            {filtered.map(t => (
              <button
                key={t.token_id}
                className={`token-selector-item ${selected?.token_id === t.token_id ? 'active' : ''}`}
                onClick={() => { onSelect(t); setOpen(false); setSearch('') }}
              >
                <TokenIcon name={displayName(t)} size={20} />
                <span className="token-selector-item-name">{displayName(t)}</span>
                {t.balance !== undefined && (
                  <span className="token-selector-item-balance">
                    {formatTokenAmount(t.balance, t.decimals)}
                  </span>
                )}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/TokenSelector.tsx
git commit -m "feat: add TokenSelector component for smart swap"
```

---

## Task 3: Build RouteCard and RouteList Components

**Files:**
- Create: `frontend/src/components/RouteCard.tsx`
- Create: `frontend/src/components/RouteList.tsx`

These display route results. The design is informed by the existing `RouteCard` and `HopDetail` inline functions in `RouterTab.tsx` (lines 626-745) but adapted for the smart swap context with expandable details.

- [ ] **Step 1: Create RouteCard component**

```tsx
// frontend/src/components/RouteCard.tsx
import { useState } from 'react'
import { TokenIcon } from './tokenIcons'
import { formatTokenAmount } from '../utils/format'
import type { RouteQuote, RouteHop } from '../api/router'

interface RouteCardProps {
  routeQuote: RouteQuote
  isBest: boolean
  isSelected: boolean
  onSelect: () => void
  /** Show compact row for alternatives list */
  compact?: boolean
}

function impactClass(impact: number): string {
  if (impact > 10) return 'impact-severe'
  if (impact > 5) return 'impact-high'
  if (impact > 1) return 'impact-medium'
  return 'impact-low'
}

function formatHopAmount(amount: number, decimals: number, name: string | null | undefined): string {
  return `${formatTokenAmount(amount, decimals)} ${name || ''}`
}

function HopDetail({ hop }: { hop: RouteHop }) {
  return (
    <div className="smart-hop-detail">
      <div className="smart-hop-detail-top">
        <span className="smart-hop-label">
          {hop.pool_display_name || `${hop.token_in_name || hop.token_in.slice(0, 6)} / ${hop.token_out_name || hop.token_out.slice(0, 6)}`}
          <span className="smart-hop-pool-tag">{hop.pool_id.slice(0, 6)}</span>
        </span>
        <span className={`smart-hop-impact ${impactClass(hop.price_impact)}`}>
          {hop.price_impact.toFixed(2)}% impact
        </span>
      </div>
      <div className="smart-hop-detail-amounts">
        <span>{formatHopAmount(hop.input_amount, hop.token_in_decimals, hop.token_in_name)}</span>
        <span>&rarr;</span>
        <span>{formatHopAmount(hop.output_amount, hop.token_out_decimals, hop.token_out_name)}</span>
      </div>
      <div className="smart-hop-detail-meta">
        <span>Fee: {formatHopAmount(hop.fee_amount, hop.token_in_decimals, hop.token_in_name)}</span>
        <span>Reserves: {formatTokenAmount(hop.reserves_in, hop.token_in_decimals)} / {formatTokenAmount(hop.reserves_out, hop.token_out_decimals)}</span>
      </div>
    </div>
  )
}

export function RouteCard({ routeQuote, isBest, isSelected, onSelect, compact = false }: RouteCardProps) {
  const [expanded, setExpanded] = useState(false)
  const { route } = routeQuote
  const hops = route.hops
  const lastHop = hops[hops.length - 1]

  // Build path label: token names connected by arrows
  const pathParts: string[] = []
  if (hops.length > 0) {
    pathParts.push(hops[0].token_in_name || hops[0].token_in.slice(0, 6))
    for (const hop of hops) {
      pathParts.push(hop.token_out_name || hop.token_out.slice(0, 6))
    }
  }
  const pathLabel = pathParts.join(' → ')

  // Output formatting
  const outputAmount = formatTokenAmount(route.total_output, lastHop?.token_out_decimals ?? 0)
  const outputName = lastHop?.token_out_name || ''

  if (compact) {
    return (
      <button
        className={`smart-route-compact ${isSelected ? 'selected' : ''}`}
        onClick={onSelect}
      >
        <span className="smart-route-compact-path">{pathLabel}</span>
        <span className="smart-route-compact-output">{outputAmount} {outputName}</span>
        <span className={`smart-route-compact-impact ${impactClass(route.total_price_impact)}`}>
          {route.total_price_impact.toFixed(2)}%
        </span>
      </button>
    )
  }

  // Effective rate
  const firstHop = hops[0]
  const inputAmount = formatTokenAmount(route.total_input, firstHop?.token_in_decimals ?? 0)
  const inputName = firstHop?.token_in_name || ''
  const rate = route.effective_rate

  return (
    <div className={`smart-route-card ${isBest ? 'best' : ''} ${isSelected ? 'selected' : ''}`} onClick={onSelect}>
      <div className="smart-route-header">
        <div className="smart-route-badges">
          {isBest && <span className="smart-badge-best">BEST</span>}
          <span className="smart-badge-hops">{hops.length} hop{hops.length > 1 ? 's' : ''}</span>
        </div>
      </div>

      {/* Path visualization */}
      <div className="smart-route-path">
        {pathParts.map((name, i) => (
          <span key={i} className="smart-route-path-item">
            <TokenIcon name={name} size={16} />
            <span>{name}</span>
            {i < pathParts.length - 1 && <span className="smart-route-arrow">→</span>}
          </span>
        ))}
      </div>

      {/* Output */}
      <div className="smart-route-output">
        <span className="smart-route-output-amount">{outputAmount} {outputName}</span>
      </div>

      {/* Metrics row */}
      <div className="smart-route-metrics">
        <span>Rate: 1 {inputName} = {rate.toFixed(6)} {outputName}</span>
        <span className={impactClass(route.total_price_impact)}>
          Impact: {route.total_price_impact.toFixed(2)}%
        </span>
        <span>Fees: {formatTokenAmount(route.total_fees, firstHop?.token_in_decimals ?? 0)} {inputName}</span>
      </div>

      {/* Expandable details */}
      <button
        className="smart-route-expand-btn"
        onClick={(e) => { e.stopPropagation(); setExpanded(!expanded) }}
      >
        {expanded ? '▾ Hide details' : '▸ Route details'}
      </button>

      {expanded && (
        <div className="smart-route-details">
          {hops.map((hop, i) => <HopDetail key={i} hop={hop} />)}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Create RouteList component**

```tsx
// frontend/src/components/RouteList.tsx
import { useState } from 'react'
import { RouteCard } from './RouteCard'
import { formatTokenAmount } from '../utils/format'
import type { RouteQuote, SplitRouteDetail } from '../api/router'

interface RouteListProps {
  routes: RouteQuote[]
  split: SplitRouteDetail | null
  selectedIndex: number
  onSelectRoute: (index: number) => void
  useSplit: boolean
  onToggleSplit: (use: boolean) => void
}

export function RouteList({ routes, split, selectedIndex, onSelectRoute, useSplit, onToggleSplit }: RouteListProps) {
  const [showAlternatives, setShowAlternatives] = useState(false)

  if (routes.length === 0) return null

  const bestRoute = routes[0]
  const alternatives = routes.slice(1)

  return (
    <div className="smart-route-list">
      {/* Best route */}
      <RouteCard
        routeQuote={bestRoute}
        isBest={true}
        isSelected={selectedIndex === 0 && !useSplit}
        onSelect={() => { onSelectRoute(0); onToggleSplit(false) }}
      />

      {/* Alternative routes */}
      {alternatives.length > 0 && (
        <div className="smart-route-alternatives">
          <button
            className="smart-route-alternatives-toggle"
            onClick={() => setShowAlternatives(!showAlternatives)}
          >
            {showAlternatives ? '▾' : '▸'} {alternatives.length} other route{alternatives.length > 1 ? 's' : ''} available
          </button>

          {showAlternatives && alternatives.map((rq, idx) => (
            <RouteCard
              key={idx + 1}
              routeQuote={rq}
              isBest={false}
              isSelected={selectedIndex === idx + 1 && !useSplit}
              onSelect={() => { onSelectRoute(idx + 1); onToggleSplit(false) }}
              compact
            />
          ))}
        </div>
      )}

      {/* Split suggestion */}
      {split && (
        <div className={`smart-split-suggestion ${useSplit ? 'active' : ''}`}>
          <div className="smart-split-header">
            <span className="smart-split-label">
              Split across {split.allocations.length} routes for +{split.improvement_pct.toFixed(1)}% better output
            </span>
            <button
              className={`smart-split-toggle ${useSplit ? 'active' : ''}`}
              onClick={() => onToggleSplit(!useSplit)}
            >
              {useSplit ? 'Using split' : 'Use split'}
            </button>
          </div>
          {useSplit && (
            <div className="smart-split-allocations">
              {split.allocations.map((alloc, i) => {
                const firstHop = alloc.route.hops[0]
                const lastHop = alloc.route.hops[alloc.route.hops.length - 1]
                const pathParts = [firstHop?.token_in_name || '']
                for (const hop of alloc.route.hops) {
                  pathParts.push(hop.token_out_name || '')
                }
                return (
                  <div key={i} className="smart-split-alloc">
                    <span className="smart-split-alloc-pct">{(alloc.fraction * 100).toFixed(0)}%</span>
                    <span className="smart-split-alloc-path">{pathParts.join(' → ')}</span>
                    <span className="smart-split-alloc-output">
                      {formatTokenAmount(alloc.output_amount, lastHop?.token_out_decimals ?? 0)}
                    </span>
                  </div>
                )
              })}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 3: Verify both compile**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/RouteCard.tsx frontend/src/components/RouteList.tsx
git commit -m "feat: add RouteCard and RouteList components for smart swap"
```

---

## Task 4: Build SmartSwapView Component

**Files:**
- Create: `frontend/src/components/SmartSwapView.tsx`

The main smart swap UI that orchestrates token selection, amount input, route finding, and route display.

- [ ] **Step 1: Create SmartSwapView**

```tsx
// frontend/src/components/SmartSwapView.tsx
import { useState, useEffect, useRef, useMemo, useCallback } from 'react'
import { getAmmPools, type AmmPool } from '../api/amm'
import { findSwapRoutes, type RoutesResponse, type RouteQuote, type SplitRouteDetail } from '../api/router'
import { TokenSelector, type TokenEntry } from './TokenSelector'
import { RouteList } from './RouteList'
import { formatTokenAmount, formatErg } from '../utils/format'

interface SmartSwapViewProps {
  isConnected: boolean
  walletAddress: string | null
  walletBalance: {
    erg_nano: number
    tokens: Array<{ token_id: string; amount: number; name: string | null; decimals: number }>
  } | null
  explorerUrl: string
  pools: AmmPool[]
}

const ERG_DECIMALS = 9
const ERG_RESERVE_NANO = 10_000_000 // 0.01 ERG reserve for fees

/**
 * Derive deduplicated token list from pool graph.
 * For source mode: only wallet-held tokens.
 * For target mode: all tokens, wallet-held pinned to top.
 */
function buildTokenList(
  pools: AmmPool[],
  walletBalance: SmartSwapViewProps['walletBalance'],
  mode: 'source' | 'target',
): TokenEntry[] {
  // Collect all unique tokens from pools
  const tokenMap = new Map<string, { name: string | null; decimals: number }>()
  for (const pool of pools) {
    if (pool.token_y) {
      tokenMap.set(pool.token_y.token_id, {
        name: pool.token_y.name ?? null,
        decimals: pool.token_y.decimals ?? 0,
      })
    }
    if (pool.token_x) {
      tokenMap.set(pool.token_x.token_id, {
        name: pool.token_x.name ?? null,
        decimals: pool.token_x.decimals ?? 0,
      })
    }
  }

  // Build wallet balance lookup
  const balanceMap = new Map<string, number>()
  if (walletBalance) {
    balanceMap.set('ERG', walletBalance.erg_nano)
    for (const t of walletBalance.tokens) {
      balanceMap.set(t.token_id, t.amount)
    }
  }

  // ERG synthetic entry
  const ergEntry: TokenEntry = {
    token_id: 'ERG',
    name: 'ERG',
    decimals: ERG_DECIMALS,
    balance: walletBalance?.erg_nano,
  }

  if (mode === 'source') {
    // Only tokens the wallet holds
    const entries: TokenEntry[] = [ergEntry]
    if (walletBalance) {
      for (const t of walletBalance.tokens) {
        if (t.amount > 0) {
          // Prefer name/decimals from pool data if available
          const poolMeta = tokenMap.get(t.token_id)
          entries.push({
            token_id: t.token_id,
            name: poolMeta?.name ?? t.name,
            decimals: poolMeta?.decimals ?? t.decimals,
            balance: t.amount,
          })
        }
      }
    }
    // Sort: ERG first, then alphabetically
    entries.sort((a, b) => {
      if (a.token_id === 'ERG') return -1
      if (b.token_id === 'ERG') return 1
      return (a.name || '').localeCompare(b.name || '')
    })
    return entries
  }

  // Target mode: all pool tokens, wallet-held pinned to top
  const entries: TokenEntry[] = [ergEntry]
  for (const [tokenId, meta] of tokenMap) {
    entries.push({
      token_id: tokenId,
      name: meta.name,
      decimals: meta.decimals,
      balance: balanceMap.get(tokenId),
    })
  }

  // Sort: ERG first, then wallet-held alphabetically, then rest alphabetically
  entries.sort((a, b) => {
    if (a.token_id === 'ERG') return -1
    if (b.token_id === 'ERG') return 1
    const aHeld = a.balance !== undefined && a.balance > 0
    const bHeld = b.balance !== undefined && b.balance > 0
    if (aHeld && !bHeld) return -1
    if (!aHeld && bHeld) return 1
    return (a.name || '').localeCompare(b.name || '')
  })
  return entries
}

export function SmartSwapView({ isConnected, walletAddress, walletBalance, explorerUrl, pools }: SmartSwapViewProps) {
  const [sourceToken, setSourceToken] = useState<TokenEntry | null>(null)
  const [targetToken, setTargetToken] = useState<TokenEntry | null>(null)
  const [inputAmount, setInputAmount] = useState('')
  const [slippage, setSlippage] = useState(0.5)
  const [showSlippage, setShowSlippage] = useState(false)

  // Route state
  const [routes, setRoutes] = useState<RouteQuote[]>([])
  const [split, setSplit] = useState<SplitRouteDetail | null>(null)
  const [selectedRouteIndex, setSelectedRouteIndex] = useState(0)
  const [useSplit, setUseSplit] = useState(false)
  const [routeLoading, setRouteLoading] = useState(false)
  const [routeError, setRouteError] = useState<string | null>(null)
  const [routeStale, setRouteStale] = useState(false)

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Build token lists
  const sourceTokens = useMemo(() => buildTokenList(pools, walletBalance, 'source'), [pools, walletBalance])
  const targetTokens = useMemo(() => buildTokenList(pools, walletBalance, 'target'), [pools, walletBalance])

  // Compute raw input amount
  const rawInput = useMemo(() => {
    if (!sourceToken || !inputAmount) return 0
    const val = parseFloat(inputAmount)
    if (isNaN(val) || val <= 0) return 0
    return Math.round(val * Math.pow(10, sourceToken.decimals))
  }, [inputAmount, sourceToken])

  // Check balance
  const insufficientBalance = useMemo(() => {
    if (!sourceToken || rawInput === 0) return false
    if (sourceToken.balance === undefined) return false
    return rawInput > sourceToken.balance
  }, [sourceToken, rawInput])

  // Route finding
  const findRoutes = useCallback(async () => {
    if (!sourceToken || !targetToken || rawInput === 0) {
      setRoutes([])
      setSplit(null)
      return
    }

    setRouteLoading(true)
    setRouteError(null)
    setRouteStale(true)

    try {
      const response: RoutesResponse = await findSwapRoutes(
        sourceToken.token_id,
        targetToken.token_id,
        rawInput,
        4,   // maxHops
        5,   // maxRoutes
        slippage,
      )
      setRoutes(response.routes)
      setSplit(response.split)
      setSelectedRouteIndex(0)
      setUseSplit(false)
      setRouteStale(false)
    } catch (e) {
      setRouteError(String(e))
      setRoutes([])
      setSplit(null)
    } finally {
      setRouteLoading(false)
    }
  }, [sourceToken, targetToken, rawInput, slippage])

  // Debounced route finding on amount change
  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)
    if (!sourceToken || !targetToken || rawInput === 0) {
      setRoutes([])
      setSplit(null)
      return
    }
    debounceRef.current = setTimeout(findRoutes, 500)
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current) }
  }, [rawInput]) // eslint-disable-line react-hooks/exhaustive-deps

  // Immediate route finding on token change
  useEffect(() => {
    if (sourceToken && targetToken && rawInput > 0) {
      findRoutes()
    }
  }, [sourceToken?.token_id, targetToken?.token_id]) // eslint-disable-line react-hooks/exhaustive-deps

  // Max button
  const handleMax = () => {
    if (!sourceToken || sourceToken.balance === undefined) return
    let maxRaw = sourceToken.balance
    if (sourceToken.token_id === 'ERG') {
      maxRaw = Math.max(0, maxRaw - ERG_RESERVE_NANO)
    }
    const display = maxRaw / Math.pow(10, sourceToken.decimals)
    setInputAmount(display.toString())
  }

  // Determine if selected route is executable (1-hop N2T only)
  const selectedRoute = useSplit ? null : routes[selectedRouteIndex] ?? null
  const canExecute = selectedRoute !== null
    && selectedRoute.route.hops.length === 1
    && selectedRoute.route.hops[0].pool_type === 'N2T'
    && walletAddress !== null
    && !insufficientBalance

  const executionBlockReason = (() => {
    if (!selectedRoute) return useSplit ? 'Split execution not yet supported' : null
    if (!walletAddress) return 'Connect wallet'
    if (insufficientBalance) return 'Insufficient balance'
    if (selectedRoute.route.hops.length > 1) return 'Multi-hop execution coming soon'
    if (selectedRoute.route.hops[0].pool_type !== 'N2T') return 'T2T direct swap not yet supported'
    return null
  })()

  // Swap button handler — opens SmartSwapModal (handled by parent)
  const [showSwapModal, setShowSwapModal] = useState(false)

  return (
    <div className="smart-swap-view">
      {/* Source token + amount */}
      <div className="smart-swap-input-row">
        <div className="smart-swap-token-col">
          <label className="smart-swap-label">From</label>
          <TokenSelector
            tokens={sourceTokens}
            selected={sourceToken}
            onSelect={setSourceToken}
            placeholder="Select token"
            disabled={!isConnected}
          />
        </div>
        <div className="smart-swap-amount-col">
          <div className="smart-swap-amount-header">
            <label className="smart-swap-label">Amount</label>
            {sourceToken?.balance !== undefined && (
              <span className="smart-swap-balance" onClick={handleMax}>
                Bal: {formatTokenAmount(sourceToken.balance, sourceToken.decimals)}
              </span>
            )}
          </div>
          <div className="smart-swap-amount-input-row">
            <input
              type="text"
              className="smart-swap-amount-input"
              value={inputAmount}
              onChange={e => setInputAmount(e.target.value)}
              placeholder="0.0"
              disabled={!sourceToken}
            />
            <button className="smart-swap-max-btn" onClick={handleMax} disabled={!sourceToken}>Max</button>
            <button
              className="smart-swap-slippage-btn"
              onClick={() => setShowSlippage(!showSlippage)}
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
          <span className="smart-swap-slippage-label">Slippage tolerance</span>
          <div className="smart-swap-slippage-options">
            {[0.1, 0.5, 1.0].map(v => (
              <button
                key={v}
                className={`smart-swap-slippage-btn ${slippage === v ? 'active' : ''}`}
                onClick={() => setSlippage(v)}
              >
                {v}%
              </button>
            ))}
            <input
              type="number"
              className="smart-swap-slippage-custom"
              value={slippage}
              onChange={e => setSlippage(parseFloat(e.target.value) || 0.5)}
              step="0.1"
              min="0.01"
              max="50"
            />
          </div>
        </div>
      )}

      {/* Target token */}
      <div className="smart-swap-target-row">
        <label className="smart-swap-label">To</label>
        <TokenSelector
          tokens={targetTokens}
          selected={targetToken}
          onSelect={setTargetToken}
          placeholder="Select token"
        />
      </div>

      {/* Insufficient balance warning */}
      {insufficientBalance && (
        <div className="smart-swap-warning">Insufficient balance</div>
      )}

      {/* No wallet warning */}
      {!isConnected && (
        <div className="smart-swap-info">Connect wallet to select source tokens</div>
      )}

      {/* Route results */}
      <div className={`smart-swap-routes ${routeStale && routeLoading ? 'stale' : ''}`}>
        {routeLoading && routes.length === 0 && (
          <div className="smart-swap-loading">Finding best routes...</div>
        )}
        {routeLoading && routes.length > 0 && (
          <div className="smart-swap-loading-overlay" />
        )}

        {routeError && (
          <div className="smart-swap-error">
            {routeError.includes('No route') ? 'No route available between these tokens' : `Failed to fetch routes — ${routeError}`}
            <button className="smart-swap-retry-btn" onClick={findRoutes}>Retry</button>
          </div>
        )}

        {!routeLoading && !routeError && routes.length === 0 && sourceToken && targetToken && rawInput > 0 && (
          <div className="smart-swap-no-routes">No route available between these tokens</div>
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

      {/* Swap button */}
      <button
        className="btn btn-primary smart-swap-confirm-btn"
        disabled={!canExecute || routeLoading}
        onClick={() => setShowSwapModal(true)}
        title={executionBlockReason || undefined}
      >
        {executionBlockReason || 'Swap'}
      </button>

      {/* SmartSwapModal rendered here — Task 6 */}
    </div>
  )
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/SmartSwapView.tsx
git commit -m "feat: add SmartSwapView component with route finding"
```

---

## Task 5: Add CSS for Smart Swap Components

**Files:**
- Create: `frontend/src/components/SmartSwap.css`

Styles follow existing patterns: CSS custom properties (`--card-bg`, `--border-color`, `--emerald-400`), dark theme, consistent with `RouterTab.css` and `App.css` swap styles.

- [ ] **Step 1: Create SmartSwap.css**

```css
/* frontend/src/components/SmartSwap.css */

/* ============================================================================
   Smart Swap View
   ============================================================================ */

.smart-swap-view {
  max-width: 520px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

/* --- Token Selector --- */
.token-selector {
  position: relative;
  min-width: 140px;
}

.token-selector-trigger {
  display: flex;
  align-items: center;
  gap: var(--space-xs);
  padding: 8px 12px;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: rgb(var(--text));
  cursor: pointer;
  font-size: var(--text-sm);
  width: 100%;
}

.token-selector-trigger:hover:not(:disabled) {
  border-color: var(--emerald-400);
}

.token-selector-trigger:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.token-selector-placeholder {
  color: var(--slate-400);
}

.token-selector-arrow {
  margin-left: auto;
  color: var(--slate-400);
}

.token-selector-dropdown {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  z-index: 50;
  margin-top: 4px;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  max-height: 300px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.token-selector-search {
  padding: 8px 12px;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border-color);
  color: rgb(var(--text));
  font-size: var(--text-sm);
  outline: none;
}

.token-selector-list {
  overflow-y: auto;
  max-height: 250px;
}

.token-selector-item {
  display: flex;
  align-items: center;
  gap: var(--space-xs);
  padding: 8px 12px;
  width: 100%;
  background: transparent;
  border: none;
  color: rgb(var(--text));
  cursor: pointer;
  font-size: var(--text-sm);
  text-align: left;
}

.token-selector-item:hover {
  background: rgba(var(--emerald-raw), 0.1);
}

.token-selector-item.active {
  background: rgba(var(--emerald-raw), 0.15);
}

.token-selector-item-name {
  flex: 1;
}

.token-selector-item-balance {
  color: var(--slate-400);
  font-size: var(--text-xs);
}

.token-selector-empty {
  padding: 16px;
  text-align: center;
  color: var(--slate-400);
  font-size: var(--text-sm);
}

/* --- Input Row --- */
.smart-swap-input-row {
  display: flex;
  gap: var(--space-sm);
  align-items: flex-end;
}

.smart-swap-token-col {
  flex: 0 0 160px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.smart-swap-amount-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.smart-swap-label {
  font-size: var(--text-xs);
  color: var(--slate-400);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.smart-swap-amount-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.smart-swap-balance {
  font-size: var(--text-xs);
  color: var(--emerald-400);
  cursor: pointer;
}

.smart-swap-balance:hover {
  text-decoration: underline;
}

.smart-swap-amount-input-row {
  display: flex;
  gap: 4px;
}

.smart-swap-amount-input {
  flex: 1;
  padding: 8px 12px;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: rgb(var(--text));
  font-size: var(--text-sm);
  outline: none;
}

.smart-swap-amount-input:focus {
  border-color: var(--emerald-400);
}

.smart-swap-max-btn,
.smart-swap-slippage-btn {
  padding: 8px 10px;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: var(--emerald-400);
  cursor: pointer;
  font-size: var(--text-xs);
}

.smart-swap-max-btn:hover,
.smart-swap-slippage-btn:hover {
  border-color: var(--emerald-400);
}

/* --- Slippage Popover --- */
.smart-swap-slippage-popover {
  padding: var(--space-sm);
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

.smart-swap-slippage-label {
  font-size: var(--text-xs);
  color: var(--slate-400);
}

.smart-swap-slippage-options {
  display: flex;
  gap: 4px;
  align-items: center;
}

.smart-swap-slippage-options .smart-swap-slippage-btn {
  padding: 4px 10px;
  font-size: var(--text-xs);
  background: transparent;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: rgb(var(--text));
  cursor: pointer;
}

.smart-swap-slippage-options .smart-swap-slippage-btn.active {
  border-color: var(--emerald-400);
  color: var(--emerald-400);
}

.smart-swap-slippage-custom {
  width: 60px;
  padding: 4px 8px;
  background: transparent;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: rgb(var(--text));
  font-size: var(--text-xs);
  text-align: center;
}

/* --- Target Row --- */
.smart-swap-target-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.smart-swap-target-row .token-selector {
  width: 100%;
}

/* --- Status Messages --- */
.smart-swap-warning {
  padding: 8px 12px;
  background: rgba(251, 191, 36, 0.1);
  border: 1px solid rgba(251, 191, 36, 0.3);
  border-radius: 8px;
  color: #fbbf24;
  font-size: var(--text-sm);
}

.smart-swap-info {
  padding: 8px 12px;
  color: var(--slate-400);
  font-size: var(--text-sm);
  text-align: center;
}

.smart-swap-loading {
  padding: 24px;
  text-align: center;
  color: var(--slate-400);
  font-size: var(--text-sm);
}

.smart-swap-loading-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.3);
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.smart-swap-routes {
  position: relative;
  min-height: 60px;
}

.smart-swap-routes.stale {
  opacity: 0.6;
}

.smart-swap-error {
  padding: 12px;
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 8px;
  color: #ef4444;
  font-size: var(--text-sm);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.smart-swap-retry-btn {
  padding: 4px 10px;
  background: transparent;
  border: 1px solid rgba(239, 68, 68, 0.4);
  border-radius: 6px;
  color: #ef4444;
  cursor: pointer;
  font-size: var(--text-xs);
}

.smart-swap-no-routes {
  padding: 24px;
  text-align: center;
  color: var(--slate-400);
  font-size: var(--text-sm);
}

/* --- Route Card --- */
.smart-route-card {
  padding: var(--space-sm);
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  cursor: pointer;
  transition: border-color 0.15s;
}

.smart-route-card:hover {
  border-color: var(--slate-500);
}

.smart-route-card.best {
  border-color: var(--emerald-400);
}

.smart-route-card.selected {
  border-color: var(--emerald-400);
  box-shadow: 0 0 0 1px var(--emerald-400);
}

.smart-route-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--space-xs);
}

.smart-route-badges {
  display: flex;
  gap: 4px;
}

.smart-badge-best {
  padding: 2px 6px;
  background: rgba(var(--emerald-raw), 0.15);
  color: var(--emerald-400);
  font-size: 10px;
  font-weight: 700;
  border-radius: 4px;
  text-transform: uppercase;
}

.smart-badge-hops {
  padding: 2px 6px;
  background: rgba(100, 116, 139, 0.15);
  color: var(--slate-400);
  font-size: 10px;
  border-radius: 4px;
}

.smart-route-path {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  margin-bottom: var(--space-xs);
}

.smart-route-path-item {
  display: flex;
  align-items: center;
  gap: 3px;
  font-size: var(--text-sm);
  color: rgb(var(--text));
}

.smart-route-arrow {
  color: var(--slate-400);
  margin: 0 2px;
}

.smart-route-output {
  margin-bottom: var(--space-xs);
}

.smart-route-output-amount {
  font-size: var(--text-lg);
  font-weight: 700;
  color: rgb(var(--text));
}

.smart-route-metrics {
  display: flex;
  gap: var(--space-sm);
  flex-wrap: wrap;
  font-size: var(--text-xs);
  color: var(--slate-400);
}

.smart-route-expand-btn {
  margin-top: var(--space-xs);
  padding: 4px 0;
  background: transparent;
  border: none;
  color: var(--emerald-400);
  cursor: pointer;
  font-size: var(--text-xs);
}

.smart-route-expand-btn:hover {
  text-decoration: underline;
}

.smart-route-details {
  margin-top: var(--space-xs);
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

/* --- Hop Detail --- */
.smart-hop-detail {
  padding: var(--space-xs);
  background: rgba(30, 41, 59, 0.3);
  border-radius: 6px;
  font-size: var(--text-xs);
}

.smart-hop-detail-top {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 2px;
}

.smart-hop-label {
  color: rgb(var(--text));
}

.smart-hop-pool-tag {
  margin-left: 4px;
  color: var(--slate-500);
  font-family: monospace;
  font-size: 10px;
}

.smart-hop-impact {
  font-size: 10px;
}

.smart-hop-detail-amounts {
  display: flex;
  gap: var(--space-xs);
  color: var(--slate-300);
}

.smart-hop-detail-meta {
  display: flex;
  gap: var(--space-sm);
  color: var(--slate-500);
  font-size: 10px;
  margin-top: 2px;
}

/* --- Impact Colors --- */
.impact-low { color: var(--emerald-400); }
.impact-medium { color: #fbbf24; }
.impact-high { color: #f97316; }
.impact-severe { color: #ef4444; }

/* --- Compact Route (alternatives) --- */
.smart-route-compact {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  padding: 8px var(--space-sm);
  background: transparent;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: rgb(var(--text));
  cursor: pointer;
  font-size: var(--text-sm);
  width: 100%;
  text-align: left;
}

.smart-route-compact:hover {
  border-color: var(--slate-500);
}

.smart-route-compact.selected {
  border-color: var(--emerald-400);
}

.smart-route-compact-path {
  flex: 1;
}

.smart-route-compact-output {
  font-weight: 600;
}

.smart-route-compact-impact {
  font-size: var(--text-xs);
}

/* --- Route List --- */
.smart-route-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

.smart-route-alternatives {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

.smart-route-alternatives-toggle {
  padding: 6px 0;
  background: transparent;
  border: none;
  color: var(--slate-400);
  cursor: pointer;
  font-size: var(--text-xs);
  text-align: left;
}

.smart-route-alternatives-toggle:hover {
  color: rgb(var(--text));
}

/* --- Split Suggestion --- */
.smart-split-suggestion {
  padding: var(--space-sm);
  background: rgba(var(--emerald-raw), 0.05);
  border: 1px solid rgba(var(--emerald-raw), 0.2);
  border-radius: 8px;
}

.smart-split-suggestion.active {
  border-color: var(--emerald-400);
}

.smart-split-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.smart-split-label {
  font-size: var(--text-sm);
  color: var(--emerald-400);
}

.smart-split-toggle {
  padding: 4px 10px;
  background: transparent;
  border: 1px solid var(--emerald-400);
  border-radius: 6px;
  color: var(--emerald-400);
  cursor: pointer;
  font-size: var(--text-xs);
}

.smart-split-toggle.active {
  background: rgba(var(--emerald-raw), 0.15);
}

.smart-split-allocations {
  margin-top: var(--space-xs);
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.smart-split-alloc {
  display: flex;
  gap: var(--space-sm);
  font-size: var(--text-xs);
  color: var(--slate-300);
}

.smart-split-alloc-pct {
  font-weight: 600;
  color: var(--emerald-400);
  min-width: 32px;
}

.smart-split-alloc-path {
  flex: 1;
}

/* --- Swap Button --- */
.smart-swap-confirm-btn {
  width: 100%;
  margin-top: var(--space-xs);
}

/* --- Mode Toggle (in SwapTab) --- */
.smart-swap-mode-toggle {
  display: flex;
  gap: 2px;
  padding: 2px;
  background: rgba(30, 41, 59, 0.4);
  border-radius: 8px;
  margin-bottom: var(--space-md);
  width: fit-content;
}

.smart-swap-mode-btn {
  padding: 6px 16px;
  background: transparent;
  border: none;
  border-radius: 6px;
  color: var(--slate-400);
  cursor: pointer;
  font-size: var(--text-sm);
  font-weight: 500;
  transition: all 0.15s;
}

.smart-swap-mode-btn.active {
  background: var(--card-bg);
  color: rgb(var(--text));
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}
```

- [ ] **Step 2: Import CSS in SmartSwapView**

Add to top of `SmartSwapView.tsx`:
```tsx
import './SmartSwap.css'
```

- [ ] **Step 3: Verify build**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/SmartSwap.css frontend/src/components/SmartSwapView.tsx
git commit -m "feat: add smart swap CSS styles"
```

---

## Task 6: Build SmartSwapModal for 1-Hop Execution

**Files:**
- Create: `frontend/src/components/SmartSwapModal.tsx`

Modal for confirming and executing a 1-hop direct swap based on the selected route. Follows the **exact** pattern in `SwapModal.tsx` (lines 68-435): preview → build tx → `flow.startSigning()` → signing UI with `flow.handleNautilusSign()` / `flow.handleMobileSign()` / `flow.handleBackToChoice()`.

**IMPORTANT**: The `useTransactionFlow` hook exposes `startSigning(rid, qrUrl, nautilusUrl)`, `handleNautilusSign()`, `handleMobileSign()`, `handleBackToChoice()` as its public API. Do NOT use internal setters like `setQrUrl`, `setSignMethod`, etc. — they are not exported. Copy the signing UI JSX from `SwapModal.tsx` lines 324-406.

- [ ] **Step 1: Create SmartSwapModal**

```tsx
// frontend/src/components/SmartSwapModal.tsx
import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import {
  previewDirectSwap, buildDirectSwapTx, startSwapSign, getSwapTxStatus,
  formatErg,
  type DirectSwapPreviewResponse,
} from '../api/amm'
import type { RouteQuote } from '../api/router'
import { formatTokenAmount } from '../utils/format'
import { TxSuccess } from './TxSuccess'
import { useTransactionFlow } from '../hooks/useTransactionFlow'

interface SmartSwapModalProps {
  isOpen: boolean
  onClose: () => void
  routeQuote: RouteQuote
  sourceAmount: number  // raw units
  slippage: number
  walletAddress: string
  explorerUrl: string
  onSuccess: () => void
}

type ModalStep = 'preview' | 'signing' | 'success' | 'error'

export function SmartSwapModal({
  isOpen, onClose, routeQuote, sourceAmount, slippage,
  walletAddress, explorerUrl, onSuccess,
}: SmartSwapModalProps) {
  const [step, setStep] = useState<ModalStep>('preview')
  const [preview, setPreview] = useState<DirectSwapPreviewResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const hop = routeQuote.route.hops[0]
  const inputType: 'erg' | 'token' = hop.token_in === 'ERG' ? 'erg' : 'token'
  const tokenId = inputType === 'token' ? hop.token_in : undefined
  const inputLabel = hop.token_in_name || hop.token_in.slice(0, 8)
  const outputLabel = hop.token_out_name || hop.token_out.slice(0, 8)

  const flow = useTransactionFlow({
    pollStatus: getSwapTxStatus,
    isOpen,
    onSuccess: () => setStep('success'),
    onError: (err) => { setError(err); setStep('error') },
    watchParams: {
      protocol: 'AMM',
      operation: 'smart-swap',
      description: `Smart Swap ${inputLabel} → ${outputLabel}`,
    },
  })

  // Fetch preview on open
  useEffect(() => {
    if (isOpen) {
      setStep('preview')
      setPreview(null)
      setError(null)
      fetchPreview()
    }
  }, [isOpen])

  const fetchPreview = async () => {
    setLoading(true)
    setError(null)
    try {
      const result = await previewDirectSwap(hop.pool_id, inputType, sourceAmount, tokenId, slippage)
      setPreview(result)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  const handleConfirm = async () => {
    if (!preview) return
    setLoading(true)
    setError(null)

    try {
      const nodeStatus = await invoke<{ chain_height: number }>('get_node_status')
      const utxos = await invoke<object[]>('get_user_utxos')

      const buildResult = await buildDirectSwapTx(
        hop.pool_id,
        inputType,
        sourceAmount,
        tokenId,
        preview.min_output,
        walletAddress,
        utxos,
        nodeStatus.chain_height,
      )

      const message = `Smart Swap ${formatTokenAmount(sourceAmount, hop.token_in_decimals)} ${inputLabel} → ${outputLabel}`
      const signResult = await startSwapSign(buildResult.unsigned_tx, message)

      // Use the hook's public API — identical to SwapModal.tsx line 185
      flow.startSigning(signResult.request_id, signResult.ergopay_url, signResult.nautilus_url)
      setStep('signing')
    } catch (e) {
      const errMsg = String(e)
      if (errMsg.includes('not found') || errMsg.includes('double spending')) {
        setError('Pool state changed since quote. Please try again.')
      } else {
        setError(errMsg)
      }
      setStep('error')
    } finally {
      setLoading(false)
    }
  }

  if (!isOpen) return null

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal smart-swap-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Smart Swap {inputLabel} &rarr; {outputLabel}</h2>
          <button className="close-btn" onClick={onClose}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>

        <div className="modal-content">
          {/* Preview Step */}
          {step === 'preview' && (
            <div className="swap-preview-step">
              {loading && !preview && (
                <div className="swap-preview-loading">
                  <div className="spinner-small" />
                  <span>Fetching swap preview...</span>
                </div>
              )}

              {error && !preview && (
                <div className="swap-preview-error">
                  <div className="message error">{error}</div>
                  <button className="btn btn-secondary" onClick={fetchPreview}>Retry</button>
                </div>
              )}

              {preview && (
                <>
                  <div className="preview-section">
                    <div className="preview-row highlight">
                      <span>You Pay</span>
                      <span>{formatTokenAmount(sourceAmount, hop.token_in_decimals)} {inputLabel}</span>
                    </div>
                    <div className="preview-row highlight">
                      <span>You Receive (est.)</span>
                      <span className="text-emerald">
                        {formatTokenAmount(preview.output_amount, preview.output_decimals ?? 0)} {preview.output_token_name || outputLabel}
                      </span>
                    </div>
                    <div className="preview-row">
                      <span>Minimum Output</span>
                      <span>{formatTokenAmount(preview.min_output, preview.output_decimals ?? 0)} {preview.output_token_name || outputLabel}</span>
                    </div>
                  </div>

                  <div className="fee-breakdown">
                    <h4>Fee Breakdown</h4>
                    <div className="fee-row">
                      <span>Price Impact</span>
                      <span className={preview.price_impact > 3 ? 'text-danger' : preview.price_impact > 1 ? 'text-warning' : ''}>
                        {preview.price_impact.toFixed(2)}%
                      </span>
                    </div>
                    <div className="fee-row">
                      <span>Pool Fee</span>
                      <span>{preview.fee_amount.toLocaleString()}</span>
                    </div>
                    <div className="fee-row">
                      <span>Effective Rate</span>
                      <span>{preview.effective_rate.toFixed(6)}</span>
                    </div>
                    <div className="fee-row">
                      <span>Miner Fee</span>
                      <span>{formatErg(preview.miner_fee_nano)} ERG</span>
                    </div>
                    <div className="fee-row total">
                      <span>Total ERG Cost</span>
                      <span>{formatErg(preview.total_erg_cost_nano)} ERG</span>
                    </div>
                  </div>

                  {preview.price_impact > 3 && (
                    <div className="warning-box">
                      Price impact is high ({preview.price_impact.toFixed(2)}%). You may want to reduce your trade size.
                    </div>
                  )}

                  <div className="slippage-notice">Slippage tolerance: {slippage}%</div>

                  {error && <div className="message error">{error}</div>}

                  <div className="button-group">
                    <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
                    <button className="btn btn-primary" onClick={handleConfirm} disabled={loading}>
                      {loading ? 'Building...' : 'Confirm Swap'}
                    </button>
                  </div>
                </>
              )}
            </div>
          )}

          {/* Signing Step - Choose Method (mirrors SwapModal.tsx lines 324-355) */}
          {step === 'signing' && flow.signMethod === 'choose' && (
            <div className="mint-signing-step">
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
                    <span className="wallet-option-desc">Scan QR code with Ergo Wallet</span>
                  </div>
                </button>
              </div>
            </div>
          )}

          {/* Signing Step - Nautilus (mirrors SwapModal.tsx lines 358-376) */}
          {step === 'signing' && flow.signMethod === 'nautilus' && (
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
                <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
                <button className="btn btn-primary" onClick={flow.handleNautilusSign}>Open Nautilus Again</button>
              </div>
            </div>
          )}

          {/* Signing Step - Mobile QR (mirrors SwapModal.tsx lines 379-388) */}
          {step === 'signing' && flow.signMethod === 'mobile' && flow.qrUrl && (
            <div className="mint-signing-step">
              <p>Scan with your Ergo wallet to sign</p>
              <div className="qr-container">
                <QRCodeSVG value={flow.qrUrl} size={200} />
              </div>
              <p className="signing-hint">Waiting for signature...</p>
              <button className="btn btn-secondary" onClick={flow.handleBackToChoice}>Back</button>
            </div>
          )}

          {/* Success Step */}
          {step === 'success' && (
            <div className="success-step">
              <div className="success-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" /><path d="M9 12l2 2 4-4" />
                </svg>
              </div>
              <h3>Swap Submitted!</h3>
              <p>Your swap transaction has been submitted to the network.</p>
              {flow.txId && <TxSuccess txId={flow.txId} explorerUrl={explorerUrl} />}
              <button className="btn btn-primary" onClick={() => { onSuccess() }}>Done</button>
            </div>
          )}

          {/* Error Step */}
          {step === 'error' && (
            <div className="error-step">
              <div className="error-icon">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--red-500)" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" /><path d="M15 9l-6 6M9 9l6 6" />
                </svg>
              </div>
              <h3>Smart Swap Failed</h3>
              <p className="error-message">{error}</p>
              <p style={{ fontSize: '0.85rem', color: 'var(--text-muted)', marginTop: 4 }}>
                The pool state may have changed. Try again with a fresh quote.
              </p>
              <div className="button-group">
                <button className="btn btn-secondary" onClick={onClose}>Close</button>
                <button className="btn btn-primary" onClick={() => { setStep('preview'); setError(null); fetchPreview() }}>
                  Try Again
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Wire SmartSwapModal into SmartSwapView**

In `SmartSwapView.tsx`, add the import and render the modal at the bottom of the component's return JSX, right after the swap button:

```tsx
import { SmartSwapModal } from './SmartSwapModal'

// ... inside the return, after the swap button:
{showSwapModal && selectedRoute && walletAddress && (
  <SmartSwapModal
    isOpen={showSwapModal}
    onClose={() => setShowSwapModal(false)}
    routeQuote={selectedRoute}
    sourceAmount={rawInput}
    slippage={slippage}
    walletAddress={walletAddress}
    explorerUrl={explorerUrl}
    onSuccess={() => setShowSwapModal(false)}
  />
)}
```

- [ ] **Step 3: No new CSS needed**

The modal reuses existing CSS classes from `SwapModal` (`modal-overlay`, `modal`, `modal-header`, `close-btn`, `preview-section`, `preview-row`, `fee-breakdown`, `fee-row`, `button-group`, `mint-signing-step`, `wallet-options`, `wallet-option`, `qr-container`, `success-step`, `error-step`). No new CSS file changes required for this task.

- [ ] **Step 4: Verify build**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/SmartSwapModal.tsx frontend/src/components/SmartSwapView.tsx frontend/src/components/SmartSwap.css
git commit -m "feat: add SmartSwapModal for 1-hop direct swap execution"
```

---

## Task 7: Integrate Smart Swap into SwapTab

**Files:**
- Modify: `frontend/src/components/SwapTab.tsx`

Add the mode toggle and render SmartSwapView as the default mode.

- [ ] **Step 1: Add mode state and toggle**

At the top of the `SwapTab` component function (after the existing state declarations, around line 198), add:

```tsx
const [tabMode, setTabMode] = useState<'smart' | 'pool'>('smart')
```

- [ ] **Step 2: Add import for SmartSwapView**

Add to the imports at the top of `SwapTab.tsx`:

```tsx
import { SmartSwapView } from './SmartSwapView'
import './SmartSwap.css'
```

- [ ] **Step 3: Wrap the existing JSX with mode toggle**

Find the outermost `<div className="swap-tab">` in the component's return (around line 724/733). Wrap the existing content with the mode toggle and conditional rendering:

```tsx
return (
  <div className="swap-tab">
    {/* Mode toggle */}
    <div className="smart-swap-mode-toggle">
      <button
        className={`smart-swap-mode-btn ${tabMode === 'smart' ? 'active' : ''}`}
        onClick={() => setTabMode('smart')}
      >
        Smart Swap
      </button>
      <button
        className={`smart-swap-mode-btn ${tabMode === 'pool' ? 'active' : ''}`}
        onClick={() => setTabMode('pool')}
      >
        Pool Swap
      </button>
    </div>

    {tabMode === 'smart' ? (
      <SmartSwapView
        isConnected={isConnected}
        walletAddress={walletAddress}
        walletBalance={walletBalance}
        explorerUrl={explorerUrl}
        pools={pools}
      />
    ) : (
      <>
        {/* === existing Pool Swap JSX unchanged (everything that was here before) === */}
      </>
    )}
  </div>
)
```

The existing pool swap JSX (everything between the old `<div className="swap-tab">` and its closing `</div>`) goes inside the `tabMode === 'pool'` branch, unchanged.

**Important**: The `pools` state variable and `fetchPools` effect already exist in SwapTab — SmartSwapView receives the same pool data for token list derivation.

- [ ] **Step 4: Verify build**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 5: Manual smoke test**

Run: `cd frontend && npm run dev`

1. Open the app, navigate to DEX/Swap
2. Verify "Smart Swap" and "Pool Swap" toggle is visible, Smart is default
3. Verify Pool Swap mode shows the existing pool swap UI unchanged
4. In Smart Swap mode: verify source token selector shows wallet tokens (or is disabled if no wallet)
5. In Smart Swap mode: verify target token selector shows all pool tokens
6. Enter an amount, verify routes are fetched and displayed
7. Click a 1-hop N2T route, verify "Swap" button is enabled
8. Click a multi-hop route, verify button shows "Multi-hop execution coming soon"

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/SwapTab.tsx
git commit -m "feat: integrate smart swap as default mode in SwapTab"
```

---

## Task 8: Polish and Edge Cases

**Files:**
- Modify: `frontend/src/components/SmartSwapView.tsx`
- Modify: `frontend/src/components/SmartSwap.css`

Handle remaining edge cases from the spec.

- [ ] **Step 1: Prevent same-token selection**

In `SmartSwapView.tsx`, filter the target token list to exclude the currently selected source token, and vice versa:

```tsx
const filteredTargetTokens = useMemo(() =>
  targetTokens.filter(t => t.token_id !== sourceToken?.token_id),
  [targetTokens, sourceToken]
)

const filteredSourceTokens = useMemo(() =>
  sourceTokens.filter(t => t.token_id !== targetToken?.token_id),
  [sourceTokens, targetToken]
)
```

Use `filteredSourceTokens` and `filteredTargetTokens` in the TokenSelector props instead of `sourceTokens`/`targetTokens`.

- [ ] **Step 2: Clear routes when tokens are cleared or changed to invalid combo**

Add an effect that clears routes when source or target becomes null:

```tsx
useEffect(() => {
  if (!sourceToken || !targetToken) {
    setRoutes([])
    setSplit(null)
  }
}, [sourceToken, targetToken])
```

- [ ] **Step 3: Add timeout to route finding**

Wrap the `findSwapRoutes` call with a 10-second timeout using `Promise.race`:

```tsx
const routePromise = findSwapRoutes(sourceToken.token_id, targetToken.token_id, rawInput, 4, 5, slippage)
const timeoutPromise = new Promise<never>((_, reject) => setTimeout(() => reject(new Error('Route finding timed out')), 10000))
const response = await Promise.race([routePromise, timeoutPromise])
```

- [ ] **Step 4: Verify build and test**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors

Run: `cd frontend && npm run dev`
Test: select same token as source and target — verify it's not possible.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/SmartSwapView.tsx frontend/src/components/SmartSwap.css
git commit -m "fix: smart swap edge cases — same-token, timeout, clear on change"
```

---

## Summary

| Task | What | Files |
|------|------|-------|
| 1 | Extract token icons to shared module | `tokenIcons.tsx`, `SwapTab.tsx` |
| 2 | Build TokenSelector component | `TokenSelector.tsx` |
| 3 | Build RouteCard + RouteList | `RouteCard.tsx`, `RouteList.tsx` |
| 4 | Build SmartSwapView | `SmartSwapView.tsx` |
| 5 | Add CSS styles | `SmartSwap.css` |
| 6 | Build SmartSwapModal for execution | `SmartSwapModal.tsx` |
| 7 | Integrate into SwapTab | `SwapTab.tsx` |
| 8 | Polish edge cases | `SmartSwapView.tsx` |

**Out of scope (Phase 2):** Multi-hop chained tx execution, T2T direct swap, split route execution.
