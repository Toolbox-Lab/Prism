//! Taxonomy linter — validates taxonomy TOML files for syntax, schema and
//! consistency problems.
//!
//! Two file formats are understood:
//!
//! * **Category data files** (`crates/core/src/taxonomy/data/*.toml`), which
//!   follow [`TaxonomySchema`]: a `[category]` header plus `[[errors]]`
//!   entries.
//! * **The enhanced taxonomy database** (`taxonomy/enhanced_error_taxonomy.toml`,
//!   schema v2), made of `[error.<KEY>]` tables plus a `[metadata]` table.
//!
//! Every check is a pure function that returns structured [`Diagnostic`]s
//! (file, entry id, field, line, message) instead of panicking, so callers
//! such as the `taxonomy_linter` binary decide how to render them and which
//! exit code to use.

use crate::error::{GratError, GratResult};
use crate::taxonomy::schema::{
    CategoryMeta, ErrorCategory, TaxonomyCause, TaxonomyEntry, TaxonomyFix,
};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// Minimum length (after trimming) of descriptive text fields.
pub const MIN_DESCRIPTION_LEN: usize = 15;

/// Severities accepted in a taxonomy entry (compared case-insensitively).
pub const VALID_SEVERITIES: &[&str] = &["info", "warning", "error", "fatal"];

/// Every category of [`ErrorCategory`], used to validate and to list the
/// accepted values in diagnostics. Kept in sync by `category_name`, whose
/// exhaustive `match` fails to compile when a variant is added.
pub const KNOWN_CATEGORIES: &[ErrorCategory] = &[
    ErrorCategory::Budget,
    ErrorCategory::Storage,
    ErrorCategory::Auth,
    ErrorCategory::Context,
    ErrorCategory::Value,
    ErrorCategory::Object,
    ErrorCategory::Crypto,
    ErrorCategory::Contract,
    ErrorCategory::Wasm,
    ErrorCategory::Events,
];

/// Relationship kinds accepted in the enhanced taxonomy's `related_errors`.
pub const VALID_RELATIONSHIPS: &[&str] = &["similar", "precondition", "cascade"];

/// Required fields of an enhanced taxonomy entry when `[metadata]` does not
/// list them itself.
const DEFAULT_ENHANCED_REQUIRED: &[&str] = &["title", "description", "code", "since_protocol"];

/// Fields an enhanced taxonomy entry may carry.
const ENHANCED_KNOWN_FIELDS: &[&str] = &[
    "title",
    "description",
    "code",
    "since_protocol",
    "category",
    "related_errors",
    "documentation",
    "protocol",
    "examples",
];

/// The serialized (TOML) name of a category, e.g. `"budget"`.
fn category_name(category: &ErrorCategory) -> &'static str {
    match category {
        ErrorCategory::Budget => "budget",
        ErrorCategory::Storage => "storage",
        ErrorCategory::Auth => "auth",
        ErrorCategory::Context => "context",
        ErrorCategory::Value => "value",
        ErrorCategory::Object => "object",
        ErrorCategory::Crypto => "crypto",
        ErrorCategory::Contract => "contract",
        ErrorCategory::Wasm => "wasm",
        ErrorCategory::Events => "events",
    }
}

fn category_list() -> String {
    KNOWN_CATEGORIES
        .iter()
        .map(category_name)
        .collect::<Vec<_>>()
        .join(", ")
}

// ── Diagnostics ──────────────────────────────────────────────────────────

/// How serious a [`Diagnostic`] is. Only [`Level::Error`] fails the lint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Warning,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// One problem found in a taxonomy file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: Level,
    /// The file the problem is in, as given to the linter.
    pub file: String,
    /// The entry the problem belongs to (its `id`, or its table key in the
    /// enhanced taxonomy), when it belongs to one.
    pub entry_id: Option<String>,
    /// The offending field, when there is one.
    pub field: Option<String>,
    /// 1-based line number, when it can be located.
    pub line: Option<usize>,
    pub message: String,
}

impl Diagnostic {
    fn new(level: Level, file: &str, message: impl Into<String>) -> Self {
        Self {
            level,
            file: file.to_string(),
            entry_id: None,
            field: None,
            line: None,
            message: message.into(),
        }
    }

    fn error(file: &str, message: impl Into<String>) -> Self {
        Self::new(Level::Error, file, message)
    }

    fn warning(file: &str, message: impl Into<String>) -> Self {
        Self::new(Level::Warning, file, message)
    }

    fn entry(mut self, id: &str) -> Self {
        if !id.is_empty() {
            self.entry_id = Some(id.to_string());
        }
        self
    }

    fn field(mut self, field: &str) -> Self {
        self.field = Some(field.to_string());
        self
    }

    fn line(mut self, line: Option<usize>) -> Self {
        self.line = line;
        self
    }

    pub fn is_error(&self) -> bool {
        self.level == Level::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.level, self.file)?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        if let Some(id) = &self.entry_id {
            write!(f, " [{id}]")?;
        }
        if let Some(field) = &self.field {
            write!(f, " `{field}`")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Which taxonomy format a file follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// `[category]` + `[[errors]]` category data file.
    CategoryData,
    /// `[error.<KEY>]` + `[metadata]` enhanced taxonomy database.
    Enhanced,
}

/// What the linter checked in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSummary {
    pub file: String,
    pub kind: FileKind,
    /// Number of entries found (0 when the file could not be parsed).
    pub entries: usize,
}

/// The result of linting one or more taxonomy files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintReport {
    pub files: Vec<FileSummary>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LintReport {
    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics.len() - self.error_count()
    }

    pub fn entry_count(&self) -> usize {
        self.files.iter().map(|f| f.entries).sum()
    }

    /// `true` when no error-level diagnostic was produced.
    pub fn is_ok(&self) -> bool {
        self.error_count() == 0
    }

    /// Diagnostics belonging to `file`, in the order they were found.
    pub fn diagnostics_for<'a>(&'a self, file: &'a str) -> impl Iterator<Item = &'a Diagnostic> {
        self.diagnostics.iter().filter(move |d| d.file == file)
    }

    /// Append another report (e.g. the enhanced taxonomy) to this one.
    pub fn merge(&mut self, other: LintReport) {
        self.files.extend(other.files);
        self.diagnostics.extend(other.diagnostics);
    }
}

// ── Line lookup helpers ─────────────────────────────────────────────────

