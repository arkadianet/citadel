### Task 16: Split lending `tx_builder.rs` without semantic change

**Files:**
- Current: `crates/protocols/lending/src/tx_builder.rs` (~2065 LOC)
- Create: `crates/protocols/lending/src/tx_builder/mod.rs` plus `lend.rs`, `withdraw.rs`, `borrow.rs`, `repay.rs`, `refund.rs`, `common.rs` (input selection helpers)
- Modify: `crates/protocols/lending/src/lib.rs` re-exports — **public API names unchanged** (`build_lend_tx`, `build_withdraw_tx`, `build_borrow_tx`, `build_repay_tx`, `build_refund_tx`, …)

**Numeric target:** no single `tx_builder` file > **800 LOC**; `mod.rs` is re-exports + shared types only (<300 LOC).

- [ ] **Step 1: Add golden/regression tests for each `build_*_tx` using recorded inputs** under `crates/protocols/lending/tests/fixtures/` (minimal representative vectors). Tests must pass on the **unsplit** module first.
- [ ] **Step 2: `git mv` to directory and split by function boundaries only — no math edits**
- [ ] **Step 3: Re-run goldens — field-equal**
- [ ] **Step 4:**

```bash
cargo test -p lending
cargo clippy -p lending --all-targets -- -D warnings
wc -l crates/protocols/lending/src/tx_builder/*.rs
```

- [ ] **Commit:** `refactor(lending): split tx_builder into operation modules`

---

