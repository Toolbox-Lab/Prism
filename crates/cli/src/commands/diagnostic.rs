use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::output::theme::ColorPalette;
use anyhow::Result;
use directories::ProjectDirs;
use grat_core::taxonomy::loader::TaxonomyDatabase;
use grat_core::taxonomy::updater;
use grat_core::types::config::{GratConfig, NetworkConfig};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const HIGH_LATENCY: Duration = Duration::from_secs(2);
const CACHE_WARN_MIB: u64 = 100;
const CACHE_ERROR_MIB: u64 = 10;

const RPC_ADVICE: &str = "Check --rpc-url, DNS, firewall/proxy settings, or try another --network.";
const RPC_RESPONSE_ADVICE: &str =
    "Verify the endpoint is a Soroban RPC server and that any API key is valid.";
const LATENCY_ADVICE: &str = "Try a closer endpoint or check current network conditions.";
const ARCHIVE_ADVICE: &str =
    "Check archive_urls in ~/.grat/config.toml and connectivity to the history host.";
const CACHE_ADVICE: &str =
    "Grant read/write access to the cache directory or set cache_dir in ~/.grat/config.toml.";
const TAXONOMY_ADVICE: &str = "Run `grat db update` or reinstall Prism.";

#[derive(clap::Args)]
#[command(about = "Check binary health, network connectivity, and cache state.")]
pub struct DiagnosticArgs {
    /// Show only warnings and errors
    #[arg(long, short)]
    pub quiet: bool,
}

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

struct Check {
    name: String,
    status: Status,
    latency: Option<Duration>,
    advice: Option<&'static str>,
}

impl Check {
    fn ok(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Ok,
            latency: None,
            advice: None,
        }
    }

    fn warn(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Warning(msg.into()),
            latency: None,
            advice: None,
        }
    }

    fn error(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Error(msg.into()),
            latency: None,
            advice: None,
        }
    }

    fn latency(mut self, latency: Duration) -> Self {
        self.latency = Some(latency);
        self
    }

    fn advice(mut self, advice: &'static str) -> Self {
        self.advice = Some(advice);
        self
    }
}

fn labelled(label: &str, detail: impl std::fmt::Display) -> String {
    format!("{label:<28}{detail}")
}

fn format_duration(duration: Duration) -> String {
    format!("{}ms", duration.as_millis())
}

fn connectivity_check(
    name: String,
    detail: impl Into<String>,
    critical: bool,
    advice: &'static str,
) -> Check {
    if critical {
        Check::error(name, detail).advice(advice)
    } else {
        Check::warn(name, detail).advice(advice)
    }
}

fn check_binary_version() -> Check {
    let version = grat_core::VERSION;
    let name = labelled(
        "Binary version",
        format!(
            "v{version} (protocol {})",
            grat_core::SOROBAN_PROTOCOL_VERSION
        ),
    );

    match semver::Version::parse(version) {
        Ok(_) => Check::ok(name),
        Err(e) => Check::warn(name, format!("Unexpected version string: {e}")),
    }
}

fn http_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .ok()
}

fn rpc_request(method: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": {}
    })
}

fn protocol_version(value: Option<&serde_json::Value>) -> Option<u32> {
    let value = value?;
    let number = value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse::<u64>().ok()))?;
    u32::try_from(number).ok()
}