/// 1-based line/column of a byte offset.
fn line_col(content: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for &b in content.as_bytes().iter().take(offset) {
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Collapse a multi-line parser message onto one line.
fn one_line(message: &str) -> String {
    message
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Does `line` assign `key` (`key = ...`)?
fn assigns(line: &str, key: &str) -> bool {
    line.trim_start()
        .strip_prefix(key)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

/// Normalize a TOML table header line to `[a.b]` / `[[a.b]]` form: strip a
/// trailing comment, whitespace around the brackets and dots, and simple
/// quotes around key parts (so `[[ errors ]] # note` becomes `[[errors]]` and
/// `["error".A]` becomes `[error.A]`). Returns `None` for non-header lines.
fn normalize_header(line: &str) -> Option<String> {
    let t = line.trim();
    let (open, close) = if t.starts_with("[[") {
        ("[[", "]]")
    } else if t.starts_with('[') {
        ("[", "]")
    } else {
        return None;
    };
    let body = &t[open.len()..];
    // Find the closing bracket outside of quoted key parts.
    let mut quote: Option<char> = None;
    let mut end = None;
    for (i, c) in body.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if body[i..].starts_with(close) => {
                end = Some(i);
                break;
            }
            None => {}
        }
    }
    let end = end?;
    let rest = body[end + close.len()..].trim_start();
    if !(rest.is_empty() || rest.starts_with('#')) {
        return None;
    }
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut quote: Option<char> = None;
    for c in body[..end].chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => part.push(c),
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '.' => parts.push(std::mem::take(&mut part).trim().to_string()),
            None => part.push(c),
        }
    }
    parts.push(part.trim().to_string());
    Some(format!("{open}{}{close}", parts.join(".")))
}

/// Locates lines inside a TOML document by table header, so diagnostics can
/// point at the right place even when ids are empty or duplicated.
struct LineIndex<'a> {
    lines: Vec<&'a str>,
}

impl<'a> LineIndex<'a> {
    fn new(content: &'a str) -> Self {
        Self {
            lines: content.lines().collect(),
        }
    }

    /// Indices of lines that are the table header `header`, in any valid
    /// spelling (see [`normalize_header`]).
    fn headers(&self, header: &str) -> Vec<usize> {
        let wanted = normalize_header(header);
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| wanted.is_some() && normalize_header(l) == wanted)
            .map(|(i, _)| i)
            .collect()
    }

    /// End (exclusive) of the block that starts at `start`: the next line
    /// whose header is not a sub-table of `prefix`.
    fn block_end(&self, start: usize, prefix: &str) -> usize {
        let sub = format!("{prefix}.");
        for (i, l) in self.lines.iter().enumerate().skip(start + 1) {
            if let Some(h) = normalize_header(l) {
                let name = h.trim_matches(|c| c == '[' || c == ']');
                if !name.starts_with(&sub) {
                    return i;
                }
            }
        }
        self.lines.len()
    }

    /// First line in `[start, end)` assigning `key`, 1-based.
    fn find_key(&self, start: usize, end: usize, key: &str) -> Option<usize> {
        (start..end.min(self.lines.len()))
            .find(|&i| assigns(self.lines[i], key))
            .map(|i| i + 1)
    }

    /// First line assigning `key` before any table header, 1-based.
    fn find_key_in_table(&self, header: &str, key: &str) -> Option<usize> {
        let start = *self.headers(header).first()?;
        let normalized = normalize_header(header)?;
        let end = self.block_end(start, normalized.trim_matches(|c| c == '[' || c == ']'));
        self.find_key(start, end, key).or(Some(start + 1))
    }
}

// ── Category data files (`[category]` + `[[errors]]`) ─────────────────────

/// A parsed entry with the context needed for cross-entry checks.
#[derive(Debug, Clone)]
pub struct LocatedEntry {
    pub file: String,
    /// 1-based line of the entry's `code` field (or its header).
    pub code_line: Option<usize>,
    /// 1-based line of the entry's `id` field (or its header).
    pub id_line: Option<usize>,
    /// 1-based line of the entry's `related_errors` field (or its header).
    pub related_line: Option<usize>,
    pub entry: TaxonomyEntry,
}

/// The result of linting a single category data file.
#[derive(Debug, Clone, Default)]
pub struct CategoryFileLint {
    pub diagnostics: Vec<Diagnostic>,
    /// Entries that deserialized; [`lint_cross_entries`] needs them for
    /// checks that span files.
    pub entries: Vec<LocatedEntry>,
    /// Number of `[[errors]]` entries in the file, valid or not.
    pub entry_count: usize,
}

