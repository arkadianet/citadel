# Citadel

**Client Interface To Access DeFi on Ergo Ledger**

A local-first desktop app for interacting with DeFi protocols on the Ergo blockchain. Connects directly to your Ergo node -- no cloud services, no tracking, no middlemen.

## Protocols

- **SigmaUSD** -- Mint/redeem SigUSD and SigRSV (AgeUSD stablecoin)
- **Dexy** -- Oracle-pegged stablecoins (DexyUSD, DexyGold) with LP rate comparison
- **DEX** -- Token swaps via Spectrum AMM pools (N2T)
- **Lending** -- Duckpools supply, borrow, withdraw, repay
- **HodlCoin** -- Phoenix HodlCoin mint/burn with bank discovery
- **Rosen Bridge** -- Cross-chain bridging to Cardano and Bitcoin
- **SigmaFi Bonds** -- P2P lending: post collateral, fill bond orders, repay/liquidate
- **MewLock Timelocks** -- Time-locked asset storage with on-chain lock discovery

## Tools

- **Explorer** -- Browse addresses, transactions, and boxes on your node
- **Token Burn** -- Destroy unwanted tokens
- **UTXO Management** -- Consolidate and split boxes

## Requirements

- A running [Ergo node](https://docs.ergoplatform.com/node/install/) (recommended: `extraIndex = true`)
- [Nautilus wallet](https://github.com/capt-nemo429/nautilus-wallet) browser extension for signing

## Install

Download the latest release for your platform from [Releases](https://github.com/arkadianet/citadel/releases).

| Platform | Package |
|----------|---------|
| Ubuntu/Debian | `.deb` |
| Fedora/RHEL | `.rpm` |
| Linux (any) | raw binary |

### Linux dependencies

Citadel uses Tauri v2 which requires GTK/WebKit runtime libraries:

```bash
# Fedora
sudo dnf install gtk3 webkit2gtk4.1 libappindicator-gtk3

# Ubuntu/Debian
sudo apt install libgtk-3-0 libwebkit2gtk-4.1-0 libappindicator3-1
```

## Build from source

```bash
# Prerequisites: Rust, Node.js 18+, Tauri v2 CLI
cargo install tauri-cli --version "^2"

# Install frontend deps
cd frontend && npm install && cd ..

# Dev mode
cargo tauri dev

# Release build
cargo tauri build
```

## Architecture

Rust workspace with one crate per protocol, React/TypeScript frontend, Tauri v2 IPC.

```
crates/
  citadel-core/       Shared types, errors, config
  ergo-node-client/   Node API client with capability detection
  ergo-tx/            EIP-12 tx building, box selection, sigma encoding
  ergopay-core/       Transaction reduction for signing
  ergopay-server/     Local HTTP server for Nautilus signing flow
  protocols/          One crate per protocol
    amm/              Spectrum DEX AMM swaps
    dexy/             Dexy oracle-pegged stablecoins
    hodlcoin/         Phoenix HodlCoin
    lending/          Duckpools lending
    mewlock/          MewLock timelocks
    rosen/            Rosen Bridge cross-chain
    sigmafi/          SigmaFi P2P bonds
    sigmausd/         AgeUSD stablecoin
frontend/src/         React UI + Tauri invoke wrappers
app/                  Tauri desktop shell
```

## Status

Alpha -- under active development. Transactions use real ERG. Use at your own risk.

## License

MIT
