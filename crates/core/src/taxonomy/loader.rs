use crate::error::{GratError, GratResult};
use crate::taxonomy::schema::{ErrorCategory, TaxonomyEntry, TaxonomySchema};
use std::collections::HashMap;

pub struct TaxonomyParser;

impl TaxonomyParser {
    pub fn parse(input: &str) -> GratResult<TaxonomySchema> {
        toml::from_str(input)
            .map_err(|e| GratError::TaxonomyError(format!("TOML parse error: {e}")))
    }
}

pub struct TaxonomyDatabase {
    entries: HashMap<(ErrorCategory, u32), TaxonomyEntry>,

    all_entries: Vec<TaxonomyEntry>,
}

// Relevance weights used by `TaxonomyDatabase::search`. Higher scores rank
// first; the ordering below is what determines the result ordering.
const SCORE_EXACT_NAME: u32 = 1_000;
const SCORE_NAME_PREFIX: u32 = 900;
const SCORE_EXACT_CODE: u32 = 850;
const SCORE_CODE_PREFIX: u32 = 800;
const SCORE_NAME_SUBSTRING: u32 = 700;
const SCORE_EXACT_CATEGORY: u32 = 600;
const SCORE_CATEGORY_SUBSTRING: u32 = 550;
const SCORE_SUMMARY: u32 = 400;
const SCORE_EXPLANATION: u32 = 200;
const SCORE_COMMON_CAUSE: u32 = 150;
const SCORE_SUGGESTED_FIX: u32 = 100;

fn score_term(entry: &TaxonomyEntry, term: &str) -> Option<u32> {
    let name = entry.name.to_lowercase();
    let category = entry.category.to_string().to_lowercase();
    let summary = entry.summary.to_lowercase();
    let explanation = entry.detailed_explanation.to_lowercase();
    let code = entry.code.to_string();

    if name == term {
        return Some(SCORE_EXACT_NAME);
    }
    if name.starts_with(term) {
        return Some(SCORE_NAME_PREFIX);
    }

    if !term.is_empty() && term.chars().all(|c| c.is_ascii_digit()) {
        if code == term {
            return Some(SCORE_EXACT_CODE);
        }
        if code.starts_with(term) {
            return Some(SCORE_CODE_PREFIX);
        }
    }

    if name.contains(term) {
        return Some(SCORE_NAME_SUBSTRING);
    }
    if category == term {
        return Some(SCORE_EXACT_CATEGORY);
    }
    if category.contains(term) {
        return Some(SCORE_CATEGORY_SUBSTRING);
    }
    if summary.contains(term) {
        return Some(SCORE_SUMMARY);
    }
    if explanation.contains(term) {
        return Some(SCORE_EXPLANATION);
    }
    if entry
        .common_causes
        .iter()
        .any(|cause| cause.description.to_lowercase().contains(term))
    {
        return Some(SCORE_COMMON_CAUSE);
    }
    if entry
        .suggested_fixes
        .iter()
        .any(|fix| fix.description.to_lowercase().contains(term))
    {
        return Some(SCORE_SUGGESTED_FIX);
    }

    None
}

impl TaxonomyDatabase {
    pub fn load_latest() -> GratResult<Self> {
        if let Some(db_path) = crate::taxonomy::updater::db_file_path() {
            if db_path.exists() {
                if db_path.is_dir() {
                    if let Ok(db) = Self::load_from_dir(&db_path) {
                        return Ok(db);
                    }
                } else if let Ok(content) = std::fs::read_to_string(&db_path) {
                    if let Ok(schema) = TaxonomyParser::parse(&content) {
                        let mut db = Self {
                            entries: HashMap::new(),
                            all_entries: Vec::new(),
                        };
                        for entry in schema.errors {
                            db.entries
                                .insert((entry.category.clone(), entry.code), entry.clone());
                            db.all_entries.push(entry);
                        }
                        tracing::info!(
                            "Loaded {} taxonomy entries from downloaded file",
                            db.entries.len()
                        );
                        return Ok(db);
                    }
                }
            }
        }

        Self::load_embedded()
    }

