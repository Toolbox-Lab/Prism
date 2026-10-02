//! Markdown rendering for the terminal.
//!
//! [`MarkdownRenderer`] parses standard Markdown with `pulldown-cmark` and
//! translates the resulting events into ANSI-escaped terminal output. It
//! supports headings, emphasis, code blocks, lists, blockquotes, links,
//! tables, footnotes and horizontal rules, wraps prose to the current
//! terminal width, and degrades to plain text when colors are disabled.

use owo_colors::Style;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use unicode_width::UnicodeWidthStr;

use crate::output::theme;

/// Width used when the terminal size cannot be detected.
const FALLBACK_WIDTH: usize = 80;

/// Renders Markdown into ANSI-escaped terminal output.
#[derive(Debug, Clone, Copy)]
pub struct MarkdownRenderer {
    width: usize,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownRenderer {
    /// Creates a renderer sized to the user's current terminal window.
    pub fn new() -> Self {
        Self {
            width: detect_terminal_width(),
        }
    }

    /// Creates a renderer with an explicit output width. This is useful for
    /// tests and for contexts where no terminal is available.
    #[allow(dead_code)]
    pub fn with_width(width: usize) -> Self {
        Self {
            width: width.max(20),
        }
    }

    /// Renders a Markdown document to ANSI-styled terminal text.
    pub fn render(self, markdown: &str) -> String {
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS;
        let parser = Parser::new_ext(markdown, options);
        let mut engine = RenderEngine::new(self.width);
        for event in parser {
            engine.handle(event);
        }
        engine.finish()
    }
}

/// A single atomic unit of inline content: either a word, a whitespace run,
/// or a styled span such as inline code. `text` is plain (ANSI-free) and
/// `style` is applied at emission time so wrapping can re-open the style.
#[derive(Debug, Clone)]
struct Piece {
    text: String,
    style: Option<Style>,
}

impl Piece {
    fn new(text: String, style: Option<Style>) -> Self {
        Self { text, style }
    }
}

/// A prefix component contributed by an enclosing block. `line` is written at
/// the start of every line (e.g. a list bullet or a blockquote bar) while
/// `continuation` is written at the start of wrapped continuation lines (for
/// list items this is spaces, for blockquotes the bar is repeated).
#[derive(Debug)]
struct PrefixComponent {
    line: String,
    continuation: String,
}

#[derive(Debug)]
struct ListContext {
    ordered: bool,
    next_number: u64,
}

#[derive(Debug)]
enum Pending {
    None,
    Paragraph {
        pieces: Vec<Piece>,
        footnote: Option<String>,
    },
    Heading {
        pieces: Vec<Piece>,
    },
    Code {
        text: String,
    },
    Html {
        lines: Vec<String>,
    },
    Table(TableState),
}

#[derive(Debug, Default)]
struct TableState {
    rows: Vec<Vec<Vec<Piece>>>,
    current_row: Vec<Vec<Piece>>,
    current_cell: Vec<Piece>,
    in_cell: bool,
    has_header: bool,
}

struct RenderEngine {
    width: usize,
    out: String,
    at_line_start: bool,
    prefix_stack: Vec<PrefixComponent>,
    list_stack: Vec<ListContext>,
    link_urls: Vec<String>,
    inline_stack: Vec<Option<Style>>,
    pending: Pending,
    item_depth: usize,
}

impl RenderEngine {
    fn new(width: usize) -> Self {
        Self {
            width,
            out: String::new(),
            at_line_start: true,
            prefix_stack: Vec::new(),
            list_stack: Vec::new(),
            link_urls: Vec::new(),
            inline_stack: vec![None],
            pending: Pending::None,
            item_depth: 0,
        }
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.handle_start(tag),
            Event::End(tag_end) => self.handle_end(tag_end),
            Event::Text(text) => self.handle_text(&text),
            Event::Code(text) => {
                self.push_inline_piece(Piece::new(
                    text.into_string(),
                    styled(Style::new().reversed()),
                ));
            }
            Event::SoftBreak | Event::InlineMath(_) | Event::DisplayMath(_) => {}
            Event::HardBreak => {
                self.push_inline_piece(Piece::new("\n".to_string(), None));
            }
            Event::Rule => self.handle_rule(),
            Event::TaskListMarker(checked) => {
                if let Some(component) = self.prefix_stack.last_mut() {
                    let marker = if checked { "[x] " } else { "[ ] " };
                    let width = UnicodeWidthStr::width(marker);
                    component.line.push_str(marker);
                    component.continuation.push_str(&" ".repeat(width));
                }
            }
            Event::FootnoteReference(name) => {
                self.push_inline_piece(Piece::new(
                    format!("[{name}]"),
                    styled(Style::new().cyan()),
                ));
            }
            Event::Html(text) => match &mut self.pending {
                Pending::Html { lines } => lines.push(text.into_string()),
                _ => self.push_inline_piece(Piece::new(
                    text.into_string(),
                    styled(Style::new().dimmed()),
                )),
            },
            Event::InlineHtml(text) => {
                self.push_inline_piece(Piece::new(
                    text.into_string(),
                    styled(Style::new().dimmed()),
                ));
            }
        }
    }

