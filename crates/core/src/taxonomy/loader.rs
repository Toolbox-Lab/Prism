//! Taxonomy database loader.
//!
//! Loads TOML taxonomy files from embedded data or disk, indexes them by
//! (category, code) for O(1) lookup.

use crate::taxonomy::schema::{ErrorCategory, TaxonomyEntry, TaxonomyFile};
use crate::types::error::{PrismError, PrismResult};
use std::collections::HashMap;

/// In-memory taxonomy database indexed by (category, code).
pub struct TaxonomyDatabase {
    /// Entries indexed by (category, code).
    entries: HashMap<(ErrorCategory, u32), TaxonomyEntry>,
    /// All entries in a flat list.
    all_entries: Vec<TaxonomyEntry>,
}

impl TaxonomyDatabase {
    /// Load the taxonomy database from the embedded TOML files.
    pub fn load_embedded() -> PrismResult<Self> {
        let mut db = Self {
            entries: HashMap::new(),
            all_entries: Vec::new(),
        };

        // Load each category file
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
            match toml::from_str::<TaxonomyFile>(content) {
                Ok(file) => {
                    for entry in file.errors {
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

    /// Load the taxonomy database from a directory of TOML files.
    pub fn load_from_dir(dir: &std::path::Path) -> PrismResult<Self> {
        let mut db = Self {
            entries: HashMap::new(),
            all_entries: Vec::new(),
        };

        for entry in std::fs::read_dir(dir)
            .map_err(|e| PrismError::TaxonomyError(format!("Cannot read taxonomy dir: {e}")))?
        {
            let entry = entry.map_err(|e| PrismError::TaxonomyError(e.to_string()))?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "toml") {
                let content = std::fs::read_to_string(&path).map_err(|e| {
                    PrismError::TaxonomyError(format!("Cannot read {}: {e}", path.display()))
                })?;

                let file: TaxonomyFile = toml::from_str(&content).map_err(|e| {
                    PrismError::TaxonomyError(format!("Parse error in {}: {e}", path.display()))
                })?;

                for entry in file.errors {
                    db.entries
                        .insert((entry.category.clone(), entry.code), entry.clone());
                    db.all_entries.push(entry);
                }
            }
        }

        Ok(db)
    }

    /// Look up an error by category and code. O(1).
    pub fn lookup(&self, category: &ErrorCategory, code: u32) -> Option<&TaxonomyEntry> {
        self.entries.get(&(category.clone(), code))
    }

    /// Get all entries for a given category.
    pub fn entries_for_category(&self, category: &ErrorCategory) -> Vec<&TaxonomyEntry> {
        self.all_entries
            .iter()
            .filter(|e| &e.category == category)
            .collect()
    }

    /// Get the total number of entries in the database.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the database is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
