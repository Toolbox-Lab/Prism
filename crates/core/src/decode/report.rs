use crate::decode::host_error::ClassifiedError;
use crate::error::GratResult;
use crate::taxonomy::loader::TaxonomyDatabase;
use crate::types::report::{DiagnosticReport, RootCause, Severity, SuggestedFix};

pub fn build_report(error: &ClassifiedError) -> GratResult<DiagnosticReport> {
    let db = TaxonomyDatabase::load_latest()?;

    if let Some(entry) = db.lookup(&error.category, error.error_code) {
        let report = DiagnosticReport {
            error_category: entry.category.to_string(),
            error_code: entry.code,
            error_name: entry.name.clone(),
            summary: entry.summary.clone(),
            detailed_explanation: entry.detailed_explanation.clone(),
            severity: match entry.severity.as_str() {
                "Info" => Severity::Info,
                "Warning" => Severity::Warning,
                "Fatal" => Severity::Fatal,
                _ => Severity::Error,
            },
            root_causes: entry
                .common_causes
                .iter()
                .map(|c| RootCause {
                    description: c.description.clone(),
                    likelihood: c.likelihood.clone(),
                })
                .collect(),
            suggested_fixes: entry
                .suggested_fixes
                .iter()
                .map(|f| SuggestedFix {
                    description: f.description.clone(),
                    difficulty: f.difficulty.clone(),
                    requires_upgrade: f.requires_upgrade,
                    example: f.example.clone(),
                    id: f.id.clone().unwrap_or_else(|| "unknown".to_string()),
                    remedy_code: f.remedy_code.clone(),
                })
                .collect(),
            contract_error: None,
            transaction_context: None,
            related_errors: entry.related_errors.clone(),
            cross_contract_attribution: None,
            auth_signatures: Vec::new(),
            auth_entries: Vec::new(),
            failing_contract_id: None,
            call_chain: None,
            resource_diagnostics: None,
            operation_index: None,
            operation_count: None,
            learn_more: "https://developers.stellar.org/docs/learn/smart-contracts/errors"
                .to_string(),
        };

        Ok(report)
    } else {
        Ok(DiagnosticReport::new(
            &error.category.to_string(),
            error.error_code,
            "Unknown",
            &format!(
                "Unknown {} error with code {}",
                error.category, error.error_code
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::schema::ErrorCategory;

    #[test]
    fn tier1_common_causes_surface_in_decoded_report() {
        let classified = ClassifiedError {
            category: ErrorCategory::Budget,
            error_code: 0,
            is_contract_error: false,
            contract_id: None,
            raw_data: serde_json::Value::Null,
        };

        let report = build_report(&classified).expect("Report should build");

        assert!(
            !report.root_causes.is_empty(),
            "Decoded report should include at least one common cause"
        );
        assert!(
            report
                .root_causes
                .iter()
                .any(|cause| cause.description.contains("loops")),
            "Decoded report should include taxonomy common cause descriptions"
        );
    }
}
