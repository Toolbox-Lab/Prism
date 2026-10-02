use crate::error::{GratError, GratResult};
use crate::taxonomy::schema::{ErrorCategory, TaxonomyEntry, TaxonomySchema};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const MIN_DESCRIPTION_LEN: usize = 15;

/// Helper to locate the 1-based line number of a field in a TOML file.
fn get_line_number(toml_content: &str, entry_id: &str, field_name: Option<&str>) -> Option<usize> {
    let mut entry_line_idx = None;
    let lines: Vec<&str> = toml_content.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        let cleaned = line.replace(' ', "").replace('\'', "\"");
        if cleaned.contains(&format!("id=\"{}\"", entry_id)) {
            entry_line_idx = Some(idx);
            break;
        }
    }

    let entry_idx = entry_line_idx?;

    if let Some(field) = field_name {
        for idx in entry_idx..lines.len() {
            if idx > entry_idx
                && (lines[idx].trim().starts_with("[[errors]]")
                    || lines[idx].trim().starts_with("[metadata]")
                    || lines[idx].trim().starts_with("[category]"))
            {
                break;
            }
            let cleaned = lines[idx].replace(' ', "");
            if cleaned.starts_with(&format!("{}=", field)) {
                return Some(idx + 1);
            }
            // For nested descriptions (causes or fixes)
            if field == "description" && lines[idx].trim().starts_with("description =") {
                return Some(idx + 1);
            }
        }
    }

    Some(entry_idx + 1)
}

