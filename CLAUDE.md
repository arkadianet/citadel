# Citadel

Tauri v2 desktop app for Ergo DeFi protocols. Rust workspace + React/TypeScript frontend.

## Commands

```bash
# Rust
cargo build                              # Build all crates
cargo clippy --workspace --all-targets   # Lint (must pass with zero warnings)
cargo fmt --all -- --check               # Format check
cargo test --workspace                   # Run all tests

# Frontend
cd frontend && npm install && npm run build   # Build frontend
cd frontend && npm run dev                    # Dev server (port 5184)

# App (Tauri)
cargo tauri dev                          # Dev mode (launches Tauri + React)
cargo tauri build                        # Release build -> target/release/bundle/
```

## Architecture

```
app/src/commands/        Tauri IPC handlers (~99 commands), one module per protocol
app/src/lib.rs           App setup, state management, command registration
frontend/src/api/        Typed Tauri invoke wrappers, one file per protocol
frontend/src/components/ React UI components ({Name}Tab.tsx + CSS per protocol)
frontend/src/hooks/      useTransactionFlow (signing), useNotifications
frontend/src/utils/      Shared formatting helpers
crates/
  citadel-core/          Shared types (BoxId, TokenId, Address), errors, config
  ergo-node-client/      Node API client with capability detection
  ergo-tx/               EIP-12 tx building, box selection, sigma encoding
  ergopay-core/          Transaction reduction for signing
  ergopay-server/        Local HTTP server for Nautilus signing flow
  citadel-api/           HTTP API layer and DTOs (alternative to Tauri IPC)
  protocols/
    amm/                 Spectrum DEX AMM swaps
    dexy/                Dexy oracle-pegged stablecoins
    hodlcoin/            Phoenix HodlCoin
    lending/             Duckpools lending
    mewlock/             MewLock timelocks
    rosen/               Rosen Bridge cross-chain
    sigmafi/             SigmaFi P2P bonds
    sigmausd/            AgeUSD stablecoin
```

## Key Patterns

- **Tauri commands**: Flat arguments (not structs), `State<'_, AppState>`, return `Result<T, String>`
- **Signing flow**: Build EIP-12 tx -> `reduce_transaction` -> ErgoPay request -> poll status
- **EIP-12 transactions**: Use `Eip12UnsignedTx` from `ergo_tx`, all values as Strings
- **Frontend API**: Types + invoke wrappers in `frontend/src/api/`, one file per protocol
- **Transaction signing**: `useTransactionFlow` hook handles build -> sign -> poll lifecycle
- **CSS**: Custom CSS variables (--card-bg, --border-color, --emerald-400), dark theme, no CSS framework
- **ergo-lib**: Version 0.28.0 from sigma-rust develop branch

## Gotchas

- **ErgoTree RedeemerPropBytes**: Must be full P2PK ErgoTree bytes (`0008cd` + pubkey), NOT raw 33-byte pubkey. Using raw pubkey causes **fund loss**.
- **ErgoTree::with_constant**: Consumes self, enforces type matching at runtime. Templates must start with `0x19` (constant segregation header).
- **Pool fee_num from R4**: AMM pools store fee in R4 register. Never hardcode 997 -- different pools have different fees. Wrong fee -> "Script reduced to false".
- **Box<dyn Error> + Send**: Must convert errors to String before `.await` in Tauri commands.
- **Register access**: Use `ergo_box.additional_registers.get_constant(NonMandatoryRegisterId::R4)`, NOT `get_ordered_values()`.
- **MinFeeBox selection**: First box from token search is often a "bank" box with no registers. Filter for boxes where R4 is populated.
- **Register types vary**: R5-R9 may be `Coll[Coll[SInt]]` (i32) not `Coll[Coll[SLong]]` (i64). Try i64 first, fallback to i32.
- **Sigma Int serialization**: Type byte `0x04` + ZigZag-VLQ value. Use `Constant::sigma_parse_bytes()` to decode R4 hex strings.
- **Token ID comparison**: Use `hex::encode(token.token_id.as_ref())` -- TokenId doesn't impl Display.
- **Constant-segregated ErgoTree (0x19 prefix)**: `Address::recreate_from_ergo_tree()` strips constants, producing a WRONG P2S address. For contracts with segregated constants, hardcode the known P2S address string for box queries instead of deriving it.

## Adding a New Protocol

1. Create crate at `crates/protocols/{name}/` with `constants.rs`, `fetch.rs`, `state.rs`, `tx_builder.rs`
2. Add to workspace in root `Cargo.toml`
3. Add Tauri commands in `app/src/commands/{name}.rs`, register in `commands/mod.rs`
4. Register commands in `app/src/lib.rs` invoke_handler
5. Add frontend API wrapper in `frontend/src/api/{name}.ts`
6. Add UI component in `frontend/src/components/{Name}Tab.tsx`
7. Add sidebar entry in `Sidebar.tsx` and view routing in `App.tsx`
