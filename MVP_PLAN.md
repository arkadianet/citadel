# Citadel MVP Implementation Plan

> **Status (2026-07-22):** The dual-door decision in §B (local Axum HTTP + thin Tauri) is **superseded**. Citadel ships **Tauri IPC as the sole app door**. The only local HTTP server is **ErgoPay** (`ergopay-server`) for Nautilus signing. Unused Axum routes in `citadel-api` are deleted in the modular refactor Wave 1. Treat §B endpoint lists as historical design notes, not a build target.

## Executive Summary

This document defines the MVP scope, API design, and implementation details for **Citadel** - a fully-local Ergo DeFi interaction app. The MVP targets desktop (Tauri + React) with Nautilus wallet integration for SigmaUSD protocol interactions.

**MVP Goal**: Allow users to mint SigUSD by connecting to their local Ergo node, viewing protocol state, building transactions, and signing via Nautilus.

---

## A. MVP Scope Definition

### What's IN the MVP

1. **Node Connection Screen**
   - Configure node URL (default: `http://127.0.0.1:9053`)
   - Display node health, network info, block heights
   - Detect and display capability tier (Full / IndexLagging / Basic)

2. **Wallet Handoff (Desktop)**
   - Nautilus EIP-12 dApp connector integration
   - Connect, disconnect, get UTXOs
   - Sign unsigned transactions

3. **SigmaUSD Protocol - Mint SigUSD**
   - Read-only state display (bank reserves, oracle price, reserve ratio)
   - **One action: Mint SigUSD** (chosen over Redeem - see justification below)
   - Build unsigned EIP-12 transaction
   - Sign via Nautilus, submit to node
   - Display transaction confirmation status

### What's OUT of MVP (Deferred)

- SigUSD Redeem, SigRSV Mint/Redeem (Phase 2)
- DEX/Swap functionality (Phase 3)
- ErgoPay / Mobile support (Phase 4)
- Lite indexer for Basic mode degradation (Phase 2)
- Transaction history display

### MVP Action Justification: Mint SigUSD

**Why Mint SigUSD over Redeem?**

1. **Simpler user flow**: User needs ERG (commonly held) to mint. No token selection required.
2. **More common action**: Most users first want to acquire SigUSD stablecoins.
3. **No token validation**: Redeem requires checking user holds the specific token.
4. **Demonstrates full flow**: Covers tx building, signing, submission - same complexity as redeem.
5. **Lower risk**: Mint adds to protocol reserves; redeem reduces them (less testing risk).

---

## B. API Design

### Architecture Decision: Local HTTP + Tauri Commands

**Choice**: Internal local HTTP API (127.0.0.1) with Tauri commands as thin wrappers.

**Rationale**:
- HTTP API provides clean boundary for future UI swap (Flutter, CLI tools)
- Tauri commands simply proxy to HTTP endpoints
- Can expose HTTP directly for debugging/testing
- Flutter can consume the same HTTP API via `http` package or `flutter_rust_bridge`

**Port**: `127.0.0.1:19053` (Ergo node + 10000 offset, internal only)

### API Endpoints

#### Health & Node Status

```
GET /health
Response: { "status": "ok", "version": "0.1.0" }

GET /node/status
Response: {
  "connected": true,
  "url": "http://127.0.0.1:9053",
  "network": "mainnet",
  "chain_height": 1234567,
  "indexed_height": 1234560,      // null if no extraIndex
  "capability_tier": "Full",       // "Full" | "IndexLagging" | "Basic"
  "index_lag": 7                   // null if no extraIndex
}

POST /node/configure
Request: { "url": "http://127.0.0.1:9053", "api_key": "" }
Response: { "success": true, "status": <NodeStatus> }
```

#### SigmaUSD Protocol

