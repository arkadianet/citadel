### Task 21 (optional): Decompose `UtxoManagementTab.tsx` presentationally

**Files:**
- Modify: `frontend/src/components/UtxoManagementTab.tsx` (~2572 LOC)
- Create: e.g. `frontend/src/components/utxo/UtxoConsolidatePanel.tsx`, `UtxoSplitPanel.tsx`, `UtxoBoxList.tsx` (exact names chosen to match existing section boundaries in the tab)
- **Do not** change invoke contracts in `frontend/src/api/utxoManagement.ts`

- [ ] Extract presentational sections only; parent keeps state + `useTransactionFlow`.
- [ ] `cd frontend && npm run build`
- [ ] Commit `refactor(ui): split UtxoManagementTab into presentational subcomponents`

**Wave 4 exit:** `npm run build` green; UI Phase A complete or remaining list explicit; no new parallel button/modal systems; signing via `useTransactionFlow` on ≥2 protocols.

---

## Wave 5 — Optional rename `citadel-api` → `citadel-app`

