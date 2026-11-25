use grat_core::types::report::DiagnosticReport;
use tabled::{settings::Style, Table, Tabled};

pub struct ErrorSummaryList;

#[derive(Tabled)]
struct ErrorSummaryRow {
    #[tabled(rename = "Transaction Hash")]
    tx_hash: String,
    #[tabled(rename = "Error Category")]
    category: String,
    #[tabled(rename = "Error Name")]
    name: String,
    #[tabled(rename = "Failing Contract ID")]
    failing_contract_id: String,
}

impl ErrorSummaryList {
    pub fn render(batch_results: &[(String, Vec<DiagnosticReport>)]) {
        let report_count = batch_results
            .iter()
            .map(|(_, reports)| reports.len())
            .sum::<usize>();
        if report_count == 0 {
            println!("No errors found in the provided batch.");
            return;
        }

        let mut rows = Vec::new();

        for (tx_hash, reports) in batch_results {
            for report in reports {
                let failing_contract_id = report
                    .failing_contract_id
                    .clone()
                    .unwrap_or_else(|| "-".to_string());
                let row = ErrorSummaryRow {
                    tx_hash: tx_hash.clone(),
                    category: report.error_category.clone(),
                    name: report.error_name.clone(),
                    failing_contract_id,
                };
                if !rows.iter().any(|existing: &ErrorSummaryRow| {
                    existing.tx_hash == row.tx_hash
                        && existing.category == row.category
                        && existing.name == row.name
                        && existing.failing_contract_id == row.failing_contract_id
                }) {
                    rows.push(row);
                }
            }
        }

        let mut table = Table::new(rows);
        table.with(Style::rounded());

        println!("\n=== Batch Decoding Summary ===");
        println!("{}", table);
    }
}
