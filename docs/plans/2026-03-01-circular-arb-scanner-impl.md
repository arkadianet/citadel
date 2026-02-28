# Circular Arb Scanner Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Detect profitable circular arbitrage loops (ERG → ... → ERG) across all AMM pools and display them in a dedicated scanner tab.

**Architecture:** Brute-force cycle enumeration on the existing PoolGraph, ternary search for profit-maximizing input per cycle, view-only display with manual refresh.

**Tech Stack:** Rust (amm crate), Tauri IPC, React/TypeScript, custom CSS

---

### Task 1: Add CircularArb types and find_cycles function

**Files:**
- Modify: `crates/protocols/amm/src/router.rs` (add after line ~1151, before Helpers section)

**Step 1: Add types to router.rs**

Insert before the `// Helpers` section (line 1154):

```rust
// ---------------------------------------------------------------------------
// Step 6: Circular Arb Detection
// ---------------------------------------------------------------------------

/// A single profitable circular arbitrage opportunity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircularArb {
    pub path_label: String,
    pub hops: usize,
    pub pool_ids: Vec<String>,
    pub optimal_input_nano: u64,
    pub output_nano: u64,
    pub gross_profit_nano: i64,
    pub tx_fee_nano: u64,
    pub net_profit_nano: i64,
    pub profit_pct: f64,
    pub price_impact: f64,
}

/// Snapshot of all circular arb opportunities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircularArbSnapshot {
    pub windows: Vec<CircularArb>,
    pub total_net_profit_nano: i64,
    pub scan_time_ms: u64,
}
```

**Step 2: Add find_cycles function**

Insert after the types:

```rust
/// Find all cycles from ERG back to ERG up to max_hops.
///
/// Uses DFS. Does not revisit tokens within a cycle (except ERG as the
/// start/end). Does not reuse the same pool within a cycle.
pub fn find_cycles(graph: &PoolGraph, max_hops: usize) -> Vec<Vec<PoolEdge>> {
    let mut results: Vec<Vec<PoolEdge>> = Vec::new();

    // State: (current_token, path, visited_tokens, used_pool_ids)
    type State = (String, Vec<PoolEdge>, HashSet<String>, HashSet<String>);
    let mut stack: Vec<State> = Vec::new();

    let mut initial_visited = HashSet::new();
    initial_visited.insert(ERG_TOKEN_ID.to_string());
    stack.push((
        ERG_TOKEN_ID.to_string(),
        Vec::new(),
        initial_visited,
        HashSet::new(),
    ));

    while let Some((current, path, visited, used_pools)) = stack.pop() {
        if let Some(edges) = graph.adjacency.get(current.as_str()) {
            for edge in edges {
                if used_pools.contains(&edge.pool.pool_id) {
                    continue;
                }

                if edge.token_out == ERG_TOKEN_ID && path.len() >= 1 {
                    // Found a cycle back to ERG with >= 2 hops
                    let mut cycle = path.clone();
                    cycle.push(edge.clone());
                    results.push(cycle);
                } else if path.len() + 1 < max_hops && !visited.contains(&edge.token_out) {
                    let mut new_visited = visited.clone();
                    new_visited.insert(edge.token_out.clone());
                    let mut new_pools = used_pools.clone();
                    new_pools.insert(edge.pool.pool_id.clone());
                    let mut new_path = path.clone();
                    new_path.push(edge.clone());
                    stack.push((edge.token_out.clone(), new_path, new_visited, new_pools));
                }
            }
        }
    }

    results
}
```

**Step 3: Write tests for find_cycles**

Add to the `mod tests` block at the end of router.rs:

