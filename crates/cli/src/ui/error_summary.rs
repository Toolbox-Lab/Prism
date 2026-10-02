use colored::Colorize;
use grat_core::types::report::{DiagnosticReport, Severity};
use std::collections::HashMap;
use tabled::{settings::Style, Table, Tabled};

pub struct ErrorSummaryList;

#[derive(Tabled)]
struct ErrorSummaryRow {
    #[tabled(rename = "Severity")]
    severity: String,
    #[tabled(rename = "Category")]
    category: String,
    #[tabled(rename = "Error Name")]
    name: String,
    #[tabled(rename = "Count")]
    count: usize,
    #[tabled(rename = "Message")]
    message: String,
}

impl ErrorSummaryList {
    pub fn render(reports: &[DiagnosticReport]) {
        if reports.is_empty() {
            println!("No errors found in the provided batch.");
            return;
        }

        // Group by category, name, and severity
        let mut grouped: HashMap<(String, String, String), (usize, String)> = HashMap::new();

        for report in reports {
            let sev_str = match report.severity {
                Severity::Fatal => "Fatal",
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Info => "Info",
            }
            .to_string();

            let key = (
                report.error_category.clone(),
                report.error_name.clone(),
                sev_str,
            );
            let entry = grouped.entry(key).or_insert((0, report.summary.clone()));
            entry.0 += 1;
        }

        let mut rows: Vec<ErrorSummaryRow> = grouped
            .into_iter()
            .map(|((category, name, severity), (count, message))| {
                let sev_str = match severity.as_str() {
                    "Fatal" => "FATAL".red().bold().to_string(),
                    "Error" => "ERROR".red().to_string(),
                    "Warning" => "WARN".yellow().to_string(),
                    "Info" => "INFO".blue().to_string(),
                    _ => severity,
                };

                ErrorSummaryRow {
                    severity: sev_str,
                    category,
                    name,
                    count,
                    message,
                }
            })
            .collect();

        // Sort by severity (Fatal > Error > Warning > Info) then count descending
        rows.sort_by(|a, b| {
            let sev_a = severity_weight(&a.severity);
            let sev_b = severity_weight(&b.severity);
            sev_b.cmp(&sev_a).then(b.count.cmp(&a.count))
        });

        let mut table = Table::new(rows);
        table.with(Style::rounded());

        println!("\n=== Batch Decoding Summary ===");
        println!("{}", table);
    }
}

fn severity_weight(sev_str: &str) -> u8 {
    if sev_str.contains("FATAL") {
        4
    } else if sev_str.contains("ERROR") {
        3
    } else if sev_str.contains("WARN") {
        2
    } else {
        1
    }
}
