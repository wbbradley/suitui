# suitui

A terminal UI for managing your Sui wallet. Browse accounts, switch environments, and view coin
balances — all from the command line.

## Install

```sh
cargo install suitui
```

Or build from source:

```sh
git clone https://github.com/wbbradley/suitui.git
cd suitui
cargo build --release
```

## Usage

```sh
suitui
```

By default, suitui reads your Sui client config from `~/.sui/sui_config/client.yaml`. To use a
different config file:

```sh
suitui --config /path/to/client.yaml
```

## Keybindings

| Key          | Action                        |
|--------------|-------------------------------|
| `↑`/`↓`     | Navigate                      |
| `k`/`j`     | Navigate (vim-style)          |
| `Tab`        | Cycle focus between panes     |
| `Enter`      | Select account or environment |
| `e`          | Open environment selector     |
| `i`          | Inspect object/address by ID  |
| `t`          | Transaction history           |
| `s`          | Send tokens                   |
| `r`          | Refresh data                  |
| `Ctrl+d`/`Ctrl+u` | Half-page scroll down/up |
| `Esc`/`q`   | Quit (or close dropdown)      |
| `Ctrl+C`    | Quit                          |

## Features

- Object and address inspector with hyperlinked navigation
- Checkpoint inspector with navigable transaction links
- Transaction history and transaction inspector
- Token transfers (select coin, enter recipient/amount, review, execute)
- Per-coin decimal-aware balance formatting
- Keystore integration for signing transactions

## Layout

```
┌─ Accounts ──────────────┐┌─ Network Info ──────────┐
│ * alice  0xabc...1234   ││ Env:     testnet        │
│   bob    0xdef...5678   ││ RPC:     https://...    │
│                         ││ Chain:   4c78...        │
│                         ││ Account: alice          │
├─ Coins ─────────────────┤│                         │
│ SUI          12.500     ││                         │
│ USDC          5.000     ││                         │
│                         ││                         │
└─────────────────────────┘└─────────────────────────┘
 Enter: Select | Tab: Focus | e: Env | q: Quit
```

- **Accounts** — Your wallet accounts with aliases and addresses. `*` marks the active account.
- **Coins** — Coin balances for the selected account, fetched via gRPC.
- **Network Info** — Active environment, RPC endpoint, chain ID, and account.

## Disclaimer

This software is provided as-is, with no warranty of any kind. The authors are not responsible for
any loss of funds, tokens, or digital assets, nor for any security or privacy issues that may arise
from using this software. You are solely responsible for your accounts, keys, and transactions.
Use at your own risk.

## License

MIT
