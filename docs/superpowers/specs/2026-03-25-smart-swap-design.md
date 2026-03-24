# Smart Swap — Default Routing in SwapTab

**Date:** 2026-03-25
**Status:** Approved

## Overview

Replace the pool-first swap flow in SwapTab with a token-to-token smart routing view as the default experience. The user selects a source token (from wallet), a target token (from all known tokens), and an amount — the router automatically finds and ranks the best paths including multi-hop routes and split optimization.

The existing single-pool swap flow ("Pool Swap") remains accessible via a mode toggle.

## Motivation

Currently, users must manually browse pools to find the best rate, and have no visibility into multi-hop paths that could offer better output. The router engine (`router.rs`) already supports BFS pathfinding, split optimization, and depth analysis — but the UI doesn't surface this as the default swap experience.

## Design Decisions

1. **Smart Swap is the default mode** in SwapTab; Pool Swap is accessible via a segmented control toggle
2. **Source tokens**: wallet-held only (you can only sell what you have)
3. **Target tokens**: all tokens from pool graph, wallet-held pinned to top, searchable
4. **Auto-routing**: debounced at 500ms on amount/token change, calls `findSwapRoutes()`
5. **Route display**: best route shown prominently, alternatives collapsed, split suggestion conditional
6. **Per-hop detail**: expandable, minimal by default
7. **Execution**: 1-hop N2T direct swap on day 1; multi-hop and T2T are phase 2
8. **Multi-hop execution strategy**: build all txs upfront using predicted intermediate UTXOs, sign all, submit in sequence

## SwapTab Layout

```
┌─────────────────────────────────────────┐
│  [ Smart Swap ]  [ Pool Swap ]          │  ← segmented control, Smart is default
├─────────────────────────────────────────┤
│                                         │
│  Source: [ SigRSV ▾ ]    Bal: 1,500     │  ← wallet tokens only
│  Amount: [ 500        ]  [Max]  ⚙       │  ← gear icon for slippage settings
│                                         │
│  Target: [ SigUSD ▾ ]                   │  ← all known tokens, wallet pinned
│                                         │
├─────────────────────────────────────────┤
│                                         │
│  ┌─ Best Route ───────────────────────┐ │
│  │ SigRSV → ERG → SigUSD    2 hops   │ │
│  │                                     │ │
│  │ Output: 1.62 SigUSD                │ │
│  │ Rate: 1 SigRSV = 0.00324 SigUSD   │ │
│  │ Impact: 0.3%  Fees: 0.002 ERG     │ │
│  │                                     │ │
│  │ ▸ Route details                    │ │  ← expandable per-hop breakdown
│  └─────────────────────────────────────┘ │
│                                         │
│  ▸ 3 other routes available             │  ← collapsed alternatives
│                                         │
│  ┌─ Split suggestion ────────────────┐  │  ← only if >0.5% improvement
│  │ Split across 2 routes: +0.8%      │  │
│  │ [ Use split ]                     │  │
│  └────────────────────────────────────┘  │
│                                         │
│  [ Swap ]                               │  ← see Execution section for states
│                                         │
└─────────────────────────────────────────┘
```

## Token Selectors

### Source Token Selector
- Shows only tokens held in the connected wallet
- Each entry: token name, balance amount, token icon (reuse existing `TOKEN_ICON_MAP`/`TokenIcon` from SwapTab)
- **ERG is a synthetic entry** — sourced from `walletBalance.erg_nano`, not the `tokens` array. For routing purposes, ERG uses the sentinel token ID `"ERG"` that the router engine already accepts.
- ERG always first, remaining tokens sorted alphabetically
- Selecting a token shows balance next to the amount input
- "Max" button fills wallet balance (minus 0.01 ERG reserve if source is ERG)

### Target Token Selector
- Shows all tokens present in the pool graph (any token in at least one pool)
- **Token list derived from pool data**: the frontend already fetches all pools via `getAmmPools()` with 30s refresh. Deduplicate all `token_y` (and `token_x` for T2T pools) into a token list with name/decimals. No new backend command needed.
- Wallet-held tokens pinned to top section with balance displayed
- Remaining tokens below, sorted alphabetically
- Searchable by token name or token ID (partial match)
- ERG available as a target

