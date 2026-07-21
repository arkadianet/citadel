# Citadel Timelock — Design Spec

**Date:** 2026-07-21  
**Author:** arkadianet  
**Status:** Draft — awaiting approval (do not implement until approved)  
**Branch context:** `feat/wallet-hub-and-node-compat`  
**Relation:** Lives **beside** MewLock; does not replace or modify the MewLock contract.

---

## 1. Summary

Citadel Timelock is a new Ergo script + Citadel protocol that locks ERG and tokens until a block height, then allows **anyone** to sweep the box. Funds always return to a **fixed recipient**; the lock **self-funds** the miner fee and pays an **executor tip** set at lock time.

**Recommended model (v1):** single permissionless unlock branch after height; no early cancel; tip ≥ `MIN_BOX_VALUE`; soft max lock duration below storage-rent epoch; UI dual-protocol Timelock tab (MewLock | Citadel Timelock).

---

## 2. Assumptions (defaults if unspecified)

| Topic | Default |
|-------|---------|
| Protocol name | **Citadel Timelock** (crate: `citadel_timelock`) |
| Network | Mainnet first; same pattern for testnet later |
| Early cancel | **Out of v1** (optional Phase 2) |
| Miner fee | Fixed **1,100,000** nanoERG (`TX_FEE_NANO` / `0.0011 ERG`) |
| Default tip | **2,200,000** nanoERG (`0.0022 ERG`) |
| Min tip | **1,000,000** nanoERG (`MIN_BOX_VALUE`) — tip always forms a valid box |
| Max tip | No hard protocol max; UI soft-warn if tip > 10% of lock ERG |
| Recipient | P2PK only in v1 (GroupElement in R4), same as MewLock |
| Tokens | Full conservation to recipient; tip/fee are ERG-only |
| Dev / protocol fee | **None** (unlike MewLock’s 3%) |
| Storage rent epoch | **1,051,200** blocks (~4 years) from box creation height |
| Max lock duration | Soft cap: **1,001,200** blocks (~3.8y) = rent epoch − 50,000 safety margin |
| Script style | Constant-segregated ErgoTree (`0x19…`); hardcode P2S address for queries |
| Discovery | Indexed node (`unspent_boxes_by_address`), same capability gate as MewLock |

---

## 3. Threat Model

### Prevented

| Threat | How |
|--------|-----|
| Divert principal away from recipient | Unlock path pins `OUTPUTS(0).propositionBytes` to R4-derived P2PK tree |
| Steal / skim tokens | Token bags on recipient output must match `SELF.tokens` exactly |
| Unlock before height | Branch requires `HEIGHT >= unlockHeight` |
| Executor drains tip+fee from recipient incorrectly | Script enforces `recipient.value >= SELF.value - tip - fee` and exact `tip` / `fee` on later outputs |
| Change tip after lock | Tip is committed in R6 at creation |
| Change recipient after lock | Recipient committed in R4 |

### Allowed (by design)

| Action | Who |
|--------|-----|
| Spend after unlock height | **Anyone** (permissionless) |
| Choose tip destination | Executor (their address on tip output) |
| Choose when to sweep (after unlock) | Anyone; incentivized by tip |

### Out of scope / residual risks

- **Storage rent after ~4y:** a rent charge can reduce box value and potentially make tip+fee conservation fail. Mitigated by duration cap + value floor + UI warnings — **not** by on-chain “force unlock before rent” (that can brick funds).
- **Stuck if tip too high / value too low:** prevented at lock-build time by min-value checks.
- **No MEV / front-running protection:** competing unlock txs are fine; first valid inclusion wins.
- **Recipient key compromise:** same as any P2PK custody after unlock.
- **Script bugs:** audit + unit tests on register decode and output layout before mainnet.

---

## 4. Approaches Considered

### A — Permissionless fixed-output sweep (recommended)

One spend path after height: recipient + tip + miner fee. Self-funding. Simple script, clear UX (“Unlock / Sweep”).

- **Pros:** Matches product goals; no owner signature needed post-unlock; tip markets keepers.
- **Cons:** No early abort; mistyped recipient or height is permanent until height.

