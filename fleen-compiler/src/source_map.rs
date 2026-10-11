//! Source file registry: file identity for spans and diagnostics (0.0.3 V01).
//!
//! Before 0.0.3 every `Span` was a byte range into one source string. With
//! the module system (PLAN §3.4.5) diagnostics must name their file, so
//! spans carry a [`FileId`] and the compilation session holds a
//! [`SourceMap`] mapping ids back to file name + text. File identity is
//! compile-time only: the `.flnc` layout is unchanged and no file table is
//! serialized (PLAN §5.5 / D10).

use std::fmt;

/// Unique index into a [`SourceMap`].
///
/// IDs start at 0 and are allocated contiguously by
/// [`SourceMap::add_file`](SourceMap::add_file) (SPEC §5 ID rules). The
/// single-file convention is `FileId(0)` — `compile(&str)` registers exactly
/// one file, so every pre-0.0.3 span behavior is preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u32);

impl FileId {
    /// File id for spans that have no source file at all (e.g.
    /// [`Span::SYNTHETIC`](crate::lexer::Span::SYNTHETIC), builtin
    /// declarations). Never returned by `add_file`; consumers must check
    /// `Span::is_synthetic` before resolving through a source map.
    pub const SYNTHETIC: FileId = FileId(u32::MAX);
}

/// One registered source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Path as given on the command line / module path, used verbatim in
    /// `-->` diagnostic rendering.
    pub name: String,
    /// Full source text.
    pub source: String,
}

/// Registry of all source files of one compilation; index = [`FileId`].
#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Create an empty source map.
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Register a file and return its id (0, 1, 2, … in call order).
    pub fn add_file(&mut self, name: impl Into<String>, source: impl Into<String>) -> FileId {
        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile {
            name: name.into(),
            source: source.into(),
        });
        id
    }

    /// The file registered under `id`.
    ///
    /// # Panics
    /// If `id` was never registered (an unregistered id is an internal
    /// invariant violation — spans are only created by lexing a registered
    /// file).
    pub fn get(&self, id: FileId) -> &SourceFile {
        self.files
            .get(id.0 as usize)
            .unwrap_or_else(|| panic!("FileId({}) not registered in the source map", id.0))
    }

    /// Number of registered files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True if no file is registered.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Convert a byte offset in file `id` to a 1-based `(line, column)` pair.
    ///
    /// Columns count `char`s, not bytes; `\r` occupies no column. The
    /// algorithm matches `fleen-vm`'s `diag::locate` (the runtime `-->`
    /// renderer) so compile-time and runtime diagnostics agree; the
    /// duplication is deliberate — the VM's single-file path must keep
    /// working without a source map.
    ///
    /// # Panics
    /// If `id` was never registered.
    pub fn line_col(&self, id: FileId, offset: u32) -> (u32, u32) {
        let (line, col, _) = self.locate(id, offset);
        (line, col)
    }

    /// Locate `offset` in file `id` as `(line, column, line text)`.
    ///
    /// Out-of-range or mid-char offsets clamp to the last char boundary of
    /// that file's text (same contract as `fleen-vm`'s `diag::locate`).
    ///
    /// # Panics
    /// If `id` was never registered.
    pub fn locate(&self, id: FileId, offset: u32) -> (u32, u32, &str) {
        let source = &self.get(id).source;
        // Floor a mid-char (never produced by the lexer) or out-of-range
        // offset to a char boundary so the slicing below cannot panic.
        let mut offset = (offset as usize).min(source.len());
        while offset > 0 && !source.is_char_boundary(offset) {
            offset -= 1;
        }
        let line_start = source[..offset].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source[line_start..]
            .find('\n')
            .map_or(source.len(), |i| line_start + i);
        let text = &source[line_start..line_end];
        let line = source[..line_start].bytes().filter(|&b| b == b'\n').count() as u32 + 1;
        let col = source[line_start..offset]
            .chars()
            .filter(|&c| c != '\r')
            .count() as u32
            + 1;
        (line, col, text)
    }

    /// Render the rustc-style `-->`/source-line/caret block for `span`.
    ///
    /// The file name comes from the span's [`FileId`], e.g.:
    ///
    /// ```text
    ///   --> src/net/tcp.fln:12:5
    ///    |
    /// 12 |     auth.secret();
    ///    |     ^^^^^^^^^^^
    /// ```
    ///
    /// Synthetic spans render a file-less `--> <synthetic>` placeholder
    /// (diagnostics must never panic on the spans they render); callers that
    /// can degrade more gracefully — e.g. to function name + instruction
    /// offset, as the runtime path does — should skip calling this instead.
    /// Out-of-range or mid-char span ends clamp exactly like `locate`, so
    /// rendering never panics on offsets. The format matches `fleen-vm`'s
    /// `diag::render_location` so the two diagnostic paths are visually
    /// identical.
    ///
    /// # Panics
    /// If `span.file` was never registered (synthetic spans never reach the
    /// source map).
    pub fn render_location(&self, span: crate::lexer::Span) -> String {
        if span.is_synthetic() {
            return String::from("  --> <synthetic>");
        }
        let (line, col, text) = self.locate(span.file, span.start);
        let file = &self.get(span.file).name;
        // A span may cross a newline (multi-line expression); only the first
        // line is shown, so the caret run stops at its end. `\r` never renders.
        let text = text.trim_end_matches('\r');
        let num = line.to_string();
        let bar = " ".repeat(num.len() + 2);
        let source = &self.get(span.file).source;
        // Caret run: clamp/floor the span ends exactly like `locate` floors
        // its offset, so an out-of-range or mid-char span renders (worst case
        // one caret) instead of panicking — diagnostics render best-effort.
        let mut start = (span.start as usize).min(source.len());
        while start > 0 && !source.is_char_boundary(start) {
            start -= 1;
        }
        let end = (span.end as usize).min(source.len()).max(start);
        let carets = source[start..end]
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .filter(|&c| c != '\r')
            .count()
            .max(1);
        format!(
            "  --> {file}:{line}:{col}\n{bar}|\n {num} | {text}\n{bar}| {}{}",
            " ".repeat(col as usize - 1),
            "^".repeat(carets),
        )
    }
}

