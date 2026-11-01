//! Shared span-diagnostic rendering (0.0.5 CLI overhaul, D6).
//!
//! The compile-time SourceMap and runtime fleen-vm diagnostic paths once
//! duplicated the byte-offset -> line/column and rustc-style block logic.
//! This module is the single shared implementation both now delegate to, so
//! the three CLI rewrites (fleen-vm, fleen-verify, flint-cli) render
//! diagnostics identically.

/// Convert a source byte offset to a 1-based (line, column) pair.
///
/// Columns count chars, not bytes (the source is UTF-8). A carriage return
/// occupies no column. An offset past the end of source (or inside a
/// multi-byte char) clamps to the position after the last char.
pub fn line_col(source: &str, offset: u32) -> (u32, u32) {
    let (line, col, _) = locate(source, offset);
    (line, col)
}

/// The line containing offset, as (1-based line, 1-based column, line text).
/// Clamping follows line_col.
pub fn locate(source: &str, offset: u32) -> (u32, u32, &str) {
    // Floor a mid-char (never produced by the lexer) or out-of-range offset
    // to a char boundary so the slicing below cannot panic.
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

/// Render the -->/source-line/caret block for a span inside source.
///
/// A span may cross a newline; only the first line is shown. Out-of-range or
/// mid-char span ends clamp exactly like locate, so rendering never panics on
/// offsets.
pub fn render_location(file: &str, source: &str, span: (u32, u32)) -> String {
    let (line, col, text) = locate(source, span.0);
    // A span may cross a newline (multi-line expression); only the first
    // line is shown, so the caret run stops at its end. '\r' never renders.
    let text = text.trim_end_matches('\r');
    let num = line.to_string();
    let bar = " ".repeat(num.len() + 2);
    // Caret run: clamp/floor the span ends exactly like locate floors its
    // offset, so an out-of-range or mid-char span renders (worst case one
    // caret) instead of panicking — diagnostics render best-effort.
    let mut start = (span.0 as usize).min(source.len());
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    let end = (span.1 as usize).min(source.len()).max(start);
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

/// Whether ANSI color should be used for a diagnostic.
///
/// Color is off when the caller passed --no-color (no_color) or when the
/// NO_COLOR environment variable is set (D6; no color crate).
pub fn color_enabled(no_color: bool) -> bool {
    !no_color && std::env::var_os("NO_COLOR").is_none()
}

/// Wrap s in bold ANSI escapes when enabled, otherwise return it as-is.
pub fn colorize(s: &str, enabled: bool) -> String {
    if enabled {
        format!("\x1b[1m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// Render a full diagnostic: a message line, an optional span block, then
/// indented note: and suggestion: lines.
///
/// The result has no trailing newline. When color is true the entire
/// resulting string is wrapped in bold ANSI escapes (the inner text stays
/// contiguous, so substring checks keep working).
pub fn render_diagnostic(
    message: &str,
    file: &str,
    source: Option<&str>,
    span: Option<(u32, u32)>,
    notes: &[String],
    suggestions: &[String],
    color: bool,
) -> String {
    let mut out = String::new();
    out.push_str(message);
    if let (Some(src), Some((start, end))) = (source, span) {
        out.push('\n');
        out.push_str(&render_location(file, src, (start, end)));
    }
    for note in notes {
        out.push_str("\n  note: ");
        out.push_str(note);
    }
    for suggestion in suggestions {
        out.push_str("\n  suggestion: ");
        out.push_str(suggestion);
    }
    if color { colorize(&out, true) } else { out }
}

#[cfg(test)]
mod tests {
    use super::{color_enabled, colorize, line_col, locate, render_diagnostic, render_location};

    #[test]
    fn line_col_counts_chars_not_bytes() {
        assert_eq!(line_col("héllo\nx", 3), (1, 3));
        assert_eq!(line_col("", 0), (1, 1));
    }

    #[test]
    fn locate_returns_line_text() {
        let (line, col, text) = locate("first\nsecond line\nthird", 8);
        assert_eq!((line, col), (2, 3));
        assert_eq!(text, "second line");
    }

    #[test]
    fn render_location_matches_rustc_block() {
        let src = "x = 1;\nq = a / b;\n";
        assert_eq!(
            render_location("crash.fln", src, (11, 12)),
            "  --> crash.fln:2:5\n   |\n 2 | q = a / b;\n   |     ^"
        );
    }

    #[test]
    fn render_diagnostic_with_notes_and_suggestions() {
        let notes = vec!["note one".to_string()];
        let suggestions = vec!["suggest two".to_string()];
        let out = render_diagnostic(
            "type error: expected int",
            "src/main.fln",
            Some("x = 1;\nq = a / b;\n"),
            Some((11, 12)),
            &notes,
            &suggestions,
            false,
        );
        assert_eq!(
            out,
            "type error: expected int\n  --> src/main.fln:2:5\n   |\n 2 | q = a / b;\n   |     ^\n  note: note one\n  suggestion: suggest two"
        );
    }

    #[test]
    fn render_diagnostic_color_wraps_whole_string() {
        let out = render_diagnostic("boom", "f.fln", Some("abc"), Some((0, 1)), &[], &[], true);
        assert!(out.starts_with("\x1b[1m"), "full output: {out:?}");
        assert!(out.ends_with("\x1b[0m"), "full output: {out:?}");
        assert!(out.contains("\x1b[1m"), "full output: {out:?}");
        assert!(out.contains("\x1b[0m"), "full output: {out:?}");
        assert!(out.contains("boom"), "full output: {out:?}");
    }

    #[test]
    fn colorize_only_marks_when_enabled() {
        assert_eq!(colorize("x", false), "x");
        assert_eq!(colorize("x", true), "\x1b[1mx\x1b[0m");
    }

    #[test]
    fn color_enabled_honors_no_color_flag() {
        assert!(!color_enabled(true));
        // The NO_COLOR path is environment-dependent; the test must not mutate
        // the environment, so it asserts only under the unset precondition.
        if std::env::var_os("NO_COLOR").is_none() {
            assert!(color_enabled(false));
        }
    }
}