### B — Dual path: early cancel by owner + permissionless unlock

Add `proveDlog(recipientPk)` path before unlock height; cancel pays miner fee from user UTXOs (or from box with different layout).

- **Pros:** Safer UX for mistakes / changed plans.
- **Cons:** Larger script; two tx builders; fee-source asymmetry vs unlock path.

### C — NFT receipt / claim token

Mint a claim NFT at lock; only NFT holder can unlock (optional tip still).

- **Pros:** Transferable claim rights.
- **Cons:** Overbuilt vs goals; breaks “anyone triggers, funds to fixed recipient” unless NFT is only for cancel.

**Decision:** Ship **A** for v1. Keep **B** as Phase 2 open question. Reject **C** for now.

---

## 5. Contract Model

### 5.1 Script branches (v1)

**Only branch — Unlock / Sweep**

Preconditions:

1. `HEIGHT >= SELF.R5[Int].get` (unlock height)
2. Exactly the required outputs (see §5.3); no unconstrained change from the lock box itself
3. Recipient, tip amount, and miner fee constraints below

Conceptual ErgoScript (illustrative — final compiled tree TBD in implementation):

```scala
{
  val recipientPk = SELF.R4[GroupElement].get
  val unlockHeight = SELF.R5[Int].get
  val tip = SELF.R6[Long].get
  val fee = 1100000L
  val recipientTree = proveDlog(recipientPk).propBytes  // or equivalent P2PK tree bytes

  val afterHeight = HEIGHT >= unlockHeight

  val recipientOk =
    OUTPUTS(0).propositionBytes == recipientTree &&
    OUTPUTS(0).value >= SELF.value - tip - fee &&
    OUTPUTS(0).tokens == SELF.tokens

  val tipOk =
    OUTPUTS(1).value == tip &&
    tip >= 1000000L

  val feeOk =
    OUTPUTS(2).value == fee &&
    OUTPUTS(2).propositionBytes == fromBase16("1005040004000e36…") // MINER_FEE_ERGO_TREE

  sigmaProp(afterHeight && recipientOk && tipOk && feeOk)
}
```

Notes:

- Tip **destination is unconstrained** (any proposition) — that is the executor incentive.
- Fee proposition is the standard Ergo miner fee tree (`citadel_core::constants::MINER_FEE_ERGO_TREE`).
- Prefer **exact** tip/fee equality to avoid dust games; recipient gets the remainder via `>=` (allows attaching extra ERG from other inputs if needed, but v1 tx builder will not).

### 5.2 Optional Phase 2 — early cancel

```text
proveDlog(recipientPk) && HEIGHT < unlockHeight
→ OUTPUTS(0) to recipient with full tokens + (SELF.value - fee)
→ OUTPUTS(1) miner fee
→ no tip required
```

Owner-funded fee from extra inputs is also acceptable if script allows `OUTPUTS(0).value >= SELF.value` with fee from other inputs — prefer documenting one cancel layout only if Phase 2 is approved.

### 5.3 Registers / constants

| Location | Type | Meaning |
|----------|------|---------|
| **R4** | `GroupElement` | Recipient public key (P2PK) |
| **R5** | `Int` | Unlock height |
| **R6** | `Long` | Executor tip (nanoERG), ≥ `MIN_BOX_VALUE` |
| **R7** | `Coll[Byte]` optional | Lock name (UTF-8), metadata only — **not** enforced by script |
| **R8** | `Coll[Byte]` optional | Description — metadata only |
| Box `creationHeight` | chain field | Used off-chain for storage-rent warnings |
| Script constant | `Long` | `fee = 1_100_000` |
| Script constant | fee tree bytes | Miner fee proposition |

**Not stored in registers:** miner fee (fixed constant), creation height (on box), rent epoch (protocol constant off-chain).

### 5.4 Unlock output layout