    fn handle_start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                // Some containers (tight list items, footnote definitions) open
                // a paragraph implicitly; only create one if none is pending.
                if matches!(self.pending, Pending::None) {
                    self.before_block();
                    self.pending = Pending::Paragraph {
                        pieces: Vec::new(),
                        footnote: None,
                    };
                }
            }
            Tag::Heading { .. } => {
                self.before_block();
                self.pending = Pending::Heading { pieces: Vec::new() };
            }
            Tag::CodeBlock(_) => {
                self.before_block();
                self.pending = Pending::Code {
                    text: String::new(),
                };
            }
            Tag::HtmlBlock => {
                self.before_block();
                self.pending = Pending::Html { lines: Vec::new() };
            }
            Tag::List(start) => {
                // A nested list begins inside an open item whose implicit
                // paragraph may still be collecting text; flush it first so
                // the list items render on their own lines.
                self.flush_paragraph();
                if self.list_stack.is_empty() {
                    self.before_block();
                }
                self.list_stack.push(ListContext {
                    ordered: start.is_some(),
                    next_number: start.unwrap_or(1),
                });
            }
            Tag::Item => self.start_item(),
            Tag::BlockQuote(_) => self.start_block_quote(),
            Tag::Table(_) => {
                self.before_block();
                self.pending = Pending::Table(TableState::default());
            }
            Tag::TableHead => {
                if let Pending::Table(table) = &mut self.pending {
                    table.has_header = true;
                }
            }
            Tag::TableRow => {
                if let Pending::Table(table) = &mut self.pending {
                    table.current_row = Vec::new();
                }
            }
            Tag::TableCell => {
                if let Pending::Table(table) = &mut self.pending {
                    table.current_cell = Vec::new();
                    table.in_cell = true;
                }
            }
            Tag::FootnoteDefinition(name) => {
                self.before_block();
                self.pending = Pending::Paragraph {
                    pieces: Vec::new(),
                    footnote: Some(name.into_string()),
                };
            }
            Tag::Emphasis => self.push_inline_style(Style::italic),
            Tag::Strong => self.push_inline_style(Style::bold),
            Tag::Strikethrough => self.push_inline_style(Style::strikethrough),
            Tag::Link { dest_url, .. } => {
                self.link_urls.push(dest_url.into_string());
                self.push_inline_style(|style| style.cyan().underline());
            }
            Tag::Image { .. } => {
                self.push_inline_style(Style::dimmed);
            }
            Tag::Subscript
            | Tag::Superscript
            | Tag::MetadataBlock(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition => {}
        }
    }

    fn handle_end(&mut self, tag_end: TagEnd) {
        match tag_end {
            TagEnd::Paragraph => self.flush_paragraph(),
            TagEnd::Heading(level) => {
                if let Pending::Heading { pieces, .. } =
                    std::mem::replace(&mut self.pending, Pending::None)
                {
                    let base = heading_style(level);
                    let heading_pieces: Vec<Piece> = pieces
                        .into_iter()
                        .map(|piece| Piece::new(piece.text, base))
                        .collect();
                    self.emit_wrapped(&heading_pieces);
                    self.newline();
                }
            }
            TagEnd::CodeBlock => {
                if let Pending::Code { text } = std::mem::replace(&mut self.pending, Pending::None)
                {
                    self.emit_code(&text);
                }
            }
            TagEnd::HtmlBlock => {
                if let Pending::Html { lines } = std::mem::replace(&mut self.pending, Pending::None)
                {
                    for line in &lines {
                        self.write_raw_line(line, None);
                    }
                }
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
            }
            TagEnd::Item => {
                self.flush_paragraph();
                self.prefix_stack.pop();
                self.item_depth = self.item_depth.saturating_sub(1);
            }
            TagEnd::BlockQuote(_) => {
                self.prefix_stack.pop();
            }
            TagEnd::Table => {
                if let Pending::Table(table) = std::mem::replace(&mut self.pending, Pending::None) {
                    self.emit_table(table);
                }
            }
            TagEnd::TableHead => {
                // The header row is not wrapped in a `TableRow`, so finalize it
                // here rather than waiting for a row-end event.
                if let Pending::Table(table) = &mut self.pending {
                    table.in_cell = false;
                    if !table.current_row.is_empty() {
                        table.rows.push(std::mem::take(&mut table.current_row));
                    }
                }
            }
            TagEnd::TableRow => {
                if let Pending::Table(table) = &mut self.pending {
                    table.in_cell = false;
                    if !table.current_row.is_empty() {
                        table.rows.push(std::mem::take(&mut table.current_row));
                    }
                }
            }
            TagEnd::TableCell => {
                if let Pending::Table(table) = &mut self.pending {
                    table.in_cell = false;
                    table
                        .current_row
                        .push(std::mem::take(&mut table.current_cell));
                }
            }
            TagEnd::Link => {
                self.inline_stack.pop();
                if let Some(url) = self.link_urls.pop() {
                    if !url.is_empty() {
                        self.push_inline_piece(Piece::new(
                            format!(" ({url})"),
                            styled(Style::new().dimmed()),
                        ));
                    }
                }
            }
            TagEnd::Image | TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.pop_inline_style();
            }
            TagEnd::FootnoteDefinition
            | TagEnd::Subscript
            | TagEnd::Superscript
            | TagEnd::MetadataBlock(_)
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition => {}
        }
    }

    fn handle_text(&mut self, text: &str) {
        let style = self.current_inline_style();
        match &mut self.pending {
            Pending::Paragraph { pieces, .. } | Pending::Heading { pieces, .. } => {
                push_words(pieces, text, style);
            }
            Pending::Table(table) => {
                if table.in_cell {
                    push_words(&mut table.current_cell, text, style);
                }
            }
            Pending::Code { text: code } => code.push_str(text),
            Pending::Html { lines } => lines.push(text.to_string()),
            Pending::None => {}
        }
    }

    /// Begins a new list item: opens an implicit paragraph (tight list items
    /// are not wrapped in a `Paragraph` event) and pushes a bullet prefix.
    fn start_item(&mut self) {
        self.ensure_newline();
        if matches!(self.pending, Pending::None) {
            self.pending = Pending::Paragraph {
                pieces: Vec::new(),
                footnote: None,
            };
        }
        let context = self
            .list_stack
            .last_mut()
            .expect("list item outside of a list");
        let bullet = if context.ordered {
            let number = context.next_number;
            context.next_number += 1;
            format!("{number}. ")
        } else {
            bullet_for_depth(self.list_stack.len()).to_string()
        };
        let indent = "  ".repeat(self.list_stack.len().saturating_sub(1));
        let line = format!("{indent}{bullet}");
        let width = UnicodeWidthStr::width(line.as_str());
        self.prefix_stack.push(PrefixComponent {
            line,
            continuation: " ".repeat(width),
        });
        self.item_depth += 1;
    }

    /// Begins a blockquote by adding a vertical-bar prefix component.
    fn start_block_quote(&mut self) {
        self.before_block();
        self.prefix_stack.push(PrefixComponent {
            line: "│ ".to_string(),
            continuation: "│ ".to_string(),
        });
    }

    /// Emits a pending paragraph (implicit or explicit) if it has content and
    /// resets the pending state.
    fn flush_paragraph(&mut self) {
        let Pending::Paragraph { pieces, footnote } =
            std::mem::replace(&mut self.pending, Pending::None)
        else {
            return;
        };
        if pieces.is_empty() {
            return;
        }
        let mut all_pieces = Vec::with_capacity(pieces.len() + 1);
        if let Some(name) = footnote {
            all_pieces.push(Piece::new(
                format!("[{name}] "),
                styled(Style::new().cyan()),
            ));
        }
        all_pieces.extend(pieces);
        self.emit_wrapped(&all_pieces);
        self.newline();
    }

    fn push_inline_piece(&mut self, piece: Piece) {
        match &mut self.pending {
            Pending::Paragraph { pieces, .. } | Pending::Heading { pieces, .. } => {
                pieces.push(piece);
            }
            Pending::Table(table) if table.in_cell => table.current_cell.push(piece),
            _ => {}
        }
    }

    fn push_inline_style(&mut self, apply: impl FnOnce(Style) -> Style) {
        let base = self.current_inline_style();
        let next = base.map_or_else(Style::new, apply);
        self.inline_stack.push(styled(next));
    }

    fn pop_inline_style(&mut self) {
        if self.inline_stack.len() > 1 {
            self.inline_stack.pop();
        }
    }

    fn current_inline_style(&self) -> Option<Style> {
        self.inline_stack.last().copied().flatten()
    }

    fn handle_rule(&mut self) {
        self.before_block();
        let available = self.width.saturating_sub(self.current_prefix_width());
        let rule = "─".repeat(available.max(3));
        self.write_raw_line(&rule, styled(Style::new().dimmed()));
    }

    fn emit_code(&mut self, text: &str) {
        let style = styled(Style::new().dimmed());
        let mut lines: Vec<&str> = text.split('\n').collect();
        // The fenced block's content ends with a newline before the closing
        // fence; drop that trailing empty line.
        while lines.last() == Some(&"") {
            lines.pop();
        }
        for line in lines {
            let indented = if line.is_empty() {
                String::new()
            } else {
                format!("  {line}")
            };
            self.write_raw_line(&indented, style);
        }
    }

    fn emit_table(&mut self, table: TableState) {
        for (row_index, row) in table.rows.iter().enumerate() {
            let is_header = table.has_header && row_index == 0;
            let cells: Vec<String> = row.iter().map(|cell| inline_to_string(cell)).collect();
            let line = cells.join(" | ");
            let style = if is_header {
                styled(Style::new().bold())
            } else {
                None
            };
            self.write_raw_line(&line, style);
        }
    }

    fn emit_wrapped(&mut self, pieces: &[Piece]) {
        let continuation = self.current_continuation();
        let continuation_width = UnicodeWidthStr::width(continuation.as_str());
        self.emit_line_start();
        let mut column = self.current_prefix_width();
        let mut line_has_content = false;
        let mut pending_space = false;
        let mut active: Option<Style> = None;

        for piece in pieces {
            let text = piece.text.as_str();
            if text.is_empty() {
                continue;
            }
            if text == "\n" {
                // Hard break: end the line and restart after the continuation.
                self.finish_style(active);
                active = None;
                self.newline();
                self.out.push_str(&continuation);
                column = continuation_width;
                line_has_content = false;
                pending_space = false;
                continue;
            }
            if text.chars().all(char::is_whitespace) {
                // Defer the space so it is not emitted at the end of a line
                // when the next word wraps.
                pending_space = true;
                continue;
            }
            let piece_width = UnicodeWidthStr::width(text);
            if pending_space {
                if line_has_content && column + 1 + piece_width <= self.width {
                    // The space keeps the previous word's active style so
                    // styled runs (bold, italic, headings) stay continuous.
                    self.out.push(' ');
                    column += 1;
                } else if line_has_content {
                    self.finish_style(active);
                    active = None;
                    self.newline();
                    self.out.push_str(&continuation);
                    column = continuation_width;
                }
                pending_space = false;
            }
            self.apply_style(&mut active, piece.style);
            self.out.push_str(text);
            column += piece_width;
            line_has_content = true;
        }
        self.finish_style(active);
    }

    /// Switches the active ANSI style, emitting suffix/prefix codes as needed.
    fn apply_style(&mut self, active: &mut Option<Style>, style: Option<Style>) {
        if *active == style {
            return;
        }
        if let Some(style) = *active {
            self.out.push_str(&format!("{}", style.suffix_formatter()));
        }
        *active = style;
        if let Some(style) = *active {
            self.out.push_str(&format!("{}", style.prefix_formatter()));
        }
    }

    fn finish_style(&mut self, style: Option<Style>) {
        if let Some(style) = style {
            self.out.push_str(&format!("{}", style.suffix_formatter()));
        }
    }

    fn write_raw_line(&mut self, text: &str, style: Option<Style>) {
        self.emit_line_start();
        match style {
            Some(style) => {
                let styled = style.style(text);
                self.out.push_str(&format!("{styled}"));
            }
            None => self.out.push_str(text),
        }
        self.newline();
    }

    fn emit_line_start(&mut self) {
        if self.at_line_start {
            self.out.push_str(&self.current_prefix());
            self.at_line_start = false;
        }
    }

    fn newline(&mut self) {
        self.out.push('\n');
        self.at_line_start = true;
    }

    fn ensure_newline(&mut self) {
        if !self.out.ends_with('\n') {
            self.newline();
        }
    }

    fn before_block(&mut self) {
        if self.item_depth > 0 {
            self.ensure_newline();
            return;
        }
        if self.out.is_empty() {
            return;
        }
        if !self.out.ends_with('\n') {
            self.newline();
        }
        if !self.out.ends_with("\n\n") {
            self.newline();
        }
    }

    fn current_prefix(&self) -> String {
        self.prefix_stack
            .iter()
            .map(|component| component.line.as_str())
            .collect()
    }

    fn current_continuation(&self) -> String {
        self.prefix_stack
            .iter()
            .map(|component| component.continuation.as_str())
            .collect()
    }

    fn current_prefix_width(&self) -> usize {
        UnicodeWidthStr::width(self.current_prefix().as_str())
    }

    fn finish(mut self) -> String {
        if self.out.is_empty() {
            return String::new();
        }
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out.push('\n');
        self.out
    }
}

