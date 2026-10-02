use grat_core::types::report::DiagnosticReport;

use crate::output::renderers::{
    render_cause_list, render_error_card, render_fee_breakdown, render_fix_list,
    render_section_header, BudgetBar,
};
use crate::ui::markdown::MarkdownRenderer;

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
