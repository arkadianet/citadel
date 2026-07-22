### Task 17: Split dexy `tx_builder.rs` without semantic change

**Files:**
- Current: `crates/protocols/dexy/src/tx_builder.rs` (~2287 LOC)
- Create: `tx_builder/{mod,mint,swap,lp_deposit,lp_redeem,validate}.rs`
- Public API unchanged: `build_mint_dexy_tx`, `build_swap_dexy_tx`, `build_lp_deposit_tx`, `build_lp_redeem_tx`, validators

**Numeric target:** each operation file < **900 LOC**; no fee/math changes.

- [ ] Goldens first → split → goldens pass → clippy → commit `refactor(dexy): split tx_builder by operation`

---