```
GET /sigmausd/state
Response: {
  "bank_erg_nano": 10000000000000000,
  "sigusd_circulating": 500000000,
  "sigrsv_circulating": 100000000000,
  "oracle_erg_per_usd_nano": 1851851851,
  "reserve_ratio_pct": 542.13,
  "sigusd_price_nano": 1851851851,
  "sigrsv_price_nano": 7408163,
  "can_mint_sigusd": true,
  "can_mint_sigrsv": true,
  "can_redeem_sigusd": true,
  "can_redeem_sigrsv": true,
  "max_sigusd_mintable": 1234567890,
  "bank_box_id": "abc123...",
  "oracle_box_id": "def456..."
}

POST /sigmausd/mint/preview
Request: {
  "amount": 10000,  // 100.00 SigUSD (2 decimals)
  "user_address": "9f..."
}
Response: {
  "erg_cost_nano": 188888888802,
  "protocol_fee_nano": 3703703702,
  "tx_fee_nano": 1100000,
  "total_cost_nano": 189988888802,
  "can_execute": true,
  "error": null
}

POST /sigmausd/mint/build
Request: {
  "amount": 10000,
  "user_address": "9f...",
  "user_utxos": [
    {
      "boxId": "...",
      "transactionId": "...",
      "index": 0,
      "value": "1000000000000",
      "ergoTree": "0008cd...",
      "assets": [],
      "creationHeight": 1234567,
      "additionalRegisters": {}
    }
  ],
  "current_height": 1234570
}
Response: {
  "unsigned_tx": { ... EIP-12 format ... },
  "summary": {
    "action": "mint_sigusd",
    "erg_amount_nano": 188888888802,
    "token_amount": 10000,
    "token_name": "SigUSD",
    "protocol_fee_nano": 3703703702,
    "tx_fee_nano": 1100000
  }
}
```

#### Transaction Submission

```
POST /tx/submit
Request: {
  "signed_tx": { ... signed transaction JSON ... }
}
Response: {
  "tx_id": "abc123...",
  "submitted": true
}

GET /tx/status/{tx_id}
Response: {
  "tx_id": "abc123...",
  "status": "pending" | "confirmed" | "not_found",
  "confirmations": 0
}
```

#### Wallet (informational - actual signing via browser)

```
POST /wallet/nautilus/utxos
Request: { "address": "9f..." }
Note: This just validates address format.
      Actual UTXOs come from browser wallet.
```

### Request/Response Schemas (TypeScript)

```typescript
// Node types
interface NodeStatus {
  connected: boolean;
  url: string;
  network: "mainnet" | "testnet";
  chain_height: number;
  indexed_height: number | null;
  capability_tier: "Full" | "IndexLagging" | "Basic";
  index_lag: number | null;
}

// SigmaUSD types
interface SigmaUsdState {
  bank_erg_nano: string;  // BigInt as string
  sigusd_circulating: string;
  sigrsv_circulating: string;
  oracle_erg_per_usd_nano: string;
  reserve_ratio_pct: number;
  sigusd_price_nano: string;
  sigrsv_price_nano: string;
  can_mint_sigusd: boolean;
  can_mint_sigrsv: boolean;
  can_redeem_sigusd: boolean;
  can_redeem_sigrsv: boolean;
  max_sigusd_mintable: string;
  bank_box_id: string;
  oracle_box_id: string;
}

interface MintPreviewRequest {
  amount: number;  // Raw SigUSD units (2 decimals)
  user_address: string;
}

interface MintPreviewResponse {
  erg_cost_nano: string;
  protocol_fee_nano: string;
  tx_fee_nano: string;
  total_cost_nano: string;
  can_execute: boolean;
  error: string | null;
}

// EIP-12 transaction types (for wallet)
interface Eip12UnsignedTx {
  inputs: Eip12InputBox[];
  dataInputs: Eip12DataInputBox[];
  outputs: Eip12Output[];
}

interface Eip12InputBox {
  boxId: string;
  transactionId: string;
  index: number;
  value: string;
  ergoTree: string;
  assets: Array<{ tokenId: string; amount: string }>;
  creationHeight: number;
  additionalRegisters: Record<string, string>;
  extension: Record<string, string>;
}

interface Eip12DataInputBox {
  boxId: string;
  transactionId: string;
  index: number;
  value: string;
  ergoTree: string;
  assets: Array<{ tokenId: string; amount: string }>;
  creationHeight: number;
  additionalRegisters: Record<string, string>;
}

interface Eip12Output {
  value: string;
  ergoTree: string;
  assets: Array<{ tokenId: string; amount: string }>;
  creationHeight: number;
  additionalRegisters: Record<string, string>;
}
```

---

## C. Node Capability Detection

### Detection Flow