```rust
// -- Circular Arb Detection --

#[test]
fn test_find_cycles_triangle() {
    // ERG -> A -> B -> ERG (3 pools forming a triangle)
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "token_a", "TokenA", 50_000, 997),
        make_n2t_pool("p3", 100_000_000_000, "token_b", "TokenB", 50_000, 997),
        make_t2t_pool("p2", "token_a", "TokenA", 50_000, "token_b", "TokenB", 50_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let cycles = find_cycles(&graph, 4);
    // Should find cycles: ERG->A->B->ERG, ERG->B->A->ERG
    assert!(cycles.len() >= 2);
    for cycle in &cycles {
        assert!(cycle.len() >= 2);
        // First hop starts from ERG, last hop ends at ERG
        assert_eq!(cycle.last().unwrap().token_out, ERG_TOKEN_ID);
    }
}

#[test]
fn test_find_cycles_no_loop() {
    // Single N2T pool: ERG -> token. No way back except same pool (which would be
    // a 2-hop cycle ERG->token->ERG). That's a valid cycle.
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "token_a", "TokenA", 50_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let cycles = find_cycles(&graph, 4);
    // ERG -> token_a -> ERG via same pool? No — used_pools prevents reusing p1.
    // ERG -> token_a has only p1. token_a -> ERG has only p1 (reverse edge).
    // But the pool_id is the same for both directions, so it gets filtered.
    // Wait — N2T pools create TWO edges with the same pool_id. So used_pools
    // will block the return hop. No cycle found.
    assert_eq!(cycles.len(), 0);
}

#[test]
fn test_find_cycles_respects_max_hops() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "a", "A", 50_000, 997),
        make_n2t_pool("p2", 100_000_000_000, "b", "B", 50_000, 997),
        make_t2t_pool("p3", "a", "A", 50_000, "b", "B", 50_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    // max_hops=2: must find 2-hop cycles only (ERG->A->ERG needs different pools)
    let cycles_2 = find_cycles(&graph, 2);
    for c in &cycles_2 {
        assert!(c.len() <= 2);
    }
    // max_hops=4: should find 3-hop cycles too
    let cycles_4 = find_cycles(&graph, 4);
    assert!(cycles_4.len() >= cycles_2.len());
}
```

**Step 4: Run tests**

Run: `cargo test --package amm -- test_find_cycles`
Expected: All 3 tests pass.

**Step 5: Commit**

```bash
git add crates/protocols/amm/src/router.rs
git commit -m "feat: add CircularArb types and find_cycles function"
```

---

### Task 2: Add find_circular_arbs with ternary search profit optimization

**Files:**
- Modify: `crates/protocols/amm/src/router.rs`

**Step 1: Add the profit optimization function**

Insert after `find_cycles` in router.rs:

```rust
/// Scan all ERG→...→ERG cycles and find profitable arb opportunities.
///
/// Uses ternary search to find the input that maximizes profit for each cycle.
/// Profit = output_erg - input_erg. The function is unimodal (rises as you
/// capture the arb, then falls as price impact dominates).
pub fn find_circular_arbs(
    graph: &PoolGraph,
    max_hops: usize,
    min_profit_nano: i64,
) -> CircularArbSnapshot {
    let start = std::time::Instant::now();
    let cycles = find_cycles(graph, max_hops);

    let tx_fee_per_hop: u64 = 1_000_000; // 0.001 ERG

    let mut windows: Vec<CircularArb> = Vec::new();

    for cycle in &cycles {
        if cycle.is_empty() {
            continue;
        }

        // Upper bound: min of first pool reserves_in and 1000 ERG
        let hi_cap = cycle[0].reserves_in.min(1_000_000_000_000); // 1000 ERG
        let lo: u64 = 10_000_000; // 0.01 ERG

        if hi_cap <= lo {
            continue;
        }

        // Ternary search for profit-maximizing input.
        // Profit(x) = quote_route(cycle, x).total_output - x
        // This is unimodal: increases then decreases.
        let mut a = lo;
        let mut b = hi_cap;

        for _ in 0..80 {
            if b - a < 1_000_000 {
                // 0.001 ERG precision
                break;
            }
            let m1 = a + (b - a) / 3;
            let m2 = b - (b - a) / 3;

            let p1 = quote_route(cycle, m1)
                .map(|r| r.total_output as i64 - m1 as i64)
                .unwrap_or(i64::MIN);
            let p2 = quote_route(cycle, m2)
                .map(|r| r.total_output as i64 - m2 as i64)
                .unwrap_or(i64::MIN);

            if p1 < p2 {
                a = m1;
            } else {
                b = m2;
            }
        }

        // Evaluate at the converged optimal point
        let optimal_input = (a + b) / 2;
        let route = match quote_route(cycle, optimal_input) {
            Some(r) => r,
            None => continue,
        };

        let output = route.total_output;
        let gross_profit = output as i64 - optimal_input as i64;
        let hops = cycle.len();
        let tx_fee = tx_fee_per_hop * hops as u64;
        let net_profit = gross_profit - tx_fee as i64;

        if net_profit < min_profit_nano {
            continue;
        }

        let profit_pct = if optimal_input > 0 {
            net_profit as f64 / optimal_input as f64 * 100.0
        } else {
            0.0
        };

        // Build path label
        let mut label_parts: Vec<String> = vec!["ERG".to_string()];
        for edge in cycle {
            let name = resolve_token_name(&edge.pool, &edge.token_out)
                .unwrap_or_else(|| edge.token_out[..6.min(edge.token_out.len())].to_string());
            label_parts.push(name);
        }
        let path_label = label_parts.join(" → ");

        let pool_ids: Vec<String> = cycle.iter().map(|e| e.pool.pool_id.clone()).collect();

        windows.push(CircularArb {
            path_label,
            hops,
            pool_ids,
            optimal_input_nano: optimal_input,
            output_nano: output,
            gross_profit_nano: gross_profit,
            tx_fee_nano: tx_fee,
            net_profit_nano: net_profit,
            profit_pct,
            price_impact: route.total_price_impact,
        });
    }

    // Sort by net profit descending
    windows.sort_by(|a, b| b.net_profit_nano.cmp(&a.net_profit_nano));

    let total_net = windows.iter().map(|w| w.net_profit_nano).sum();
    let elapsed = start.elapsed().as_millis() as u64;

    CircularArbSnapshot {
        windows,
        total_net_profit_nano: total_net,
        scan_time_ms: elapsed,
    }
}
```

**Step 2: Write tests**

Add to `mod tests`:

```rust
#[test]
fn test_circular_arb_profitable() {
    // Create a triangle with mispriced pools:
    // ERG->A pool: 100 ERG, 200 A (rate: 2 A per ERG)
    // A->B pool: 200 A, 100 B (rate: 0.5 B per A)
    // B->ERG pool: 50 B, 200 ERG (rate: 4 ERG per B)
    // Cycle: 1 ERG -> ~2 A -> ~1 B -> ~4 ERG = profit!
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "aa", "TokenA", 200_000, 997),
        make_n2t_pool("p3", 200_000_000_000, "bb", "TokenB", 50_000, 997),
        make_t2t_pool("p2", "aa", "TokenA", 200_000, "bb", "TokenB", 100_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let snap = find_circular_arbs(&graph, 4, 0);
    // Should find at least one profitable cycle
    assert!(!snap.windows.is_empty(), "Should find profitable arbs");
    let best = &snap.windows[0];
    assert!(best.net_profit_nano > 0, "Best arb should be profitable");
    assert!(best.optimal_input_nano > 0);
    assert!(best.output_nano > best.optimal_input_nano);
}

#[test]
fn test_circular_arb_no_opportunity() {
    // Two pools with same rate: ERG/A at 100:50, ERG/B at 100:50
    // A/B pool at 50:50 (fair). No arb possible.
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "aa", "A", 50_000, 997),
        make_n2t_pool("p2", 100_000_000_000, "bb", "B", 50_000, 997),
        make_t2t_pool("p3", "aa", "A", 50_000, "bb", "B", 50_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    // With reasonable min_profit, no arb should be found
    // (fees eat any tiny rounding gains)
    let snap = find_circular_arbs(&graph, 4, 1_000_000);
    assert!(snap.windows.is_empty(), "Balanced pools should have no arb");
}

#[test]
fn test_circular_arb_tx_fees_deducted() {
    let pools = vec![
        make_n2t_pool("p1", 100_000_000_000, "aa", "TokenA", 200_000, 997),
        make_n2t_pool("p3", 200_000_000_000, "bb", "TokenB", 50_000, 997),
        make_t2t_pool("p2", "aa", "TokenA", 200_000, "bb", "TokenB", 100_000, 997),
    ];
    let graph = build_pool_graph(&pools, 0);
    let snap = find_circular_arbs(&graph, 4, 0);
    for arb in &snap.windows {
        assert_eq!(arb.tx_fee_nano, 1_000_000 * arb.hops as u64);
        assert_eq!(arb.net_profit_nano, arb.gross_profit_nano - arb.tx_fee_nano as i64);
    }
}
```