/// Lint one category data file's contents.
#[allow(clippy::too_many_lines)]
pub fn lint_category_str(file: &str, content: &str) -> CategoryFileLint {
    let mut diags = Vec::new();
    let mut located = Vec::new();
    let done = |diagnostics, entries: Vec<LocatedEntry>, entry_count| CategoryFileLint {
        diagnostics,
        entries,
        entry_count,
    };

    let table: toml::Table = match content.parse() {
        Ok(t) => t,
        Err(e) => {
            let line = e.span().map(|s| line_col(content, s.start));
            let msg = match line {
                Some((_, col)) => format!(
                    "TOML syntax error (column {col}): {}",
                    one_line(e.message())
                ),
                None => format!("TOML syntax error: {}", one_line(e.message())),
            };
            diags.push(Diagnostic::error(file, msg).line(line.map(|(l, _)| l)));
            return done(diags, located, 0);
        }
    };

    let idx = LineIndex::new(content);

    for key in table.keys() {
        if key != "category" && key != "errors" {
            diags.push(
                Diagnostic::error(file, format!("unknown top-level key `{key}`"))
                    .field(key)
                    .line(
                        idx.headers(&format!("[{key}]"))
                            .first()
                            .map(|i| i + 1)
                            .or_else(|| idx.find_key(0, idx.lines.len(), key)),
                    ),
            );
        }
    }

    // [category] header
    match table.get("category") {
        None => diags.push(Diagnostic::error(file, "missing `[category]` table").field("category")),
        Some(value) => match value.clone().try_into::<CategoryMeta>() {
            Ok(meta) => {
                for (field, text) in [("name", &meta.name), ("source_module", &meta.source_module)]
                {
                    if text.trim().is_empty() {
                        diags.push(
                            Diagnostic::error(file, format!("category {field} is empty"))
                                .field(field)
                                .line(idx.find_key_in_table("[category]", field)),
                        );
                    }
                }
                if meta.description.trim().chars().count() < MIN_DESCRIPTION_LEN {
                    diags.push(
                        Diagnostic::error(
                            file,
                            format!(
                                "category description is too short (must be at least \
                                 {MIN_DESCRIPTION_LEN} characters)"
                            ),
                        )
                        .field("description")
                        .line(idx.find_key_in_table("[category]", "description")),
                    );
                }
            }
            Err(e) => diags.push(
                Diagnostic::error(file, format!("invalid `[category]` table: {}", e.message()))
                    .field("category")
                    .line(idx.headers("[category]").first().map(|i| i + 1)),
            ),
        },
    }

    // [[errors]] entries
    let headers = idx.headers("[[errors]]");
    let entries = match table.get("errors") {
        None => {
            diags.push(Diagnostic::error(file, "missing `[[errors]]` entries").field("errors"));
            return done(diags, located, 0);
        }
        Some(toml::Value::Array(items)) => items,
        Some(_) => {
            diags.push(
                Diagnostic::error(file, "`errors` must be an array of tables (`[[errors]]`)")
                    .field("errors"),
            );
            return done(diags, located, 0);
        }
    };

    // Pair entries with headers by position only when every entry's header
    // was found; otherwise leave the lines out rather than point at another
    // entry (e.g. entries written as an inline array).
    let headers_align = headers.len() == entries.len();
    for (i, raw) in entries.iter().enumerate() {
        let start = if headers_align {
            headers.get(i).copied()
        } else {
            None
        };
        let end = start.map_or(0, |s| idx.block_end(s, "errors"));
        let header_line = start.map(|s| s + 1);
        let line_of = |key: &str| {
            start
                .and_then(|s| idx.find_key(s, end, key))
                .or(header_line)
        };

        let Some(raw_table) = raw.as_table() else {
            diags.push(
                Diagnostic::error(file, format!("errors[{i}] is not a table")).line(header_line),
            );
            continue;
        };
        let raw_id = raw_table
            .get("id")
            .and_then(toml::Value::as_str)
            .unwrap_or("")
            .to_string();
        let label = if raw_id.is_empty() {
            format!("errors[{i}]")
        } else {
            raw_id.clone()
        };

        // Validate category and severity on the raw value first, so a bad
        // value gets a precise message instead of a generic serde error.
        let mut raw_ok = true;
        if let Some(cat) = raw_table.get("category") {
            let valid = cat
                .clone()
                .try_into::<ErrorCategory>()
                .is_ok_and(|c| KNOWN_CATEGORIES.contains(&c));
            if !valid {
                raw_ok = false;
                diags.push(
                    Diagnostic::error(
                        file,
                        format!("invalid category {cat}: must be one of {}", category_list()),
                    )
                    .entry(&label)
                    .field("category")
                    .line(line_of("category")),
                );
            }
        }
        if let Some(sev) = raw_table.get("severity") {
            let valid = sev
                .as_str()
                .is_some_and(|s| VALID_SEVERITIES.contains(&s.to_ascii_lowercase().as_str()));
            if !valid {
                raw_ok = false;
                diags.push(
                    Diagnostic::error(
                        file,
                        format!(
                            "invalid severity {sev}: must be one of {}",
                            VALID_SEVERITIES.join(", ")
                        ),
                    )
                    .entry(&label)
                    .field("severity")
                    .line(line_of("severity")),
                );
            }
        }

        let entry: TaxonomyEntry = match raw.clone().try_into() {
            Ok(e) => e,
            Err(e) => {
                if raw_ok {
                    diags.push(
                        Diagnostic::error(file, format!("schema error: {}", one_line(e.message())))
                            .entry(&label)
                            .line(header_line),
                    );
                }
                continue;
            }
        };

        diags.extend(lint_entry(file, &label, &entry, &line_of, &idx, start, end));

        located.push(LocatedEntry {
            file: file.to_string(),
            code_line: line_of("code"),
            id_line: line_of("id"),
            related_line: line_of("related_errors"),
            entry,
        });
    }

    let count = entries.len();
    done(diags, located, count)
}

/// Per-entry field checks for a successfully deserialized entry.
#[allow(clippy::too_many_lines)]
fn lint_entry(
    file: &str,
    label: &str,
    entry: &TaxonomyEntry,
    line_of: &dyn Fn(&str) -> Option<usize>,
    idx: &LineIndex<'_>,
    start: Option<usize>,
    end: usize,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let err = |field: &str, msg: String| {
        Diagnostic::error(file, msg)
            .entry(label)
            .field(field)
            .line(line_of(field))
    };

    if entry.id.trim().is_empty() {
        diags.push(err("id", "entry id is empty".into()));
    }
    if entry.name.trim().is_empty() {
        diags.push(err("name", "name is empty".into()));
    }

    for (field, text) in [
        ("summary", &entry.summary),
        ("detailed_explanation", &entry.detailed_explanation),
    ] {
        let len = text.trim().chars().count();
        if len == 0 {
            diags.push(err(field, format!("{field} is empty")));
        } else if len < MIN_DESCRIPTION_LEN {
            diags.push(err(
                field,
                format!(
                    "{field} is too short ({len} characters, must be at least \
                     {MIN_DESCRIPTION_LEN})"
                ),
            ));
        }
    }

    if entry.since_protocol == Some(0) {
        diags.push(err("since_protocol", "since_protocol must be > 0".into()));
    }
    if let (Some(dp), Some(sp)) = (entry.deprecated_protocol, entry.since_protocol) {
        if dp < sp {
            diags.push(err(
                "deprecated_protocol",
                format!("deprecated_protocol ({dp}) must be >= since_protocol ({sp})"),
            ));
        }
    }

    if let Some(doc_url) = &entry.documentation_url {
        if let Err(reason) = check_http_url(doc_url) {
            diags.push(err(
                "documentation_url",
                format!("documentation_url {doc_url:?} is not a valid URL: {reason}"),
            ));
        }
    }

    let sub_lines = |header: &str| -> Vec<usize> {
        start.map_or_else(Vec::new, |s| {
            idx.headers(header)
                .into_iter()
                .filter(|&h| h > s && h < end)
                .map(|h| h + 1)
                .collect()
        })
    };
    let cause_lines = sub_lines("[[errors.common_causes]]");
    for (n, TaxonomyCause { description, .. }) in entry.common_causes.iter().enumerate() {
        if description.trim().chars().count() < MIN_DESCRIPTION_LEN {
            diags.push(
                Diagnostic::error(
                    file,
                    format!(
                        "common_causes[{n}] description is too short (must be at least \
                         {MIN_DESCRIPTION_LEN} characters)"
                    ),
                )
                .entry(label)
                .field("common_causes.description")
                .line(
                    cause_lines
                        .get(n)
                        .copied()
                        .or_else(|| line_of("common_causes")),
                ),
            );
        }
    }
    let fix_lines = sub_lines("[[errors.suggested_fixes]]");
    for (n, TaxonomyFix { description, .. }) in entry.suggested_fixes.iter().enumerate() {
        if description.trim().chars().count() < MIN_DESCRIPTION_LEN {
            diags.push(
                Diagnostic::error(
                    file,
                    format!(
                        "suggested_fixes[{n}] description is too short (must be at least \
                         {MIN_DESCRIPTION_LEN} characters)"
                    ),
                )
                .entry(label)
                .field("suggested_fixes.description")
                .line(
                    fix_lines
                        .get(n)
                        .copied()
                        .or_else(|| line_of("suggested_fixes")),
                ),
            );
        }
    }

    diags
}