async fn check_protocol_compatibility(label: &str, url: &str, client: &reqwest::Client) -> Check {
    let start = Instant::now();
    let name = labelled("Protocol compatibility", label);

    let response = match client
        .post(url)
        .json(&rpc_request("getLatestLedger"))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return Check::error(name, format!("HTTP {}", response.status()))
                .latency(start.elapsed())
                .advice(RPC_RESPONSE_ADVICE);
        }
        Err(e) if e.is_timeout() => {
            return Check::error(
                name,
                format!("Timed out after {}s", REQUEST_TIMEOUT.as_secs()),
            )
            .latency(start.elapsed())
            .advice(RPC_ADVICE);
        }
        Err(e) => {
            return Check::error(name, format!("Unreachable — {e}"))
                .latency(start.elapsed())
                .advice(RPC_ADVICE);
        }
    };

    let body = match response.json::<serde_json::Value>().await {
        Ok(body) => body,
        Err(e) => {
            return Check::error(name, format!("Invalid response — {e}"))
                .latency(start.elapsed())
                .advice(RPC_RESPONSE_ADVICE);
        }
    };

    if let Some(error) = body.get("error") {
        return Check::error(name, format!("RPC error — {error}"))
            .latency(start.elapsed())
            .advice(RPC_RESPONSE_ADVICE);
    }

    let Some(network_protocol) = protocol_version(
        body.get("result")
            .and_then(|result| result.get("protocolVersion")),
    ) else {
        return Check::warn(name, "RPC did not report a protocol version")
            .latency(start.elapsed())
            .advice(RPC_RESPONSE_ADVICE);
    };

    let local_protocol = grat_core::SOROBAN_PROTOCOL_VERSION;
    if network_protocol > local_protocol {
        return Check::warn(
            name,
            format!(
                "Network protocol {network_protocol} is newer than supported protocol {local_protocol}"
            ),
        )
        .latency(start.elapsed())
        .advice("Update Prism to a release that supports this protocol version.");
    }

    Check::ok(labelled(
        "Protocol compatibility",
        format!("{label}  local {local_protocol} / network {network_protocol}"),
    ))
    .latency(start.elapsed())
}

async fn check_rpc(label: &str, url: &str, client: &reqwest::Client, critical: bool) -> Vec<Check> {
    let start = Instant::now();
    let name = labelled("RPC endpoint", format!("{label}  {url}"));

    let response = match client
        .post(url)
        .json(&rpc_request("getHealth"))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return vec![connectivity_check(
                name,
                format!("HTTP {}", response.status()),
                critical,
                RPC_RESPONSE_ADVICE,
            )
            .latency(start.elapsed())];
        }
        Err(e) if e.is_timeout() => {
            return vec![connectivity_check(
                name,
                format!("Timed out after {}s", REQUEST_TIMEOUT.as_secs()),
                critical,
                RPC_ADVICE,
            )
            .latency(start.elapsed())];
        }
        Err(e) => {
            return vec![connectivity_check(
                name,
                format!("Unreachable — {e}"),
                critical,
                RPC_ADVICE,
            )
            .latency(start.elapsed())];
        }
    };

    let body = match response.json::<serde_json::Value>().await {
        Ok(body) => body,
        Err(e) => {
            return vec![connectivity_check(
                name,
                format!("Invalid response — {e}"),
                critical,
                RPC_RESPONSE_ADVICE,
            )
            .latency(start.elapsed())];
        }
    };

    if let Some(error) = body.get("error") {
        return vec![connectivity_check(
            name,
            format!("RPC error — {error}"),
            critical,
            RPC_RESPONSE_ADVICE,
        )
        .latency(start.elapsed())];
    }

    let elapsed = start.elapsed();
    let mut checks = vec![if elapsed >= HIGH_LATENCY {
        Check::warn(name, format!("High latency: {}", format_duration(elapsed)))
            .latency(elapsed)
            .advice(LATENCY_ADVICE)
    } else {
        Check::ok(name).latency(elapsed)
    }];

    if critical {
        checks.push(check_protocol_compatibility(label, url, client).await);
    }

    checks
}

fn configured_endpoints(
    active: &NetworkConfig,
    config: &GratConfig,
) -> Vec<(String, NetworkConfig)> {
    let mut endpoints = vec![(active.network.as_key().to_string(), active.clone())];

    for network in &config.networks {
        if endpoints
            .iter()
            .any(|(_, existing)| existing.rpc_url == network.rpc_url)
        {
            continue;
        }
        endpoints.push((network.network.as_key().to_string(), network.clone()));
    }

    endpoints
}

fn archive_probe_url(archive_url: &str) -> String {
    format!(
        "{}/.well-known/stellar-history.json",
        archive_url.trim_end_matches('/')
    )
}