**Step 3: Run tests**

Run: `cargo test --package amm -- test_circular_arb`
Expected: All 3 tests pass.

**Step 4: Commit**

```bash
git add crates/protocols/amm/src/router.rs
git commit -m "feat: add find_circular_arbs with ternary search profit optimization"
```

---

### Task 3: Export types and add Tauri command

**Files:**
- Modify: `crates/protocols/amm/src/lib.rs`
- Modify: `app/src/commands/amm.rs`
- Modify: `app/src/lib.rs`

**Step 1: Add exports to lib.rs**

In `crates/protocols/amm/src/lib.rs`, add to the `pub use router::{...}` block:

```rust
    find_circular_arbs, find_cycles, CircularArb, CircularArbSnapshot,
```

**Step 2: Add Tauri command**

In `app/src/commands/amm.rs`, add at the end (before the closing of the file):

```rust
/// Scan for profitable circular arbitrage loops (ERG → ... → ERG).
///
/// Returns all cycles where the output ERG exceeds input + tx fees.
#[tauri::command]
pub async fn scan_circular_arbs(
    state: State<'_, AppState>,
    max_hops: Option<usize>,
) -> Result<amm::CircularArbSnapshot, String> {
    let client = state.node_client().await.ok_or("Node not connected")?;
    let pools = amm::discover_pools(&client)
        .await
        .map_err(|e| e.to_string())?;

    let graph = amm::build_pool_graph(&pools, amm::DEFAULT_MIN_LIQUIDITY_NANO);
    let max_hops = max_hops.unwrap_or(4);
    // Min profit: 0.0001 ERG = 100_000 nanoERG
    Ok(amm::find_circular_arbs(&graph, max_hops, 100_000))
}
```

**Step 3: Register in invoke handler**

In `app/src/lib.rs`, add `commands::scan_circular_arbs` after `commands::get_sigusd_arb_snapshot` in the invoke_handler list.

**Step 4: Verify build**

Run: `cargo build`
Expected: Compiles with no errors.

Run: `cargo clippy --workspace --all-targets`
Expected: Zero warnings (ignore the ergo-rest patch warning).

**Step 5: Commit**

```bash
git add crates/protocols/amm/src/lib.rs app/src/commands/amm.rs app/src/lib.rs
git commit -m "feat: add scan_circular_arbs Tauri command"
```

---

### Task 4: TypeScript API layer

**Files:**
- Create: `frontend/src/api/arb.ts`

**Step 1: Create the API file**