| Index | Role | Value | Assets | Proposition |
|-------|------|-------|--------|-------------|
| 0 | Recipient | `≥ SELF.value − tip − fee` | Exact copy of `SELF.tokens` | P2PK from R4 |
| 1 | Executor tip | `== tip` (R6) | none | Executor-chosen |
| 2 | Miner fee | `== 1_100_000` | none | `MINER_FEE_ERGO_TREE` |

**Token conservation:** all tokens → output 0; tip/fee carry no tokens.

**Min box values:**

- Tip ≥ `MIN_BOX_VALUE` (1e6) so output 1 is valid.
- Recipient ≥ `MIN_BOX_VALUE` after subtracting tip+fee.
- Fee output is 1.1e6 (> min).

### 5.5 Lock creation — value floor

```text
lock_value_min =
    MIN_BOX_VALUE          // recipient remainder floor
  + tip                    // R6
  + MINER_FEE              // 1_100_000
  + rent_buffer            // see §6 (default 0 if duration capped)
```

So:

```text
recipient_out = lock_value - tip - fee  ≥  MIN_BOX_VALUE
⇒ lock_value ≥ MIN_BOX_VALUE + tip + fee
```

With default tip `2_200_000`:

```text
lock_value ≥ 1_000_000 + 2_200_000 + 1_100_000 = 4_300_000 nanoERG (0.0043 ERG)
```

Plus creation tx still needs a separate miner fee from user inputs (lock creation is a normal user-paid fee tx), same as MewLock `build_lock_tx`.

**Tokens-only locks:** if locking tokens with zero “user ERG intent”, still fund the box with at least `lock_value_min` ERG (like MewLock’s `MIN_BOX_VALUE`, but higher because tip+fee are reserved).

**Tip chosen by user** at lock time (slider / input); default 0.0022 ERG; min 0.001 ERG.

### 5.6 Ergo practicalities

| Item | Guidance |
|------|----------|
| Miner fee address / tree | Reuse `MINER_FEE_ERGO_TREE` / `Eip12Output::fee` |
| `MIN_BOX_VALUE` | `1_000_000` nanoERG |
| `TX_FEE` | `1_100_000` nanoERG |
| Script size | Keep single-branch; avoid large Coll literals — fee tree as constant is fine |
| Constant segregation | Compile with segregation; **hardcode P2S address** for node queries (same gotcha as MewLock — do not derive via `Address::recreate_from_ergo_tree`) |
| Sigma Int vs Long | R5 = `Int` (height); R6 = `Long` (nanoERG tip) — match encoding helpers in `ergo_tx::sigma` |
| Redeemer / proveDlog | Recipient tree must be full P2PK ErgoTree (`0008cd` + pubkey), never raw 33-byte pubkey |

---

## 6. Storage Rent (1,051,200 blocks)

### Facts

- After a box’s age reaches **1,051,200** blocks, storage rent rules can reduce its ERG value.
- A rent-reduced lock may no longer satisfy `value ≥ tip + fee + MIN_BOX_VALUE`, making unlock **impossible** until someone tops up (v1 has no top-up path) — effectively stranded.

### Design stance

| Layer | Behavior |
|-------|----------|
| **Script** | Do **not** gate unlock on “before rent”. Do **not** auto-force unlock. Script stays height + output layout only. |
| **tx_builder (lock)** | Reject `unlock_height - current_height > MAX_LOCK_DURATION` where `MAX_LOCK_DURATION = 1_051_200 - 50_000 = 1_001_200`. |
| **UI** | Warn when remaining lifetime until rent epoch is &lt; 100k blocks; block create if over soft max; show “rent risk” badge on existing locks approaching age limit. |
| **Value buffer** | With duration soft-cap, default `rent_buffer = 0`. Optional advanced: add buffer equal to estimated one rent charge if we later allow longer locks. |
| **Discovery** | Surface `creationHeight`, `ageBlocks`, `blocksUntilRent`, `rentRisk: none \| warn \| critical`. |

### Duration presets (Citadel Timelock)

Align with MewLock where useful, plus a long option under the soft cap:

| Label | Blocks | ~Time |
|-------|--------|-------|
| 1 Month | 21,600 | ~30d |
| 3 Months | 64,800 | ~90d |
| 6 Months | 129,600 | ~180d |
| 1 Year | 259,200 | ~1y |
| 2 Years | 518,400 | ~2y |
| 3 Years | 777,600 | ~3y |
| Custom | any | clamped to soft max |

---

## 7. Comparison vs MewLock

| | **MewLock** | **Citadel Timelock** |
|--|-------------|----------------------|
| Who unlocks | Owner (R4) only | Anyone after height |
| Unlock fee model | 3% ERG + tokens → treasury | Fixed tip → executor; **0%** protocol skim |
| Miner fee source | Extra user UTXO | **From lock box** |
| Funds diversion | Owner receives (must sign) | Always to locked recipient |
| Tip / keeper incentive | None | R6 tip |
| Registers | R4 GE, R5 unlock, R6–R8 meta | R4 GE, R5 unlock, **R6 tip**, R7–R8 meta |
| Early cancel | N/A (wait or never) | v1: none; Phase 2 optional |
| Storage rent UX | Implicit only | Explicit warnings + soft max duration |
| Fee on small locks | Fee thresholds in contract | Flat tip+fee reservation |
| Citadel UI | Existing TimelockTab | Sibling section / sub-tab |

---

## 8. Citadel UX

### 8.1 Timelock tab structure

Keep sidebar entry **Timelocks**. Inside `TimelockTab`:

```text
[ MewLock ]  [ Citadel Timelock ]
```

- **MewLock** — existing UI unchanged (header, filters, create, unlock-own-only).
- **Citadel Timelock** — parallel list + create + **Unlock / Sweep** for any expired lock.

Update Sidebar/Dashboard subtitle from “MewLock” → “MewLock & Citadel Timelock” (copy only).

### 8.2 Citadel Timelock views

1. **List** — all contract boxes (or filter: All / Mine / Unlockable).
   - Show: recipient, unlock height, tip, ERG, tokens, rent-risk chip, own badge.
2. **Create lock** — amount, tokens, recipient (default: connected wallet), unlock height / preset, tip (default 0.0022), optional name.
   - Live preview: `you receive ≈ value − tip − fee`, rent warning, min value check.
3. **Unlock / Sweep** — available when `HEIGHT >= unlockHeight` for **any** wallet.
   - Connected wallet becomes tip recipient.
   - No extra ERG required from user if lock is correctly funded (inputs = `[lockBox]` only).
   - Signing flow: same `useTransactionFlow` / ErgoPay as MewLock.

### 8.3 Filter semantics

| Filter | MewLock | Citadel Timelock |
|--------|---------|------------------|
| Unlockable | own && past height | **past height** (any) |
| Mine | `isOwn` (R4 matches wallet) | same |

---

## 9. Citadel Integration Plan

Follow CLAUDE.md “Adding a New Protocol” — **new crate**, do not fold into `mewlock`.

```text
crates/protocols/citadel_timelock/
  Cargo.toml
  src/
    lib.rs
    constants.rs      # ERGO_TREE, ADDRESS, fees, presets, rent constants
    state.rs          # CitadelTimelockBox, State
    fetch.rs          # unspent_boxes_by_address(ADDRESS)
    tx_builder.rs     # build_lock_tx, build_unlock_tx
```

| Layer | Files |
|-------|-------|
| Workspace | root `Cargo.toml` member |
| Commands | `app/src/commands/citadel_timelock.rs`; register in `mod.rs` + `lib.rs` |
| Frontend API | `frontend/src/api/citadelTimelock.ts` |
| UI | Extend `TimelockTab.tsx` / CSS with protocol sub-tabs; avoid replacing MewLock components blindly — extract shared list chrome if needed |
| Signing | Reuse `useTransactionFlow` |

### Suggested IPC (flat args, `Result<T, String>`)

| Command | Purpose |
|---------|---------|
| `citadel_timelock_fetch_state` | List locks + height + rent fields |
| `citadel_timelock_get_durations` | Presets |
| `citadel_timelock_build_lock` | Create lock EIP-12 tx |
| `citadel_timelock_build_unlock` | Sweep EIP-12 tx (tip → caller tree) |