```
1. GET /info                        → Check node is reachable
2. GET /blockchain/indexedHeight    → Check extraIndex availability
   - 200 + valid JSON              → extraIndex = true
   - 404 or error                  → extraIndex = false
3. Compare indexed_height vs chain_height
   - Difference <= 10              → Full mode
   - Difference > 10               → IndexLagging mode
   - No indexed_height             → Basic mode
```

### Capability Tiers

| Tier | extraIndex | Index Lag | SigmaUSD State | User UTXOs | Actions |
|------|------------|-----------|----------------|------------|---------|
| Full | true | ≤10 blocks | Fresh from index | From wallet | All |
| IndexLagging | true | >10 blocks | May be stale (warn) | From wallet | All (with warning) |
| Basic | false | N/A | Requires known box ID | From wallet | Limited |

### Implementation

```rust
// ergo-node-client/src/capabilities.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityTier {
    Full,
    IndexLagging,
    Basic,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeCapabilities {
    pub is_online: bool,
    pub has_extra_index: Option<bool>,
    pub indexed_height: Option<u32>,
    pub chain_height: u32,
    pub capability_tier: CapabilityTier,
}

impl NodeCapabilities {
    pub fn index_lag(&self) -> Option<u32> {
        self.indexed_height.map(|ih| self.chain_height.saturating_sub(ih))
    }

    pub fn detect(node: &NodeInterface) -> Self {
        let chain_height = node.current_block_height();
        let extra_index = node.has_extra_index();

        let (indexed_height, tier) = match extra_index {
            Some(true) => {
                let ih = node.get_indexed_height().ok().map(|h| h.indexed_height);
                let lag = ih.map(|h| chain_height.saturating_sub(h)).unwrap_or(u32::MAX);
                let tier = if lag <= 10 {
                    CapabilityTier::Full
                } else {
                    CapabilityTier::IndexLagging
                };
                (ih, tier)
            }
            _ => (None, CapabilityTier::Basic),
        };

        Self {
            is_online: true,
            has_extra_index: extra_index,
            indexed_height,
            chain_height,
            capability_tier: tier,
        }
    }
}
```

---

## D. SigmaUSD State Derivation

### Data Sources

| Data | Source Box | Register/Field | Encoding |
|------|-----------|----------------|----------|
| Bank ERG value | Bank box | `value` | nanoERG (i64) |
| SigUSD circulating | Bank box | R4 | Sigma Long (VLQ zigzag) |
| SigRSV circulating | Bank box | R5 | Sigma Long |
| Oracle ERG/USD rate | Oracle box | R4 | Sigma Long (nanoERG per 1 USD) |
| Bank box ID | Bank box | `boxId` | Hex string |
| Oracle box ID | Oracle box | `boxId` | Hex string |

### Box Discovery

**Bank Box** (identified by Bank NFT):
```
Token ID: 7d672d1def471720ca5782fd6473e47e796d9ac0c138d9911346f118b2f6d9d9
Query: /blockchain/box/unspent/byTokenId/{bank_nft_id}?offset=0&limit=1
```

**Oracle Box** (identified by Oracle Pool NFT):
```
Token ID: 011d3364de07e5a26f0c4eef0852cddb387039a921b7154ef3cab22c6eda887f
Query: /blockchain/box/unspent/byTokenId/{oracle_nft_id}?offset=0&limit=1
```

### State Calculation

```rust
// protocols/sigmausd/src/calculator.rs

pub fn calculate_state(input: &ProtocolInput) -> SigmaUsdState {
    // Liabilities = SigUSD value in nanoERG
    // SigUSD has 2 decimals, so divide by 100
    let liabilities_nano = (input.sigusd_circulating as i128)
        * (input.nanoerg_per_usd as i128) / 100;

    // Equity = Reserves - Liabilities
    let equity_nano = (input.bank_erg_nano as i128) - liabilities_nano;

    // Reserve ratio = (reserves / liabilities) * 100
    let reserve_ratio_pct = if liabilities_nano > 0 {
        (input.bank_erg_nano as f64) / (liabilities_nano as f64) * 100.0
    } else {
        f64::MAX
    };

    // SigUSD price = nanoERG per 1 SigUSD (= 1 USD)
    let sigusd_price_nano = input.nanoerg_per_usd;

    // SigRSV price = equity / circulating
    let sigrsv_price_nano = if input.sigrsv_circulating > 0 && equity_nano > 0 {
        (equity_nano / input.sigrsv_circulating as i128) as i64
    } else {
        0
    };

    // Mint/redeem status
    let can_mint_sigusd = reserve_ratio_pct > 400.0;  // MIN_RESERVE_RATIO
    let can_mint_sigrsv = reserve_ratio_pct < 800.0;  // MAX_RESERVE_RATIO
    let can_redeem_sigrsv = reserve_ratio_pct > 400.0;

    SigmaUsdState {
        reserve_ratio_pct,
        sigusd_price_nano,
        sigrsv_price_nano,
        can_mint_sigusd,
        can_mint_sigrsv,
        can_redeem_sigusd: true,  // Always can redeem if have tokens
        can_redeem_sigrsv,
        // ... other fields
    }
}
```

