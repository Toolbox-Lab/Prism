//! `prism diagnostic` — Health check for binary, network, and cache state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use directories::ProjectDirs;
use crate::output::theme::ColorPalette;

// ─── Args ────────────────────────────────────────────────────────────────────

#[derive(clap::Args)]
#[command(about = "Check binary health, network connectivity, and cache state.")]
pub struct DiagnosticArgs {
    /// Only show warnings and errors (suppress OK lines).
    #[arg(long, short)]
    pub quiet: bool,
}

// ─── Status ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Status {
    Ok,
    Warning(String),
    Error(String),
}

impl Status {
    fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    fn detail(&self) -> Option<&str> {
        match self {
            Self::Ok => None,
            Self::Warning(msg) | Self::Error(msg) => Some(msg.as_str()),
        }
    }

    fn label(&self) -> String {
        let palette = ColorPalette::default();
        match self {
            Self::Ok => palette.success_text("  OK   "),
            Self::Warning(_) => palette.warning_text(" WARN  "),
            Self::Error(_) => palette.error_text(" ERROR "),
        }
    }
}

// ─── Check ───────────────────────────────────────────────────────────────────

struct Check {
    name: String,
    status: Status,
}

impl Check {
    fn ok(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Ok,
        }
    }

    fn warn(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Warning(msg.into()),
        }
    }

    fn error(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Error(msg.into()),
        }
    }
}

// ─── Checks ──────────────────────────────────────────────────────────────────

fn check_binary_version() -> Check {
    let version = env!("CARGO_PKG_VERSION");
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok()) {
        Check::ok(format!("Binary version                    v{}", version))
    } else {
        Check::warn(
            "Binary version",
            format!("Unexpected version string: {}", version),
        )
    }
}

async fn check_rpc(
    label: &str,
    network_config: &prism_core::types::config::NetworkConfig,
) -> Check {
    let start = Instant::now();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("reqwest client");

    let result = client
        .post(&network_config.rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getHealth",
            "params": {}
        }))
        .send()
        .await;

    let check_name = format!("RPC connectivity  ({:<8})", label);

    match result {
        Ok(resp) if resp.status().is_success() => {
            let ms = start.elapsed().as_millis();
            if ms > 2_000 {
                Check::warn(check_name, format!("High latency: {}ms", ms))
            } else {
                Check::ok(format!("{}  {}ms", check_name, ms))
            }
        }
        Ok(resp) => Check::error(check_name, format!("HTTP {}", resp.status())),
        Err(e) if e.is_timeout() => Check::error(check_name, "Timed out after 5s"),
        Err(e) => Check::error(check_name, format!("Unreachable — {}", e)),
    }
}

async fn check_network() -> Vec<Check> {
    let configs = [
        (
            "mainnet",
            prism_core::network::config::resolve_network("mainnet"),
        ),
        (
            "testnet",
            prism_core::network::config::resolve_network("testnet"),
        ),
    ];

    let mut checks = Vec::new();
    for (label, cfg) in &configs {
        checks.push(check_rpc(label, cfg).await);
    }
    checks
}

fn cache_dir() -> Option<PathBuf> {
    ProjectDirs::from("io", "prism", "prism").map(|p| p.cache_dir().to_path_buf())
}

