# Architecture Overview

Grat is built as a modular diagnostic suite for the Stellar Soroban ecosystem.

## Modular Monorepo Structure

### Core Logic (`crates/`)

- **`grat-core`**: The heart of the platform. It handles error classification, ledger state reconstruction, and transaction simulation.
- **`grat-cli`**: The Rust-native command-line tool providing high-speed diagnostics.
- **`grat-wasm`**: Core engine compiled to WebAssembly, enabling client-side decoding in the web dashboard.

### Applications (`apps/`)

- **`grat-web`**: A Next.js 16 frontend for interactive debugging and sharing trace sessions.
- **`grat-server`**: A Node.js/Rust backend that handles heavy lifting for the web app (S3/GCS history archive fetching and state reconstruction).

## Diagnostic Engines

### 1. Decode Engine (Tier 1)

Identifies the failure point in the transaction XDR, resolves custom contract errors via WASM metadata, and enriches reports with plain-English descriptions and fixes.

### 2. Replay Engine (Tier 2)

Fetches historical ledger entries from Stellar History Archives and reconstructs the state locally. It then executes the transaction in a modified Soroban host to capture a full execution trace.

### 3. Time-Travel Debugger (Tier 3)

Built on top of the Replay Engine, it provides a breakpoint controller and a "What-If" engine for non-destructive re-simulation of transactions with modified inputs.

## Shared Infrastructure

- **XDR Codec**: High-level wrappers around `stellar-xdr`.
- **Archive Client**: Parallelized fethcing of ledger snapshots from S3/GCS.
- **Taxonomy Database**: A versioned catalog of every known Soroban error code.