    pub fn load_embedded() -> GratResult<Self> {
        let mut db = Self {
            entries: HashMap::new(),
            all_entries: Vec::new(),
        };

        let categories = [
            ("budget", include_str!("data/budget.toml")),
            ("storage", include_str!("data/storage.toml")),
            ("auth", include_str!("data/auth.toml")),
            ("context", include_str!("data/context.toml")),
            ("value", include_str!("data/value.toml")),
            ("object", include_str!("data/object.toml")),
            ("crypto", include_str!("data/crypto.toml")),
            ("contract", include_str!("data/contract.toml")),
            ("wasm", include_str!("data/wasm.toml")),
            ("events", include_str!("data/events.toml")),
        ];

        for (name, content) in categories {
            match TaxonomyParser::parse(content) {
                Ok(schema) => {
                    for entry in schema.errors {
                        db.entries
                            .insert((entry.category.clone(), entry.code), entry.clone());
                        db.all_entries.push(entry);
                    }
                }
                Err(e) => {
                    return Err(GratError::TaxonomyError(format!(
                        "Failed to parse embedded taxonomy file '{name}': {e}"
                    )));
                }
            }
        }

        tracing::info!("Loaded {} taxonomy entries", db.entries.len());
        Ok(db)
    }

    pub fn load_from_dir(dir: &std::path::Path) -> GratResult<Self> {
        let mut db = Self {
            entries: HashMap::new(),
            all_entries: Vec::new(),
        };

        for entry in std::fs::read_dir(dir)
            .map_err(|e| GratError::TaxonomyError(format!("Cannot read taxonomy dir: {e}")))?
        {
            let entry = entry.map_err(|e| GratError::TaxonomyError(e.to_string()))?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "toml") {
                let content = std::fs::read_to_string(&path).map_err(|e| {
                    GratError::TaxonomyError(format!("Cannot read {}: {e}", path.display()))
                })?;

                let schema = TaxonomyParser::parse(&content).map_err(|e| {
                    GratError::TaxonomyError(format!("Parse error in {}: {e}", path.display()))
                })?;

                for entry in schema.errors {
                    db.entries
                        .insert((entry.category.clone(), entry.code), entry.clone());
                    db.all_entries.push(entry);
                }
            }
        }

