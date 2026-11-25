use grat_core::types::report::{DiagnosticReport, ResourceSummary};

use crate::output::renderers::{
    render_cause_list, render_error_card, render_fee_breakdown, render_fix_list,
    render_section_header, BudgetBar,
};
use crate::ui::markdown::MarkdownRenderer;

/// Shown instead of empty usage bars when the decoded data has no resource
/// figures, for example a classic transaction or a lone result XDR.
const NO_RESOURCE_DATA: &str = "No resource usage data is available for this transaction.";

/// Whether any resource figure was reported; all zeros means none was.
fn has_resource_data(resources: &ResourceSummary) -> bool {
    resources.cpu_instructions_used > 0
        || resources.cpu_instructions_limit > 0
        || resources.memory_bytes_used > 0
        || resources.memory_bytes_limit > 0
        || resources.read_bytes > 0
        || resources.read_bytes_limit > 0
        || resources.write_bytes > 0
}

pub fn print_report(report: &DiagnosticReport) -> anyhow::Result<()> {
    println!("{}", render_error_card(report));
    println!();

    println!("{}", render_section_header("Transaction Summary"));
    println!(
        "Error: {} ({}:{})",
        report.error_name, report.error_category, report.error_code
    );
    println!("Summary: {}", report.summary);

    if !report.detailed_explanation.trim().is_empty() {
        println!();
        println!("{}", render_section_header("Detailed Explanation"));
        let rendered = MarkdownRenderer::new().render(&report.detailed_explanation);
        print!("{rendered}");
    }

    if let Some(context) = &report.transaction_context {
        println!();
        println!("{}", render_section_header("Resource Usage"));
        if has_resource_data(&context.resources) {
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
            println!(
                "{}",
                BudgetBar::new(
                    "Read",
                    context.resources.read_bytes,
                    context.resources.read_bytes_limit
                )
                .render()
            );
        } else {
            println!("{NO_RESOURCE_DATA}");
        }
        if let Some(diag) = &report.resource_diagnostics {
            if diag.writes.allocated > 0 || diag.writes.consumed > 0 {
                println!(
                    "{}",
                    BudgetBar::new("Write", diag.writes.consumed, diag.writes.allocated).render()
                );
            }
            if diag.has_breach() {
                println!();
                for metric in diag.breached_diagnostics() {
                    println!("Budget Exceeded: {}", metric.format_summary());
                }
            }
        }
        println!();
        print!("{}", render_fee_breakdown(&context.fee));
    }

    if !report.root_causes.is_empty() {
        println!();
        println!("{}", render_cause_list(&report.root_causes));
    }

    if !report.suggested_fixes.is_empty() {
        println!();
        println!("{}", render_fix_list(&report.suggested_fixes));
    }

    if let Some(attribution) = &report.cross_contract_attribution {
        println!();
        println!(
            "{}",
            render_section_header("Cross-Contract Failure Attribution")
        );
        println!("Origin Contract : {}", attribution.contract_address);
        if let Some(fn_name) = &attribution.function_name {
            println!("Failed Function : {fn_name}");
        }
        println!("Call Depth      : {}", attribution.call_depth);
        println!("Details         : {}", attribution.origin_description);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::has_resource_data;
    use grat_core::types::report::ResourceSummary;

    fn resources(cpu_used: u64, cpu_limit: u64) -> ResourceSummary {
        ResourceSummary {
            cpu_instructions_used: cpu_used,
            cpu_instructions_limit: cpu_limit,
            memory_bytes_used: 0,
            memory_bytes_limit: 0,
            read_bytes: 0,
            read_bytes_limit: 0,
            write_bytes: 0,
        }
    }

    #[test]
    fn all_zero_resources_count_as_missing() {
        assert!(!has_resource_data(&resources(0, 0)));
    }

    #[test]
    fn any_reported_figure_counts_as_data() {
        assert!(has_resource_data(&resources(0, 100)));
        assert!(has_resource_data(&resources(5, 0)));
    }
}