/// Splits a text run into word and whitespace pieces, each tagged with the
/// currently active inline style.
fn push_words(pieces: &mut Vec<Piece>, text: &str, style: Option<Style>) {
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if !ch.is_whitespace() {
            continue;
        }
        if index > start {
            pieces.push(Piece::new(text[start..index].to_string(), style));
        }
        let mut end = index + ch.len_utf8();
        while let Some(next) = text[end..].chars().next() {
            if !next.is_whitespace() {
                break;
            }
            end += next.len_utf8();
        }
        pieces.push(Piece::new(text[index..end].to_string(), style));
        start = end;
    }
    if start < text.len() {
        pieces.push(Piece::new(text[start..].to_string(), style));
    }
}

/// Joins inline pieces into a single ANSI-styled string (used for table cells,
/// which are not wrapped).
fn inline_to_string(pieces: &[Piece]) -> String {
    let mut out = String::new();
    let mut active: Option<Style> = None;
    for piece in pieces {
        if active != piece.style {
            if let Some(style) = active {
                out.push_str(&format!("{}", style.suffix_formatter()));
            }
            active = piece.style;
            if let Some(style) = active {
                out.push_str(&format!("{}", style.prefix_formatter()));
            }
        }
        out.push_str(&piece.text);
    }
    if let Some(style) = active {
        out.push_str(&format!("{}", style.suffix_formatter()));
    }
    out
}

