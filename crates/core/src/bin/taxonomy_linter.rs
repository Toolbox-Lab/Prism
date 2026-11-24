//! Taxonomy Linter — validate taxonomy TOML files for syntax, schema and
//! consistency errors.
//!
//! Usage:
//!   taxonomy_linter [--no-color] [<path>...]
//!
//! With no paths, lints the category data files in
//! `crates/core/src/taxonomy/data/` and the enhanced database
//! `taxonomy/enhanced_error_taxonomy.toml`. A directory argument lints every
//! `*.toml` file in it; each file is linted as whichever format its contents
//! follow, and the cross-file checks span every category data file given.
//!
//! Colors are used only when stdout is a terminal and `NO_COLOR` is unset
//! (`--no-color` disables them, `FORCE_COLOR`/`CLICOLOR_FORCE` force them).
//! Exits with status 0 when no errors are found (warnings are allowed),
//! 1 otherwise.

use grat_core::taxonomy::linter::{
    lint_category_sources, taxonomy_files_in, Diagnostic, FileKind, Level, LintReport,
};
use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

struct Style {
    color: bool,
}

impl Style {
    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn pass(&self) -> String {
        self.paint("1;30;42", " PASS ")
    }

    fn fail(&self) -> String {
        self.paint("1;37;41", " FAIL ")
    }

    fn warn(&self) -> String {
        self.paint("1;30;43", " WARN ")
    }

    fn level(&self, level: Level) -> String {
        match level {
            Level::Error => self.paint("1;31", "error"),
            Level::Warning => self.paint("1;33", "warning"),
        }
    }

    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
}

fn color_enabled(no_color_flag: bool) -> bool {
    let set = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty() && v != "0");
    if no_color_flag || std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    set("FORCE_COLOR") || set("CLICOLOR_FORCE") || auto_color_supported()
}

/// Whether stdout is a terminal that interprets ANSI escape sequences.
///
/// A native Windows console can be a terminal without virtual-terminal
/// processing enabled, in which case the badges would print as raw escape
/// codes. Without a console API dependency we cannot query or enable it, so
/// on Windows automatic color is used only under hosts known to handle ANSI
/// (Windows Terminal, ConEmu, ANSICON, or a `TERM`-setting emulator such as
/// mintty). `FORCE_COLOR`/`CLICOLOR_FORCE` still turn color on anywhere.
fn auto_color_supported() -> bool {
    if !std::io::stdout().is_terminal() {
        return false;
    }
    let term_ok = std::env::var_os("TERM").is_some_and(|t| !t.is_empty() && t != "dumb");
    if cfg!(windows) {
        std::env::var_os("WT_SESSION").is_some()
            || std::env::var_os("ANSICON").is_some()
            || std::env::var_os("ConEmuANSI").is_some_and(|v| v == "ON")
            || term_ok
    } else {
        true
    }
}

fn default_paths() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        root.join("src/taxonomy/data"),
        root.join("../../taxonomy/enhanced_error_taxonomy.toml"),
    ]
}

/// Show paths relative to the current directory when possible.
fn display_path(path: &Path) -> PathBuf {
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    std::env::current_dir()
        .ok()
        .and_then(|cwd| absolute.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or(absolute)
}

/// A label for `path` that keeps distinct paths distinct: valid UTF-8 paths
/// are shown as-is, others with their invalid bytes escaped.
fn path_label(path: &Path) -> String {
    match path.to_str() {
        Some(s) => s.to_string(),
        None => {
            let debug = format!("{:?}", path.as_os_str());
            debug
                .strip_prefix('"')
                .and_then(|d| d.strip_suffix('"'))
                .map_or_else(|| debug.clone(), str::to_string)
        }
    }
}

/// Escape control characters (newlines, ANSI escapes, …) in text taken from
/// the linted files or the file system, so it cannot break or restyle the
/// output. The structured report keeps the original values.
fn clean(text: &str) -> String {
    if !text.chars().any(char::is_control) {
        return text.to_string();
    }
    text.chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Expand the command-line paths into the files to lint: a directory
/// contributes exactly the files a directory scan selects
/// ([`taxonomy_files_in`]), a file contributes itself. Each file is linted
/// once even when it is named twice or also found through a directory.
fn collect_files(paths: &[PathBuf], errors: &mut Vec<String>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            match taxonomy_files_in(path) {
                Ok(found) => files.extend(found),
                Err(e) => errors.push(e.to_string()),
            }
        } else if path.is_file() {
            files.push(path.clone());
        } else {
            errors.push(format!("'{}' is not a file or directory", path_label(path)));
        }
    }
    let mut seen = HashSet::new();
    files.retain(|p| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())));
    files
}

/// Read every file and lint them together, so that the cross-file checks
/// (unique ids, non-overlapping codes, related-error references) span all
/// category data files given, whether named directly or found in a directory.
fn lint_files(files: &[PathBuf], errors: &mut Vec<String>) -> LintReport {
    let mut labels = HashSet::new();
    let mut sources = Vec::with_capacity(files.len());
    for path in files {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                errors.push(format!("cannot read {}: {e}", path_label(path)));
                continue;
            }
        };
        // Labels are the report's file keys, so they must be unique.
        let base = path_label(&display_path(path));
        let mut label = base.clone();
        let mut n = 2;
        while !labels.insert(label.clone()) {
            label = format!("{base} [{n}]");
            n += 1;
        }
        sources.push((label, content));
    }
    lint_category_sources(&sources)
}

