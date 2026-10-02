#[cfg(test)]
mod tests {
    use crate::decode::mappings::{budget, context, value};
    use crate::taxonomy::loader::TaxonomyDatabase;
    use crate::taxonomy::schema::ErrorCategory;
    use crate::types::report::Severity;

    #[test]
    fn error_severity_critical_maps_to_fatal() {
        let detail = budget::lookup(0).expect("budget code 0 exists");
        let severity: Severity = detail.severity.clone().into();
        assert_eq!(severity, Severity::Fatal);
    }

    #[test]
    fn error_severity_error_maps_to_error() {
        let detail = budget::lookup(8).expect("budget code 8 exists");
        let severity: Severity = detail.severity.clone().into();
        assert_eq!(severity, Severity::Error);
    }

    #[test]
    fn error_severity_warning_maps_to_warning() {
        use crate::decode::mappings::budget::ErrorSeverity;
        let severity: Severity = ErrorSeverity::Warning.into();
        assert_eq!(severity, Severity::Warning);
    }

    #[test]
    fn error_severity_info_maps_to_info() {
        use crate::decode::mappings::budget::ErrorSeverity;
        let severity: Severity = ErrorSeverity::Info.into();
        assert_eq!(severity, Severity::Info);
    }

    #[test]
    fn value_internal_error_maps_to_fatal() {
        let detail = value::lookup(4).expect("value code 4 exists");
        let severity: Severity = detail.severity.clone().into();
        assert_eq!(severity, Severity::Fatal);
    }

    #[test]
    fn value_invalid_input_maps_to_error() {
        let detail = value::lookup(6).expect("value code 6 exists");
        let severity: Severity = detail.severity.clone().into();
        assert_eq!(severity, Severity::Error);
    }

    #[test]
    fn context_internal_error_is_fatal() {
        let detail = context::lookup(7).expect("context code 7 exists");
        assert_eq!(detail.severity, Severity::Fatal);
    }

    #[test]
    fn context_invalid_action_is_error() {
        let detail = context::lookup(6).expect("context code 6 exists");
        assert_eq!(detail.severity, Severity::Error);
    }

    #[test]
    fn storage_near_expiry_maps_to_warning() {
        use crate::decode::mappings::storage;

        let detail = storage::lookup(4).expect("storage code 4 exists");
        assert_eq!(detail.severity, Severity::Warning);
    }

    #[test]
    fn taxonomy_warning_severity_is_correctly_parsed() {
        let db = TaxonomyDatabase::load_embedded().expect("taxonomy loads");
        let entry = db
            .lookup(&ErrorCategory::Storage, 4)
            .expect("storage code 4 in taxonomy");
        assert_eq!(entry.severity, "Warning");
    }

    #[test]
    fn build_report_context_internal_error_is_fatal() {
        use crate::decode::host_error::ClassifiedError;
        use crate::decode::report::build_report;

        let classified = ClassifiedError {
            category: ErrorCategory::Context,
            error_code: 7,
            is_contract_error: false,
            contract_id: None,
            raw_data: serde_json::Value::Null,
        };
        let report = build_report(&classified).expect("report should build");
        assert_eq!(report.severity, Severity::Fatal);
    }

    #[test]
    fn build_report_auth_missing_auth_is_error() {
        use crate::decode::host_error::ClassifiedError;
        use crate::decode::report::build_report;

        let classified = ClassifiedError {
            category: ErrorCategory::Auth,
            error_code: 2,
            is_contract_error: false,
            contract_id: None,
            raw_data: serde_json::Value::Null,
        };
        let report = build_report(&classified).expect("report should build");
        assert_eq!(report.severity, Severity::Error);
    }

    #[test]
    fn build_report_storage_near_expiry_is_warning() {
        use crate::decode::host_error::ClassifiedError;
        use crate::decode::report::build_report;

        let classified = ClassifiedError {
            category: ErrorCategory::Storage,
            error_code: 4,
            is_contract_error: false,
            contract_id: None,
            raw_data: serde_json::Value::Null,
        };
        let report = build_report(&classified).expect("report should build");
        assert_eq!(report.severity, Severity::Warning);
    }