fn check_cache() -> Vec<Check> {
    let Some(dir) = cache_dir() else {
        return vec![Check::error(
            "Cache directory",
            "Could not determine OS cache directory",
        )];
    };

    let mut checks = Vec::new();

    // Existence
    if !dir.exists() {
        checks.push(Check::warn(
            format!("Cache directory   {}", dir.display()),
            "Does not exist — will be created on first use",
        ));
        return checks;
    }
    checks.push(Check::ok(format!("Cache directory   {}", dir.display())));

    // Writability
    let probe = dir.join(".prism_diag_probe");
    match std::fs::write(&probe, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            checks.push(Check::ok("Cache writability"));
        }
        Err(e) => {
            checks.push(Check::error(
                "Cache writability",
                format!("Cannot write — {}", e),
            ));
        }
    }

    // Disk space
    match free_bytes(&dir) {
        Some(free) => {
            let mib = free / (1024 * 1024);
            const WARN_MIB: u64 = 100;
            const ERROR_MIB: u64 = 10;
            if mib < ERROR_MIB {
                checks.push(Check::error(
                    format!("Disk space                        {}MiB free", mib),
                    "Cache writes may fail",
                ));
            } else if mib < WARN_MIB {
                checks.push(Check::warn(
                    format!("Disk space                        {}MiB free", mib),
                    "Running low on disk space",
                ));
            } else {
                checks.push(Check::ok(format!(
                    "Disk space                        {}MiB free",
                    mib
                )));
            }
        }
        None => {
            checks.push(Check::warn("Disk space", "Could not determine free space"));
        }
    }

    // Cache size (informational)
    if let Ok(used) = dir_size_mib(&dir) {
        checks.push(Check::ok(format!(
            "Cache size                        {}MiB used",
            used
        )));
    }

    checks
}

#[cfg(unix)]
fn free_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    let cpath = CString::new(path.to_string_lossy().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(cpath.as_ptr(), &mut stat) };
    if rc == 0 {
        Some(stat.f_bavail * stat.f_frsize)
    } else {
        None
    }
}

#[cfg(not(unix))]
fn free_bytes(_path: &Path) -> Option<u64> {
    None
}

fn dir_size_mib(path: &PathBuf) -> Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)?.flatten() {
        let meta = entry.metadata()?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total / (1024 * 1024))
}

// ─── Report ───────────────────────────────────────────────────────────────────

fn print_report(checks: &[Check], quiet: bool) {
    let palette = ColorPalette::default();
    let sep = "─".repeat(58);
    println!("\n  {}", palette.accent_text("Prism Diagnostic Report"));
    println!("  {}\n", palette.muted_text(&sep));

    for check in checks {
        if quiet && check.status.is_ok() {
            continue;
        }
        println!("  [{}]  {}", check.status.label(), check.name);
        if let Some(detail) = check.status.detail() {
            println!(
                "           {} {}",
                palette.muted_text("└─"),
                palette.muted_text(detail)
            );
        }
    }

    println!("\n  {}", palette.muted_text(&sep));

    let warnings = checks
        .iter()
        .filter(|c| matches!(c.status, Status::Warning(_)))
        .count();
    let errors = checks.iter().filter(|c| c.status.is_error()).count();

    if errors == 0 && warnings == 0 {
        println!("  {}\n", palette.success_text("All checks passed."));
    } else {
        println!(
            "  {} warning(s), {} error(s).\n",
            palette.warning_text(&warnings.to_string()),
            palette.error_text(&errors.to_string())
        );
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────

pub async fn run(args: DiagnosticArgs) -> Result<()> {
    let palette = ColorPalette::default();
    println!("{}", palette.muted_text("Running diagnostics..."));

    let mut checks: Vec<Check> = Vec::new();

    checks.push(check_binary_version());
    checks.extend(check_network().await);
    checks.extend(check_cache());

    print_report(&checks, args.quiet);

    if checks.iter().any(|c| c.status.is_error()) {
        anyhow::bail!("One or more diagnostic checks failed.");
    }

    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_version_is_valid_semver() {
        let check = check_binary_version();
        assert!(check.status.is_ok(), "Expected OK, got: {:?}", check.status);
    }

    #[test]
    fn status_detail_returns_message() {
        assert_eq!(Status::Warning("w".into()).detail(), Some("w"));
        assert_eq!(Status::Error("e".into()).detail(), Some("e"));
        assert_eq!(Status::Ok.detail(), None);
    }

    #[test]
    fn cache_checks_do_not_panic() {
        let _ = check_cache();
    }

    #[test]
    fn check_constructors_set_correct_status() {
        assert!(Check::ok("x").status.is_ok());
        assert!(matches!(Check::warn("x", "w").status, Status::Warning(_)));
        assert!(Check::error("x", "e").status.is_error());
    }
}
