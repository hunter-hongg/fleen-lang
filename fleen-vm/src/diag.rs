//! Runtime diagnostics: source byte offset → line/column (0.0.2 U10).
//!
//! Error-path only; every scan is a single O(n) pass over the source.

use std::path::Path;

use crate::error::RuntimeError;

/// Convert a source byte offset to a 1-based `(line, column)` pair.
///
/// Columns count `char`s, not bytes (the source is UTF-8). `\r` occupies no
/// column: the lexer treats it as whitespace and folds `\r\n` into a single
/// newline (ASI.md §2), so it can never sit inside a token span. An `offset`
/// past the end of `source` (or inside a multi-byte char) clamps to the
/// position after the last `char`.
pub fn line_col(source: &str, offset: u32) -> (u32, u32) {
    let (line, col, _) = locate(source, offset);
    (line, col)
}

/// The line containing `offset`, as `(1-based line, 1-based column, line
/// text)` — the text form [`line_col`] is usually paired with when rendering
/// a diagnostic. Clamping follows [`line_col`].
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

/// Render a runtime error as the CLI prints it on stderr (0.0.2 U10).
///
/// With the source text and a span (the `.fln` one-command path) the error
/// renders rustc-style, e.g.:
///
/// ```text
/// runtime error: division by zero
///   --> crash.fln:5:9
///    |
///  5 |     q = a / b;
///    |         ^^^^^
/// ```
///
/// Without a usable location — `.flnc` execution (no source), a module with
/// no `span_map` entries (v1 bytecode), or a span outside the source — the
/// report degenerates to the message plus, when known, the function and
/// instruction offset: `runtime error: … (in `div`, pc 0x12)`.
///
/// The returned string has no trailing newline; callers add one.
pub fn render_runtime_error(path: &Path, source: Option<&str>, e: &RuntimeError) -> String {
    if let (Some(src), Some((start, end))) = (source, e.span)
        && (start as usize) < src.len()
        && end <= src.len() as u32
    {
        let file = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        return format!(
            "runtime error: {e}\n{}",
            render_location(&file, src, (start, end))
        );
    }
    let suffix = match (&e.func, e.pc) {
        (Some(f), Some(pc)) => format!(" (in `{f}`, pc 0x{pc:x})"),
        _ => String::new(),
    };
    format!("runtime error: {e}{suffix}")
}