    #[test]
    fn build_report_budget_approaching_limit_is_info() {
        use crate::decode::host_error::ClassifiedError;
        use crate::decode::report::build_report;

        let classified = ClassifiedError {
            category: ErrorCategory::Budget,
            error_code: 3,
            is_contract_error: false,
            contract_id: None,
            raw_data: serde_json::Value::Null,
        };
        let report = build_report(&classified).expect("report should build");
        assert_eq!(report.severity, Severity::Info);
    }

    #[test]
    fn build_report_unknown_code_defaults_to_error() {
        use crate::decode::host_error::ClassifiedError;
        use crate::decode::report::build_report;

        let classified = ClassifiedError {
            category: ErrorCategory::Budget,
            error_code: 9999,
            is_contract_error: false,
            contract_id: None,
            raw_data: serde_json::Value::Null,
        };
        let report = build_report(&classified).expect("report should build");
        assert_eq!(report.severity, Severity::Error);
    }

    #[test]
    fn taxonomy_fatal_severity_is_correctly_parsed() {
        let db = TaxonomyDatabase::load_embedded().expect("taxonomy loads");
        let entry = db
            .lookup(&ErrorCategory::Context, 7)
            .expect("context code 7 in taxonomy");
        assert_eq!(entry.severity, "Fatal");
    }

    #[test]
    fn taxonomy_error_severity_is_correctly_parsed() {
        let db = TaxonomyDatabase::load_embedded().expect("taxonomy loads");
        let entry = db
            .lookup(&ErrorCategory::Auth, 1)
            .expect("auth code 1 in taxonomy");
        assert_eq!(entry.severity, "Error");
    }

    #[test]
    fn all_budget_entries_have_valid_severity() {
        use crate::decode::mappings::budget::BUDGET_ERROR_DETAILS;

        for entry in BUDGET_ERROR_DETAILS {
            let sev: Severity = entry.severity.clone().into();
            assert!(
                matches!(
                    sev,
                    Severity::Fatal | Severity::Error | Severity::Warning | Severity::Info
                ),
                "Unexpected severity for budget code {}: {:?}",
                entry.code,
                sev
            );
        }
    }

    #[test]
    fn all_value_entries_have_valid_severity() {
        use crate::decode::mappings::value::VALUE_ERROR_DETAILS;

        for entry in VALUE_ERROR_DETAILS {
            let sev: Severity = entry.severity.clone().into();
            assert!(
                matches!(
                    sev,
                    Severity::Fatal | Severity::Error | Severity::Warning | Severity::Info
                ),
                "Unexpected severity for value code {}: {:?}",
                entry.code,
                sev
            );
        }
    }

    #[test]
    fn all_storage_entries_have_valid_severity() {
        use crate::decode::mappings::storage::STORAGE_ERROR_DETAILS;

        for entry in STORAGE_ERROR_DETAILS {
            assert!(
                matches!(
                    entry.severity,
                    Severity::Fatal | Severity::Error | Severity::Warning | Severity::Info
                ),
                "Unexpected severity for storage code {}: {:?}",
                entry.code,
                entry.severity
            );
        }
    }

    #[test]
    fn all_context_entries_have_valid_severity() {
        use crate::decode::mappings::context::CONTEXT_ERROR_DETAILS;

        for entry in CONTEXT_ERROR_DETAILS {
            assert!(
                matches!(
                    entry.severity,
                    Severity::Fatal | Severity::Error | Severity::Warning | Severity::Info
                ),
                "Unexpected severity for context code {}: {:?}",
                entry.code,
                entry.severity
            );
        }
    }

    #[test]
    fn all_auth_entries_have_valid_severity() {
        use crate::decode::mappings::auth::AUTH_ERROR_DETAILS;

        for entry in AUTH_ERROR_DETAILS {
            assert!(
                matches!(
                    entry.severity,
                    Severity::Fatal | Severity::Error | Severity::Warning | Severity::Info
                ),
                "Unexpected severity for auth code {}: {:?}",
                entry.code,
                entry.severity
            );
        }
    }

    #[test]
    fn all_contract_entries_have_valid_severity() {
        use crate::decode::mappings::contract::CONTRACT_ERROR_DETAILS;

        for entry in CONTRACT_ERROR_DETAILS {
            assert!(
                matches!(
                    entry.severity,
                    Severity::Fatal | Severity::Error | Severity::Warning | Severity::Info
                ),
                "Unexpected severity for contract code {}: {:?}",
                entry.code,
                entry.severity
            );
        }
    }
}
