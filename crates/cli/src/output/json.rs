use grat_core::types::report::DiagnosticReport;

pub fn print_report(report: &DiagnosticReport) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(report)?);
    Ok(())
}