---

## 10. Data Flow

### Lock

```text
UI Create
  → citadel_timelock_build_lock(user_tree, recipient?, erg, tokens, unlock_height, tip, …)
  → select user UTXOs (erg + tokens + creation fee)
  → outputs: [lockBox(R4,R5,R6,…), change?, minerFee]
  → reduce → ErgoPay → broadcast
```

### Unlock / Sweep

```text
UI Unlock/Sweep (any user, height ok)
  → citadel_timelock_build_unlock(lock_box, executor_tree, height)
  → inputs: [lockBox] only
  → outputs: [recipient(R4), tip→executor, minerFee]
  → reduce → ErgoPay → broadcast
```

### Fetch

```text
Node unspent_boxes_by_address(CITADEL_TIMELOCK_ADDRESS)
  → parse R4/R5/R6 (+ optional R7/R8)
  → enrich isOwn, isUnlockable, blocksRemaining, rentRisk
```

---

## 11. Implementation Phases

Do **not** start until this spec is approved.

### Phase 0 — Spec approval

- Resolve open questions (§12)
- Freeze register layout + output indices

### Phase 1 — Contract

- Write / compile ErgoScript (or assemble tree with constants)
- Golden vectors: encode R4/R5/R6; valid unlock tx; reject early height; reject diverted recipient; reject wrong tip/fee
- Publish `CITADEL_TIMELOCK_ERGO_TREE` + hardcoded `CITADEL_TIMELOCK_ADDRESS`
- Document verification steps (parse roundtrip, address prefix test like MewLock)

### Phase 2 — Protocol crate

- `constants`, `state`, `fetch`, `tx_builder` with unit tests mirroring `mewlock` coverage
- Min-value and max-duration validation in `build_lock_tx`
- Unlock builder: single-input, three-output; no user fee UTXO

### Phase 3 — Tauri commands

- Wire `citadel_timelock_*` commands; capability tier same as MewLock (indexed node)

### Phase 4 — Frontend

- Protocol sub-tabs in `TimelockTab`
- Create + Sweep flows; rent warnings; tip default
- Sidebar/Dashboard copy update

### Phase 5 — Hardening

- Mainnet dry-run with small value
- Optional Phase 2 contract: early cancel (only if approved)
- Clippy / fmt / frontend build clean

---

## 12. Open Questions (need your call)

1. **Early cancel (Phase 2)?**  
   - **Default in this spec:** no for v1.  
   - Want owner `proveDlog` cancel before height?

2. **Tip minimum / zero tip?**  
   - **Default:** min tip = `MIN_BOX_VALUE` (always 3 outputs).  
   - Allow tip = 0 (2-output unlock) for self-service with no keeper incentive?

3. **Recipient flexibility?**  
   - **Default:** P2PK only (GroupElement).  
   - Allow arbitrary ErgoTree / P2S recipient in a later version?

4. **Soft max duration 1,001,200 blocks** — OK, or prefer stricter (e.g. hard-cap at 2 years like MewLock’s longest preset)?

5. **Rent buffer ERG** — keep `0` under duration cap, or always add a small buffer (e.g. 0.01 ERG) regardless?

6. **Protocol naming in UI** — “Citadel Timelock” vs “Citadel Lock” vs “Keeper Timelock”?

7. **Public directory of unlockable locks** — show *all* contract boxes (keeper marketplace) or default to “Mine” and hide others behind a toggle?  
   - **Default:** list all (needed for permissionless sweep discovery).

8. **Contract authorship** — compile from ErgoScript source in-repo, or check in only hex + address (MewLock style)?

9. **Testnet first?** — deploy testnet tree before mainnet constants land?

---

## 13. Approval Ask

Please review this spec and reply with:

- **Approve as-is**, or  
- **Approve with changes** (answer §12 where you care), or  
- **Revise** (call out sections).

No implementation (crate stub, commands, or UI) will start until you approve. After approval, next step is an implementation plan (`writing-plans`), then phased coding.
