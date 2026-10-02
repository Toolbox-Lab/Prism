# Grat Architecture

## High-Level Component Map

```
┌──────────────────────────────────────────────┐
│                 Interfaces                    │
│  CLI (Rust)  │  VS Code Extension  │  Web    │
├──────────────────────────────────────────────┤
│              grat-core (Rust)                │
│   Decode Engine  │  Replay Engine             │
├──────────────────────────────────────────────┤
│           Shared Infrastructure               │
│  XDR │ ContractSpec │ RPC │ Archive │ Cache   │
└──────────────────────────────────────────────┘
```

## Crate Structure

- **`grat-core`** — Core library. All decode, replay, and debugger logic.
- **`grat` (CLI)** — Command-line binary. Thin layer over `grat-core`.
- **`grat-wasm`** — WASM target. Exposes Tier 1 decode for browsers.

## Data Flow

1. CLI/Web/Extension receives a TX hash
2. `grat-core` fetches transaction via Soroban RPC
3. Decode Engine classifies the error using the taxonomy database
4. Contract Error Resolver fetches WASM and parses contractspec
5. Report Generator assembles a `DiagnosticReport`
6. Interface renders the report

## Key Design Decisions

- All logic in Rust (`grat-core`), interfaces are thin wrappers
- VS Code extension shells out to CLI binary — no duplicate logic
- Web Tier 1 runs client-side via WASM, Tier 2-3 require backend
- Error taxonomy is community-contributable TOML, not code
