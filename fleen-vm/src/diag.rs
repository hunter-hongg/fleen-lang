//! Runtime diagnostics: source byte offset → line/column (0.0.2 U10).
//!
//! Error-path only; every scan is a single O(n) pass over the source.

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

#[cfg(test)]
mod tests {
    use super::{line_col, locate};

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
        let src = "你好\nx";
        assert_eq!(line_col(src, 0), (1, 1));
        assert_eq!(line_col(src, 3), (1, 2));
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
        assert_eq!(line_col("你", 1), (1, 1));
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
}
