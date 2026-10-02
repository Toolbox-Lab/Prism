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
                    tracing::warn!("Failed to parse taxonomy file '{name}': {e}");
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

    pub fn search(&self, query: &str) -> Vec<&TaxonomyEntry> {
        let query = query.to_lowercase();
        self.all_entries
            .iter()
            .filter(|entry| {
                entry.name.to_lowercase().contains(&query)
                    || entry.category.to_string().to_lowercase().contains(&query)
                    || entry.summary.to_lowercase().contains(&query)
                    || entry.detailed_explanation.to_lowercase().contains(&query)
            })
            .collect()
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