/// Render the `-->`/source-line/caret block for a span inside `source`.
fn render_location(file: &str, source: &str, span: (u32, u32)) -> String {
    let (line, col, text) = locate(source, span.0);
    // A span may cross a newline (multi-line expression); only the first
    // line is shown, so the caret run stops at its end. `\r` never renders.
    let text = text.trim_end_matches('\r');
    let num = line.to_string();
    let bar = " ".repeat(num.len() + 2);
    // Caret run: clamp/floor the span ends exactly like `locate` floors its
    // offset, so an out-of-range or mid-char span renders (worst case one
    // caret) instead of panicking — diagnostics render best-effort. Must
    // stay in lockstep with `fleen_compiler`'s `SourceMap::render_location`
    // (pinned by `compiler_source_map_and_vm_diag_agree` below).
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{line_col, locate, render_runtime_error};
    use crate::error::{RuntimeError, RuntimeErrorKind};

    /// A located division-by-zero error at `span` in `func`.
    fn div_err(span: Option<(u32, u32)>) -> RuntimeError {
        RuntimeError {
            kind: RuntimeErrorKind::DivisionByZero,
            span,
            func: Some("main".into()),
            pc: Some(16),
        }
    }

    #[test]
    fn start_of_file_is_one_one() {
        assert_eq!(line_col("abc", 0), (1, 1));
        assert_eq!(line_col("", 0), (1, 1));
    }

    #[test]
    fn counts_lines_from_newlines() {
        let src = "ab\ncd\n";
        assert_eq!(line_col(src, 3), (2, 1));
        assert_eq!(line_col(src, 5), (2, 3));
        // Offset at the trailing newline starts the next line.
        assert_eq!(line_col(src, 6), (3, 1));
    }

    #[test]
    fn columns_count_chars_not_bytes() {
        // Multibyte: `é` is 2 bytes; columns count chars.
        let src = "héllo\nx";
        assert_eq!(line_col(src, 0), (1, 1));
        // Byte 3 sits past the 2-byte `é`: char column 3, not byte column 4.
        assert_eq!(line_col(src, 3), (1, 3));
        // "héllo\n" is 7 bytes; byte 7 starts line 2.
        assert_eq!(line_col(src, 7), (2, 1));
    }

    #[test]
    fn carriage_return_takes_no_column() {
        let src = "ab\r\ncd";
        // 'c' starts line 2 (byte 4); '\r' before the '\n' takes no column.
        assert_eq!(line_col(src, 4), (2, 1));
        assert_eq!(line_col(src, 5), (2, 2));
    }

    #[test]
    fn offsets_past_end_clamp_to_last_position() {
        assert_eq!(line_col("ab", 100), (1, 3));
        // A mid-char offset floors to the boundary of the char containing
        // it, i.e. that char's own column.
        assert_eq!(line_col("é", 1), (1, 1));
    }

    #[test]
    fn locate_returns_line_text() {
        let src = "first\nsecond line\nthird";
        // Byte 8 is the 'c' of "second" (3rd char of line 2).
        let (line, col, text) = locate(src, 8);
        assert_eq!((line, col), (2, 3));
        assert_eq!(text, "second line");
        let (_, _, text) = locate(src, u32::MAX);
        assert_eq!(text, "third");
    }

    // -------------------------------------------------------------------
    // CLI rendering (`render_runtime_error`): exact-output locks for the
    // `-->`/source-line/caret block and both degradation forms.
    // -------------------------------------------------------------------

    #[test]
    fn renders_rustc_style_block() {
        // 'a' of `a / b` sits at byte 11 (line 2, char 5).
        let src = "x = 1;\nq = a / b;\n";
        let out = render_runtime_error(Path::new("crash.fln"), Some(src), &div_err(Some((11, 12))));
        assert_eq!(
            out,
            "runtime error: division by zero\n  --> crash.fln:2:5\n   |\n 2 | q = a / b;\n   |     ^"
        );
    }

    #[test]
    fn caret_run_matches_span_width() {
        // A wide span (e.g. once binary spans cover the whole expression)
        // draws one caret per char.
        let src = "x = 1;\nq = a / b;\n";
        let out = render_runtime_error(Path::new("c.fln"), Some(src), &div_err(Some((11, 16))));
        assert!(out.ends_with("|     ^^^^^"), "full output: {out:?}");
    }

    #[test]
    fn multiline_span_caret_stops_at_first_line() {
        // The span runs from `a` across the newline into the next line (5
        // bytes); the block shows only the first line and the caret run
        // stops at its end — 3 carets, not the span's byte width.
        let src = "q = a /\nb;";
        let out = render_runtime_error(Path::new("c.fln"), Some(src), &div_err(Some((4, 9))));
        assert_eq!(
            out,
            "runtime error: division by zero\n  --> c.fln:1:5\n   |\n 1 | q = a /\n   |     ^^^"
        );
    }

    #[test]
    fn carriage_return_is_invisible_in_the_block() {
        // `\r` takes no column (ASI.md §2) and never appears in the output.
        let src = "q = a / b;\r\nx = 1;\r\n";
        let out = render_runtime_error(Path::new("c.fln"), Some(src), &div_err(Some((4, 5))));
        assert!(!out.contains('\r'), "full output: {out:?}");
        assert!(out.contains("--> c.fln:1:5"), "full output: {out:?}");
    }

    #[test]
    fn flnc_degrades_to_func_and_pc() {
        let out = render_runtime_error(Path::new("m.flnc"), None, &div_err(Some((0, 1))));
        assert_eq!(out, "runtime error: division by zero (in `main`, pc 0x10)");
    }

    #[test]
    fn error_without_any_location_prints_the_message_only() {
        let e = RuntimeError {
            kind: RuntimeErrorKind::StackUnderflow,
            span: None,
            func: None,
            pc: None,
        };
        let out = render_runtime_error(Path::new("m.flnc"), None, &e);
        assert_eq!(out, "runtime error: stack underflow");
    }

    #[test]
    fn span_outside_source_degrades() {
        // Defensive: a span past the end (corrupt span_map) must not panic
        // or slice; the report falls back to the function/offset form.
        let src = "x = 1;\n";
        let out = render_runtime_error(Path::new("c.fln"), Some(src), &div_err(Some((100, 102))));
        assert_eq!(out, "runtime error: division by zero (in `main`, pc 0x10)");
    }

    #[test]
    fn compiler_source_map_and_vm_diag_agree() {
        // The locate/render logic is deliberately duplicated: the compiler's
        // `SourceMap` serves multi-file compile-time diagnostics, the VM's
        // `diag` serves single-file runtime ones (0.0.3 V01). Two copies can
        // drift silently, so every offset of a tricky-source corpus must
        // produce identical line/column, line text, and rendered blocks.
        // V05 wires the VM to the SourceMap; until then this test is the pin.
        use fleen_compiler::SourceMap;
        use fleen_compiler::lexer::Span;

        let sources = [
            "",
            "x = 1;",
            "x = 1;\nq = a / b;\n",
            "héllo\r\nx",
            "\tindented\tx",
            "q = a /\nb;",
            "\n\nz = 3;",
        ];
        for src in sources {
            let mut sm = SourceMap::new();
            let f = sm.add_file("c.fln", src);
            for offset in 0..=src.len() as u32 {
                assert_eq!(
                    sm.line_col(f, offset),
                    line_col(src, offset),
                    "line/col drift in {src:?} at offset {offset}"
                );
                let (_, _, sm_text) = sm.locate(f, offset);
                let (_, _, vm_text) = locate(src, offset);
                assert_eq!(sm_text, vm_text, "line-text drift in {src:?} at {offset}");
            }
            for span in [(0, 1), (4, 9), (0, src.len() as u32)] {
                let from_map = sm.render_location(Span::new(f, span.0, span.1));
                let from_vm = super::render_location("c.fln", src, span);
                assert_eq!(from_map, from_vm, "render drift in {src:?} at {span:?}");
            }
        }
    }
}