### Sigma Register Parsing

```rust
/// Decode a Sigma Long from register hex string
/// Format: 0x05 (type tag) + VLQ zigzag encoded value
pub fn decode_sigma_long(hex: &str) -> Result<i64, ParseError> {
    let bytes = hex::decode(hex)?;
    if bytes.is_empty() || bytes[0] != 0x05 {
        return Err(ParseError::InvalidTypeTag);
    }

    // VLQ decode
    let mut result: u64 = 0;
    let mut shift = 0;
    for &byte in &bytes[1..] {
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    // Zigzag decode
    let value = if result & 1 == 0 {
        (result >> 1) as i64
    } else {
        -((result >> 1) as i64) - 1
    };

    Ok(value)
}
```

---

## E. MVP Transaction Flow

### Mint SigUSD Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Frontend  │     │  Backend    │     │   Nautilus  │     │  Ergo Node  │
│   (React)   │     │  (Rust API) │     │   Wallet    │     │             │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │                   │
       │ 1. Load page      │                   │                   │
       ├──────────────────►│ GET /sigmausd/state                   │
       │                   ├───────────────────────────────────────►
       │                   │ Query bank/oracle by token ID         │
       │                   │◄───────────────────────────────────────
       │◄──────────────────┤ Return state                          │
       │                   │                   │                   │
       │ 2. Connect wallet │                   │                   │
       ├───────────────────────────────────────►                   │
       │                   │ ergo.connect()    │                   │
       │◄───────────────────────────────────────                   │
       │                   │ Wallet connected  │                   │
       │                   │                   │                   │
       │ 3. Enter amount,  │                   │                   │
       │    click preview  │                   │                   │
       ├──────────────────►│ POST /sigmausd/mint/preview           │
       │                   │ Calculate costs   │                   │
       │◄──────────────────┤ Return preview    │                   │
       │                   │                   │                   │
       │ 4. Confirm mint   │                   │                   │
       │    Get UTXOs      │                   │                   │
       ├───────────────────────────────────────►                   │
       │                   │ api.get_utxos()   │                   │
       │◄───────────────────────────────────────                   │
       │                   │                   │                   │
       │ 5. Build tx       │                   │                   │
       ├──────────────────►│ POST /sigmausd/mint/build             │
       │                   │                   │                   │
       │                   │ Refresh state (freshness check)       │
       │                   ├───────────────────────────────────────►
       │                   │◄───────────────────────────────────────
       │                   │                   │                   │
       │                   │ Build EIP-12 tx   │                   │
       │                   │ - Bank box input  │                   │
       │                   │ - User inputs     │                   │
       │                   │ - Oracle data input (FULL box data!)  │
       │                   │ - Bank output (updated state)         │
       │                   │ - User output (SigUSD tokens)         │
       │                   │ - Change output   │                   │
       │                   │ - Fee output      │                   │
       │◄──────────────────┤ Return unsigned_tx                    │
       │                   │                   │                   │
       │ 6. Sign tx        │                   │                   │
       ├───────────────────────────────────────►                   │
       │                   │ api.sign_tx(unsigned_tx)              │
       │                   │ User reviews & approves               │
       │◄───────────────────────────────────────                   │
       │                   │ Return signed_tx  │                   │
       │                   │                   │                   │
       │ 7. Submit tx      │                   │                   │
       ├───────────────────────────────────────►                   │
       │                   │ api.submit_tx(signed_tx)              │
       │                   │                   ├───────────────────►
       │                   │                   │◄───────────────────
       │◄───────────────────────────────────────                   │
       │                   │ Return tx_id      │                   │
       │                   │                   │                   │
       │ 8. Show success   │                   │                   │
       │    with tx_id     │                   │                   │
