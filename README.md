# Prism

## Soroban transaction debugger: from cryptic error to root cause in one command.

Prism turns opaque Soroban errors into plain English, replays historical transactions against reconstructed ledger state, and lets you step through contract execution with full time-travel debugging. It handles everything from decoding `Error(Contract, #3)` into the actual enum name defined in the contract, to showing you exactly which host function call consumed the last byte of your CPU budget.

If a Soroban transaction failed and you need to know why, Sextant is the tool.

---

## Why Prism Exists

Soroban errors are notoriously opaque. `HostError(Budget, LimitExceeded)` tells you the budget was exceeded but not which call was expensive. `Error(Contract, #3)` tells you the contract returned error number three but not what three means. `Error(Storage, InternalError)` tells you almost nothing at all.

Today, developers debug these errors by grepping through Stellar Core source code, asking on Discord, or adding print statements and redeploying. There is no tool that decodes errors comprehensively, no tool that cross-references contract metadata to resolve custom error codes, and no tool that can replay a failed mainnet transaction locally to show you what happened inside the invocation.

Prism is the missing diagnostic layer. It is purpose-built for Soroban developers — CLI-native, IDE-integrated, and designed with the assumption that you want the answer in seconds, not hours.

---

## Features

- **Instant error decoding** — any Soroban host error decoded to plain English with root cause analysis and suggested fixes, powered by a comprehensive error taxonomy database covering every known error code
- **Contract-specific error resolution** — fetches the contract's WASM metadata and cross-references `contractspecv0` to resolve `#3` into `InsufficientBalance` (or whatever the developer named it), including doc comments
- **Full transaction context** — decoded function arguments, auth requirements vs what was provided, resource consumption vs limits, fee breakdown, and the complete invocation chain for nested cross-contract calls
- **Historical state reconstruction** — fetches the exact ledger state at the time of any transaction from Stellar History Archives and reconstructs it locally for replay
- **Execution trace replay** — replays the transaction against historical state and captures every host function call, every storage read/write, every auth check, every event emission, and every budget checkpoint as a structured timeline
- **Resource profiling** — identifies the most expensive operations in a transaction, flags budget hotspots, and warns when consumption is dangerously close to limits
- **State diff viewer** — side-by-side comparison of ledger state before and after execution, with contract values decoded into human-readable field names
- **Interactive time-travel debugging** — breakpoints, step-through execution, state inspection at any point, and a what-if mode that lets you modify inputs and re-simulate to see what would have changed
- **Four interfaces** — Rust CLI, VS Code extension with inline error decoding, web application with shareable debug sessions, and a reusable core library published as both a Rust crate and WASM package

---

## Requirements

- Rust 1.77 or higher (for building from source)
- Access to a Soroban RPC endpoint (public testnet and mainnet endpoints work out of the box)
- Stellar Core binary (required only for Tier 2–3 replay features; Sextant manages it via captive core)

---

## Installation

```bash
cargo install sextant
```

Pre-built binaries for Linux, macOS, and Windows are available on the GitHub Releases page.

---

## How It Works

Prism is organized into three tiers of depth. Each tier is independently useful and builds on the one before it. Most debugging sessions end at Tier 1. The rare hard cases escalate to Tier 2. Tier 3 is for when you need to understand execution at the instruction level.

### Tier 1 — Decode

The fast path. Sextant fetches the transaction, classifies the error, looks it up in the taxonomy database, resolves contract-specific error codes against the WASM metadata, and returns a structured diagnostic report. This runs in under two seconds and answers the question every developer asks first: *what does this error actually mean?*

The error taxonomy database is a versioned, structured catalog of every known Soroban host error. Each entry includes the plain English description, ranked common causes, ranked suggested fixes, related errors, and links to the relevant Stellar Core source. The database is updated with every protocol upgrade and published as a standalone file so other ecosystem tools can reuse it.

For contract-specific errors, Sextant fetches the contract's WASM bytecode, parses the embedded `contractspecv0` metadata, and extracts the error enum definitions. Error `#3` stops being a mystery and becomes the name the developer gave it, with any doc comments they wrote.

### Tier 2 — Trace

The diagnostic path. Sextant reconstructs the exact ledger state that existed when the transaction executed — using Soroban RPC for recent transactions and Stellar History Archives for older ones — then replays the transaction locally in a modified Soroban sandbox that emits a trace event for every operation.

The output is a hierarchical execution timeline: the transaction at the top, contract invocations nested beneath, individual host function calls within each invocation, with storage accesses, auth checks, budget consumption, and event emissions annotated at every level. The failure point is highlighted with the Tier 1 decoded error inline.

