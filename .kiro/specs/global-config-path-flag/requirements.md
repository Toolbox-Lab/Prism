# Requirements Document

## Introduction

This feature adds a global `--config-path` CLI flag to the `prism` tool, allowing users to override the default configuration file location (`~/.prism/config.toml`). When the flag is supplied, the specified path is loaded strictly — an error is returned if the file does not exist. When the flag is omitted, existing lenient behavior is preserved (defaults are returned when `~/.prism/config.toml` is absent).

## Glossary

- **CLI**: The `prism` command-line binary built from `crates/cli/src/main.rs`.
- **Cli_Struct**: The `Cli` struct parsed by `clap` at program startup.
- **ConfigManager**: The `ConfigManager` struct in `crates/cli/src/config.rs` responsible for reading and writing `PrismConfig` values.
- **Default_Config_Path**: The platform-resolved path `~/.prism/config.toml` returned by `default_config_path()`.
- **Custom_Config_Path**: A user-supplied filesystem path passed via `--config-path`.
- **PrismConfig**: The configuration value type defined in `prism_core::types::config`.

## Requirements

### Requirement 1: Global `--config-path` Flag on the CLI Struct

**User Story:** As a developer, I want to pass `--config-path` before or after any subcommand, so that I can target a custom configuration file without being constrained by argument position.

#### Acceptance Criteria

1. THE Cli_Struct SHALL expose an optional `config_path` field of type `Option<PathBuf>` bound to the `--config-path` long flag.
2. THE Cli_Struct SHALL declare `global = true` on the `config_path` argument so it is accepted before and after any subcommand.
3. IF `--config-path` is absent from the invocation, THEN THE Cli_Struct SHALL set `config_path` to `None` without performing any file-existence check at parse time.
4. WHEN `--config-path <value>` is provided, THE Cli_Struct SHALL set `config_path` to `Some(<value>)` where `<value>` is the verbatim string parsed as a `PathBuf`, without performing any file-existence check at parse time.
5. WHEN `--config-path` is provided without a following value, THE CLI SHALL exit with a clap argument-parsing error before entering `main()`'s async body.

### Requirement 2: Strict Loading of a Custom Config Path

**User Story:** As a developer, I want an explicit error when `--config-path` points to a missing file, so that I am not silently running with incorrect defaults.

#### Acceptance Criteria

1. WHEN a Custom_Config_Path is provided and the file at that path exists and contains valid TOML, THE ConfigManager SHALL return `Ok(PrismConfig)` with the deserialized values.
2. IF a Custom_Config_Path is provided and the file at that path does not exist, THEN THE ConfigManager SHALL return `Err(_)` with an error message that includes the missing path string.
3. IF a Custom_Config_Path is provided and the file at that path exists but fails TOML parsing (not a read/I/O failure), THEN THE ConfigManager SHALL return `Err(_)` with a parse-error message that includes the path string.
4. IF a Custom_Config_Path is provided, THEN THE ConfigManager SHALL NOT return `Ok(PrismConfig::default())` under any circumstance — all error conditions must surface as `Err(_)`.

### Requirement 3: Lenient Loading of the Default Config Path

**User Story:** As a developer, I want the default config-loading behavior to remain unchanged when `--config-path` is not specified, so that existing users are unaffected.

#### Acceptance Criteria

1. WHEN no Custom_Config_Path is provided and the Default_Config_Path file does not exist, THE ConfigManager SHALL return `Ok(PrismConfig::default())` without error.
2. WHEN no Custom_Config_Path is provided and the Default_Config_Path file exists and deserializes successfully, THE ConfigManager SHALL return `Ok(PrismConfig)` with the values from that file.
3. WHEN no Custom_Config_Path is provided and the Default_Config_Path file exists but contains invalid TOML or fails deserialization, THE ConfigManager SHALL return `Err(_)` with a parse-error message that includes the Default_Config_Path string.

### Requirement 4: Wiring `--config-path` into `main()`

**User Story:** As a developer, I want the `config_path` flag parsed from the CLI to be passed into `ConfigManager` at startup, so that the rest of the program uses the correct configuration source.

#### Acceptance Criteria

1. WHEN `cli.config_path` is `Some(path)`, THE CLI SHALL construct `ConfigManager` via `ConfigManager::with_path(path)`, using that explicit path for all subsequent `load()` calls.
2. WHEN `cli.config_path` is `None`, THE CLI SHALL construct `ConfigManager` via `ConfigManager::new()`, which resolves to the Default_Config_Path.
3. IF `ConfigManager::new()` returns `Err(_)` (e.g., the home directory cannot be resolved), THE CLI SHALL propagate that error and exit with a non-zero status before dispatching any subcommand.

### Requirement 5: Build Compliance

**User Story:** As a maintainer, I want the project to compile cleanly after this change, so that CI remains green.

#### Acceptance Criteria

1. THE CLI crate SHALL compile without errors when built with `cargo build -p prism-cli` (or equivalent workspace build) in both debug and release profiles.
2. THE CLI crate SHALL produce zero compiler warnings in `crates/cli/src/main.rs` and `crates/cli/src/config.rs` after the changes are applied.
3. THE implementation SHALL NOT modify source files outside of `crates/cli/src/main.rs`, `crates/cli/src/config.rs`, `crates/cli/Cargo.toml`, and `crates/cli/build.rs`.
