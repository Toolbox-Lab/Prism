# Prism

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.77+-orange?style=for-the-badge&logo=rust" alt="Rust 1.77" />
  <img src="https://img.shields.io/badge/Next.js-16.x-black?style=for-the-badge&logo=next.js" alt="Next.js 16" />
  <img src="https://img.shields.io/badge/TypeScript-5.x-blue?style=for-the-badge&logo=typescript" alt="TS 5" />
  <img src="https://img.shields.io/badge/Documentation-Live-blue?style=for-the-badge&logo=gitbook&logoColor=white" alt="Docs" />
</p>

**Prism** is a developer tool that makes Soroban smart contract errors easy to understand. It takes raw, cryptic error codes from failed transactions and turns them into plain English explanations with suggested fixes. It also lets developers replay past transactions step by step to see exactly what went wrong and why, so they can find and fix the problem in seconds instead of guessing.

## Features

- **Instant Error Decoding**: Decodes Soroban host errors into plain English with suggested fixes.
- **Contract-Specific Resolution**: Cross-references WASM metadata to resolve custom error codes (e.g., `#3` → `InsufficientBalance`).
- **Execution Trace Replay**: Replays transactions against historical ledger state for deep inspection.
- **Resource Profiling**: Identifies budget hotspots and expensive host function calls.
- **Time-Travel Debugging**: Supports breakpoints, step-through execution, and "what-if" re-simulation.
- **Multi-Interface Support**: Available via Rust CLI, VS Code Extension, and a Web Application.
- **Authorization Type Detection**: Distinguishes Ed25519 account signatures from Smart Wallet (contract) authorizations and surfaces the relevant address or contract ID.

## Architecture

Grat is organized as a modular monorepo:

- **Core Library (`crates/core`)**: The shared Rust engine for decoding, replaying, and debugging.
- **CLI (`crates/cli`)**: Powerful command-line interface for terminal-native diagnostics.
- **WASM (`crates/wasm`)**: Core logic compiled to WASM for client-side web integration.
- **Web App (`apps/web`)**: Interactive Next.js 16 dashboard for shareable debug sessions.
- **Server (`apps/server`)**: Async task processor and WebSocket server for trace streaming.

## Tech Stack

- **Core Engine**: Rust (edition 2021)
- **Blockchain**: Stellar Soroban SDK (v21)
- **Web Frontend**: Next.js 16, React 19, TypeScript 5
- **WASM Processing**: wasmparser & wasm-pack
- **CLI Framework**: Clap & Ratatui (TUI)

## Quick Start

1. **Prerequisites**:
   - Rust 1.77 or higher.
   - Node.js 20 or higher.
   - pnpm installed.

2. **Clone and Prepare**:

   ```bash
   git clone https://github.com/grat-soroban/grat.git
   cd Grat
   pnpm install
   ```

3. **Build from Source**:

   ```bash
   cargo build --release
   ```

4. **Run the CLI**:
   ```bash
   ./target/release/grat decode <tx-hash>
   ```

## Documentation

For full technical specifications, architecture deep-dives, and API references, visit our [Live Documentation](https://grat-ddf93e61.mintlify.app/docs/introduction).

<CardGroup cols={2}>
  <Card title="Quickstart" icon="bolt" href="https://grat-ddf93e61.mintlify.app/docs/quickstart">
    Get up and running in under 60 seconds.
  </Card>
  <Card title="CLI Reference" icon="terminal" href="https://grat-ddf93e61.mintlify.app/docs/cli/decode">
    Complete guide to all Grat commands.
  </Card>
  <Card title="Architecture" icon="sitemap" href="https://grat-ddf93e61.mintlify.app/docs/architecture/overview">
    Deep dive into the 3-tier diagnostic engine.
  </Card>
  <Card title="Guides" icon="book" href="https://grat-ddf93e61.mintlify.app/docs/guides/debugging-transactions">
    Real-world walkthroughs and optimization tips.
  </Card>
</CardGroup>

## Use Cases

### Debugging Failed Transactions

Instantly understand why a mainnet transaction failed without redeploying or adding print statements.

### Resource Optimization

Profile contract execution to identify expensive storage reads or CPU-heavy host function calls before deploying to mainnet.

### Regression Testing

Export failed transactions as standalone test cases to ensure bugs are permanently resolved.

## Authorization Type Detection

Grat automatically identifies the kind of authorization used in each Soroban transaction and includes this information in every diagnostic report.

### Supported Types

| Type             | Address Prefix | Description                                                                               |
| ---------------- | -------------- | ----------------------------------------------------------------------------------------- |
| **Ed25519**      | `G...`         | Classic Stellar account signing with its ed25519 key pair.                                |
| **Smart Wallet** | `C...`         | Deployed contract implementing custom signature verification (e.g., multi-sig, passkeys). |

### Detection Logic

Detection is based on the `ScAddress` variant inside each `SorobanAddressCredentials` entry:

- `ScAddress::Account(...)` → **Ed25519** — a standard Stellar account.
- `ScAddress::Contract(...)` → **Smart Wallet** — a deployed contract acting as an authorizer.

`SourceAccount` credentials (where the transaction's own source account implicitly authorizes the entry) are not typed because they carry no separate address.

### Report Fields

Each decoded `DiagnosticReport` includes an `auth_entries` array with one entry per authorization found in the transaction:

```json
{
  "auth_entries": [
    {
      "auth_type": "Ed25519",
      "address": "GABC...XYZ"
    },
    {
      "auth_type": "Smart Wallet",
      "address": "CABC...XYZ",
      "contract_id": "CABC...XYZ"
    }
  ]
}
```

The existing `auth_signatures` field (hex-encoded ed25519 signature bytes) is preserved unchanged for backward compatibility.

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](./CONTRIBUTING.md) for detailed guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](./LICENSE) file for details.

---

Empowering Soroban developers with clear, actionable diagnostics.
