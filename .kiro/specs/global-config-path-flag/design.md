# Design Document

## Overview

This feature adds a `--config-path` global flag to the `prism` CLI. The flag lets users point to a custom config file instead of the default `~/.prism/config.toml`. Two files are touched: `crates/cli/src/main.rs` (CLI struct + wiring) and `crates/cli/src/config.rs` (strict vs. lenient load logic).

## Architecture

The change is entirely within the `prism-cli` crate. No public API on `prism-core` is affected.

```
Cli::parse()
  └─ cli.config_path: Option<PathBuf>
        │
        ├─ Some(path) ──► ConfigManager::with_path(path)
        │                      └─ load_strict()  ← errors if file missing
        │
        └─ None ──────► ConfigManager::new()
                               └─ load()  ← returns default if file missing
```

## Component Design

### 1. `Cli` struct — `crates/cli/src/main.rs`

Add one field after the existing `save` field:

```rust
/// Override the default config file location (~/.prism/config.toml).
///
/// The file must exist; an error is returned if it does not.
///
/// Example: prism --config-path /tmp/my.toml trace <hash>
#[arg(long, global = true, value_name = "PATH")]
config_path: Option<std::path::PathBuf>,
```

`global = true` is required so clap accepts the flag regardless of argument position relative to the subcommand.

No `value_parser` is needed — clap parses `PathBuf` from a string verbatim with no file-existence check at parse time.

### 2. `ConfigManager` — `crates/cli/src/config.rs`

The existing `with_path` constructor already stores an explicit path, but `load()` silently returns `PrismConfig::default()` when the file is missing. That is correct for the default path but wrong for a user-supplied path.

The fix is to add a `load_strict()` method that skips the missing-file shortcut:

```rust
/// Load config from disk, returning an error when the file does not exist.
///
/// Use this when the path was explicitly provided by the user via
/// `--config-path`. Unlike `load()`, a missing file is always an error.
pub fn load_strict(&self) -> anyhow::Result<PrismConfig> {
    if !self.config_path.exists() {
        anyhow::bail!(
            "Config file not found: {}",
            self.config_path.display()
        );
    }

    let content = std::fs::read_to_string(&self.config_path).with_context(|| {
        format!("Failed to read config file {}", self.config_path.display())
    })?;

    let config: PrismConfig = toml::from_str(&content).with_context(|| {
        format!(
            "Failed to parse config file {} as TOML",
            self.config_path.display()
        )
    })?;

    Ok(config)
}
```

`load()` is unchanged, preserving all existing behavior for the default path.

### 3. Wiring in `main()` — `crates/cli/src/main.rs`

After logging is initialised and before subcommand dispatch, construct `ConfigManager` and load the config:

```rust
// Resolve config — strict if the user supplied --config-path, lenient otherwise.
let config_manager = match cli.config_path {
    Some(path) => config::ConfigManager::with_path(path),
    None => config::ConfigManager::new()?,
};
let _config = config_manager.load_strict_or_lenient()?;
```

Wait — rather than adding a third method, the call site in `main()` can branch directly:

```rust
let config_manager = match cli.config_path {
    Some(path) => {
        let mgr = config::ConfigManager::with_path(path);
        mgr.load_strict()?;   // eagerly validate; error propagates out of main
        mgr
    }
    None => config::ConfigManager::new()?,
};
```

Actually the cleanest approach (avoids loading twice) is:

```rust
let (_config, config_manager) = if let Some(path) = cli.config_path {
    let mgr = config::ConfigManager::with_path(path);
    let cfg = mgr.load_strict()?;
    (cfg, mgr)
} else {
    let mgr = config::ConfigManager::new()?;
    let cfg = mgr.load()?;
    (cfg, mgr)
};
```

The `_config` binding is prefixed with `_` for now since no subcommand currently reads it; the `ConfigManager` is available for any future command that needs to call `save()`.

> **Pre-existing bug fix (in scope per R5):** `main.rs` contains a duplicate `Commands::Serve` match arm, which is an unreachable-pattern compiler error. The second arm (the one without `&network`) must be removed as part of making the file compile cleanly.

## Data Flow

```
argv
  │
  ▼
Cli::parse()          ← clap; no I/O
  │
  ├─ cli.config_path = Some(p) ──► ConfigManager::with_path(p)
  │                                      │
  │                                      ▼
  │                               load_strict()
  │                                 file exists? ──No──► Err (includes path)
  │                                      │Yes
  │                                      ▼
  │                               toml::from_str()
  │                                 valid TOML? ──No──► Err (includes path)
  │                                      │Yes
  │                                      ▼
  │                               Ok(PrismConfig)
  │
  └─ cli.config_path = None ──► ConfigManager::new()
                                      │
                                      ▼
                               load()
                                 file exists? ──No──► Ok(PrismConfig::default())
                                      │Yes
                                      ▼
                               toml::from_str()
                                 valid TOML? ──No──► Err (includes path)
                                      │Yes
                                      ▼
                               Ok(PrismConfig)
```

## Test Plan

### Unit tests — `crates/cli/src/config.rs`

| Test | Setup | Expected |
|------|-------|----------|
| `load_strict_errors_when_file_missing` | `with_path` pointing at non-existent path | `Err` whose message contains the path |
| `load_strict_errors_on_invalid_toml` | Write `"not toml ]["` to a temp file | `Err` whose message contains the path |
| `load_strict_succeeds_on_valid_toml` | Write a valid TOML config to a temp file | `Ok(PrismConfig)` with expected values |
| `load_returns_default_when_file_missing` (existing) | Already covers the lenient case | No change |

### Unit tests — `crates/cli/src/main.rs`

| Test | Setup | Expected |
|------|-------|----------|
| `config_path_absent_by_default` | `Cli::try_parse_from(["prism", "db", "update"])` | `cli.config_path == None` |
| `config_path_parsed_before_subcommand` | `["prism", "--config-path", "/tmp/x.toml", "db", "update"]` | `cli.config_path == Some(PathBuf::from("/tmp/x.toml"))` |
| `config_path_parsed_after_subcommand` | `["prism", "db", "update", "--config-path", "/tmp/x.toml"]` | `cli.config_path == Some(PathBuf::from("/tmp/x.toml"))` |

## Constraints

- No changes outside `crates/cli/src/main.rs` and `crates/cli/src/config.rs` (the pre-existing duplicate `Serve` arm fix is in `main.rs` and is required for the file to compile).
- `load()` signature and behavior are unchanged to avoid breaking `ConfigManager::with_path` callers in existing tests.
- `load_strict()` is a new additive method — no existing call sites are affected.