impl fmt::Display for SourceMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} file(s)", self.files.len())
    }
}

#[cfg(test)]
mod tests {
    use super::{FileId, SourceMap};
    use crate::lexer::Span;

    #[test]
    fn add_file_assigns_contiguous_ids_from_zero() {
        let mut sm = SourceMap::new();
        assert!(sm.is_empty());
        let a = sm.add_file("a.fln", "x = 1;");
        let b = sm.add_file("b.fln", "y = 2;");
        assert_eq!(a, FileId(0));
        assert_eq!(b, FileId(1));
        assert_eq!(sm.len(), 2);
        assert_eq!(sm.get(a).name, "a.fln");
        assert_eq!(sm.get(b).source, "y = 2;");
    }

    #[test]
    fn line_col_per_file() {
        // The same byte offset means different positions in different files:
        // file identity must come from the span, not from a global source.
        let mut sm = SourceMap::new();
        let a = sm.add_file("a.fln", "x = 1;\ny = 2;");
        let b = sm.add_file("b.fln", "\n\nz = 3;");
        assert_eq!(sm.line_col(a, 0), (1, 1));
        // Byte 6 is the newline itself → still line 1; byte 7 is `y` on line 2.
        assert_eq!(sm.line_col(a, 6), (1, 7));
        assert_eq!(sm.line_col(a, 7), (2, 1));
        assert_eq!(sm.line_col(b, 0), (1, 1));
        assert_eq!(sm.line_col(b, 2), (3, 1));
        assert_eq!(sm.line_col(b, 4), (3, 3));
    }