```

### Critical Implementation Notes

1. **EIP-12 Data Inputs require FULL box data**
   - NOT just box ID (design doc had this wrong)
   - Include: boxId, transactionId, index, value, ergoTree, assets, creationHeight, additionalRegisters
   - Reference: kadia.io `oracle_box_to_eip12_data_input()` function

2. **Bank box MUST be input[0]**
   - Contract requirement: `SELF.tokens(1) vs OUTPUTS(0).tokens(1)`
   - User boxes follow as input[1], input[2], etc.

3. **Token order must match exactly**
   - Bank output tokens must be in same order as bank input
   - Contract validates by token index, not token ID

4. **Freshness check before building**
   - Re-query bank box to verify it hasn't been spent
   - If box ID changed, someone else interacted - abort and refetch

---

## F. Security Model

### No Key Custody

**Principle**: The app NEVER handles private keys, seeds, or signing.

| Component | Has Access To |
|-----------|--------------|
| Backend (Rust) | Public protocol data, unsigned transactions |
| Frontend (React) | Wallet connection state, UTXOs (via wallet API) |
| Nautilus Wallet | Private keys, signing capability |
| Ergo Node | Blockchain data, transaction submission |

### Trust Boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│ TRUSTED ENVIRONMENT (User's machine)                            │
│                                                                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐         │
│  │ Citadel   │◄──►│  Nautilus   │◄──►│ Private Key │         │
│  │ Backend     │    │  Wallet     │    │ Storage     │         │
│  └──────┬──────┘    └──────┬──────┘    └─────────────┘         │
│         │                  │                                    │
│         │                  │                                    │
│  ┌──────▼──────┐    ┌──────▼──────┐                            │
│  │ Local Ergo  │◄──►│  P2P        │                            │
│  │ Node        │    │  Network    │                            │
│  └─────────────┘    └─────────────┘                            │
└─────────────────────────────────────────────────────────────────┘
```

### What Could Go Wrong (and Mitigations)

| Threat | Mitigation |
|--------|-----------|
| Malicious tx building | Wallet shows tx details before signing |
| Stale protocol state | Freshness check before build; user sees preview |
| Man-in-middle on node | Local connection only (127.0.0.1) |
| Backend compromise | No keys to steal; worst case: bad UX |

---

## G. ErgoPay Path (Post-MVP)

Once MVP is stable, ErgoPay static link generation follows this path:

### Static ErgoPay URL Generation

1. **Build reduced transaction** (not full unsigned tx)
   - Reduced tx replaces input boxes with commitments
   - Requires state context (last 10 block headers)

2. **Serialize with ergo-lib**
   ```rust
   let reduced = tx_context.reduce(&state_context)?;
   let bytes = reduced.sigma_serialize_bytes();
   ```

3. **Encode with URL-safe base64 WITH PADDING**
   - **CRITICAL**: Use `base64::URL_SAFE` (with padding), NOT `URL_SAFE_NO_PAD`
   - Ergo Wallet app decoder expects length multiple of 4

   ```rust
   // CORRECT (design doc had this wrong)
   let encoded = base64::encode_config(&bytes, base64::URL_SAFE);
   let url = format!("ergopay:{}", encoded);
   ```

4. **Generate QR code** for mobile scanning
   ```rust
   let code = QrCode::with_error_correction_level(&url, EcLevel::M)?;
   ```

### Transaction Reduction Requirements

| Input | Source | When |
|-------|--------|------|
| Unsigned transaction | Protocol tx_builder | At tx build |
| Input boxes (full) | Node or wallet | At tx build |
| Data input boxes (full) | Node | At tx build |
| State context (headers) | Node `/blocks/lastHeaders/10` | At reduction |

---

## H. Acceptance Criteria

### Phase 0: Scaffold + Node Connection