```typescript
/**
 * Circular Arb Scanner API
 *
 * TypeScript types and invoke wrappers for circular arbitrage detection.
 */

import { invoke } from '@tauri-apps/api/core'

export interface CircularArb {
  path_label: string
  hops: number
  pool_ids: string[]
  optimal_input_nano: number
  output_nano: number
  gross_profit_nano: number
  tx_fee_nano: number
  net_profit_nano: number
  profit_pct: number
  price_impact: number
}

export interface CircularArbSnapshot {
  windows: CircularArb[]
  total_net_profit_nano: number
  scan_time_ms: number
}

/**
 * Scan for profitable circular arb loops (ERG → ... → ERG).
 */
export async function scanCircularArbs(
  maxHops?: number,
): Promise<CircularArbSnapshot> {
  return await invoke<CircularArbSnapshot>('scan_circular_arbs', {
    maxHops,
  })
}
```

**Step 2: Verify TypeScript compiles**

Run: `cd frontend && npx tsc --noEmit`
Expected: No errors.

**Step 3: Commit**

```bash
git add frontend/src/api/arb.ts
git commit -m "feat: add circular arb scanner TypeScript API"
```

---

### Task 5: Frontend UI — ArbScannerTab component

**Files:**
- Create: `frontend/src/components/ArbScannerTab.tsx`
- Create: `frontend/src/components/ArbScannerTab.css`

**Step 1: Create ArbScannerTab.tsx**

Follow the pattern from `RouterTab.tsx`. Key elements:
- `scanCircularArbs()` called on mount
- Manual refresh button
- Loading spinner
- Card list sorted by net profit
- Each card: path label, hops, input/output, gross/fees/net breakdown, profit %, impact
- Empty state message
- `formatErg` helper (same as RouterTab)

```typescript
import { useState, useEffect, useCallback } from 'react'
import { scanCircularArbs, CircularArbSnapshot, CircularArb } from '../api/arb'
import './ArbScannerTab.css'

interface ArbScannerTabProps {
  walletAddress: string | null
}

function formatErg(nano: number): string {
  return (nano / 1e9).toLocaleString(undefined, {
    minimumFractionDigits: 4,
    maximumFractionDigits: 4,
  })
}

function formatErgSigned(nano: number): string {
  const prefix = nano >= 0 ? '+' : ''
  return prefix + formatErg(Math.abs(nano))
}

function impactClass(impact: number): string {
  if (impact < 3) return 'impact-low'
  if (impact < 10) return 'impact-medium'
  return 'impact-high'
}

export function ArbScannerTab({ walletAddress }: ArbScannerTabProps) {
  const [snapshot, setSnapshot] = useState<CircularArbSnapshot | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const doScan = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const result = await scanCircularArbs(4)
      setSnapshot(result)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    doScan()
  }, [doScan])

  return (
    <div className="arb-scanner-tab">
      <div className="arb-scanner-header">
        <div>
          <h2>Arb Scanner</h2>
          <p className="arb-scanner-desc">
            Scan for circular arbitrage opportunities across all DEX pools
          </p>
        </div>
        <button
          className="arb-scanner-refresh"
          onClick={doScan}
          disabled={loading}
        >
          {loading ? 'Scanning...' : 'Refresh'}
        </button>
      </div>

      {error && <div className="message error">{error}</div>}

      {loading && !snapshot && (
        <div className="empty-state">
          <div className="spinner" />
          <p>Scanning pools for arb opportunities...</p>
        </div>
      )}

      {snapshot && (
        <>
          {snapshot.windows.length > 0 ? (
            <>
              <div className="arb-scanner-summary">
                <span className="arb-scanner-count">
                  {snapshot.windows.length} opportunit{snapshot.windows.length === 1 ? 'y' : 'ies'} found
                </span>
                <span className="arb-scanner-total">
                  Total net profit: {formatErgSigned(snapshot.total_net_profit_nano)} ERG
                </span>
                <span className="arb-scanner-time">
                  Scanned in {snapshot.scan_time_ms}ms
                </span>
              </div>

              <div className="arb-scanner-cards">
                {snapshot.windows.map((arb, idx) => (
                  <ArbCard key={idx} arb={arb} />
                ))}
              </div>
            </>
          ) : (
            <div className="arb-scanner-empty">
              <p>No profitable arbs found.</p>
              <p className="arb-scanner-empty-hint">
                Circular arbs appear when pool prices diverge from each other.
                Check back after large trades move prices.
              </p>
            </div>
          )}
        </>
      )}
    </div>
  )
}

function ArbCard({ arb }: { arb: CircularArb }) {
  return (
    <div className="arb-card">
      <div className="arb-card-header">
        <span className="arb-card-path">{arb.path_label}</span>
        <span className="arb-card-hops">{arb.hops} hop{arb.hops > 1 ? 's' : ''}</span>
        <span className="arb-card-profit-badge">
          {arb.profit_pct >= 0 ? '+' : ''}{arb.profit_pct.toFixed(2)}%
        </span>
      </div>

      <div className="arb-card-amounts">
        <div className="arb-card-amount">
          <span className="arb-card-label">Input</span>
          <span className="arb-card-value">{formatErg(arb.optimal_input_nano)} ERG</span>
        </div>
        <div className="arb-card-amount">
          <span className="arb-card-label">Output</span>
          <span className="arb-card-value">{formatErg(arb.output_nano)} ERG</span>
        </div>
      </div>

      <div className="arb-card-breakdown">
        <div className="arb-card-detail">
          <span className="arb-card-label">Gross</span>
          <span className="arb-card-value profit">{formatErgSigned(arb.gross_profit_nano)} ERG</span>
        </div>
        <div className="arb-card-detail">
          <span className="arb-card-label">Fees</span>
          <span className="arb-card-value fee">-{formatErg(arb.tx_fee_nano)} ERG</span>
        </div>
        <div className="arb-card-detail">
          <span className="arb-card-label">Net</span>
          <span className="arb-card-value net">{formatErgSigned(arb.net_profit_nano)} ERG</span>
        </div>
      </div>

      <div className="arb-card-footer">
        <span className={`arb-card-impact ${impactClass(arb.price_impact)}`}>
          Impact: {arb.price_impact.toFixed(1)}%
        </span>
      </div>
    </div>
  )
}
```

