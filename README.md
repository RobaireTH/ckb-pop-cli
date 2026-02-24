# ckb-pop-cli

A keyless CLI for the Proof of Presence (PoP) protocol on Nervos CKB. Create events, open attendance windows with rotating QR codes, and mint soulbound badges — all without storing a private key locally.

## Table of Contents

- [What It Does](#what-it-does)
- [How It Works](#how-it-works)
- [Installation](#installation)
- [Getting Help](#getting-help)
- [Initial Setup](#initial-setup)
- [Command Reference](#command-reference)
- [Workflows](#workflows)
- [On-Chain Contracts](#on-chain-contracts)
- [Signer Architecture](#signer-architecture)
- [Configuration](#configuration)
- [Development](#development)
- [Tech Stack](#tech-stack)

---

## What It Does

ckb-pop-cli is the organizer and attendee tool for [ckb-pop.xyz](https://ckb-pop.xyz), a Proof of Presence protocol built on Nervos CKB. It serves two roles:

**For event organizers:**

- Register an event with the ckb-pop backend and anchor it immutably on-chain.
- Open a timed attendance window that displays rotating QR codes in the terminal.
- Manually mint badges for specific attendees when needed.

**For attendees:**

- Scan an organizer's QR code and run a single command to verify attendance and mint a soulbound badge to your wallet address.

All signing is delegated to an external wallet (browser-based, Ledger, passkey, or WalletConnect). No private keys are ever stored by this tool.

---

## How It Works

```
Organizer                              Attendee
---------                              --------
ckb-pop event create                   ckb-pop attend "<qr_data>"
  └─ Signs creation proof via wallet     └─ Parses QR payload
  └─ POSTs to backend registry           └─ Verifies HMAC freshness (60s)
  └─ Builds event-anchor tx              └─ Signs attendance proof via wallet
  └─ Broadcasts on-chain                 └─ Builds dob-badge tx
  └─ Backend activates event             └─ Broadcasts on-chain

ckb-pop event window <event_id>
  └─ Signs window-open proof
  └─ Derives window secret (HMAC key)
  └─ Displays rotating QR every 30s
  └─ Each QR encodes: event_id|timestamp|hmac
```

The two on-chain type scripts (`dob-badge` and `event-anchor`) enforce uniqueness constraints at the protocol level. A badge cannot be minted twice for the same `(event_id, address)` pair, and no two anchors can exist for the same `(event_id, creator)` pair. The CKB chain is the source of truth; the backend provides event discovery and indexing.

---

## Installation

**Prerequisites:** Rust toolchain (stable, 2021 edition).

```sh
git clone https://github.com/RobaireTH/ckb-pop-cli
cd ckb-pop-cli
cargo build --release
```

Install the binary to your PATH:

```sh
cargo install --path .
```

Or run directly without installing:

```sh
cargo run -- [COMMAND] [OPTIONS]
```

---

## Getting Help

Every command and subcommand accepts `--help` (or `-h`) to print usage and available flags. The root command also accepts `--version` (or `-V`).

```sh
# Print version
ckb-pop --version
ckb-pop -V

# Top-level help: lists all commands and global flags
ckb-pop --help
ckb-pop -h

# Help for a command group
ckb-pop signer --help
ckb-pop event --help
ckb-pop badge --help
ckb-pop tx --help

# Help for a specific subcommand
ckb-pop signer set --help
ckb-pop signer connect --help
ckb-pop signer status --help

ckb-pop event create --help
ckb-pop event list --help
ckb-pop event show --help
ckb-pop event window --help

ckb-pop attend --help

ckb-pop badge mint --help
ckb-pop badge list --help
ckb-pop badge verify --help

ckb-pop tx status --help
```

---

## Initial Setup

### 1. Set your signing method

```sh
ckb-pop signer set --method browser
```

The browser signer is the default and requires no additional hardware. See [Signer Architecture](#signer-architecture) for other options.

### 2. Connect your wallet

```sh
ckb-pop signer connect
```

This opens a local signing page in your browser. Connect your CKB wallet (JoyID, MetaMask, UniSat, OKX, or any CCC-compatible wallet), and the CLI stores your address in `~/.ckb-pop/config.toml`.

### 3. Verify your configuration

```sh
ckb-pop signer status
```

---

## Command Reference

### Global Options

These flags apply to all commands and override the values in `~/.ckb-pop/config.toml`.

| Flag                  | Description                                                               | Default     |
| --------------------- | ------------------------------------------------------------------------- | ----------- |
| `--network <NETWORK>` | Target network (`testnet` or `mainnet`)                                   | `testnet`   |
| `--rpc-url <URL>`     | Override the CKB RPC endpoint URL                                         | From config |
| `--signer <METHOD>`   | Override signing method (`browser`, `ledger`, `passkey`, `walletconnect`) | From config |
| `--address <ADDRESS>` | Override the active CKB address                                           | From config |

---

### `signer` — Manage Signing Configuration

#### `signer set`

Set the default signing method.

```sh
ckb-pop signer set --method <METHOD>
```

**Options:**

- `--method <METHOD>` — `browser`, `ledger`, `passkey`, or `walletconnect`

#### `signer connect`

Open a browser signing page to authenticate your wallet and store your address in the config.

```sh
ckb-pop signer connect
```

#### `signer status`

Display the current signing configuration: method, stored address, network, and RPC URL.

```sh
ckb-pop signer status
```

---

### `event` — Create and Query Events

#### `event create`

Register a new event with the ckb-pop backend and create an immutable on-chain anchor.

```sh
ckb-pop event create \
  --name "My Conference" \
  --description "Annual tech conference." \
  [--image-url <URL>] \
  [--location <LOCATION>] \
  [--start <ISO8601>] \
  [--end <ISO8601>]
```

**Required:**

- `--name <NAME>` — Event name.
- `--description <DESC>` — Event description.

**Optional:**

- `--image-url <URL>` — URL for the event image or badge art.
- `--location <LOCATION>` — Event location.
- `--start <ISO8601>` — Event start time (e.g., `2026-05-15T09:00:00Z`).
- `--end <ISO8601>` — Event end time.

**What happens:**

1. Prints your creator address so you can verify it matches your connected wallet before signing.
2. Prompts your wallet to sign a creation proof.
3. Posts the proof and metadata to the backend, which returns a canonical `event_id`.
4. Builds and broadcasts an `event-anchor` transaction on-chain.
5. Polls for confirmation (~90 seconds), then activates the event on the backend.
6. Prints the `event_id` and the event URL on [ckb-pop.xyz](https://ckb-pop.xyz).

#### `event list`

List event anchors on-chain, optionally filtered by creator address.

```sh
ckb-pop event list [--creator <ADDRESS>]
```

#### `event show`

Show the details of a specific event anchor.

```sh
ckb-pop event show <EVENT_ID>
```

#### `event window`

Open a timed attendance window and display rotating QR codes in the terminal.

```sh
ckb-pop event window <EVENT_ID> [--duration <MINUTES>]
```

**Options:**

- `--duration <MINUTES>` — How long the window stays open. Default: `60`.

**What happens:**

1. Prompts your wallet to sign a window-opening proof.
2. Derives a window secret from the event ID, start time, and your signature.
3. Clears the screen and displays a QR code that refreshes every 30 seconds.
4. Each QR encodes `event_id|timestamp|hmac` where the HMAC is derived from the window secret.
5. Attendees have a 60-second window to scan and use any given QR code.
6. Exits when the duration expires or you press Ctrl-C.

---

### `attend` — Record Attendance and Mint a Badge

Parse a QR code, verify it, sign an attendance proof, and mint a soulbound badge to your address.

```sh
ckb-pop attend "<QR_DATA>"
```

**Example:**

```sh
ckb-pop attend "abc123def456...|1748000000|deadbeef01234567"
```

**What happens:**

1. Parses the QR payload: `event_id|timestamp|hmac`.
2. Checks that the QR timestamp is within the last 60 seconds (freshness).
3. Verifies the HMAC against the event's window secret.
4. Prompts your wallet to sign an attendance proof.
5. Builds a `dob-badge` transaction and broadcasts it on-chain.
6. Prints the badge transaction hash.

> The QR data string is typically produced by scanning a terminal QR code. You can also paste it directly from the organizer.

---

### `badge` — Query and Mint Badges

#### `badge mint`

Manually mint a badge for a specific recipient. This is an organizer action for cases where the attendee cannot run the CLI themselves.

```sh
ckb-pop badge mint <EVENT_ID> --to <ADDRESS>
```

**Options:**

- `--to <ADDRESS>` — The recipient's CKB address.

#### `badge list`

List all badges held by a given address.

```sh
ckb-pop badge list --address <ADDRESS>
```

#### `badge verify`

Check whether a specific badge exists on-chain for a given event and address.

```sh
ckb-pop badge verify <EVENT_ID> <ADDRESS>
```

---

### `tx` — Check Transaction Status

#### `tx status`

Query the confirmation status of a transaction by its hash.

```sh
ckb-pop tx status <TX_HASH>
```

---

## Workflows

### Create an Event

```sh
# 1. Set up your signer (once)
ckb-pop signer set --method browser
ckb-pop signer connect

# 2. Create the event
ckb-pop event create \
  --name "CKB Builders Day" \
  --description "A day for CKB builders to meet and collaborate." \
  --location "San Francisco" \
  --start "2026-06-01T10:00:00Z" \
  --end "2026-06-01T18:00:00Z"

# Output includes your event_id and URL on ckb-pop.xyz
```

### Run an Attendance Window

```sh
# Open a 90-minute window with rotating QR codes
ckb-pop event window <EVENT_ID> --duration 90

# Terminal shows a QR code that refreshes every 30 seconds.
# Attendees scan and run: ckb-pop attend "<qr_data>"
```

### Attend an Event

```sh
# Run this after scanning the organizer's QR code
ckb-pop attend "abc123...|1748000000|deadbeef01234567"

# Your badge transaction hash is printed on success.
# The badge appears in your gallery on ckb-pop.xyz.
```

### Verify Attendance On-Chain

```sh
# Check if a badge exists for any address and event
ckb-pop badge verify <EVENT_ID> <ADDRESS>

# List all badges for an address
ckb-pop badge list --address ckt1qzda...
```

---

## On-Chain Contracts

Both contracts are RISC-V type scripts deployed on CKB testnet. They use `hash_type: "type"` (the type ID pattern), which means their identity is stable across upgrades.

**Deployment transaction:** `0x40ca450088affcbc5f2d8a06545566717106e99caad306776704aab9f3127934`

### `dob-badge` — Soulbound Attendance Badge

- **Code hash:** `0xb36ed7616c4c87c0779a6c1238e78a84ea68a2638173f25ed140650e0454fbb9`
- **Deploy index:** 0
- **Data hash:** `0x6e550910a640a41f21614d97d1d7b8c1830cbf11cce5c868c76a6fd0f25ba7a9`

**Type script args (64 bytes):** `SHA256(event_id) || SHA256(recipient_address)`

The script enforces one badge per `(event_id, recipient_address)` pair. It rejects any transaction that has a badge cell with matching args in both inputs and outputs, making badges soulbound — they cannot be transferred.

**Cell data (34 bytes):**

| Bytes | Content                                     |
| ----- | ------------------------------------------- |
| 0     | Version (`0x01`)                            |
| 1     | Flags (`0x01` = off-chain metadata present) |
| 2–33  | SHA256 hash of the off-chain content JSON   |

### `event-anchor` — Immutable Event Record

- **Code hash:** `0xd565d738ad5ac99addddc59fd3af5e0d54469dc9834cf766260c7e0d23c70b37`
- **Deploy index:** 1
- **Data hash:** `0x24dfb1d2a7aca1e967c405e60017204459f3f8fe80e2c21683c4288ad4f5befb`

**Type script args (64 bytes):** `SHA256(event_id) || SHA256(creator_address)`

The script enforces one anchor per `(event_id, creator_address)` pair. Once created, the anchor cannot be destroyed or modified, creating an immutable on-chain record of the event's existence.

**Cell data:** JSON object containing `event_id`, `creator_address`, `metadata_hash`, and `created_at_block`.

### Indexer Queries

Both contracts use the 64-byte args format so that the CKB indexer can find all badges or anchors for a given event using `script_search_mode: "prefix"`. The first 32 bytes (`SHA256(event_id)`) serve as the prefix for discovery, and the second 32 bytes (`SHA256(address)`) narrow results to a specific holder.

### Mainnet

Neither contract is deployed on mainnet yet.

---

## Signer Architecture

The CLI defines a `Signer` trait that abstracts signing across all supported methods:

```rust
trait Signer: Send + Sync {
    fn address(&self) -> &str;
    async fn sign_message(&self, message: &str) -> Result<String>;   // 65-byte hex signature
    async fn sign_transaction(&self, tx: Transaction) -> Result<Transaction>;
}
```

All commands follow the same pattern: build an unsigned transaction → route to the active signer → broadcast the signed transaction.

### Signing Methods

| Method          | How It Works                                                                                                                                                                                                                                                                                                            |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `browser`       | The CLI starts a local HTTP server on a random port and opens a bundled signing page in your browser. The page loads the CCC SDK (embedded in the binary), connects to your wallet, presents the signing request, and POSTs the result back to the local server. No external network calls are needed to load the page. |
| `ledger`        | USB HID communication with a Ledger hardware wallet running the CKB Ledger app.                                                                                                                                                                                                                                         |
| `passkey`       | FIDO2 assertion via platform authenticator. CKB supports passkey-based lock scripts natively.                                                                                                                                                                                                                           |
| `walletconnect` | The CLI displays a WalletConnect v2 QR code in the terminal. A mobile wallet scans it and approves the signing request.                                                                                                                                                                                                 |

The browser signer is the default because it supports the widest range of wallets with zero hardware dependencies.

### Browser Signer Details

The bundled signing page (`src/signer/ccc-bundle.js`, ~836 KB) is compiled into the binary at build time using `include_bytes!()`. When invoked, the CLI:

1. Binds a TCP listener on a port in the 17500–17599 range.
2. Serves the HTML signing page and CCC bundle from memory.
3. Opens the page in the system's default browser.
4. The page connects to your wallet and presents the signing request.
5. On approval, the JavaScript converts the CCC SDK's camelCase output to snake_case (CKB RPC format) and POSTs it to `/callback`.
6. The CLI receives the signed data and continues.

---

## Configuration

The config file is created automatically at `~/.ckb-pop/config.toml` on first use.

```toml
[network]
default = "testnet"
testnet_rpc = "https://testnet.ckb.dev/rpc"
mainnet_rpc = "https://mainnet.ckb.dev/rpc"

[signer]
method = "browser"         # browser | ledger | passkey | walletconnect
address = "ckt1qzda..."    # Set by 'ckb-pop signer connect'
```

All config values can be overridden per-command with the [global flags](#global-options).

No database or local cache is maintained beyond this config file. The chain is the source of truth for badges and anchors.

---

## Development

### Running Tests

Unit tests are embedded in each module and cover cryptographic operations, config serialization, QR parsing, and transaction structure.

```sh
cargo test --lib
```

Integration tests require a live testnet connection and a configured signer. They are marked `#[ignore]` and must be run explicitly.

```sh
cargo test --test integration -- --ignored --nocapture
```

The integration test suite includes an end-to-end test (`event_creation_and_badge_mint_e2e`) that exercises the full workflow: event creation, anchor confirmation, badge minting, and on-chain verification.

### Project Structure

```
src/
├── main.rs              # Entry point
├── lib.rs               # Module declarations
├── cli.rs               # Command definitions (clap)
├── config.rs            # Config file management
├── contracts.rs         # On-chain contract addresses and cell deps
├── crypto.rs            # SHA256, HMAC, QR generation and verification
├── rpc.rs               # CKB RPC and indexer client
├── tx_builder.rs        # Unsigned transaction construction
├── commands/
│   ├── mod.rs           # Shared command helpers
│   ├── signer.rs        # signer subcommands
│   ├── event.rs         # event subcommands
│   ├── attend.rs        # attend command
│   ├── badge.rs         # badge subcommands
│   └── tx.rs            # tx subcommands
└── signer/
    ├── mod.rs            # Signer trait
    ├── browser.rs        # Browser signer implementation
    └── ccc-bundle.js     # Pre-built CCC SDK (embedded asset)
tests/
└── integration.rs        # Integration tests (require network)
docs/
└── plans/                # Design documents
```

---

## Tech Stack

| Crate                   | Purpose                                                 |
| ----------------------- | ------------------------------------------------------- |
| `ckb-sdk` 5.x           | CKB RPC client, transaction building, address handling  |
| `ckb-types` 1.x         | CKB data types (`H256`, `Script`, `Cell`, etc.)         |
| `ckb-jsonrpc-types` 1.x | CKB RPC JSON serialization                              |
| `clap` 4                | CLI argument parsing                                    |
| `tokio` 1               | Async runtime                                           |
| `reqwest` 0.12          | HTTP client for browser signer callback and backend API |
| `serde` / `toml`        | Config serialization                                    |
| `sha2` / `hmac`         | Event ID generation and QR HMAC verification            |
| `qrcode`                | Terminal QR code display                                |
| `anyhow` / `thiserror`  | Error handling                                          |
| `chrono`                | Timestamp handling                                      |
| `opener`                | Open URLs in the system browser                         |
