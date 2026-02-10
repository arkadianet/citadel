# Citadel

**Client Interface To Access DeFi on Ergo Ledger**

A local-first desktop app for interacting with DeFi protocols on the Ergo blockchain. Connects directly to your Ergo node -- no cloud services, no tracking, no middlemen.

## Protocols

- **SigmaUSD** -- Mint/redeem SigUSD and SigRSV
- **Dexy** -- Oracle-pegged stablecoins (DexyUSD, DexyGold)
- **DEX** -- Token swaps via Spectrum AMM pools
- **Lending** -- Duckpools supply, borrow, withdraw, repay
- **HodlCoin** -- Phoenix HodlCoin mint/burn
- **Rosen Bridge** -- Cross-chain bridging to Cardano and Bitcoin

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

## Status

Alpha -- under active development. Transactions use real ERG. Use at your own risk.

## License

MIT