async fn check_archives(
    client: &reqwest::Client,
    endpoints: &[(String, NetworkConfig)],
) -> Vec<Check> {
    let mut urls: Vec<String> = Vec::new();
    let active_archives: Vec<&str> = endpoints
        .first()
        .map(|(_, network)| network.archive_urls.iter().map(String::as_str).collect())
        .unwrap_or_default();

    for (_, network) in endpoints {
        for url in &network.archive_urls {
            if !urls.contains(url) {
                urls.push(url.clone());
            }
        }
    }

    if urls.is_empty() {
        return vec![Check::warn(
            labelled("History archive", "none configured"),
            "No archive URLs configured for this network",
        )
        .advice(ARCHIVE_ADVICE)];
    }

    let mut checks = Vec::with_capacity(urls.len());
    for url in urls {
        let critical = active_archives.contains(&url.as_str());
        let start = Instant::now();
        let name = labelled("History archive", &url);

        match client.get(archive_probe_url(&url)).send().await {
            Ok(response) if response.status().is_success() => {
                let elapsed = start.elapsed();
                let ledger = response
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|body| body.get("currentLedger").cloned())
                    .and_then(|value| {
                        value
                            .as_u64()
                            .or_else(|| value.as_str().and_then(|s| s.parse::<u64>().ok()))
                    });
                let ledger_detail = ledger
                    .map(|ledger| format!("  ledger {ledger}"))
                    .unwrap_or_default();

                if elapsed >= HIGH_LATENCY {
                    checks.push(
                        Check::warn(
                            format!("{name}{ledger_detail}"),
                            format!("High latency: {}", format_duration(elapsed)),
                        )
                        .latency(elapsed)
                        .advice(LATENCY_ADVICE),
                    );
                } else {
                    checks.push(Check::ok(format!("{name}{ledger_detail}")).latency(elapsed));
                }
            }
            Ok(response) => checks.push(
                connectivity_check(
                    name,
                    format!("HTTP {}", response.status()),
                    critical,
                    ARCHIVE_ADVICE,
                )
                .latency(start.elapsed()),
            ),
            Err(e) if e.is_timeout() => checks.push(
                connectivity_check(
                    name,
                    format!("Timed out after {}s", REQUEST_TIMEOUT.as_secs()),
                    critical,
                    ARCHIVE_ADVICE,
                )
                .latency(start.elapsed()),
            ),
            Err(e) => checks.push(
                connectivity_check(name, format!("Unreachable — {e}"), critical, ARCHIVE_ADVICE)
                    .latency(start.elapsed()),
            ),
        }
    }

    checks
}

fn cache_dir(config: &GratConfig) -> Option<PathBuf> {
    if let Some(dir) = config.cache_dir.as_deref().filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }

    ProjectDirs::from("dev", "grat", "grat").map(|dirs| dirs.cache_dir().to_path_buf())
}