fn print_diagnostic(style: &Style, d: &Diagnostic) {
    let mut location = clean(&d.file);
    if let Some(line) = d.line {
        location.push_str(&format!(":{line}"));
    }
    let mut subject = String::new();
    if let Some(id) = &d.entry_id {
        subject.push_str(&clean(id));
    }
    if let Some(field) = &d.field {
        if !subject.is_empty() {
            subject.push_str(" › ");
        }
        subject.push_str(&clean(field));
    }
    let subject = if subject.is_empty() {
        String::new()
    } else {
        format!(" {subject}:")
    };
    println!(
        "         {} {}{} {}",
        style.level(d.level),
        style.dim(&location),
        subject,
        clean(&d.message)
    );
}

fn main() -> ExitCode {
    let mut no_color = false;
    let mut paths = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("--no-color") => no_color = true,
            Some("-h" | "--help") => {
                println!("usage: taxonomy_linter [--no-color] [<path>...]");
                println!("Lints taxonomy TOML files (defaults: crates/core/src/taxonomy/data and taxonomy/enhanced_error_taxonomy.toml).");
                return ExitCode::SUCCESS;
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if paths.is_empty() {
        paths = default_paths();
    }
    let style = Style {
        color: color_enabled(no_color),
    };

    let mut io_errors = Vec::new();
    let files = collect_files(&paths, &mut io_errors);
    let report = lint_files(&files, &mut io_errors);
    for e in &io_errors {
        println!("{} {}", style.fail(), clean(e));
    }
    let io_failures = io_errors.len();

    println!("Linting {} taxonomy file(s)…", report.files.len());
    for file in &report.files {
        let diags: Vec<_> = report.diagnostics_for(&file.file).collect();
        let errors = diags.iter().filter(|d| d.is_error()).count();
        let warnings = diags.len() - errors;
        let badge = if errors > 0 {
            style.fail()
        } else if warnings > 0 {
            style.warn()
        } else {
            style.pass()
        };
        let noun = if file.entries == 1 {
            "entry"
        } else {
            "entries"
        };
        let mut counts = format!("{} {noun}", file.entries);
        if file.kind == FileKind::Enhanced {
            counts.push_str(", enhanced schema");
        }
        if errors > 0 {
            counts.push_str(&format!(", {errors} error(s)"));
        }
        if warnings > 0 {
            counts.push_str(&format!(", {warnings} warning(s)"));
        }
        println!(
            "{badge} {} {}",
            clean(&file.file),
            style.dim(&format!("({counts})"))
        );
        for d in diags {
            print_diagnostic(&style, d);
        }
    }

    let errors = report.error_count() + io_failures;
    let summary = format!(
        "{} file(s), {} entr{}, {} error(s), {} warning(s)",
        report.files.len(),
        report.entry_count(),
        if report.entry_count() == 1 {
            "y"
        } else {
            "ies"
        },
        errors,
        report.warning_count()
    );
    println!();
    if errors == 0 {
        println!("{} taxonomy lint passed: {summary}", style.pass());
        ExitCode::SUCCESS
    } else {
        println!("{} taxonomy lint failed: {summary}", style.fail());
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATEGORY: &str = r#"
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
"#;

    #[test]
    fn clean_escapes_control_characters() {
        assert_eq!(clean("plain › text"), "plain › text");
        assert_eq!(clean("a\nb\x1b[31m"), "a\\nb\\u{1b}[31m");
    }

    #[test]
    fn explicit_non_toml_file_is_kept_next_to_its_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.toml"), CATEGORY).expect("write");
        std::fs::write(dir.path().join("b.txt"), "not toml [").expect("write");
        let mut errors = Vec::new();
        let files = collect_files(
            &[
                dir.path().to_path_buf(),
                dir.path().join("b.txt"),
                dir.path().join("a.toml"),
            ],
            &mut errors,
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(files.len(), 2, "{files:?}");
        let report = lint_files(&files, &mut errors);
        assert_eq!(report.files.len(), 2);
        assert!(!report.is_ok(), "malformed b.txt must fail the lint");
    }

    #[test]
    fn cross_file_checks_span_explicit_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let a = dir.path().join("a.toml");
        let b = dir.path().join("b.toml");
        std::fs::write(&a, CATEGORY).expect("write");
        std::fs::write(&b, CATEGORY.replace("code = 1", "code = 2")).expect("write");
        let mut errors = Vec::new();
        let files = collect_files(&[a, b], &mut errors);
        let report = lint_files(&files, &mut errors);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.is_error() && d.message.contains("duplicate")),
            "{:#?}",
            report.diagnostics
        );
    }

    #[test]
    fn missing_path_is_reported() {
        let mut errors = Vec::new();
        let files = collect_files(&[PathBuf::from("/definitely/not/here")], &mut errors);
        assert!(files.is_empty());
        assert_eq!(errors.len(), 1);
    }
}