- [ ] `cargo build` succeeds for all crates
- [ ] `cargo test` passes
- [ ] `npm run tauri dev` launches app
- [ ] App displays "Connecting..." then node status
- [ ] Capability tier displays correctly (Full/IndexLagging/Basic)
- [ ] Offline node shows appropriate error

### Phase 1: SigmaUSD State Display

- [ ] GET /sigmausd/state returns valid data
- [ ] Bank box discovered by NFT token ID
- [ ] Oracle box discovered by NFT token ID
- [ ] Reserve ratio calculation matches reference implementation (within 0.01%)
- [ ] Prices display correctly
- [ ] Mint/redeem availability indicators correct
- [ ] UI refreshes state on button click

### Phase 2: Mint SigUSD E2E

- [ ] Nautilus connects successfully
- [ ] UTXOs retrieved from wallet
- [ ] Preview shows correct costs
- [ ] Build produces valid EIP-12 JSON
- [ ] Data inputs contain FULL box data (not just IDs)
- [ ] Wallet signs successfully
- [ ] Transaction submits to node
- [ ] TX ID displayed
- [ ] New state reflects mint (after confirmation)

---

## I. Run Commands

> Obsolete: `cargo run -p citadel-api` / `curl http://127.0.0.1:19053/...` — no standalone app HTTP API after Wave 1.

### Development

```bash
# Build all Rust crates
cargo build

# Run tests
cargo test

# Run backend API server standalone (for debugging)
cargo run -p Citadel-api

# Run Tauri dev mode (includes frontend)
cd app
npm install
npm run tauri dev
```

### Production

```bash
# Build release
cargo build --release

# Build Tauri app bundle
cd app
npm run tauri build
```

### Testing with Node

```bash
# Requires running Ergo node at 127.0.0.1:9053
# With extraIndex=true for full functionality

# Quick API test
curl http://127.0.0.1:19053/health
curl http://127.0.0.1:19053/node/status
curl http://127.0.0.1:19053/sigmausd/state
```

---

## J. Divergences from Design Doc

| Design Doc | MVP Plan | Reason |
|------------|----------|--------|
| `data_inputs: vec![convert_to_eip12_data_input(state.oracle_box_id)]` | Full box data required | EIP-12 spec requires full box, not just ID |
| `base64::URL_SAFE_NO_PAD` | `base64::URL_SAFE` (with padding) | Ergo Wallet decoder requires length % 4 == 0 |
| `let parent_id = block.header.parent_id.clone()` | JSON field access | NodeInterface returns JsonValue, not typed struct |
| All 4 SigmaUSD actions | Mint SigUSD only | MVP scope reduction; others follow same pattern |
| apps/desktop/ | app/ | Simpler path for single-platform MVP |

---

## K. File Structure Preview

```
Citadel/
├── Cargo.toml                      # Workspace root
├── MVP_PLAN.md                     # This document
├── crates/
│   ├── Citadel-core/            # Types, errors, config
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs
│   │       ├── errors.rs
│   │       └── config.rs
│   ├── ergo-node-client/          # Node interface wrapper
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── capabilities.rs
│   │       └── queries.rs
│   ├── Citadel-api/             # HTTP API layer
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs
│   │       ├── routes/
│   │       │   ├── mod.rs
│   │       │   ├── health.rs
│   │       │   ├── node.rs
│   │       │   └── sigmausd.rs
│   │       └── dto.rs
│   ├── ergo-tx/                   # Transaction building
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── eip12.rs
│   │       ├── sigma.rs
│   │       └── box_selector.rs
│   └── protocols/
│       └── sigmausd/              # SigmaUSD protocol
│           ├── Cargo.toml
│           └── src/
│               ├── lib.rs
│               ├── constants.rs
│               ├── state.rs
│               ├── calculator.rs
│               └── tx_builder.rs
└── app/                           # Tauri + React
    ├── Cargo.toml                 # Tauri backend
    ├── tauri.conf.json
    ├── src/
    │   ├── main.rs
    │   └── commands.rs
    ├── package.json
    └── src-ui/                    # React frontend
        ├── App.tsx
        ├── contexts/
        │   ├── NodeContext.tsx
        │   └── WalletContext.tsx
        ├── hooks/
        │   └── useSigmaUsd.ts
        └── components/
            ├── NodeStatus.tsx
            ├── WalletConnect.tsx
            └── SigmaUsdCard.tsx
```