This also produces a resource profile (where the CPU and memory budget was spent) and a state diff (what changed or would have changed in the ledger).

### Tier 3 — Time-Travel

The investigation path. From the Tier 2 replay, Sextant adds interactive debugging: set breakpoints on function entries, storage accesses, budget thresholds, or specific contract addresses. Step through execution one host call at a time. Inspect the full state at any point — storage values, call stack, remaining budget, auth context.

The what-if mode lets you modify inputs (function arguments, ledger state, auth context, resource limits) and re-simulate from any checkpoint. Sextant runs both the original and modified execution and shows you a diff of where they diverge. This answers questions like "would this have succeeded if the slippage tolerance was 2% instead of 1%?" without redeploying anything.

---

## Architecture

Prism is organized into a core library that all interfaces share, a shared infrastructure layer for network and data access, and four interface targets that consume the core.

```
┌─────────────────────────────────────────────────────┐
│                     Interfaces                       │
│                                                      │
│   CLI (Rust)    VS Code Extension    Web App (React) │
│       │               │                   │          │
│       └───────────────┼───────────────────┘          │
│                       ▼                              │
│           ┌───────────────────────┐                  │
│           │   Prism-core (Rust) │                  │
│           │                       │                  │
│           │  Decode Engine        │                  │
│           │  Replay Engine        │                  │
│           │  Breakpoint Controller│                  │
│           │  What-If Engine       │                  │
│           └───────────┬───────────┘                  │
│                       │                              │
│           ┌───────────▼───────────┐                  │
│           │  Shared Infrastructure│                  │
│           │                       │                  │
│           │  XDR Codec            │                  │
│           │  ContractSpec Decoder │                  │
│           │  Soroban RPC Client   │                  │
│           │  History Archive Client│                 │
│           │  Error Taxonomy DB    │                  │
│           │  Cache Layer          │                  │
│           └───────────────────────┘                  │
└─────────────────────────────────────────────────────┘
```

### Decode Engine
Classifies errors from transaction result XDR, resolves contract-specific codes via WASM metadata, processes diagnostic event chains for nested error cascades, and enriches reports with full transaction context. Produces a structured `DiagnosticReport` that any interface can render.

### Replay Engine
Reconstructs historical ledger state using the History Archive client and Stellar Captive Core. Executes transactions in a modified Soroban sandbox that intercepts every host function call and emits structured trace events. The Trace Collector assembles events into a hierarchical execution tree annotated with budget, storage, and auth data.

### Breakpoint Controller
Evaluates breakpoint conditions at each trace point during replay. Supports breakpoints on function entry/exit, storage access patterns, budget thresholds, and specific contract addresses. Snapshots execution state at each breakpoint for backward stepping without full re-execution.

### What-If Engine
Accepts patches to function arguments, ledger state, auth context, or resource limits. Forks execution from any checkpoint, replays with modifications, and produces a comparison trace highlighting the first point of divergence between original and modified execution.

### Shared Infrastructure
The XDR Codec wraps `stellar-xdr` with convenience methods for common patterns. The ContractSpec Decoder parses WASM custom sections. The RPC Client handles `getTransaction`, `simulateTransaction`, `getLedgerEntries`, and `getEvents` with retry logic and rate limit management. The History Archive Client fetches and decompresses archive files from S3, GCS, or HTTP backends. The Cache Layer stores fetched WASM blobs, parsed specs, and reconstructed ledger entries in a local content-addressed database.

---

## Interfaces

### CLI

The primary interface. Every feature is accessible from the command line with human-readable colored output by default and a `--output json` flag for machine consumption.

| Command | Tier | What it does |
|---|---|---|
| `Prism decode <tx-hash>` | 1 | Decode the error, show root cause and fixes |
| `Prism decode --raw <error-string>` | 1 | Decode a raw error from logs or test output |
| `Prism inspect <tx-hash>` | 1 | Full TX context: arguments, auth, resources, fees |
| `Prism trace <tx-hash>` | 2 | Replay and output the execution timeline |
| `Prism profile <tx-hash>` | 2 | Resource consumption hotspot analysis |
| `Prism diff <tx-hash>` | 2 | Ledger state before vs after |
| `Prism replay <tx-hash> --interactive` | 3 | Launch the TUI debugger with breakpoints |
| `Prism whatif <tx-hash> --modify <patch>` | 3 | Re-simulate with modified inputs, compare outcomes |
| `Prism export <tx-hash> --format test` | 3 | Export as a regression test case |
| `Prism db update` | — | Update the error taxonomy database |

### VS Code Extension

