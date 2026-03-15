# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.4.1] - 2026-03-15

### Added
- Auto-shorten well-known package IDs in type displays (e.g. `0x0...0002::coin::Coin` → `0x2::coin::Coin`)
- Ctrl-d/Ctrl-u half-page scrolling in all views
- Network name shown in coins pane title
- Inspect modal accepts Object ID, Address, or base58 Tx Digest

### Changed
- j/k/arrow selection now clamps at list boundaries instead of wrapping around

## [0.4.0] - 2026-03-15

### Added
- Transaction inspector with detail view, navigation, and scrolling
- Per-event sender display in transaction inspector
- Address inspector for browsing owned objects and balances
- Polymorphic inspect stack enabling Object ↔ Address ↔ Transaction navigation
- Graceful object-not-found handling with "try as address" fallback
- Per-coin decimal metadata fetching for accurate balance formatting
- Inspector auto-follow for selected links during scrolling

### Fixed
- Coin type display showing trailing `>` (e.g. `SUI>` instead of `SUI`)
- Transaction history and transfers now use selected address instead of active address

## [0.3.0] - 2026-03-14

### Added
- Token transfers with coin selection, recipient/amount input, and PTB execution
- Keystore loading and transaction signing (Ed25519, Secp256k1, Secp256r1)
- Transaction history view with batch fetch and dedup
- Object inspector with dynamic fields, properties, and hyperlinked navigation
- Address input modal for inspecting arbitrary object IDs
- View stack infrastructure for multi-level navigation
- Custom coin decimals via GetCoinInfo RPC
- Chain ID fetching when missing from config
- `--config` CLI flag for custom config path
- Per-key TTL cache and in-flight dedup for coin balance fetches
- Config persistence (active env/account saved to client.yaml on Enter)

### Fixed
- Environment switch not updating Network Info or triggering coin refetch

## [0.2.1] - 2026-03-14

Initial crates.io release with account management, coin balances via gRPC, environment switching,
and TUI scaffolding.