fn heading_style(level: HeadingLevel) -> Option<Style> {
    let style = match level {
        HeadingLevel::H1 => Style::new().white().bold().underline(),
        HeadingLevel::H2 => Style::new().cyan().bold(),
        HeadingLevel::H3 => Style::new().cyan(),
        HeadingLevel::H4 | HeadingLevel::H5 | HeadingLevel::H6 => Style::new().bold(),
    };
    styled(style)
}

fn bullet_for_depth(depth: usize) -> &'static str {
    match (depth - 1) % 3 {
        0 => "• ",
        1 => "◦ ",
        _ => "▪ ",
    }
}

/// Returns the style when colors are enabled, or `None` when the CLI is
/// running with colors disabled (`--no-color`).
fn styled(style: Style) -> Option<Style> {
    if theme::colors_enabled() {
        Some(style)
    } else {
        None
    }
}

fn detect_terminal_width() -> usize {
    match crossterm::terminal::size() {
        Ok((columns, _)) => usize::from(columns).max(20),
        Err(_) => FALLBACK_WIDTH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                for next in chars.by_ref() {
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn render(markdown: &str) -> String {
        MarkdownRenderer::with_width(80).render(markdown)
    }

    fn render_plain(markdown: &str) -> String {
        strip_ansi(&render(markdown))
    }

    #[test]
    fn renders_headings_and_emphasis() {
        let out = render_plain("# Title\n\nSome **bold** and *italic* and ~~struck~~ text.");
        assert!(out.contains("Title"), "heading text missing: {out:?}");
        assert!(
            out.contains("Some bold and italic and struck text."),
            "paragraph text missing: {out:?}"
        );
        let colored = render("# Title");
        assert!(
            colored.contains('\u{1b}'),
            "expected ANSI escapes when colors are on"
        );
    }

    #[test]
    fn renders_unordered_list_with_bullets() {
        let out = render_plain("- one\n- two\n- three");
        assert!(out.contains("• one"), "bullet missing: {out:?}");
        assert!(out.contains("• two"));
        assert!(out.contains("• three"));
    }

    #[test]
    fn renders_ordered_list_with_numbers() {
        let out = render_plain("1. first\n2. second");
        assert!(out.contains("1. first"), "numbering missing: {out:?}");
        assert!(out.contains("2. second"));
    }

    #[test]
    fn renders_nested_list_with_indentation() {
        let out = render_plain("- outer\n  - inner");
        assert!(out.contains("• outer"));
        assert!(out.contains("◦ inner"), "nested bullet missing: {out:?}");
    }

    #[test]
    fn renders_task_list_checkboxes() {
        let out = render_plain("- [x] done\n- [ ] pending");
        assert!(
            out.contains("• [x] done"),
            "checked marker missing: {out:?}"
        );
        assert!(out.contains("• [ ] pending"));
    }

    #[test]
    fn renders_code_blocks_verbatim() {
        let out = render_plain("```rust\nfn main() {\n    let x = 1;\n}\n```");
        assert!(out.contains("fn main() {"), "code missing: {out:?}");
        assert!(out.contains("let x = 1;"));
        assert!(out.contains('}'));
    }

    #[test]
    fn renders_blockquotes_with_bar() {
        let out = render_plain("> quoted wisdom");
        assert!(
            out.contains("│ quoted wisdom"),
            "quote bar missing: {out:?}"
        );
    }

    #[test]
    fn renders_links_with_url() {
        let out = render_plain("See [docs](https://example.com) for details.");
        assert!(
            out.contains("See docs (https://example.com) for details."),
            "link missing: {out:?}"
        );
    }

    #[test]
    fn wraps_long_paragraph_to_width() {
        let width = 30;
        let out = strip_ansi(
            &MarkdownRenderer::with_width(width)
                .render("one two three four five six seven eight nine ten eleven twelve thirteen"),
        );
        for line in out.lines() {
            assert!(
                line.chars().count() <= width,
                "line {line:?} exceeds width {width}"
            );
        }
    }

    #[test]
    fn wraps_list_items_with_hanging_indent() {
        let width = 20;
        let out = strip_ansi(
            &MarkdownRenderer::with_width(width)
                .render("- a long list item that really should wrap onto several lines of text"),
        );
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() > 1, "expected wrapping: {out:?}");
        for line in &lines {
            assert!(line.chars().count() <= width, "line {line:?} exceeds width");
        }
    }

    #[test]
    fn renders_tables() {
        let out = render_plain("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(out.contains("A | B"), "header missing: {out:?}");
        assert!(out.contains("1 | 2"));
    }

    #[test]
    fn renders_footnotes() {
        let out = render_plain("Reference[^1].\n\n[^1]: The footnote body.");
        assert!(
            out.contains("[1] The footnote body."),
            "footnote missing: {out:?}"
        );
    }

    #[test]
    fn renders_horizontal_rule() {
        let out = render_plain("---");
        assert!(out.contains('─'), "rule missing: {out:?}");
    }

    #[test]
    fn empty_input_renders_empty_output() {
        assert_eq!(render(""), "");
        assert_eq!(render("   \n\n  "), "");
    }

    #[test]
    fn colors_can_be_disabled() {
        theme::set_color_enabled(false);
        let out = render("# Heading\n\nSome **bold** text.");
        theme::set_color_enabled(true);
        assert!(!out.contains('\u{1b}'), "expected no ANSI codes: {out:?}");
        assert!(out.contains("Heading"));
        assert!(out.contains("Some bold text."));
    }

    #[test]
    fn code_spans_are_rendered_inline() {
        let out = render_plain("Run `grat decode` now.");
        assert!(
            out.contains("Run grat decode now."),
            "code span missing: {out:?}"
        );
    }

    #[test]
    fn renders_loose_list_with_paragraph_items() {
        let out = render_plain("- first paragraph\n\n- second paragraph");
        assert!(out.contains("• first paragraph"));
        assert!(out.contains("• second paragraph"));
    }

    #[test]
    fn paragraphs_and_headings_are_separated_by_blank_lines() {
        let out = render_plain("# Title\n\nFirst paragraph.\n\nSecond paragraph.");
        assert!(
            out.contains("Title\n\nFirst paragraph."),
            "missing blank: {out:?}"
        );
        assert!(
            out.contains("First paragraph.\n\nSecond paragraph."),
            "missing blank between paragraphs: {out:?}"
        );
    }

    #[test]
    fn wrapped_lines_do_not_end_with_trailing_spaces() {
        let out = strip_ansi(
            &MarkdownRenderer::with_width(20)
                .render("alpha beta gamma delta epsilon zeta eta theta iota kappa lambda"),
        );
        for line in out.lines() {
            assert!(
                !line.ends_with(' '),
                "line has trailing space: {line:?} (full: {out:?})"
            );
        }
    }

    #[test]
    fn heading_levels_produce_style_codes() {
        let out = render("# One\n\n## Two\n\n### Three");
        assert!(out.contains("\u{1b}["), "headings not styled: {out:?}");
        assert!(out.contains("One"));
        assert!(out.contains("Two"));
        assert!(out.contains("Three"));
    }
}