**Step 2: Create ArbScannerTab.css**

Follow the existing project CSS patterns (dark theme, CSS variables, card-based layout):

```css
/* ============================================================
   Arb Scanner Tab
   ============================================================ */

.arb-scanner-tab {
  padding: 1.5rem;
  max-width: 900px;
}

.arb-scanner-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 1.5rem;
}

.arb-scanner-header h2 {
  margin: 0 0 0.25rem 0;
  font-size: 1.25rem;
}

.arb-scanner-desc {
  margin: 0;
  color: var(--slate-400);
  font-size: 0.85rem;
}

.arb-scanner-refresh {
  padding: 0.5rem 1rem;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: var(--card-bg);
  color: var(--slate-200);
  cursor: pointer;
  font-size: 0.85rem;
  transition: border-color 0.15s;
}

.arb-scanner-refresh:hover:not(:disabled) {
  border-color: var(--emerald-400);
}

.arb-scanner-refresh:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Summary bar */
.arb-scanner-summary {
  display: flex;
  gap: 1.5rem;
  align-items: center;
  padding: 0.75rem 1rem;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  margin-bottom: 1rem;
  font-size: 0.85rem;
}

.arb-scanner-count {
  color: var(--emerald-400);
  font-weight: 600;
}

.arb-scanner-total {
  color: var(--slate-200);
  font-family: 'JetBrains Mono', monospace;
}

.arb-scanner-time {
  color: var(--slate-500);
  margin-left: auto;
}

/* Cards */
.arb-scanner-cards {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.arb-card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 1rem;
}

.arb-card-header {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 0.75rem;
}

.arb-card-path {
  font-weight: 600;
  font-size: 0.9rem;
  color: var(--slate-100);
}

.arb-card-hops {
  font-size: 0.75rem;
  color: var(--slate-500);
  padding: 0.15rem 0.5rem;
  border: 1px solid var(--border-color);
  border-radius: 4px;
}

.arb-card-profit-badge {
  margin-left: auto;
  font-weight: 700;
  font-size: 0.85rem;
  color: var(--emerald-400);
  background: rgba(52, 211, 153, 0.1);
  padding: 0.2rem 0.6rem;
  border-radius: 6px;
}

/* Amounts row */
.arb-card-amounts {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.5rem;
  margin-bottom: 0.75rem;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid var(--border-color);
}

.arb-card-amount,
.arb-card-detail {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.arb-card-label {
  font-size: 0.8rem;
  color: var(--slate-400);
}

.arb-card-value {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.85rem;
  color: var(--slate-200);
}

/* Breakdown row */
.arb-card-breakdown {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.arb-card-value.profit { color: var(--emerald-400); }
.arb-card-value.fee { color: var(--slate-500); }
.arb-card-value.net { color: var(--emerald-300); font-weight: 600; }

/* Footer */
.arb-card-footer {
  display: flex;
  justify-content: flex-end;
}

.arb-card-impact {
  font-size: 0.8rem;
  font-family: 'JetBrains Mono', monospace;
}

.arb-card-impact.impact-low { color: var(--emerald-400); }
.arb-card-impact.impact-medium { color: var(--amber-400, #fbbf24); }
.arb-card-impact.impact-high { color: var(--red-400, #f87171); }

/* Empty state */
.arb-scanner-empty {
  text-align: center;
  padding: 3rem 1rem;
  color: var(--slate-400);
}

.arb-scanner-empty p:first-child {
  font-size: 1rem;
  margin-bottom: 0.5rem;
}

.arb-scanner-empty-hint {
  font-size: 0.85rem;
  color: var(--slate-500);
}
```

