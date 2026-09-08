# Implementation Plan

- [x] 1. Add `load_strict()` to `ConfigManager` and update `load()` behavior
  - Add `pub fn load_strict(&self) -> anyhow::Result<PrismConfig>` to `ConfigManager` in `crates/cli/src/config.rs`
  - `load_strict()` must return `Err` (with path in message) when the file is missing — no silent default fallback
  - `load_strict()` reads and TOML-parses the file identically to `load()`, propagating read and parse errors with the path in their messages
  - Existing `load()` method is left completely unchanged
  - Add unit tests in the `#[cfg(test)]` block:
    - `load_strict_errors_when_file_missing`: `with_path` to a non-existent path → `Err` containing the path string
    - `load_strict_errors_on_invalid_toml`: write `"not valid toml ]["` to a temp file → `Err` containing the path string
    - `load_strict_succeeds_on_valid_toml`: write a minimal valid TOML config to a temp file → `Ok(PrismConfig)` with expected field values
  - **Files:** `crates/cli/src/config.rs`
  - **Requirements:** R2 (AC1–AC4), R3 (AC1–AC3)

- [x] 2. Add `--config-path` global flag to the `Cli` struct
  - Add `config_path: Option<std::path::PathBuf>` field to the `Cli` struct in `crates/cli/src/main.rs`
  - Annotate with `#[arg(long, global = true, value_name = "PATH")]` and a doc comment
  - Place the field after the existing `save` field to keep the struct tidy
  - Add unit tests in the `#[cfg(test)]` block:
    - `config_path_absent_by_default`: parse `["prism", "db", "update"]` → `cli.config_path == None`
    - `config_path_parsed_before_subcommand`: parse `["prism", "--config-path", "/tmp/x.toml", "db", "update"]` → `cli.config_path == Some(PathBuf::from("/tmp/x.toml"))`
    - `config_path_parsed_after_subcommand`: parse `["prism", "db", "update", "--config-path", "/tmp/x.toml"]` → `cli.config_path == Some(PathBuf::from("/tmp/x.toml"))`
  - **Files:** `crates/cli/src/main.rs`
  - **Requirements:** R1 (AC1–AC5)

- [x] 3. Wire `--config-path` into `main()` and fix duplicate `Serve` arm
  - In `main()`, after the `tracing` subscriber is initialised, add the `ConfigManager` construction and config load:
    - If `cli.config_path` is `Some(path)`: construct `ConfigManager::with_path(path)`, call `load_strict()?` — propagate error
    - If `cli.config_path` is `None`: construct `ConfigManager::new()?` — propagate error — call `load()?`
  - Remove the duplicate `Commands::Serve(args) => commands::serve::run(args).await?,` match arm (the second one, which omits `&network`) — this is a pre-existing unreachable-pattern compile error that must be fixed for the crate to build
  - Verify `cargo build -p prism-cli` produces zero errors and zero new warnings
  - **Files:** `crates/cli/src/main.rs`
  - **Requirements:** R4 (AC1–AC3), R5 (AC1–AC2)