fn check_cache(config: &GratConfig) -> Vec<Check> {
    let Some(dir) = cache_dir(config) else {
        return vec![Check::error(
            labelled("Cache directory", "unknown"),
            "Could not determine OS cache directory",
        )
        .advice(CACHE_ADVICE)];
    };

    if !dir.exists() {
        return vec![Check::warn(
            labelled("Cache directory", dir.display()),
            "Directory does not exist — it will be created on first use",
        )
        .advice(CACHE_ADVICE)];
    }

    if !dir.is_dir() {
        return vec![Check::error(
            labelled("Cache directory", dir.display()),
            "Path exists but is not a directory",
        )
        .advice(CACHE_ADVICE)];
    }

    let mut checks = vec![Check::ok(labelled("Cache directory", dir.display()))];

    let probe = dir.join(format!(".grat_diag_probe_{}", std::process::id()));
    let probe_result = std::fs::write(&probe, b"ok")
        .and_then(|()| std::fs::read_to_string(&probe))
        .map(|content| content == "ok");
    let _ = std::fs::remove_file(&probe);

    match probe_result {
        Ok(true) => checks.push(Check::ok(labelled("Cache read/write", "writable"))),
        Ok(false) => {
            checks.push(
                Check::error(
                    labelled("Cache read/write", "unexpected content"),
                    "Probe file did not contain the expected data",
                )
                .advice(CACHE_ADVICE),
            );
        }
        Err(e) => checks.push(
            Check::error(
                labelled("Cache read/write", "not writable"),
                format!("Cannot read/write — {e}"),
            )
            .advice(CACHE_ADVICE),
        ),
    }

    match free_bytes(&dir) {
        Some(free) => {
            let mib = free / (1024 * 1024);
            let name = labelled("Cache free space", format!("{mib}MiB"));
            if mib < CACHE_ERROR_MIB {
                checks.push(
                    Check::error(name, "Cache writes will likely fail")
                        .advice("Free disk space or set cache_dir to a volume with more room."),
                );
            } else if mib < CACHE_WARN_MIB {
                checks.push(
                    Check::warn(name, "Running low on disk space")
                        .advice("Free disk space or set cache_dir to a volume with more room."),
                );
            } else {
                checks.push(Check::ok(name));
            }
        }
        None => checks.push(
            Check::warn(
                labelled("Cache free space", "unknown"),
                "Could not determine free space",
            )
            .advice(CACHE_ADVICE),
        ),
    }

    match dir_size_mib(&dir) {
        Ok(used) => checks.push(Check::ok(labelled("Cache size", format!("{used}MiB used")))),
        Err(e) => checks.push(Check::warn(
            labelled("Cache size", "unknown"),
            format!("Could not measure — {e}"),
        )),
    }

    checks
}

#[cfg(unix)]
#[allow(unsafe_code)]
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

fn dir_size_mib(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
        if entry.file_type().is_file() {
            total += entry.metadata()?.len();
        }
    }
    Ok(total / (1024 * 1024))
}

fn check_taxonomy() -> Check {
    let start = Instant::now();

    match TaxonomyDatabase::load_latest() {
        Ok(db) => {
            let count = db.len();
            let source = if updater::db_file_path().is_some_and(|path| path.exists()) {
                "disk"
            } else {
                "embedded"
            };
            let name = labelled("Taxonomy database", format!("{count} records ({source})"));

            if count == 0 {
                Check::error(name, "Database loaded but contains no records")
                    .latency(start.elapsed())
                    .advice(TAXONOMY_ADVICE)
            } else {
                Check::ok(name).latency(start.elapsed())
            }
        }
        Err(e) => Check::error(labelled("Taxonomy database", "load failed"), format!("{e}"))
            .latency(start.elapsed())
            .advice(TAXONOMY_ADVICE),
    }
}

fn print_report(checks: &[Check], quiet: bool) {
    let palette = ColorPalette::default();
    let sep = "─".repeat(72);
    println!("\n  {}", palette.accent_text("Grat Diagnostic Report"));
    println!("  {}\n", palette.muted_text(&sep));

    for check in checks {
        if quiet && check.status.is_ok() {
            continue;
        }

        let latency = check
            .latency
            .map(|latency| palette.muted_text(&format!("  {}", format_duration(latency))))
            .unwrap_or_default();
        println!("  [{}]  {}{}", check.status.label(), check.name, latency);

        if let Some(detail) = check.status.detail() {
            println!(
                "           {} {}",
                palette.muted_text("└─"),
                palette.muted_text(detail)
            );
        }

        if let Some(advice) = check.advice {
            println!(
                "           {} {}",
                palette.warning_text("→"),
                palette.muted_text(advice)
            );
        }
    }

    println!("\n  {}", palette.muted_text(&sep));

    let warnings = checks
        .iter()
        .filter(|check| matches!(check.status, Status::Warning(_)))
        .count();
    let errors = checks
        .iter()
        .filter(|check| check.status.is_error())
        .count();

    if errors == 0 && warnings == 0 {
        println!(
            "  {}\n",
            palette.success_text("All checks passed — environment ready.")
        );
    } else {
        println!(
            "  {} {} warning(s), {} error(s).\n",
            palette.accent_text("Environment check:"),
            palette.warning_text(&warnings.to_string()),
            palette.error_text(&errors.to_string())
        );
    }
}