**Step 3: Verify build**

Run: `cd frontend && npm run build`
Expected: Builds with no errors (component isn't routed yet, but should compile).

**Step 4: Commit**

```bash
git add frontend/src/components/ArbScannerTab.tsx frontend/src/components/ArbScannerTab.css frontend/src/api/arb.ts
git commit -m "feat: add ArbScannerTab UI and CSS"
```

---

### Task 6: Wire up sidebar and routing

**Files:**
- Modify: `frontend/src/components/Sidebar.tsx`
- Modify: `frontend/src/App.tsx`

**Step 1: Add 'arb-scanner' to the View type**

In `App.tsx`, find the `View` type union and add `'arb-scanner'`.

In `Sidebar.tsx`, find the matching `View` type and add `'arb-scanner'`.

**Step 2: Add sidebar button**

In `Sidebar.tsx`, add an "Arb Scanner" button in the Tools section, after the Router entry. Use a circular-arrow SVG icon. Follow the exact pattern of the Router sidebar button.

**Step 3: Add view rendering**

In `App.tsx`, import `ArbScannerTab` and add the conditional render block:

```tsx
{view === 'arb-scanner' && (
  <ArbScannerTab
    walletAddress={walletAddress}
  />
)}
```

**Step 4: Verify full build**

Run: `cd frontend && npm run build`
Expected: Clean build.

Run: `cargo clippy --workspace --all-targets`
Expected: Zero warnings.

Run: `cargo test --workspace`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add frontend/src/App.tsx frontend/src/components/Sidebar.tsx
git commit -m "feat: wire up Arb Scanner tab in sidebar and routing"
```

---

## Build & Verification Sequence

After all tasks:

1. `cargo test --workspace` — all pass including new circular arb tests
2. `cargo clippy --workspace --all-targets` — zero warnings
3. `cd frontend && npm run build` — clean
4. Manual: open Arb Scanner tab → see scan results or "no arbs" empty state → click Refresh
