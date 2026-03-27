//! `prism decode` — Decode a transaction error into plain English.

use clap::Args;
use prism_core::types::config::NetworkConfig;
use prism_core::types::report::DiagnosticReport;

/// Arguments for the decode command.
#[derive(Args)]
pub struct DecodeArgs {
    /// Transaction hash to decode (32-byte hex string).
    #[arg(value_name = "HASH", value_parser = validate_hash)]
    pub hash: String,

    /// Decode a raw error string instead of fetching by TX hash.
    #[arg(long)]
    pub raw: bool,

    /// Show short one-line summary only.
    #[arg(long)]
    pub short: bool,
}

/// Execute the decode command.
pub async fn run(
    args: DecodeArgs,
    network: &NetworkConfig,
    output_format: &str,
    save: Option<&str>,
) -> anyhow::Result<()> {
    let report = if args.raw {
        build_raw_xdr_report(&args.hash)?
    } else {
        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_message(format!(
            "Fetching transaction {}...",
            &args.hash[..8.min(args.hash.len())]
        ));
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let r = prism_core::decode::decode_transaction(&args.hash, network).await?;

        spinner.finish_and_clear();
        r
    };

    // --- Terminal output (always shown) ---
    let effective_output = if args.short { "short" } else { output_format };
    crate::output::print_diagnostic_report(&report, effective_output)?;

    // --- Optional JSON save (--save flag) ---
    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{}': {}", path, e))?;
        eprintln!("Saved report to {path}");
    }

    Ok(())
}

/// Validate that a string is a 32-byte hex hash.
fn validate_hash(s: &str) -> Result<String, String> {
    if s.len() != 64 {
        return Err(format!(
            "Transaction hash must be 64 characters long, got {}",
            s.len()
        ));
    }
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Transaction hash must be a valid hex string".to_string());
    }
    Ok(s.to_string())
}

/// Build a report from a raw XDR string.
fn build_raw_xdr_report(raw_xdr: &str) -> anyhow::Result<DiagnosticReport> {
    let mut report =
        DiagnosticReport::new("raw-xdr", 0, "RawXdr", "Decoded raw XDR input from --raw");

    // Estimate decoded byte length: base64 encodes 3 bytes as 4 chars.
    // If the input isn't valid base64 we fall back to half the string length.
    let len = if raw_xdr
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
    {
        (raw_xdr.trim_end_matches('=').len() * 3) / 4
    } else {
        raw_xdr.len() / 2
    };
    report.detailed_explanation = format!("Raw XDR payload ({} bytes)", len);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_xdr_input_builds_a_local_report() {
        let report = build_raw_xdr_report("AAAA").expect("raw XDR should decode");

        assert_eq!(report.error_category, "raw-xdr");
        assert_eq!(report.error_name, "RawXdr");
        assert_eq!(report.summary, "Decoded raw XDR input from --raw");
        assert!(report.detailed_explanation.contains("bytes"));
    }

    #[test]
    fn validate_hash_accepts_valid_hash() {
        let valid = "a".repeat(64);
        assert!(validate_hash(&valid).is_ok());
    }

    #[test]
    fn validate_hash_rejects_invalid_length() {
        let invalid = "a".repeat(63);
        assert!(validate_hash(&invalid).is_err());
    }

    #[test]
    fn validate_hash_rejects_invalid_chars() {
        let mut invalid = "a".repeat(64);
        invalid.replace_range(0..1, "g");
        assert!(validate_hash(&invalid).is_err());
    }
}
