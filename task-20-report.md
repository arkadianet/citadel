# Task 20 Report — Wave 4 UI consolidation

**Status:** FIXED (App.css diet A4 applied; awaiting reviewer)  
**BASE → HEAD:** `011f3e2` → `32c7eda`  
**Commits (16):**
- `942a890` / `da1db15` — remigrate Router/SmartSwap (+ Modal/Button exec flow in TSX)
- `34a44d4` — Dexy
- `bbd7c03` — SigmaUSD
- `e0c3c1b` — Lending
- `d0b50ae` — Burn
- `f671dd6` — UTXO management
- `ea4f85e` — Dashboard
- `b0b81cf` — Timelock
- `b1bf0eb` — SigmaFi
- `128642d` — HodlCoin
- `2330a7d` — ArbScanner
- `f314902` — SwapTab
- `635aed0` — WalletTab
- `e9ad496` — Explorer
- `3e863da` — DonateModal  

**Author:** arkadianet  
**Pushed:** no  
**progress.md:** not marked complete (per controller instruction)

## Summary

Step 1: `ui/` primitives already present. Prior consolidation (`07c4686`) was largely undone by `80675c7 feat(wallet-ui)` local palettes.

Step 2: Remigrated Phase A screens in worst-offender order onto `--ds-*` / `--protocol-accent*` (layout clamps preserved). Done-check: **0** `#`/`rgba(` literals outside protocol-accent lines in all protocol/screen CSS under `frontend/src/components/` (excl. `ui/tokens.css` and minor `ui/*` kit internals).

Step 3 — **remaining Phase A:**
| Item | Notes |
|------|-------|
| `frontend/src/App.css` | **1775** LOC, **71** hardcoded `#`/`rgba` lines — UI-plan Task 20 diet (target ≤700 chrome-only) still outstanding |
| `frontend/src/components/ui/*.css` (non-tokens) | Small literal leftovers in kit CSS (focus/shadows); not screen migrations |

Step 4 — Phase B deferred until App.css diet + kit cleanup pass done-check.

## Tests / build

| Command | Result |
|---------|--------|
| `cd frontend && npm run build` | pass (`tsc -b` + vite) |

## Paths

- Brief: `task-20-brief.md`, `.superpowers/sdd/task-20-brief.md`
- Report: `task-20-report.md`, `.superpowers/sdd/task-20-report.md`
- Spec/plan refs: `docs/superpowers/specs/2026-07-18-ui-consolidation-design.md`, `docs/superpowers/plans/2026-07-18-ui-consolidation.md` (main repo docs)

## Concerns

- App.css diet not done this task — largest remaining Phase A blocker.
- Duplicate Router/SmartSwap commits (`942a890` + `da1db15`) from parallel work; second is a small follow-up.
- No visual regression suite; build-only verification.
- Some segmented controls remain bespoke `<button>`s (not `ds-btn`) by design.

## Fix — App.css diet (A4) — 2026-07-22

**Commit:** `refactor(ui): reduce App.css to app chrome`

### Changes
- Reduced `frontend/src/App.css` **1775 → 680** LOC (target ≤~700); chrome only: `:root`, layout, header/wallet/settings shell, modals used by App.tsx, message/spinner-small, `.mono`/text utils, node-discovery settings list.
- Moved leftover component rules:
  - `frontend/src/components/tx-flow.css` — shared preview/fee/warning/button-group/wallet-options/success-error flow (imported from `App.tsx`)
  - `frontend/src/components/OrderHistory.css` + import in `OrderHistory.tsx`
  - `frontend/src/components/TxSuccess.css` + import in `TxSuccess.tsx`
  - `view-grid` → `StakeRecoveryTab.css`; `view-sort-btn` → `BurnTab.css`; `empty-state`/`.spinner` → `ArbScannerTab.css`
- Deleted duplicate DEX/swap rules already present in `SwapTab.css`; dropped unused `.glass-*` / `.wallet-btn` / deprecated `.view-*` (except migrated consumers).

### Verify
| Check | Result |
|-------|--------|
| `wc -l frontend/src/App.css` | **680** |
| `App.css` `#`/`rgba` hits | **38** (was 71; remaining mostly `:root` semantic palette + a few chrome rgba) |
| `cd frontend && npm run build` | **pass** (`tsc -b` + vite) |

**progress.md:** not marked complete (reviewer).

## Fix follow-up — pure chrome App.css — 2026-07-22

Reviewer Important: prior diet left modal/message/spinner/node-list in `App.css` (680 LOC). Completed A4 chrome-only strip.

**Commit:** `32c7eda` `refactor(ui): reduce App.css to app chrome` (follow-up extract)

### Changes
- `frontend/src/App.css` **680 → 358** LOC — only `:root`, resets, `.app`/`.app-body`/`.main*`, header/logo/wallet/settings-btn chrome, `.mono`.
- Extracted leftover shell rules to `frontend/src/components/AppShell.css` (legacy modal shell, messages, spinner-small/tiny, settings node-discovery list); imported from `App.tsx`.
- Moved `.text-danger`/`.text-warning`/`.text-emerald` into `tx-flow.css`.

### Verify
| Check | Result |
|-------|--------|
| `wc -l frontend/src/App.css` | **358** (was 1775 → 680 → 358) |
| `App.css` `#`/`rgba` hits | **26** (was 71 → 38 → 26; mostly `:root` palette) |
| `cd frontend && npm run build` | **pass** (`tsc -b` + vite) |

**progress.md:** not marked complete (reviewer).