        Ok(db)
    }

    pub fn lookup(&self, category: &ErrorCategory, code: u32) -> Option<&TaxonomyEntry> {
        self.entries.get(&(category.clone(), code))
    }

    pub fn entries_for_category(&self, category: &ErrorCategory) -> Vec<&TaxonomyEntry> {
        self.all_entries
            .iter()
            .filter(|e| &e.category == category)
            .collect()
    }

    /// Searches the taxonomy for entries matching every whitespace-separated
    /// term in `query`.
    ///
    /// Matching is case-insensitive and looks at the entry name, summary and
    /// detailed explanation, the category name, the numerical error code
    /// (exact and prefix matches), and the human-readable remediation guidance
    /// (`common_causes` and `suggested_fixes` descriptions). Results are ranked
    /// by relevance: exact name matches come first, followed by name prefixes,
    /// code matches, name substrings, category matches, summaries, explanations,
    /// common causes, and finally suggested fixes.
    pub fn search(&self, query: &str) -> Vec<&TaxonomyEntry> {
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();

        if terms.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<(u32, &TaxonomyEntry)> = self
            .all_entries
            .iter()
            .filter_map(|entry| {
                let mut score = 0_u32;
                for term in &terms {
                    score += score_term(entry, term)?;
                }
                Some((score, entry))
            })
            .collect();

        matches.sort_by(|(score_a, entry_a), (score_b, entry_b)| {
            score_b
                .cmp(score_a)
                .then_with(|| {
                    entry_a
                        .name
                        .to_lowercase()
                        .cmp(&entry_b.name.to_lowercase())
                })
                .then_with(|| entry_a.code.cmp(&entry_b.code))
        });

        matches.into_iter().map(|(_, entry)| entry).collect()
    }

    pub fn all_entries(&self) -> &[TaxonomyEntry] {
        &self.all_entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::schema::{TaxonomyCause, TaxonomyFix};

    fn entry(
        category: ErrorCategory,
        code: u32,
        name: &str,
        summary: &str,
        detailed_explanation: &str,
    ) -> TaxonomyEntry {
        TaxonomyEntry {
            id: format!("test.{}.{}", category, code),
            category,
            code,
            name: name.to_string(),
            severity: "Error".to_string(),
            since_protocol: None,
            deprecated_protocol: None,
            summary: summary.to_string(),
            detailed_explanation: detailed_explanation.to_string(),
            common_causes: Vec::new(),
            suggested_fixes: Vec::new(),
            related_errors: Vec::new(),
            source_file: None,
            source_line: None,
            documentation_url: None,
        }
    }

    fn db_with(entries: Vec<TaxonomyEntry>) -> TaxonomyDatabase {
        let mut db = TaxonomyDatabase {
            entries: HashMap::new(),
            all_entries: Vec::new(),
        };

        for entry in entries {
            db.entries
                .insert((entry.category.clone(), entry.code), entry.clone());
            db.all_entries.push(entry);
        }

        db
    }

    #[test]
    fn search_is_case_insensitive_across_name_summary_and_explanation() {
        let db = db_with(vec![
            entry(
                ErrorCategory::Budget,
                1,
                "CpuLimitExceeded",
                "Too much work",
                "",
            ),
            entry(
                ErrorCategory::Budget,
                2,
                "MemLimitExceeded",
                "CPU and memory",
                "",
            ),
            entry(
                ErrorCategory::Budget,
                3,
                "Other",
                "Nothing here",
                "THE CPU WAS EXHAUSTED",
            ),
        ]);

        let names: Vec<&str> = db.search("cpu").iter().map(|e| e.name.as_str()).collect();

        assert_eq!(names, vec!["CpuLimitExceeded", "MemLimitExceeded", "Other"]);
    }

    #[test]
    fn search_matches_codes_exactly_and_by_prefix() {
        let db = db_with(vec![
            entry(ErrorCategory::Auth, 1, "Unauthorized", "", ""),
            entry(ErrorCategory::Auth, 12, "Expired", "", ""),
            entry(ErrorCategory::Auth, 2, "Forbidden", "", ""),
        ]);

        let names: Vec<&str> = db.search("12").iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Expired"]);

        let names: Vec<&str> = db.search("1").iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Unauthorized", "Expired"]);
    }

    #[test]
    fn search_surfaces_entries_by_category() {
        let db = db_with(vec![
            entry(ErrorCategory::Budget, 1, "CpuLimitExceeded", "", ""),
            entry(ErrorCategory::Budget, 2, "MemLimitExceeded", "", ""),
            entry(ErrorCategory::Storage, 1, "MissingValue", "", ""),
        ]);

        let results = db.search("budget");

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.category == ErrorCategory::Budget));
    }

    #[test]
    fn search_ranks_exact_name_matches_before_summary_matches() {
        let db = db_with(vec![
            entry(
                ErrorCategory::Context,
                2,
                "Other",
                "budget related problem",
                "",
            ),
            entry(ErrorCategory::Context, 1, "Budget", "", ""),
        ]);

        let names: Vec<&str> = db
            .search("budget")
            .iter()
            .map(|e| e.name.as_str())
            .collect();

        assert_eq!(names, vec!["Budget", "Other"]);
    }

    #[test]
    fn search_matches_common_cause_descriptions() {
        let mut contract_error = entry(ErrorCategory::Contract, 0, "ContractError", "", "");
        contract_error.common_causes.push(TaxonomyCause {
            description: "Business logic assertion failure (e.g. insufficient balance)".to_string(),
            likelihood: "high".to_string(),
        });

        let db = db_with(vec![contract_error]);

        let results = db.search("BALANCE");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "ContractError");
    }

    #[test]
    fn search_matches_suggested_fix_descriptions() {
        let mut division_error = entry(ErrorCategory::Wasm, 5, "IntegerDivisionByZero", "", "");
        division_error.suggested_fixes.push(TaxonomyFix {
            description: "Guard against a zero divisor before dividing the balance".to_string(),
            difficulty: "easy".to_string(),
            requires_upgrade: false,
            example: None,
            id: None,
            remedy_code: None,
        });

        let db = db_with(vec![division_error]);

        let results = db.search("divisor");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "IntegerDivisionByZero");
    }

    #[test]
    fn search_ranks_summary_matches_before_cause_and_fix_matches() {
        let summary_match = entry(
            ErrorCategory::Storage,
            1,
            "SummaryHit",
            "balance related problem",
            "",
        );

        let mut cause_match = entry(ErrorCategory::Storage, 2, "CauseHit", "", "");
        cause_match.common_causes.push(TaxonomyCause {
            description: "the balance was corrupted".to_string(),
            likelihood: "medium".to_string(),
        });

        let mut fix_match = entry(ErrorCategory::Storage, 3, "FixHit", "", "");
        fix_match.suggested_fixes.push(TaxonomyFix {
            description: "re-read the balance before writing".to_string(),
            difficulty: "medium".to_string(),
            requires_upgrade: false,
            example: None,
            id: None,
            remedy_code: None,
        });

        let db = db_with(vec![fix_match, cause_match, summary_match]);

        let names: Vec<&str> = db
            .search("balance")
            .iter()
            .map(|e| e.name.as_str())
            .collect();

        assert_eq!(names, vec!["SummaryHit", "CauseHit", "FixHit"]);
    }

    #[test]
    fn embedded_taxonomy_search_finds_entries_by_common_cause() {
        let db = TaxonomyDatabase::load_embedded().expect("embedded taxonomy should load");

        let names: Vec<&str> = db
            .search("balance")
            .iter()
            .map(|e| e.name.as_str())
            .collect();

        assert!(
            names.contains(&"ContractError"),
            "expected ContractError (insufficient balance cause) in {names:?}"
        );
        assert!(
            names.contains(&"IntegerDivisionByZero"),
            "expected IntegerDivisionByZero (zero balance divisor cause) in {names:?}"
        );
    }

    #[test]
    fn search_requires_all_keywords_to_match() {
        let db = db_with(vec![
            entry(
                ErrorCategory::Budget,
                1,
                "CpuLimitExceeded",
                "CPU limit exceeded",
                "",
            ),
            entry(
                ErrorCategory::Budget,
                2,
                "CpuUnavailable",
                "The CPU is busy",
                "",
            ),
            entry(
                ErrorCategory::Budget,
                3,
                "MemLimitExceeded",
                "Memory limit exceeded",
                "",
            ),
        ]);

        let names: Vec<&str> = db
            .search("cpu limit")
            .iter()
            .map(|e| e.name.as_str())
            .collect();

        assert_eq!(names, vec!["CpuLimitExceeded"]);
    }

    #[test]
    fn search_treats_special_characters_literally() {
        let db = db_with(vec![
            entry(ErrorCategory::Contract, 1, "C++Trap", "", ""),
            entry(ErrorCategory::Auth, 2, "Denied", "auth::denied (100%)", ""),
            entry(ErrorCategory::Value, 3, "Quoted", "quote \"error\"", ""),
        ]);

        assert_eq!(db.search("c++").len(), 1);
        assert_eq!(db.search("C++TRAP")[0].name, "C++Trap");
        assert_eq!(db.search("auth::denied (100%)").len(), 1);
        assert_eq!(db.search("\"error\"").len(), 1);
        assert!(db.search("[").is_empty());
    }

    #[test]
    fn search_returns_nothing_for_a_blank_query() {
        let db = db_with(vec![entry(
            ErrorCategory::Budget,
            1,
            "CpuLimitExceeded",
            "",
            "",
        )]);

        assert!(db.search("").is_empty());
        assert!(db.search("   \t  ").is_empty());
    }

    #[test]
    fn test_load_taxonomy_success() {
        let toml = r#"
            [category]
            name = "budget"
            description = "Resource budget errors"
            source_module = "soroban-env-host"

            [[errors]]
            id = "host.budget.limit_exceeded.cpu"
            category = "budget"
            code = 1
            name = "CpuLimitExceeded"
            severity = "error"
            summary = "CPU limit exceeded"
            detailed_explanation = "The contract used more CPU than allowed."
            common_causes = []
            suggested_fixes = []
            related_errors = []
        "#;

        let result = TaxonomyParser::parse(toml);
        assert!(result.is_ok());
        let schema = result.unwrap();
        assert_eq!(schema.category.name, "budget");
        assert_eq!(schema.errors.len(), 1);
        assert_eq!(schema.errors[0].name, "CpuLimitExceeded");
    }

    #[test]
    fn test_load_taxonomy_invalid() {
        let toml = "invalid toml = [[";
        let result = TaxonomyParser::parse(toml);
        assert!(result.is_err());
    }

    #[test]
    fn tier1_entries_with_fixes_have_common_causes() {
        let db = TaxonomyDatabase::load_embedded().expect("Taxonomy should load");
        assert!(!db.is_empty(), "Taxonomy should contain entries");

        for entry in db.all_entries() {
            if entry.suggested_fixes.is_empty() {
                continue;
            }

            assert!(
                !entry.common_causes.is_empty(),
                "{} has suggested fixes but no common causes",
                entry.id
            );
            assert!(
                entry.common_causes.len() <= 3,
                "{} has more than three common causes",
                entry.id
            );
            assert!(
                entry
                    .common_causes
                    .iter()
                    .all(|cause| !cause.description.trim().is_empty()),
                "{} has an empty common cause description",
                entry.id
            );
        }
    }

    #[test]
    #[cfg(feature = "decode")]
    fn taxonomy_covers_tier1_static_mapping_codes() {
        let db = TaxonomyDatabase::load_embedded().expect("Taxonomy should load");

        let expected_codes = [
            (
                ErrorCategory::Budget,
                crate::decode::mappings::budget::BUDGET_ERROR_DETAILS
                    .iter()
                    .map(|detail| (detail.code, detail.name))
                    .collect::<Vec<_>>(),
            ),
            (
                ErrorCategory::Storage,
                crate::decode::mappings::storage::STORAGE_ERROR_DETAILS
                    .iter()
                    .map(|detail| (detail.code, detail.name))
                    .collect::<Vec<_>>(),
            ),
            (
                ErrorCategory::Auth,
                crate::decode::mappings::auth::AUTH_ERROR_DETAILS
                    .iter()
                    .map(|detail| (detail.code, detail.name))
                    .collect::<Vec<_>>(),
            ),
            (
                ErrorCategory::Context,
                crate::decode::mappings::context::CONTEXT_ERROR_DETAILS
                    .iter()
                    .map(|detail| (detail.code, detail.name))
                    .collect::<Vec<_>>(),
            ),
            (
                ErrorCategory::Value,
                crate::decode::mappings::value::VALUE_ERROR_DETAILS
                    .iter()
                    .map(|detail| (detail.code, detail.name))
                    .collect::<Vec<_>>(),
            ),
            (
                ErrorCategory::Wasm,
                crate::decode::mappings::wasm::WASM_ERROR_DETAILS
                    .iter()
                    .map(|detail| (detail.code, detail.name))
                    .collect::<Vec<_>>(),
            ),
            (
                ErrorCategory::Contract,
                crate::decode::mappings::contract::CONTRACT_ERROR_DETAILS
                    .iter()
                    .map(|detail| (detail.code, detail.name))
                    .collect::<Vec<_>>(),
            ),
        ];

        for (category, details) in expected_codes {
            for (code, name) in details {
                let entry = db.lookup(&category, code).unwrap_or_else(|| {
                    panic!("Missing taxonomy entry for {category} code {code} ({name})")
                });

                assert_eq!(
                    entry.name, name,
                    "Taxonomy entry name for {category} code {code} should match static mapping"
                );
                assert!(
                    !entry.common_causes.is_empty(),
                    "Taxonomy entry for {category} code {code} ({name}) has no common causes"
                );
                assert!(
                    entry.common_causes.len() <= 3,
                    "Taxonomy entry for {category} code {code} ({name}) has more than three common causes"
                );
            }
        }
    }
}