## Slippage Configuration

- Gear icon (⚙) next to the amount input opens a slippage popover
- Default: 0.5% (same as current SwapTab)
- Adjustable: preset buttons (0.1%, 0.5%, 1.0%) + custom input
- Slippage value passed to `findSwapRoutes()` and used for `min_output` calculation in route quotes

## Route Finding Triggers

Route finding (`findSwapRoutes`) is triggered when all three inputs are set (source token, target token, amount > 0):
- **Amount change**: debounced at 500ms (user is typing)
- **Token change** (source or target): immediate (deliberate user action, no debounce)
- **Stale route behavior**: previous routes remain visible (dimmed) while new routes load, with a small spinner overlay. Replaced once new results arrive.

## Route Display

### Best Route Card (always visible when routes exist)
- **Path visualization**: token names connected by arrows (e.g. `SigRSV → ERG → SigUSD`)
- **Output amount**: large, prominent text
- **Effective rate**: e.g. "1 SigRSV = 0.00324 SigUSD"
- **Total price impact %**: color-coded (green <1%, yellow 1-5%, red >5%)
- **Total fees**: summarized
- **Hop count badge**: e.g. "2 hops"

### Expandable Per-Hop Detail
Clicking "Route details" expands to show each hop:
- Pool ID (truncated)
- Input amount → Output amount
- Price impact for this hop
- Fee amount
- Pool reserves (in/out)

### Alternative Routes (collapsed)
- "N other routes available" toggle
- Each alternative as a compact row: path label, output amount, price impact
- Clicking one promotes it to selected route

### Split Route Suggestion (conditional)
- The `find_swap_routes` response already includes `split: SplitRouteDetail | null` — no separate API call needed
- Only shown when split data is present (router already enforces >0.5% improvement threshold)
- Callout: "Split across N routes for +X% better output"
- Toggle/button to select the split route
- Expands to show per-route allocation (route path + fraction + amounts)

## No-Route / Edge States

- **No wallet connected**: "Connect wallet to select source tokens" with source selector disabled
- **No route found**: "No route available between these tokens"
- **High price impact**: route displayed normally but impact shown in red (>5%) or yellow (>1%)
- **Amount exceeds balance**: "Insufficient balance" warning, Swap button disabled
- **Loading**: spinner overlay on route area, previous routes dimmed underneath
- **Network/node error**: "Failed to fetch routes — check node connection" with retry button
- **Backend timeout**: same as network error treatment, with a 10s timeout on the invoke call

## Execution

### Route-to-Swap Parameter Mapping

