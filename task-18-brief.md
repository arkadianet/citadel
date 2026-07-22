### Task 18: Split AMM giants (router / direct_swap) — mid-size layout

**Decision (locked):** Prefer **mid-size files** under `crates/protocols/amm/src/` (existing modules stay; further split only files still >1500 LOC). Do **not** create deep `amm/src/{swap,lp,router,arb}/` trees in the first split PR unless a single file remains >1500 LOC after extracting obvious private submodules.

**Files:**
- `router.rs` (~2222) → e.g. `router/mod.rs`, `router/search.rs`, `router/types.rs` (names as needed) with `pub use` preserving `amm::router::*`
- `direct_swap.rs` (~1426) → split quote vs build if still >1200 after review
- **Do not** edit calculator formulas or arb heuristics in the same commit as a split

**Numeric targets:**
- `router` entry surface reviewable: no single file > **1200 LOC**
- `direct_swap` no single file > **900 LOC**
- `commands/amm.rs` already thinned in Wave 2 (target < **200 LOC**)

- [ ] **Step 1: Goldens for one router path + one direct swap path** (fixtures from current outputs).
- [ ] **Step 2: Split files only; re-export; goldens field-equal.**
- [ ] **Step 3:**

```bash
cargo test -p amm
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
wc -l crates/protocols/amm/src/router*.rs crates/protocols/amm/src/direct_swap*.rs crates/protocols/amm/src/router/**/*.rs 2>/dev/null
```

- [ ] **Commit:** `refactor(amm): split oversized router/direct_swap modules without behavior change`

**Wave 3 exit:** Extracted helpers tested; goldens green; clippy/fmt clean; giant files under numeric targets.

---

## Wave 4 — Frontend API consistency + UI token migration