Intercepts Soroban errors in test output from `stellar contract test` and `cargo test`. Decoded errors appear as inline annotations and hover tooltips. A dedicated diagnostics panel groups recent failures by error category. Transaction hashes detected in logs or test output get a clickable "Debug This TX" code lens that opens the web debugger. Where possible, VS Code Quick Fixes suggest code-level changes.

### Web Application

Paste a transaction hash, select the network, click Diagnose. The results view shows the decoded error, contract-specific resolution, transaction context, and suggested fixes. Click "Full Trace" to see the interactive execution timeline with collapsible invocation trees, state diff, and resource profile charts. Click "Debug" to enter the interactive debugger with breakpoints, stepping, and what-if mode. Every debug session gets a shareable URL — no login required for read access.

### Core Library

The Rust core library is published as `sextant-core` on crates.io and as a WASM package via `wasm-pack`. Other ecosystem tools can embed decoding and replay functionality directly. The public API exposes `decode_error`, `resolve_contract_error`, `replay_transaction`, `profile_transaction`, and `diff_state`.

---

## Error Taxonomy Database

The taxonomy is a versioned, structured catalog shipped with Sextant and updated independently via `sextant db update`. It covers every error category in the Soroban host: Budget, Storage, Auth, Context, Value, Object, Crypto, Contract, WASM, and Events.

Each entry includes the official error name, a one-sentence plain English summary, a detailed multi-paragraph explanation, ranked common causes with likelihood ratings, ranked suggested fixes with difficulty ratings, related errors that commonly co-occur, and direct links to the Stellar Core source where the error is defined.

The database is published as a standalone TOML file alongside the crate so other tools can import it without depending on Sextant itself.

---

## Security Considerations

**No private keys involved.** Sextant is a read-only diagnostic tool. It fetches public transaction data and contract metadata from the network. It never signs transactions, never handles secret keys, and never submits anything to the ledger.

**External input is validated at the boundary.** Transaction hashes, RPC responses, archive data, and contractspec metadata are all validated before internal processing. Malformed input is rejected with a descriptive error.

**The replay sandbox is isolated.** Transaction replay executes contract code locally in a sandboxed Soroban host environment. The sandbox has no network access and cannot modify any real ledger state. It operates strictly on the reconstructed historical snapshot.

**Cache is local and non-sensitive.** The disk cache stores public WASM bytecode, parsed contract specs, and historical ledger entries. No private data is cached. The cache can be cleared at any time with `sextant cache clear`.

---

## Supported Networks

| Network | Decode (Tier 1) | Replay (Tier 2–3) | Source |
|---|---|---|---|
| Mainnet | Yes | Yes | Soroban RPC + History Archives |
| Testnet | Yes | Yes | Soroban RPC + History Archives |
| Futurenet | Yes | Limited | Soroban RPC (archive coverage varies) |
| Custom / Standalone | Yes | Yes | User-configured RPC endpoint + archives |

---

## Compatibility

Prism reads standard Stellar XDR and communicates with standard Soroban RPC and Horizon endpoints. It is compatible with any Stellar SDK, any Soroban contract, and any deployment toolchain. It complements — and does not replicate — existing tools like StellarExpert (which shows transaction results but not execution internals), Stellar Laboratory (which constructs transactions but does not diagnose failures), and the Stellar CLI (which provides `contract invoke` but no post-mortem analysis).

---

## Roadmap

**Phase 1 — Decode (current focus)**
Ship the CLI with full Tier 1 error decoding, contract-specific resolution, and transaction context enrichment. Publish the error taxonomy database. This alone is the most useful Soroban debugging tool that exists.

**Phase 2 — IDE integration**
Ship the VS Code extension with inline error decoding and the diagnostics panel.

**Phase 3 — Web application**
Ship the web tool with client-side decoding via WASM. No backend required for Tier 1.

**Phase 4 — Replay engine**
Ship Tier 2: historical state reconstruction, execution trace replay, resource profiling, and state diffs. Add the replay backend for the web app.

**Phase 5 — Time-travel debugger**
Ship Tier 3: breakpoints, step-through, what-if mode, execution comparison, collaborative debug sessions, and regression test export.

**Phase 6 — Ecosystem**
GitHub Action for CI integration, LSP implementation for editor-agnostic support, and community contributions to the error taxonomy.

---

## Contributing

Contributions to the error taxonomy database are especially welcome — if you've encountered a Soroban error and figured out the root cause, adding your knowledge to the taxonomy helps every developer who hits the same error next. See `docs/error-taxonomy-guide.md` for the entry format and submission process.

For code contributions, see `docs/contributing.md` for architecture orientation, development setup, and the PR process.

---

## License

MIT