When executing a 1-hop route via `buildDirectSwapTx`, map from the `RouteHop`:
- `pool_id` → `RouteHop.pool_id`
- `input_type` → `"erg"` if `RouteHop.token_in == "ERG"`, else `"token"`
- `token_id` → `RouteHop.token_in` (only when `input_type == "token"`)
- `amount` → source amount entered by user (raw nanoERG or raw token units)
- `min_output` → `RouteHop.output_amount` adjusted by slippage (from route quote's `min_output`)
- User UTXOs sourced from wallet state as in current direct swap flow

### Phase 1: Single-Hop N2T (Day 1)
- If selected route is 1 hop **and** the pool is N2T type, the "Swap" button triggers `buildDirectSwapTx` → `useTransactionFlow` signing
- **T2T 1-hop routes**: displayed and quoted but Swap button shows "T2T direct swap not yet supported" — T2T direct swap (`direct_swap.rs`) is not implemented per project memory
- **Multi-hop routes**: Swap button shows "Multi-hop execution coming soon"

### Phase 2: Multi-Hop Execution
- **Strategy**: Build all transactions upfront using predicted intermediate UTXOs
- Each tx in the chain spends the output box of the previous tx (box ID = `blake2b256(tx_bytes || output_index)`, deterministic from full serialized tx)
- All txs presented to user for sequential signing
- Submit in order after all are signed
- If any tx fails on-chain, subsequent txs are automatically invalid (no partial execution beyond the failed tx)

### Multi-Hop Investigation Items
- Verify pool contracts allow spending in a chained tx context
- Confirm box ID prediction works correctly with ergo-lib serialization
- Determine if ErgoPay can handle sequential signing of multiple txs
- **Pool state staleness** (key risk): if pool state changes between tx building and submission (even one block), the first tx's outputs change and invalidate all subsequent predicted box IDs. Mitigations to investigate: per-hop slippage margins, mempool awareness, or building from latest state just before signing
- T2T direct swap support (prerequisite for T2T routes in any phase)

## Backend Changes

### Existing (no changes needed)
- `router.rs`: `build_pool_graph()`, `find_best_routes()`, `quote_route()`, `optimize_split_detailed()`, `calculate_all_depth_tiers()`
- `find_swap_routes` Tauri command: already returns routes, depth tiers, and split optimization. **Note**: this command fetches pools internally via `discover_pools` — the frontend does not need to pass pool data.
- `buildDirectSwapTx`: works for 1-hop N2T execution
- Pool fetching with 30s auto-refresh (frontend uses this for token list derivation)
- Wallet balance fetching

### No New Commands for Phase 1
- Use `find_swap_routes` directly from frontend with defaults: `max_hops=4`, `max_routes=5`, slippage from user setting
- `executable` flag computed on frontend: `route.hops.length === 1 && route.hops[0].pool_type === "N2T"`

### Phase 2 New Command
- `build_chained_swap_txs(route, input_amount, slippage_pct, wallet_address)` — builds array of unsigned txs for sequential signing

## Frontend Component Structure

### Modified Files
- **`SwapTab.tsx`** — add mode toggle (Smart Swap / Pool Swap), render `SmartSwapView` as default, existing pool swap flow as Pool Swap mode

### New Components
- **`SmartSwapView.tsx`** — main smart swap UI: token selectors, amount input, route results area, swap button
- **`TokenSelector.tsx`** — reusable dropdown for source (wallet-only) and target (all tokens) selection with search, balance display, pinning
- **`RouteCard.tsx`** — single route display: path visualization, output, impact, fees, expandable per-hop detail
- **`RouteList.tsx`** — best route card + collapsed alternatives + split suggestion callout

### Reused Existing Code
- `useTransactionFlow` hook (signing)
- `TOKEN_ICON_MAP` / `TokenIcon` — extract from SwapTab into shared location for reuse
- Pool fetching + 30s auto-refresh (from SwapTab, for token list derivation)
- Formatting utilities from `frontend/src/utils/`
- CSS variables and dark theme patterns
- `frontend/src/api/router.ts` invoke wrappers

### New CSS
- `SmartSwap.css` — styles for SmartSwapView, TokenSelector, RouteCard, RouteList (following project convention of component-level CSS files)

### Data Flow
```
SmartSwapView
  ├─ TokenSelector (source: wallet tokens only, ERG synthetic entry)
  ├─ Amount input (debounce 500ms) + slippage gear
  ├─ TokenSelector (target: all pool tokens, derived from getAmmPools)
  ├─ → findSwapRoutes(source, target, amount, slippage)
  │     (pools fetched internally by backend, no duplication)
  ├─ RouteList
  │   ├─ RouteCard (best route, selected by default)
  │   ├─ RouteCard[] (alternatives, collapsed)
  │   └─ SplitSuggestion (from response.split, if present)
  └─ Swap button
      ├─ 1-hop N2T → buildDirectSwapTx → useTransactionFlow
      ├─ 1-hop T2T → disabled ("T2T not yet supported")
      └─ multi-hop → disabled ("coming soon")
```

## Phases

### Phase 1: Smart Swap UI
- Mode toggle in SwapTab
- SmartSwapView with token selectors and auto-routing
- Route display (best + alternatives + split suggestion)
- 1-hop N2T direct swap execution
- All no-route/edge states handled
- Slippage configuration

### Phase 2: Multi-Hop Execution
- Investigate chained tx building with predicted box IDs
- Investigate pool state staleness mitigations
- Implement `build_chained_swap_txs` backend command
- Sequential signing flow in frontend
- Enable Swap button for multi-hop routes
- T2T direct swap support (may be a prerequisite or parallel effort)