/// `Ok` when `raw` is an absolute `http`/`https` URL with a host.
pub fn check_http_url(raw: &str) -> Result<(), String> {
    let parsed = url::Url::parse(raw).map_err(|e| e.to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!("unsupported scheme `{}`", parsed.scheme()));
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err("missing host".into());
    }
    Ok(())
}

/// Checks that span entries (and files): unique ids, non-overlapping
/// `(category, code)` pairs, and `related_errors` that resolve.
pub fn lint_cross_entries(entries: &[LocatedEntry]) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let mut ids: HashMap<&str, &LocatedEntry> = HashMap::new();
    for e in entries {
        let id = e.entry.id.as_str();
        if id.trim().is_empty() {
            continue;
        }
        if let Some(prev) = ids.get(id) {
            diags.push(
                Diagnostic::error(
                    &e.file,
                    format!(
                        "duplicate id {id:?}: already defined in {}{}",
                        prev.file,
                        prev.id_line.map_or_else(String::new, |l| format!(":{l}"))
                    ),
                )
                .entry(id)
                .field("id")
                .line(e.id_line),
            );
        } else {
            ids.insert(id, e);
        }
    }

    let mut codes: HashMap<(&ErrorCategory, u32), &LocatedEntry> = HashMap::new();
    for e in entries {
        let key = (&e.entry.category, e.entry.code);
        if let Some(prev) = codes.get(&key) {
            diags.push(
                Diagnostic::error(
                    &e.file,
                    format!(
                        "error code {} overlaps in category {}: already used by {:?} in {}{}",
                        e.entry.code,
                        category_name(&e.entry.category),
                        prev.entry.id,
                        prev.file,
                        prev.code_line.map_or_else(String::new, |l| format!(":{l}"))
                    ),
                )
                .entry(&e.entry.id)
                .field("code")
                .line(e.code_line),
            );
        } else {
            codes.insert(key, e);
        }
    }

    let known: HashSet<&str> = entries.iter().map(|e| e.entry.id.as_str()).collect();
    for e in entries {
        for rel in &e.entry.related_errors {
            if rel == &e.entry.id {
                diags.push(
                    Diagnostic::error(&e.file, "related_errors references the entry itself")
                        .entry(&e.entry.id)
                        .field("related_errors")
                        .line(e.related_line),
                );
            } else if !known.contains(rel.as_str()) {
                diags.push(
                    Diagnostic::error(
                        &e.file,
                        format!("related_errors references unknown id {rel:?}"),
                    )
                    .entry(&e.entry.id)
                    .field("related_errors")
                    .line(e.related_line),
                );
            }
        }
    }

    diags
}

/// Lint a set of taxonomy files given as `(file name, contents)` pairs,
/// including the cross-file checks between category data files. A file in
/// the enhanced format (see [`detect_kind`]) is linted as such.
pub fn lint_category_sources(sources: &[(String, String)]) -> LintReport {
    let mut report = LintReport::default();
    let mut all = Vec::new();
    for (file, content) in sources {
        if detect_kind(content) == FileKind::Enhanced {
            report.merge(lint_enhanced_str(file, content));
            continue;
        }
        let lint = lint_category_str(file, content);
        report.files.push(FileSummary {
            file: file.clone(),
            kind: FileKind::CategoryData,
            entries: lint.entry_count,
        });
        report.diagnostics.extend(lint.diagnostics);
        all.extend(lint.entries);
    }
    report.diagnostics.extend(lint_cross_entries(&all));
    report
}

