use grat_core::types::report::DiagnosticReport;

pub fn print_report(report: &DiagnosticReport) -> anyhow::Result<()> {
    println!(
        "[{}] {}: {}",
        report.error_category, report.error_name, report.summary
    );
    Ok(())
}
