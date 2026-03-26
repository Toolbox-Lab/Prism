//! Human-readable colored terminal output formatter.

use prism_core::types::report::DiagnosticReport;

use crate::output::renderers::{render_section_header, BudgetBar};

/// Print a diagnostic report in human-readable colored format.
pub fn print_report(report: &DiagnosticReport) -> anyhow::Result<()> {
    println!("{}", render_section_header("Transaction Summary"));
    println!(
        "Error: {} ({}:{})",
        report.error_name, report.error_category, report.error_code
    );
    println!("Summary: {}", report.summary);

    if let Some(context) = &report.transaction_context {
        println!();
        println!("{}", render_section_header("Resource Usage"));
        println!(
            "{}",
            BudgetBar::new(
                "CPU",
                context.resources.cpu_instructions_used,
                context.resources.cpu_instructions_limit
            )
            .render()
        );
        println!(
            "{}",
            BudgetBar::new(
                "RAM",
                context.resources.memory_bytes_used,
                context.resources.memory_bytes_limit
            )
            .render()
        );
    }

    Ok(())
}
