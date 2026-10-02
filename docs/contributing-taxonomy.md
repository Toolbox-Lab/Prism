# Contributing to the Error Taxonomy

The error taxonomy is a community-contributable database of every known Soroban host error. It's the easiest way to contribute to Grat — no Rust knowledge required!

## File Location

Taxonomy files are in `crates/core/src/taxonomy/data/`, one file per error category:

- `budget.toml`, `storage.toml`, `auth.toml`, `context.toml`, `value.toml`
- `object.toml`, `crypto.toml`, `contract.toml`, `wasm.toml`, `events.toml`

## How to Contribute

1. Find the TOML file for the error category
2. Add or improve an entry (see Entry Format below)
3. Submit a PR

## Entry Format

```toml
[[errors]]
id = "host.category.error_name"
category = "category"
code = 0
name = "ErrorName"
severity = "Error"
summary = "One-sentence description."
detailed_explanation = """Multi-paragraph explanation."""

[[errors.common_causes]]
description = "Root cause description"
likelihood = "high"  # high | medium | low

[[errors.suggested_fixes]]
description = "Fix description"
difficulty = "easy"  # easy | medium | hard
requires_upgrade = false
```
