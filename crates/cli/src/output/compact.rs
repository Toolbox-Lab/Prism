//! Compact one-line output formatter.

use prism_core::types::report::DiagnosticReport;

/// Print a one-line compact summary of the diagnostic report.
pub fn print_report(report: &DiagnosticReport) -> anyhow::Result<()> {
    println!(
        "[{}] {}: {}",
        report.error_category, report.error_name, report.summary
    );
    Ok(())
}