    #[test]
    fn columns_count_chars_and_skip_cr_per_file() {
        let mut sm = SourceMap::new();
        let f = sm.add_file("t.fln", "héllo\r\nx");
        // Byte 3 sits past the 2-byte `é`: char column 3, not byte column 4.
        assert_eq!(sm.line_col(f, 3), (1, 3));
        // Byte 7 is the newline of the `\r\n` pair (line 1); byte 8 is `x`.
        assert_eq!(sm.line_col(f, 7), (1, 6));
        assert_eq!(sm.line_col(f, 8), (2, 1));
    }

    #[test]
    fn out_of_range_and_mid_char_offsets_clamp() {
        let mut sm = SourceMap::new();
        let f = sm.add_file("t.fln", "ab");
        assert_eq!(sm.line_col(f, 100), (1, 3));
        // A mid-char offset floors to the boundary of the char containing it.
        let g = sm.add_file("u.fln", "é");
        assert_eq!(sm.line_col(g, 1), (1, 1));
    }

    #[test]
    fn render_location_names_the_file_per_span() {
        // Two files, one span in each: the rendered `-->` must name each
        // span's own file (the V01 multi-file acceptance case).
        let mut sm = SourceMap::new();
        let a = sm.add_file("src/main.fln", "x = 1;\ny = a / b;\n");
        let b = sm.add_file("src/net/tcp.fln", "func f(): int = 42\n");
        let in_a = sm.render_location(Span::new(a, 7, 12));
        assert_eq!(
            in_a,
            "  --> src/main.fln:2:1\n   |\n 2 | y = a / b;\n   | ^^^^^"
        );
        let in_b = sm.render_location(Span::new(b, 5, 6));
        assert_eq!(
            in_b,
            "  --> src/net/tcp.fln:1:6\n   |\n 1 | func f(): int = 42\n   |      ^"
        );
    }

    #[test]
    fn multiline_span_caret_stops_at_first_line() {
        let mut sm = SourceMap::new();
        let f = sm.add_file("c.fln", "q = a /\nb;");
        let out = sm.render_location(Span::new(f, 4, 9));
        assert_eq!(out, "  --> c.fln:1:5\n   |\n 1 | q = a /\n   |     ^^^");
    }

    #[test]
    fn cr_is_invisible_in_rendered_block() {
        let mut sm = SourceMap::new();
        let f = sm.add_file("c.fln", "q = a / b;\r\nx = 1;\r\n");
        let out = sm.render_location(Span::new(f, 4, 5));
        assert!(!out.contains('\r'), "full output: {out:?}");
        assert!(out.contains("--> c.fln:1:5"), "full output: {out:?}");
    }

    #[test]
    fn tab_counts_one_column() {
        // A tab is one char, not an expanded run of spaces: columns advance
        // by one (same rule as `fleen-vm`'s `diag::line_col`).
        let mut sm = SourceMap::new();
        let f = sm.add_file("t.fln", "\tx\ty");
        assert_eq!(sm.line_col(f, 1), (1, 2));
        assert_eq!(sm.line_col(f, 3), (1, 4));
    }

    #[test]
    fn synthetic_span_renders_a_placeholder() {
        // Synthetic spans carry no file: they must not panic (diagnostics
        // render best-effort) and must not resolve through the map.
        let mut sm = SourceMap::new();
        sm.add_file("c.fln", "x = 1;");
        assert_eq!(sm.render_location(Span::SYNTHETIC), "  --> <synthetic>");
    }

    #[test]
    fn out_of_range_span_renders_one_caret() {
        // A span past the end (corrupt span_map) clamps: the caret lands at
        // the last position of the file and the run degrades to one caret.
        let mut sm = SourceMap::new();
        let f = sm.add_file("c.fln", "x = 1;");
        let out = sm.render_location(Span::new(f, 100, 102));
        assert!(out.contains("--> c.fln:1:7"), "full output: {out:?}");
        assert!(out.ends_with('^'), "full output: {out:?}");
    }
}