/// Lint all `*.toml` taxonomy files in `dir`.
///
/// Under the strict build-time validation rules, this function will panic
/// immediately if any syntax error, unknown field, duplicate code, or invalid/too-short
/// property is encountered.
pub fn lint_dir(dir: &Path) -> GratResult<()> {
    // 1. Gather all *.toml files in the directory
    let mut toml_files: Vec<std::path::PathBuf> = Vec::new();
    let dir_reader = std::fs::read_dir(dir)
        .map_err(|e| GratError::TaxonomyError(format!("Cannot read taxonomy dir: {e}")))?;

    for entry in dir_reader {
        let entry = entry.map_err(|e| GratError::TaxonomyError(e.to_string()))?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            toml_files.push(path);
        }
    }

    // All successfully-parsed entries, annotated with their source file name and file content.
    let mut all_entries: Vec<(String, String, TaxonomyEntry)> = Vec::new();

    // 2. Parse every file and validate per-entry rules
    for path in &toml_files {
        let file_name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("unknown")
            .to_string();

        let content = std::fs::read_to_string(path)
            .map_err(|e| GratError::TaxonomyError(format!("Cannot read file {file_name}: {e}")))?;

        // Attempt strict deserialization. If it fails, extract line info from the error.
        let schema: TaxonomySchema = match toml::from_str(&content) {
            Ok(s) => s,
            Err(e) => {
                let span = e.span();
                let (line, col) = if let Some(span) = span {
                    let mut line = 1;
                    let mut col = 1;
                    for &ch in content.as_bytes().iter().take(span.start) {
                        if ch == b'\n' {
                            line += 1;
                            col = 1;
                        } else {
                            col += 1;
                        }
                    }
                    (line, col)
                } else {
                    (1, 1)
                };
                panic!(
                    "Taxonomy TOML parse error in {} at line {} (column {}): {}",
                    file_name, line, col, e
                );
            }
        };

        // Check category description length
        if schema.category.description.trim().len() < MIN_DESCRIPTION_LEN {
            let line_num = content
                .lines()
                .position(|l| l.trim().starts_with("description ="))
                .map(|idx| idx + 1)
                .unwrap_or(1);
            panic!(
                "Taxonomy validation error in {} at line {}: category description is too short (must be at least {} characters)",
                file_name, line_num, MIN_DESCRIPTION_LEN
            );
        }

        for entry in schema.errors {
            let entry_id = entry.id.clone();

            // id must be non-empty
            if entry.id.trim().is_empty() {
                let line_num = get_line_number(&content, &entry_id, None).unwrap_or(1);
                panic!(
                    "Taxonomy validation error in {} at line {}: entry id is empty",
                    file_name, line_num
                );
            }

            // name must be non-empty
            if entry.name.trim().is_empty() {
                let line_num = get_line_number(&content, &entry_id, Some("name")).unwrap_or(1);
                panic!(
                    "Taxonomy validation error in {} at line {}: name is empty",
                    file_name, line_num
                );
            }

            // summary must meet length requirement
            if entry.summary.trim().len() < MIN_DESCRIPTION_LEN {
                let line_num = get_line_number(&content, &entry_id, Some("summary")).unwrap_or(1);
                panic!(
                    "Taxonomy validation error in {} at line {}: summary for entry '{}' is too short (must be at least {} characters)",
                    file_name, line_num, entry_id, MIN_DESCRIPTION_LEN
                );
            }

            // detailed_explanation must meet length requirement
            if entry.detailed_explanation.trim().len() < MIN_DESCRIPTION_LEN {
                let line_num =
                    get_line_number(&content, &entry_id, Some("detailed_explanation")).unwrap_or(1);
                panic!(
                    "Taxonomy validation error in {} at line {}: detailed_explanation for entry '{}' is too short (must be at least {} characters)",
                    file_name, line_num, entry_id, MIN_DESCRIPTION_LEN
                );
            }

            // severity must be a known value
            const VALID_SEVERITIES: &[&str] = &["Error", "Warning", "Info", "Fatal"];
            if !VALID_SEVERITIES.contains(&entry.severity.as_str()) {
                let line_num = get_line_number(&content, &entry_id, Some("severity")).unwrap_or(1);
                panic!(
                    "Taxonomy validation error in {} at line {}: invalid severity '{}': must be one of {}",
                    file_name, line_num, entry.severity, VALID_SEVERITIES.join(", ")
                );
            }

            // since_protocol > 0 when present
            if let Some(sp) = entry.since_protocol {
                if sp == 0 {
                    let line_num =
                        get_line_number(&content, &entry_id, Some("since_protocol")).unwrap_or(1);
                    panic!(
                        "Taxonomy validation error in {} at line {}: since_protocol must be > 0",
                        file_name, line_num
                    );
                }
            }

            // deprecated_protocol >= since_protocol
            if let (Some(dp), Some(sp)) = (entry.deprecated_protocol, entry.since_protocol) {
                if dp < sp {
                    let line_num =
                        get_line_number(&content, &entry_id, Some("deprecated_protocol"))
                            .unwrap_or(1);
                    panic!(
                        "Taxonomy validation error in {} at line {}: deprecated_protocol ({dp}) must be >= since_protocol ({sp})",
                        file_name, line_num
                    );
                }
            }

            // documentation_url must parse as a URL when present
            if let Some(ref doc_url) = entry.documentation_url {
                if url::Url::parse(doc_url).is_err() {
                    let line_num = get_line_number(&content, &entry_id, Some("documentation_url"))
                        .unwrap_or(1);
                    panic!(
                        "Taxonomy validation error in {} at line {}: documentation_url '{doc_url}' is not a valid URL",
                        file_name, line_num
                    );
                }
            }

            // check causes descriptions
            for cause in &entry.common_causes {
                if cause.description.trim().len() < MIN_DESCRIPTION_LEN {
                    let line_num =
                        get_line_number(&content, &entry_id, Some("description")).unwrap_or(1);
                    panic!(
                        "Taxonomy validation error in {} at line {}: common cause description is too short (must be at least {} characters)",
                        file_name, line_num, MIN_DESCRIPTION_LEN
                    );
                }
            }

            // check fixes descriptions
            for fix in &entry.suggested_fixes {
                if fix.description.trim().len() < MIN_DESCRIPTION_LEN {
                    let line_num =
                        get_line_number(&content, &entry_id, Some("description")).unwrap_or(1);
                    panic!(
                        "Taxonomy validation error in {} at line {}: suggested fix description is too short (must be at least {} characters)",
                        file_name, line_num, MIN_DESCRIPTION_LEN
                    );
                }
            }

            all_entries.push((file_name.clone(), content.clone(), entry));
        }
    }

    // 3. Cross-entry checks

    // Duplicate (category, code) pairs
    let mut seen: HashMap<(ErrorCategory, u32), (String, String)> = HashMap::new();
    for (file_name, content, entry) in &all_entries {
        let key = (entry.category.clone(), entry.code);
        if let Some((prev_file, prev_id)) = seen.get(&key) {
            let line_num = get_line_number(content, &entry.id, Some("code")).unwrap_or(1);
            panic!(
                "Taxonomy validation error in {} at line {}: duplicate (category, code) pair ({}, {}) already defined by entry '{}' in {}",
                file_name, line_num, entry.category, entry.code, prev_id, prev_file
            );
        } else {
            seen.insert(key, (file_name.clone(), entry.id.clone()));
        }
    }

    // related_errors should reference existing ids
    let all_ids: HashSet<&str> = all_entries.iter().map(|(_, _, e)| e.id.as_str()).collect();
    for (file_name, content, entry) in &all_entries {
        for rel in &entry.related_errors {
            if !all_ids.contains(rel.as_str()) {
                let line_num =
                    get_line_number(content, &entry.id, Some("related_errors")).unwrap_or(1);
                panic!(
                    "Taxonomy validation error in {} at line {}: related_errors references '{}' which does not exist in any loaded file",
                    file_name, line_num, rel
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::schema::{CategoryMeta, ErrorCategory, TaxonomyEntry, TaxonomySchema};

    fn valid_entry(id: &str, category: ErrorCategory, code: u32) -> TaxonomyEntry {
        TaxonomyEntry {
            id: id.to_string(),
            category,
            code,
            name: format!("TestName{code}"),
            severity: "Error".to_string(),
            since_protocol: Some(20),
            deprecated_protocol: None,
            summary: "Test summary is long enough.".to_string(),
            detailed_explanation: "Test detailed explanation is also long enough.".to_string(),
            common_causes: vec![],
            suggested_fixes: vec![],
            related_errors: vec![],
            source_file: None,
            source_line: None,
            documentation_url: None,
        }
    }

    fn write_taxonomy_file(
        dir: &std::path::Path,
        name: &str,
        entries: Vec<TaxonomyEntry>,
    ) -> std::path::PathBuf {
        let schema = TaxonomySchema {
            category: CategoryMeta {
                name: "Test".to_string(),
                description: "Test data (must be long enough)".to_string(),
                source_module: "test".to_string(),
            },
            errors: entries,
        };
        let toml_str = toml::to_string(&schema).expect("serialize schema");
        let path = dir.join(name);
        std::fs::write(&path, toml_str).expect("write taxonomy file");
        path
    }

    #[test]
    fn valid_entry_passes() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_taxonomy_file(
            dir.path(),
            "test.toml",
            vec![
                valid_entry("test.entry.1", ErrorCategory::Budget, 1),
                valid_entry("test.entry.2", ErrorCategory::Storage, 2),
            ],
        );

        let result = lint_dir(dir.path());
        assert!(result.is_ok());
    }

    #[test]
    #[should_panic(expected = "Taxonomy TOML parse error")]
    fn unknown_field_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let toml_str = r#"
[category]
name = "Test"
description = "Test data (must be long enough)"
source_module = "test"

[[errors]]
id = "test.err"
category = "budget"
code = 1
name = "TestErr"
severity = "Error"
summary = "Test summary is long enough."
detailed_explanation = "Test detailed explanation is also long enough."
unknown_field = "oops"
"#;
        std::fs::write(dir.path().join("test.toml"), toml_str).expect("write");
        let _ = lint_dir(dir.path());
    }

    #[test]
    #[should_panic(expected = "duplicate (category, code) pair")]
    fn duplicate_category_code_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_taxonomy_file(
            dir.path(),
            "test.toml",
            vec![
                valid_entry("test.dup.a", ErrorCategory::Budget, 1),
                valid_entry("test.dup.b", ErrorCategory::Budget, 1),
            ],
        );

        let _ = lint_dir(dir.path());
    }

    #[test]
    #[should_panic(expected = "documentation_url")]
    fn malformed_documentation_url_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut entry = valid_entry("test.bad.url", ErrorCategory::Budget, 42);
        entry.documentation_url = Some("not a url".to_string());
        write_taxonomy_file(dir.path(), "test.toml", vec![entry]);

        let _ = lint_dir(dir.path());
    }

    #[test]
    fn valid_documentation_url_passes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut entry = valid_entry("test.good.url", ErrorCategory::Budget, 43);
        entry.documentation_url = Some("https://example.com/docs".to_string());
        write_taxonomy_file(dir.path(), "test.toml", vec![entry]);

        let result = lint_dir(dir.path());
        assert!(result.is_ok());
    }

    #[test]
    #[should_panic(expected = "invalid severity")]
    fn bad_severity_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut entry = valid_entry("test.bad.severity", ErrorCategory::Budget, 44);
        entry.severity = "BadSeverity".to_string();
        write_taxonomy_file(dir.path(), "test.toml", vec![entry]);

        let _ = lint_dir(dir.path());
    }

    #[test]
    #[should_panic(expected = "since_protocol must be > 0")]
    fn since_protocol_zero_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut entry = valid_entry("test.bad.sp", ErrorCategory::Budget, 45);
        entry.since_protocol = Some(0);
        write_taxonomy_file(dir.path(), "test.toml", vec![entry]);

        let _ = lint_dir(dir.path());
    }

    #[test]
    #[should_panic(expected = "deprecated_protocol")]
    fn deprecated_before_since_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut entry = valid_entry("test.bad.depr", ErrorCategory::Budget, 46);
        entry.since_protocol = Some(20);
        entry.deprecated_protocol = Some(15);
        write_taxonomy_file(dir.path(), "test.toml", vec![entry]);

        let _ = lint_dir(dir.path());
    }

    #[test]
    #[should_panic(expected = "related_errors references")]
    fn unresolved_related_error_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut entry = valid_entry("test.rel", ErrorCategory::Budget, 47);
        entry.related_errors = vec!["nonexistent.id".to_string()];
        write_taxonomy_file(dir.path(), "test.toml", vec![entry]);

        let _ = lint_dir(dir.path());
    }

    #[test]
    #[should_panic(expected = "summary for entry 'test.short' is too short")]
    fn short_summary_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut entry = valid_entry("test.short", ErrorCategory::Budget, 48);
        entry.summary = "Error occurred".to_string(); // 14 characters
        write_taxonomy_file(dir.path(), "test.toml", vec![entry]);

        let _ = lint_dir(dir.path());
    }

    #[test]
    fn run_linter_on_production_taxonomy_data() {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("src/taxonomy/data");
        lint_dir(&p).expect("Linter failed on production data");
    }
}