pub async fn run(args: DiagnosticArgs, network: &NetworkConfig, config: &GratConfig) -> Result<()> {
    let palette = ColorPalette::default();
    println!("{}", palette.muted_text("Running diagnostics..."));

    let mut checks = vec![check_binary_version()];
    let endpoints = configured_endpoints(network, config);

    if let Some(client) = http_client() {
        for (index, (label, endpoint)) in endpoints.iter().enumerate() {
            checks.extend(check_rpc(label, &endpoint.rpc_url, &client, index == 0).await);
        }
        checks.extend(check_archives(&client, &endpoints).await);
    } else {
        checks.push(
            Check::error(
                labelled("HTTP client", "unavailable"),
                "Could not initialize an HTTP client",
            )
            .advice("Check system TLS/network configuration; network checks were skipped."),
        );
    }

    checks.extend(check_cache(config));
    checks.push(check_taxonomy());

    print_report(&checks, args.quiet);

    if checks.iter().any(|check| check.status.is_error()) {
        anyhow::bail!("One or more diagnostic checks failed.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_version_is_valid_semver() {
        let check = check_binary_version();
        assert!(check.status.is_ok(), "Expected OK, got: {:?}", check.status);
    }

    #[test]
    fn binary_version_report_includes_protocol() {
        let check = check_binary_version();
        assert!(check.name.contains(grat_core::VERSION));
        assert!(check
            .name
            .contains(&grat_core::SOROBAN_PROTOCOL_VERSION.to_string()));
    }

    #[test]
    fn status_detail_returns_message() {
        assert_eq!(Status::Warning("w".into()).detail(), Some("w"));
        assert_eq!(Status::Error("e".into()).detail(), Some("e"));
        assert_eq!(Status::Ok.detail(), None);
    }

    #[test]
    fn cache_checks_do_not_panic() {
        let _ = check_cache(&GratConfig::default());
    }

    #[test]
    fn check_constructors_set_correct_status() {
        assert!(Check::ok("x").status.is_ok());
        assert!(matches!(Check::warn("x", "w").status, Status::Warning(_)));
        assert!(Check::error("x", "e").status.is_error());
    }

    #[test]
    fn check_builder_attaches_latency_and_advice() {
        let check = Check::error("x", "e")
            .latency(Duration::from_millis(250))
            .advice("try this");
        assert_eq!(check.latency, Some(Duration::from_millis(250)));
        assert_eq!(check.advice, Some("try this"));
    }

    #[test]
    fn protocol_version_accepts_numbers_and_strings() {
        assert_eq!(protocol_version(Some(&serde_json::json!(21))), Some(21));
        assert_eq!(protocol_version(Some(&serde_json::json!("22"))), Some(22));
        assert_eq!(protocol_version(Some(&serde_json::json!("bad"))), None);
        assert_eq!(protocol_version(None), None);
    }

    #[test]
    fn archive_probe_url_handles_trailing_slashes() {
        assert_eq!(
            archive_probe_url("https://history.example.com/archive/"),
            "https://history.example.com/archive/.well-known/stellar-history.json"
        );
    }

    #[test]
    fn configured_endpoints_are_deduplicated() {
        let active = NetworkConfig::testnet();
        let config = GratConfig::default();
        let endpoints = configured_endpoints(&active, &config);

        assert_eq!(endpoints[0].1.rpc_url, active.rpc_url);
        let unique_urls: std::collections::HashSet<&str> = endpoints
            .iter()
            .map(|(_, network)| network.rpc_url.as_str())
            .collect();
        assert_eq!(unique_urls.len(), endpoints.len());
    }

    #[test]
    fn taxonomy_database_loads_records() {
        let db = TaxonomyDatabase::load_embedded().expect("embedded taxonomy should load");
        assert!(db.len() > 0);
    }
}