/// The `*.toml` regular files directly inside `dir`, sorted by name: the
/// files [`lint_dir`] lints.
///
/// Errors while listing the directory, or while reading the metadata of a
/// `*.toml` candidate, are returned rather than skipped, so a scan never
/// passes by silently checking fewer files.
pub fn taxonomy_files_in(dir: &Path) -> GratResult<Vec<PathBuf>> {
    let io_err = |what: String| GratError::TaxonomyError(what);
    let mut paths = Vec::new();
    let entries = std::fs::read_dir(dir)
        .map_err(|e| io_err(format!("cannot read taxonomy dir {}: {e}", dir.display())))?;
    for entry in entries {
        let entry = entry
            .map_err(|e| io_err(format!("cannot read taxonomy dir {}: {e}", dir.display())))?;
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "toml") {
            continue;
        }
        let meta = std::fs::metadata(&path)
            .map_err(|e| io_err(format!("cannot read {}: {e}", path.display())))?;
        if meta.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Lint every `*.toml` taxonomy file in `dir` (sorted by name).
///
/// I/O failures (unreadable directory or file) are returned as `Err`;
/// problems in the files themselves are reported as diagnostics.
pub fn lint_dir(dir: &Path) -> GratResult<LintReport> {
    let paths = taxonomy_files_in(dir)?;
    let mut sources = Vec::with_capacity(paths.len());
    for path in paths {
        let content = std::fs::read_to_string(&path).map_err(|e| {
            GratError::TaxonomyError(format!("cannot read {}: {e}", path.display()))
        })?;
        sources.push((path.display().to_string(), content));
    }
    Ok(lint_category_sources(&sources))
}

// ── Enhanced taxonomy (`[error.<KEY>]` + `[metadata]`) ────────────────────

fn non_empty_str(value: Option<&toml::Value>) -> Option<&str> {
    value
        .and_then(toml::Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

fn is_error_code(code: &str) -> bool {
    code.strip_prefix("ERR_").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    })
}

/// Lint the contents of the enhanced taxonomy database (schema v2).
#[allow(clippy::too_many_lines)]
pub fn lint_enhanced_str(file: &str, content: &str) -> LintReport {
    let mut report = LintReport::default();
    let mut summary = FileSummary {
        file: file.to_string(),
        kind: FileKind::Enhanced,
        entries: 0,
    };
    let d = &mut report.diagnostics;

    let table: toml::Table = match content.parse() {
        Ok(t) => t,
        Err(e) => {
            let line = e.span().map(|s| line_col(content, s.start));
            let msg = match line {
                Some((_, col)) => format!(
                    "TOML syntax error (column {col}): {}",
                    one_line(e.message())
                ),
                None => format!("TOML syntax error: {}", one_line(e.message())),
            };
            d.push(Diagnostic::error(file, msg).line(line.map(|(l, _)| l)));
            report.files.push(summary);
            return report;
        }
    };
    let idx = LineIndex::new(content);

    for key in table.keys() {
        if key != "error" && key != "metadata" {
            d.push(
                Diagnostic::error(file, format!("unknown top-level key `{key}`"))
                    .field(key)
                    .line(idx.headers(&format!("[{key}]")).first().map(|i| i + 1)),
            );
        }
    }

    // [metadata]
    let mut required: Vec<String> = DEFAULT_ENHANCED_REQUIRED
        .iter()
        .map(ToString::to_string)
        .collect();
    match table.get("metadata").and_then(toml::Value::as_table) {
        None => d.push(Diagnostic::error(file, "missing `[metadata]` table").field("metadata")),
        Some(meta) => {
            if non_empty_str(meta.get("version")).is_none() {
                d.push(
                    Diagnostic::error(file, "metadata.version must be a non-empty string")
                        .field("metadata.version")
                        .line(idx.find_key_in_table("[metadata]", "version")),
                );
            }
            if let Some(list) = meta.get("fields_required") {
                match list.as_array().and_then(|a| {
                    a.iter()
                        .map(|v| v.as_str().map(ToString::to_string))
                        .collect::<Option<Vec<_>>>()
                }) {
                    Some(fields) => required = fields,
                    None => d.push(
                        Diagnostic::error(
                            file,
                            "metadata.fields_required must be an array of strings",
                        )
                        .field("metadata.fields_required")
                        .line(idx.find_key_in_table("[metadata]", "fields_required")),
                    ),
                }
            }
        }
    }

    let Some(errors) = table.get("error").and_then(toml::Value::as_table) else {
        d.push(Diagnostic::error(file, "missing `[error.<KEY>]` entries").field("error"));
        report.files.push(summary);
        return report;
    };
    summary.entries = errors.len();

    let mut codes: HashMap<String, String> = HashMap::new();
    let defined: HashSet<&str> = errors
        .values()
        .filter_map(|v| v.get("code").and_then(toml::Value::as_str))
        .collect();

    for (key, value) in errors {
        let header = format!("[error.{key}]");
        let line_of = |field: &str| idx.find_key_in_table(&header, field);
        let sub_line = |sub: &str, n: usize| {
            idx.headers(&format!("[[error.{key}.{sub}]]"))
                .get(n)
                .map(|i| i + 1)
                .or_else(|| {
                    idx.headers(&format!("[error.{key}.{sub}]"))
                        .first()
                        .map(|i| i + 1)
                })
                .or_else(|| line_of(sub))
        };
        let err = |field: &str, line: Option<usize>, msg: String| {
            Diagnostic::error(file, msg)
                .entry(key)
                .field(field)
                .line(line)
        };

        let Some(entry) = value.as_table() else {
            d.push(err("error", None, format!("error.{key} must be a table")));
            continue;
        };

        for field in &required {
            if !entry.contains_key(field) {
                d.push(err(
                    field,
                    idx.headers(&header).first().map(|i| i + 1),
                    format!("missing required field `{field}`"),
                ));
            }
        }
        for field in entry.keys() {
            if !ENHANCED_KNOWN_FIELDS.contains(&field.as_str()) {
                d.push(err(
                    field,
                    line_of(field),
                    format!("unknown field `{field}`"),
                ));
            }
        }

        for field in ["title", "description", "category"] {
            if let Some(v) = entry.get(field) {
                if non_empty_str(Some(v)).is_none() {
                    d.push(err(
                        field,
                        line_of(field),
                        format!("{field} must be a non-empty string"),
                    ));
                }
            }
        }

        if let Some(v) = entry.get("code") {
            match v.as_str() {
                Some(code) if is_error_code(code) => {
                    if let Some(prev) = codes.get(code) {
                        d.push(err(
                            "code",
                            line_of("code"),
                            format!("duplicate code {code:?}: already used by error.{prev}"),
                        ));
                    } else {
                        codes.insert(code.to_string(), key.clone());
                    }
                }
                _ => d.push(err(
                    "code",
                    line_of("code"),
                    format!("code {v} must be a string like \"ERR_SOME_NAME\""),
                )),
            }
        }

        let since = entry
            .get("since_protocol")
            .and_then(toml::Value::as_integer);
        if entry.contains_key("since_protocol") && since.is_none_or(|s| s <= 0) {
            d.push(err(
                "since_protocol",
                line_of("since_protocol"),
                "since_protocol must be a positive integer".into(),
            ));
        }

        if let Some(protocol) = entry.get("protocol") {
            match protocol.as_table() {
                None => d.push(err(
                    "protocol",
                    line_of("protocol"),
                    "protocol must be a table".into(),
                )),
                Some(p) => {
                    let line = sub_line("protocol", 0);
                    let version = |k: &str| p.get(k).map(|v| (v, v.as_integer()));
                    let mut since_version = None;
                    for k in ["since_version", "deprecated_in", "removed_in"] {
                        if let Some((v, int)) = version(k) {
                            if int.is_none_or(|n| n <= 0) {
                                d.push(err(
                                    &format!("protocol.{k}"),
                                    line,
                                    format!(
                                        "protocol.{k} must be a positive integer (got {v}); \
                                         omit it when not applicable"
                                    ),
                                ));
                            } else if k == "since_version" {
                                since_version = int;
                            }
                        }
                    }
                    if let (Some(sv), Some(sp)) = (since_version, since) {
                        if sv != sp {
                            d.push(err(
                                "protocol.since_version",
                                line,
                                format!(
                                    "protocol.since_version ({sv}) disagrees with \
                                     since_protocol ({sp})"
                                ),
                            ));
                        }
                    }
                    let base = since_version.or(since);
                    for k in ["deprecated_in", "removed_in"] {
                        if let (Some(b), Some(n)) =
                            (base, p.get(k).and_then(toml::Value::as_integer))
                        {
                            if n < b {
                                d.push(err(
                                    &format!("protocol.{k}"),
                                    line,
                                    format!("protocol.{k} ({n}) is before since_version ({b})"),
                                ));
                            }
                        }
                    }
                }
            }
        }

        if let Some(docs) = entry.get("documentation") {
            match docs.as_table() {
                None => d.push(err(
                    "documentation",
                    line_of("documentation"),
                    "documentation must be a table".into(),
                )),
                Some(docs) => {
                    let section = format!("[error.{key}.documentation]");
                    for (k, v) in docs {
                        let line = idx.find_key_in_table(&section, k);
                        match v.as_str() {
                            Some(u) => {
                                if let Err(reason) = check_http_url(u) {
                                    d.push(err(
                                        &format!("documentation.{k}"),
                                        line,
                                        format!("{k} {u:?} is not a valid URL: {reason}"),
                                    ));
                                }
                            }
                            None => d.push(err(
                                &format!("documentation.{k}"),
                                line,
                                format!("{k} must be a URL string"),
                            )),
                        }
                    }
                }
            }
        }

        if let Some(rel) = entry.get("related_errors") {
            match rel.as_array() {
                None => d.push(err(
                    "related_errors",
                    line_of("related_errors"),
                    "related_errors must be an array of tables".into(),
                )),
                Some(items) => {
                    for (n, item) in items.iter().enumerate() {
                        let line = sub_line("related_errors", n);
                        let field = format!("related_errors[{n}]");
                        let Some(item) = item.as_table() else {
                            d.push(err(&field, line, "must be a table".into()));
                            continue;
                        };
                        match item.get("code").and_then(toml::Value::as_str) {
                            Some(c) if is_error_code(c) => {
                                if !defined.contains(c) {
                                    d.push(
                                        Diagnostic::warning(
                                            file,
                                            format!(
                                                "related code {c:?} is not defined in this file"
                                            ),
                                        )
                                        .entry(key)
                                        .field(&format!("{field}.code"))
                                        .line(line),
                                    );
                                }
                            }
                            _ => d.push(err(
                                &format!("{field}.code"),
                                line,
                                "code must be a string like \"ERR_SOME_NAME\"".into(),
                            )),
                        }
                        let relationship = item.get("relationship").and_then(toml::Value::as_str);
                        if !relationship.is_some_and(|r| VALID_RELATIONSHIPS.contains(&r)) {
                            d.push(err(
                                &format!("{field}.relationship"),
                                line,
                                format!(
                                    "relationship must be one of {}",
                                    VALID_RELATIONSHIPS.join(", ")
                                ),
                            ));
                        }
                        if non_empty_str(item.get("description")).is_none() {
                            d.push(err(
                                &format!("{field}.description"),
                                line,
                                "description must be a non-empty string".into(),
                            ));
                        }
                    }
                }
            }
        }

        if let Some(examples) = entry.get("examples") {
            match examples.as_array() {
                None => d.push(err(
                    "examples",
                    line_of("examples"),
                    "examples must be an array of tables".into(),
                )),
                Some(items) => {
                    for (n, item) in items.iter().enumerate() {
                        let line = sub_line("examples", n);
                        for k in ["contract", "scenario", "code_snippet"] {
                            if non_empty_str(item.get(k)).is_none() {
                                d.push(err(
                                    &format!("examples[{n}].{k}"),
                                    line,
                                    format!("{k} must be a non-empty string"),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    report.files.push(summary);
    report
}

/// Lint the enhanced taxonomy database at `path`.
pub fn lint_enhanced_file(path: &Path) -> GratResult<LintReport> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| GratError::TaxonomyError(format!("cannot read {}: {e}", path.display())))?;
    Ok(lint_enhanced_str(&path.display().to_string(), &content))
}

/// Guess the format of a standalone taxonomy file from its contents: a
/// top-level `error` or `metadata` key (and no `category`/`errors`) means the
/// enhanced database. Documents that do not parse fall back to looking at
/// their table headers, so the right validator reports the syntax error.
pub fn detect_kind(content: &str) -> FileKind {
    let is_enhanced = match content.parse::<toml::Table>() {
        Ok(table) => {
            (table.contains_key("error") || table.contains_key("metadata"))
                && !table.contains_key("category")
                && !table.contains_key("errors")
        }
        Err(_) => content
            .lines()
            .filter_map(normalize_header)
            .any(|h| h == "[metadata]" || h.starts_with("[error.") || h.starts_with("[[error.")),
    };
    if is_enhanced {
        FileKind::Enhanced
    } else {
        FileKind::CategoryData
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::schema::{CategoryMeta, TaxonomySchema};

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

    fn to_toml(entries: Vec<TaxonomyEntry>) -> String {
        let schema = TaxonomySchema {
            category: CategoryMeta {
                name: "Test".to_string(),
                description: "Test data (must be long enough)".to_string(),
                source_module: "test".to_string(),
            },
            errors: entries,
        };
        toml::to_string(&schema).expect("serialize schema")
    }

    fn lint_one(entries: Vec<TaxonomyEntry>) -> LintReport {
        lint_category_sources(&[("test.toml".to_string(), to_toml(entries))])
    }

    fn lint_raw(content: &str) -> LintReport {
        lint_category_sources(&[("test.toml".to_string(), content.to_string())])
    }

    /// The single error diagnostic of a report (panics otherwise).
    fn only_error(report: &LintReport) -> &Diagnostic {
        let errors: Vec<_> = report.diagnostics.iter().filter(|d| d.is_error()).collect();
        assert_eq!(
            errors.len(),
            1,
            "expected exactly one error, got {errors:#?}"
        );
        errors[0]
    }

    const RAW_ENTRY: &str = r#"
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
    fn known_categories_match_enum_serialization() {
        for c in KNOWN_CATEGORIES {
            let v = toml::Value::try_from(c).expect("serialize category");
            assert_eq!(v.as_str(), Some(category_name(c)));
        }
    }

    #[test]
    fn valid_file_passes() {
        let mut a = valid_entry("test.entry.1", ErrorCategory::Budget, 1);
        a.related_errors = vec!["test.entry.2".into()];
        a.documentation_url = Some("https://example.com/docs".into());
        let b = valid_entry("test.entry.2", ErrorCategory::Storage, 2);
        let report = lint_one(vec![a, b]);
        assert!(report.is_ok(), "{:#?}", report.diagnostics);
        assert_eq!(report.diagnostics, Vec::<Diagnostic>::new());
        assert_eq!(report.entry_count(), 2);
    }

    #[test]
    fn raw_valid_file_passes() {
        assert_eq!(lint_raw(RAW_ENTRY).diagnostics, Vec::<Diagnostic>::new());
    }

    #[test]
    fn toml_syntax_error_is_reported_with_line() {
        let report = lint_raw("[category]\nname = \"Test\"\ndescription = \n");
        let d = only_error(&report);
        assert!(d.message.contains("TOML syntax error"), "{d}");
        assert_eq!(d.line, Some(3));
        assert_eq!(report.files[0].entries, 0);
    }

    #[test]
    fn invalid_category_fails() {
        let report = lint_raw(&RAW_ENTRY.replace("category = \"budget\"", "category = \"bogus\""));
        let d = only_error(&report);
        assert_eq!(d.field.as_deref(), Some("category"));
        assert_eq!(d.entry_id.as_deref(), Some("test.err"));
        assert!(d.message.contains("invalid category"), "{d}");
        assert_eq!(d.line, Some(9));
    }

    #[test]
    fn empty_summary_fails() {
        let mut e = valid_entry("test.empty.summary", ErrorCategory::Budget, 1);
        e.summary = "   ".into();
        let report = lint_one(vec![e]);
        let d = only_error(&report);
        assert_eq!(d.field.as_deref(), Some("summary"));
        assert!(d.message.contains("summary is empty"), "{d}");
    }

    #[test]
    fn empty_explanation_fails() {
        let mut e = valid_entry("test.empty.expl", ErrorCategory::Budget, 1);
        e.detailed_explanation = String::new();
        let report = lint_one(vec![e]);
        let d = only_error(&report);
        assert_eq!(d.field.as_deref(), Some("detailed_explanation"));
        assert!(d.message.contains("is empty"), "{d}");
    }

    #[test]
    fn short_summary_fails() {
        let mut e = valid_entry("test.short", ErrorCategory::Budget, 48);
        e.summary = "Error occurred".into(); // 14 characters
        let d = only_error(&lint_one(vec![e])).clone();
        assert!(d.message.contains("summary is too short"), "{d}");
    }

    #[test]
    fn bad_severity_fails() {
        let mut e = valid_entry("test.bad.severity", ErrorCategory::Budget, 44);
        e.severity = "Critical".into();
        let report = lint_one(vec![e]);
        let d = only_error(&report);
        assert_eq!(d.field.as_deref(), Some("severity"));
        assert!(d.message.contains("invalid severity"), "{d}");
    }

    #[test]
    fn severity_is_case_insensitive() {
        for sev in ["info", "Warning", "ERROR", "fatal"] {
            let mut e = valid_entry("test.sev", ErrorCategory::Budget, 1);
            e.severity = sev.into();
            assert!(lint_one(vec![e]).is_ok(), "{sev} should be accepted");
        }
    }

    #[test]
    fn malformed_documentation_url_fails() {
        for bad in ["not a url", "ftp://example.com/docs", "https://"] {
            let mut e = valid_entry("test.bad.url", ErrorCategory::Budget, 42);
            e.documentation_url = Some(bad.into());
            let report = lint_one(vec![e]);
            let d = only_error(&report);
            assert_eq!(d.field.as_deref(), Some("documentation_url"), "{bad}");
        }
    }

    #[test]
    fn duplicate_id_fails() {
        let report = lint_one(vec![
            valid_entry("test.same", ErrorCategory::Budget, 1),
            valid_entry("test.same", ErrorCategory::Budget, 2),
        ]);
        let d = only_error(&report);
        assert_eq!(d.field.as_deref(), Some("id"));
        assert!(d.message.contains("duplicate id"), "{d}");
    }

    #[test]
    fn duplicate_id_across_files_fails() {
        let report = lint_category_sources(&[
            (
                "a.toml".into(),
                to_toml(vec![valid_entry("x.dup", ErrorCategory::Auth, 1)]),
            ),
            (
                "b.toml".into(),
                to_toml(vec![valid_entry("x.dup", ErrorCategory::Wasm, 1)]),
            ),
        ]);
        let d = only_error(&report);
        assert_eq!(d.file, "b.toml");
        assert!(d.message.contains("a.toml"), "{d}");
    }

    #[test]
    fn overlapping_code_within_category_fails() {
        let report = lint_one(vec![
            valid_entry("test.dup.a", ErrorCategory::Budget, 1),
            valid_entry("test.dup.b", ErrorCategory::Budget, 1),
        ]);
        let d = only_error(&report);
        assert_eq!(d.field.as_deref(), Some("code"));
        assert_eq!(d.entry_id.as_deref(), Some("test.dup.b"));
        assert!(d.message.contains("overlaps in category budget"), "{d}");
    }

    #[test]
    fn same_code_in_different_categories_is_allowed() {
        let report = lint_one(vec![
            valid_entry("test.a", ErrorCategory::Budget, 7),
            valid_entry("test.b", ErrorCategory::Storage, 7),
        ]);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    }

    #[test]
    fn unknown_field_fails() {
        let report = lint_raw(&format!("{RAW_ENTRY}unknown_field = \"oops\"\n"));
        let d = only_error(&report);
        assert!(d.message.contains("schema error"), "{d}");
        assert!(d.message.contains("unknown_field"), "{d}");
    }

    #[test]
    fn missing_required_field_fails() {
        let report = lint_raw(&RAW_ENTRY.replace("name = \"TestErr\"\n", ""));
        let d = only_error(&report);
        assert!(d.message.contains("missing field `name`"), "{d}");
    }

    #[test]
    fn since_protocol_zero_fails() {
        let mut e = valid_entry("test.bad.sp", ErrorCategory::Budget, 45);
        e.since_protocol = Some(0);
        assert!(only_error(&lint_one(vec![e]))
            .message
            .contains("since_protocol must be > 0"));
    }

    #[test]
    fn deprecated_before_since_fails() {
        let mut e = valid_entry("test.bad.depr", ErrorCategory::Budget, 46);
        e.deprecated_protocol = Some(15);
        assert_eq!(
            only_error(&lint_one(vec![e])).field.as_deref(),
            Some("deprecated_protocol")
        );
    }

    #[test]
    fn unresolved_related_error_fails() {
        let mut e = valid_entry("test.rel", ErrorCategory::Budget, 47);
        e.related_errors = vec!["nonexistent.id".into()];
        assert!(only_error(&lint_one(vec![e]))
            .message
            .contains("unknown id \"nonexistent.id\""));
    }

    #[test]
    fn short_cause_description_points_at_its_table() {
        let content = format!(
            "{RAW_ENTRY}\n[[errors.common_causes]]\ndescription = \"too short\"\nlikelihood = \"high\"\n"
        );
        let report = lint_raw(&content);
        let d = only_error(&report);
        assert_eq!(d.field.as_deref(), Some("common_causes.description"));
        assert_eq!(d.line, Some(16));
    }

    #[test]
    fn multiple_failures_are_all_reported() {
        let mut a = valid_entry("test.multi", ErrorCategory::Budget, 1);
        a.summary = String::new();
        a.documentation_url = Some("nope".into());
        let b = valid_entry("test.multi", ErrorCategory::Budget, 1);
        let report = lint_one(vec![a, b]);
        assert_eq!(report.error_count(), 4, "{:#?}", report.diagnostics);
    }

    #[test]
    fn lint_dir_reads_toml_files_only() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            dir.path().join("a.toml"),
            to_toml(vec![valid_entry("a.one", ErrorCategory::Budget, 1)]),
        )
        .expect("write");
        std::fs::write(dir.path().join("notes.md"), "not toml [").expect("write");
        let report = lint_dir(dir.path()).expect("lint dir");
        assert_eq!(report.files.len(), 1);
        assert!(report.is_ok());
    }

    #[test]
    fn header_with_comment_or_spaces_keeps_lines_aligned() {
        let content = format!(
            "{}\n[[ errors ]] # second\nid = \"test.two\"\ncategory = \"budget\"\ncode = 2\n\
             name = \"TestTwo\"\nseverity = \"bogus\"\nsummary = \"Test summary is long enough.\"\n\
             detailed_explanation = \"Test detailed explanation is also long enough.\"\n",
            RAW_ENTRY.replace("[[errors]]", "[[errors]] # first")
        );
        let report = lint_raw(&content);
        let d = only_error(&report);
        assert_eq!(d.entry_id.as_deref(), Some("test.two"));
        assert_eq!(d.field.as_deref(), Some("severity"));
        assert_eq!(d.line, Some(21));
    }

    #[test]
    fn normalize_header_accepts_valid_spellings() {
        assert_eq!(
            normalize_header("[[errors]]").as_deref(),
            Some("[[errors]]")
        );
        assert_eq!(
            normalize_header("  [[ errors ]]  # note").as_deref(),
            Some("[[errors]]")
        );
        assert_eq!(
            normalize_header("[\"error\" . A]").as_deref(),
            Some("[error.A]")
        );
        assert_eq!(normalize_header("[1, 2],"), None);
        assert_eq!(normalize_header("id = 1"), None);
    }

    #[test]
    fn description_length_counts_characters_not_bytes() {
        // Five CJK characters are 15 bytes but only 5 characters.
        let report = lint_raw(&RAW_ENTRY.replace(
            "summary = \"Test summary is long enough.\"",
            "summary = \"错误错误错\"",
        ));
        let d = only_error(&report);
        assert_eq!(d.field.as_deref(), Some("summary"));
        assert!(d.message.contains("too short"), "{d}");
    }

    #[test]
    fn detect_kind_uses_parsed_keys() {
        let quoted = ENHANCED.replace("[error.", "[\"error\".");
        assert_eq!(detect_kind(&quoted), FileKind::Enhanced);
        let mentions_metadata = RAW_ENTRY.replace(
            "detailed_explanation = \"Test detailed explanation is also long enough.\"",
            "detailed_explanation = \"\"\"\nTest detailed explanation\n[metadata]\nis long enough.\"\"\"",
        );
        assert_eq!(detect_kind(&mentions_metadata), FileKind::CategoryData);
        assert!(lint_raw(&mentions_metadata).is_ok());
    }

    #[test]
    fn taxonomy_files_in_lists_sorted_toml_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        for name in ["b.toml", "a.toml", "notes.md"] {
            std::fs::write(dir.path().join(name), "").expect("write");
        }
        std::fs::create_dir(dir.path().join("sub.toml")).expect("mkdir");
        let files = taxonomy_files_in(dir.path()).expect("list");
        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.toml", "b.toml"]);
    }

    #[test]
    fn lint_dir_missing_dir_is_io_error() {
        assert!(lint_dir(Path::new("/definitely/not/here")).is_err());
    }

    // ── Enhanced taxonomy ──

    const ENHANCED: &str = r#"
[error.A]
title = "A Error"
description = "Something about A."
code = "ERR_A"
since_protocol = 19
category = "account"

[[error.A.related_errors]]
code = "ERR_B"
relationship = "similar"
description = "B is like A"

[error.A.documentation]
stellar_docs_url = "https://developers.stellar.org/docs/a"

[error.A.protocol]
since_version = 19

[[error.A.examples]]
contract = "token"
scenario = "Doing A"
code_snippet = "fn a() {}"

[error.B]
title = "B Error"
description = "Something about B."
code = "ERR_B"
since_protocol = 20

[metadata]
version = "2.0"
fields_required = ["title", "description", "code", "since_protocol"]
"#;

    fn enhanced_errors(content: &str) -> Vec<Diagnostic> {
        lint_enhanced_str("enhanced.toml", content)
            .diagnostics
            .into_iter()
            .filter(Diagnostic::is_error)
            .collect()
    }

    #[test]
    fn enhanced_valid_file_passes() {
        let report = lint_enhanced_str("enhanced.toml", ENHANCED);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        assert_eq!(report.entry_count(), 2);
        assert_eq!(detect_kind(ENHANCED), FileKind::Enhanced);
        assert_eq!(detect_kind(RAW_ENTRY), FileKind::CategoryData);
    }

    #[test]
    fn enhanced_null_is_a_syntax_error() {
        let content = ENHANCED.replace(
            "since_version = 19",
            "since_version = 19\ndeprecated_in = null",
        );
        let errors = enhanced_errors(&content);
        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert!(errors[0].message.contains("TOML syntax error"));
        assert_eq!(errors[0].line, Some(19));
    }

    #[test]
    fn enhanced_missing_required_and_bad_values_fail() {
        let content = ENHANCED
            .replace("title = \"B Error\"\n", "")
            .replace("code = \"ERR_B\"\nsince", "code = \"bad code\"\nsince")
            .replace("https://developers.stellar.org/docs/a", "not a url")
            .replace("relationship = \"similar\"", "relationship = \"cousin\"");
        let errors = enhanced_errors(&content);
        let fields: Vec<_> = errors.iter().filter_map(|d| d.field.clone()).collect();
        assert!(fields.contains(&"title".to_string()), "{errors:#?}");
        assert!(fields.contains(&"code".to_string()), "{errors:#?}");
        assert!(fields.contains(&"documentation.stellar_docs_url".to_string()));
        assert!(fields.contains(&"related_errors[0].relationship".to_string()));
    }

    #[test]
    fn enhanced_duplicate_code_fails() {
        let content = ENHANCED.replace("code = \"ERR_B\"\nsince", "code = \"ERR_A\"\nsince");
        let errors = enhanced_errors(&content);
        assert!(
            errors.iter().any(|d| d.message.contains("duplicate code")),
            "{errors:#?}"
        );
    }

    #[test]
    fn enhanced_unknown_related_code_is_a_warning() {
        let content = ENHANCED.replace(
            "code = \"ERR_B\"\nrelationship",
            "code = \"ERR_ELSEWHERE\"\nrelationship",
        );
        let report = lint_enhanced_str("enhanced.toml", &content);
        assert!(report.is_ok());
        assert_eq!(report.warning_count(), 1);
    }

    #[test]
    fn enhanced_protocol_mismatch_fails() {
        let content = ENHANCED.replace("since_version = 19", "since_version = 21");
        let errors = enhanced_errors(&content);
        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert_eq!(errors[0].field.as_deref(), Some("protocol.since_version"));
    }

    // ── Real repository data ──

    #[test]
    fn production_taxonomy_data_passes() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/taxonomy/data");
        let report = lint_dir(&dir).expect("lint production data");
        assert!(report.is_ok(), "{:#?}", report.diagnostics);
        assert!(report.entry_count() > 0);
    }

    #[test]
    fn enhanced_taxonomy_database_passes() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../taxonomy/enhanced_error_taxonomy.toml");
        let report = lint_enhanced_file(&path).expect("lint enhanced taxonomy");
        assert!(report.is_ok(), "{:#?}", report.diagnostics);
    }
}
