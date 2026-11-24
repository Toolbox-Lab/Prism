//! Taxonomy Linter — validate taxonomy TOML files for syntax, schema and
//! consistency errors.
//!
//! Usage:
//!   taxonomy_linter [--no-color] [<path>...]
//!
//! With no paths, lints the category data files in
//! `crates/core/src/taxonomy/data/` and the enhanced database
//! `taxonomy/enhanced_error_taxonomy.toml`. A directory argument is linted as
//! a set of category data files; a file argument is linted as whichever
//! format its contents follow.
//!
//! Colors are used only when stdout is a terminal and `NO_COLOR` is unset
//! (`--no-color` disables them, `FORCE_COLOR`/`CLICOLOR_FORCE` force them).
//! Exits with status 0 when no errors are found (warnings are allowed),
//! 1 otherwise.

use grat_core::taxonomy::linter::{
    detect_kind, lint_category_sources, lint_dir, lint_enhanced_file, Diagnostic, FileKind, Level,
    LintReport,
};
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
    set("FORCE_COLOR") || set("CLICOLOR_FORCE") || std::io::stdout().is_terminal()
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

fn lint_path(path: &Path) -> Result<LintReport, String> {
    let shown = display_path(path);
    if path.is_dir() {
        let mut report = lint_dir(path).map_err(|e| e.to_string())?;
        // Re-label files relative to the current directory.
        let prefix = path.display().to_string();
        let relabel = |file: &mut String| {
            if let Some(rest) = file.strip_prefix(&prefix) {
                *file = shown
                    .join(rest.trim_start_matches('/'))
                    .display()
                    .to_string();
            }
        };
        report.files.iter_mut().for_each(|f| relabel(&mut f.file));
        report
            .diagnostics
            .iter_mut()
            .for_each(|d| relabel(&mut d.file));
        return Ok(report);
    }
    if path.is_file() {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let name = shown.display().to_string();
        return Ok(match detect_kind(&content) {
            FileKind::Enhanced => {
                let mut report = lint_enhanced_file(path).map_err(|e| e.to_string())?;
                report
                    .files
                    .iter_mut()
                    .for_each(|f| f.file.clone_from(&name));
                report
                    .diagnostics
                    .iter_mut()
                    .for_each(|d| d.file.clone_from(&name));
                report
            }
            FileKind::CategoryData => lint_category_sources(&[(name, content)]),
        });
    }
    Err(format!("'{}' is not a file or directory", path.display()))
}

fn print_diagnostic(style: &Style, d: &Diagnostic) {
    let mut location = d.file.clone();
    if let Some(line) = d.line {
        location.push_str(&format!(":{line}"));
    }
    let mut subject = String::new();
    if let Some(id) = &d.entry_id {
        subject.push_str(id);
    }
    if let Some(field) = &d.field {
        if !subject.is_empty() {
            subject.push_str(" › ");
        }
        subject.push_str(field);
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
        d.message
    );
}

/// Lint each file once, even when it is named twice or also sits in a
/// directory that is linted.
fn dedupe_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = std::collections::HashSet::new();
    paths.retain(|p| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())));
    let dirs: Vec<PathBuf> = paths
        .iter()
        .filter(|p| p.is_dir())
        .filter_map(|p| std::fs::canonicalize(p).ok())
        .collect();
    paths.retain(|p| {
        p.is_dir()
            || !std::fs::canonicalize(p)
                .ok()
                .and_then(|c| c.parent().map(Path::to_path_buf))
                .is_some_and(|parent| dirs.contains(&parent))
    });
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
    dedupe_paths(&mut paths);
    let style = Style {
        color: color_enabled(no_color),
    };

    let mut report = LintReport::default();
    let mut io_failures = 0usize;
    for path in &paths {
        match lint_path(path) {
            Ok(r) => report.merge(r),
            Err(e) => {
                io_failures += 1;
                println!("{} {e}", style.fail());
            }
        }
    }

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
            file.file,
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
