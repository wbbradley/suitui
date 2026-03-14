# suitui

A terminal UI for managing your Sui wallet. Browse accounts, switch environments, and view coin
balances — all from the command line.

## Install

Requires a nightly Rust toolchain (edition 2024).

```sh
cargo install suitui
```

Or install from the repo directly:

```sh
cargo install --git https://github.com/wbbradley/suitui.git
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
| `Esc`/`q`   | Quit (or close dropdown)      |
| `Ctrl+C`    | Quit                          |

## Layout

```
┌─ Accounts ──────────────┐┌─ Network Info ──────────┐
│ * alice  0xabc...1234   ││ Env:     testnet         │
│   bob    0xdef...5678   ││ RPC:     https://...     │
│                         ││ Chain:   4c78...         │
│                         ││ Account: alice           │
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

## License

MIT
