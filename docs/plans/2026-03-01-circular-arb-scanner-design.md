# Circular ARB Scanner Design

## Goal

Detect profitable circular arbitrage opportunities across Spectrum AMM pools. A circular arb is a loop where ERG routes through intermediate tokens and returns to ERG with a net gain: ERG → Token A → Token B → ERG where output > input.

Separate sidebar tab ("Arb Scanner"), view-only with manual refresh.

## Architecture

### Rust: Cycle Finding + Profit Optimization

**Location**: `crates/protocols/amm/src/router.rs`

#### Types

```rust
pub struct CircularArb {
    pub path_label: String,        // "ERG → GORT → SigUSD → ERG"
    pub hops: usize,
    pub pool_ids: Vec<String>,
    pub optimal_input_nano: u64,   // ERG to put in
    pub output_nano: u64,          // ERG returned
    pub gross_profit_nano: i64,    // output - input
    pub tx_fee_nano: u64,          // 0.001 ERG * hops
    pub net_profit_nano: i64,      // gross - tx_fee
    pub profit_pct: f64,           // net / input * 100
    pub price_impact: f64,         // at optimal input
}

pub struct CircularArbSnapshot {
    pub windows: Vec<CircularArb>,
    pub total_net_profit_nano: i64,
    pub scan_time_ms: u64,
}
```

#### Cycle Finding: `find_cycles(graph, max_hops) -> Vec<Vec<PoolEdge>>`

1. DFS from ERG_TOKEN_ID through adjacency graph
2. At each hop, follow all edges from current token
3. Record cycle when path returns to ERG with >= 2 hops
4. Don't revisit same token within a cycle (prevents A→B→A→B loops)
5. Max depth: 4 hops (configurable)

#### Profit Optimization: `find_circular_arbs(graph, max_hops, min_profit_nano) -> CircularArbSnapshot`

For each cycle:
1. Quote the cycle as a route (reuse `quote_route`)
2. Ternary search for profit-maximizing input:
   - Profit function: `f(x) = quote_cycle(x) - x` (output ERG - input ERG)
   - This is unimodal: profit rises then falls as price impact grows
   - Search bounds: lo = 0.01 ERG, hi = min(first pool reserves_in, 1000 ERG)
   - ~50 iterations for 0.001 ERG precision
3. Compute: gross profit, tx fees (0.001 ERG * hops), net profit, profit %
4. Filter: only keep cycles where net_profit_nano > min_profit_nano
5. Sort by net profit descending
6. Record scan time

### Tauri Command

**File**: `app/src/commands/amm.rs`

```rust
#[tauri::command]
pub async fn scan_circular_arbs(
    state: State<'_, AppState>,
    max_hops: Option<usize>,
) -> Result<CircularArbSnapshot, String>
```

- Discover pools, build graph with standard thresholds
- Call `find_circular_arbs(graph, max_hops.unwrap_or(4), 100_000)` (min 0.0001 ERG profit)
- Register in `app/src/lib.rs` invoke handler

### TypeScript API

**File**: `frontend/src/api/arb.ts`

Types mirror Rust structs. Single invoke wrapper: `scanCircularArbs(maxHops?: number) -> CircularArbSnapshot`

### Frontend UI

**File**: `frontend/src/components/ArbScannerTab.tsx`

- Auto-scan on mount, manual refresh button
- State: `snapshot | null`, `loading`, `error`
- Cards sorted by net profit, each showing:
  - Route path + hop count
  - Optimal input in ERG
  - Output in ERG
  - Profit breakdown: Gross / Fees / Net
  - Profit percentage
  - Price impact at optimal
- Empty state: "No profitable arbs found. Check back after large trades."

### Sidebar Entry

**File**: `frontend/src/components/Sidebar.tsx`, `frontend/src/App.tsx`

New "Arb Scanner" entry in Tools section, below Router.

## UI Card Layout

```
┌──────────────────────────────────────────────┐
│ ERG → GORT → SigUSD → ERG          3 hops   │
│                                              │
│ Input: 1.02 ERG        Output: 1.047 ERG     │
│ Gross: +0.027 ERG  Fees: -0.003  Net: +0.024 │
│ Profit: +2.3%          Impact: 8.2%          │
└──────────────────────────────────────────────┘
```

## Constraints

- View-only: no execution (multi-hop chained tx not implemented)
- Manual refresh only (no auto-polling)
- Max 4 hops per cycle
- Min profit threshold: 0.0001 ERG (filters noise)
- Uses standard pool graph thresholds (10 ERG min liquidity, 3 per pair)
