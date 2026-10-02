# Fetch Taxonomy

This example script demonstrates how the React Web Application should dynamically build its Error Dictionary by parsing the core Rust engine's source of truth TOML files.

Currently, the React frontend hardcodes Soroban error numbers (Issue #4), which is unmaintainable. This script reads `crates/core/src/taxonomy/data/contract.toml`, parses it using the `@iarna/toml` library, and writes a normalized JSON dictionary.

## Flattening

The `@iarna/toml` parser returns an AST that mirrors the Rust codebase's nested namespaces (e.g. `[errors.contract.authentication]`), which is hostile to UI code that needs instant lookups. A recursive graph walker (`flattenTaxonomy`) collapses that tree into a single-level dictionary keyed by numeric error code, so the frontend can resolve any code in O(1) with zero recursive overhead:

```json
{
  "0": { "code": 0, "name": "ContractError", "summary": "..." },
  "6": { "code": 6, "name": "AccountMissingError", "summary": "..." }
}
```

The walker uses `Object.entries()` to iterate structural grouping nodes, recurses until it reaches terminal leaf nodes (definition tables carrying `name`/description primitives and a resolvable code), and is fully defensive: unexpected shapes introduced upstream (a generic array or bare primitive where a definition table belongs, `null`, mixed arrays) are skipped with a discrete warning instead of crashing with a `TypeError`. Duplicate codes keep the first definition encountered and emit a warning.

## Installation

```bash
pnpm install
# or npm install
```

## Running the Example

```bash
npm start
```

This will read the TOML file from the `crates/core` directory, flatten it, and write the resulting dictionary to `taxonomy.json`.

## Running the Tests

```bash
npm test
```
