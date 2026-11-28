//! Fleen lexer: converts source code into a stream of tokens.

use crate::lexer::error::LexError;
use crate::lexer::token::{Span, StringPart, Token, TokenKind};
use crate::source_map::FileId;

/// Lexer state for tokenizing Fleen source code.
pub struct Lexer<'a> {
    /// File every produced span belongs to (0.0.3 V01).
    file: FileId,
    chars: std::str::Chars<'a>,
    byte_offset: u32,
    char_offset: usize,
    current_char: Option<char>,
    next_char: Option<char>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source code; all token spans carry
    /// `file`.
    pub fn new(file: FileId, source: &'a str) -> Self {
        let mut chars = source.chars();
        let current_char = chars.next();
        let next_char = chars.next();
        Self {
            file,
            chars,
            byte_offset: 0,
            char_offset: 0,
            current_char,
            next_char,
        }
    }

    /// Tokenize the entire source code into a vector of tokens.
    ///
    /// 0.0.2: line breaks are preserved as `Newline` tokens (collapsed runs of
    /// blank lines, none at file start) for the ASI pass between Lex and
    /// Parse (docs/0.0.2/ASI.md).
    pub fn tokenize(&mut self) -> Result<Vec<Token>, Vec<LexError>> {
        let mut tokens = Vec::with_capacity(self.chars.as_str().len() / 4);
        let mut errors = Vec::new();

        while !self.is_eof() {
            let ws_start = self.byte_offset;
            let saw_newline = self.skip_whitespace_and_comments(&mut errors);

            // Collapse runs of blank lines into a single Newline, and never
            // emit one at file start. EOF needs no trailing Newline — the
            // ASI pass treats EOF as a boundary itself.
            if saw_newline && !tokens.is_empty() {
                tokens.push(Token::new(
                    TokenKind::Newline,
                    Span::new(self.file, ws_start, self.byte_offset),
                ));
            }

            if self.is_eof() {
                break;
            }

            let token = self.next_token();

            match token {
                Ok(tok) => tokens.push(tok),
                Err(err) => {
                    errors.push(err);
                    // Error recovery: advance by one character
                    self.advance();
                }
            }
        }

        // Add EOF token
        tokens.push(Token::new(
            TokenKind::Eof,
            Span::new(self.file, self.byte_offset, self.byte_offset),
        ));

        if errors.is_empty() {
            Ok(tokens)
        } else {
            Err(errors)
        }
    }

    /// Get the next token from the source.
    fn next_token(&mut self) -> Result<Token, LexError> {
        let start = self.byte_offset;
        let ch = self.current_char.expect("next_token called at EOF");

        // Single-character tokens and operators
        match ch {
            '(' => return Ok(self.make_token(TokenKind::LParen, 1)),
            ')' => return Ok(self.make_token(TokenKind::RParen, 1)),
            '{' => return Ok(self.make_token(TokenKind::LBrace, 1)),
            '}' => return Ok(self.make_token(TokenKind::RBrace, 1)),
            '[' => return Ok(self.make_token(TokenKind::LBracket, 1)),
            ']' => return Ok(self.make_token(TokenKind::RBracket, 1)),
            ',' => return Ok(self.make_token(TokenKind::Comma, 1)),
            ';' => return Ok(self.make_token(TokenKind::Semi, 1)),
            '.' => return Ok(self.make_token(TokenKind::Dot, 1)),
            ':' => return Ok(self.make_token(TokenKind::Colon, 1)),
            '+' => return Ok(self.make_token(TokenKind::Plus, 1)),
            '-' => {
                // Check for ->
                if self.next_char == Some('>') {
                    return Ok(self.make_token(TokenKind::Arrow, 2));
                }
                return Ok(self.make_token(TokenKind::Minus, 1));
            }
            '*' => return Ok(self.make_token(TokenKind::Star, 1)),
            '/' => {
                // Comments are handled in skip_whitespace_and_comments
                // This shouldn't happen as / would be consumed there
                return Ok(self.make_token(TokenKind::Slash, 1));
            }
            '%' => return Ok(self.make_token(TokenKind::Percent, 1)),
            '!' => {
                if self.next_char == Some('=') {
                    return Ok(self.make_token(TokenKind::Ne, 2));
                }
                return Ok(self.make_token(TokenKind::Bang, 1));
            }
            '?' => return Ok(self.make_token(TokenKind::Question, 1)),
            '|' => return Ok(self.make_token(TokenKind::Pipe, 1)),
            // 0.0.8 T01: `#` introduces a `#feature` directive. It is a
            // token, never a comment (the comment syntax is `//` / `/* */`
            // only, unchanged) — recognized unconditionally in every file.
            '#' => return Ok(self.make_token(TokenKind::Hash, 1)),
            '=' => {
                if self.next_char == Some('=') {
                    return Ok(self.make_token(TokenKind::Eq, 2));
                }
                return Ok(self.make_token(TokenKind::Assign, 1));
            }
            '<' => {
                if self.next_char == Some('=') {
                    return Ok(self.make_token(TokenKind::Le, 2));
                }
                return Ok(self.make_token(TokenKind::Lt, 1));
            }
            '>' => {
                if self.next_char == Some('=') {
                    return Ok(self.make_token(TokenKind::Ge, 2));
                }
                return Ok(self.make_token(TokenKind::Gt, 1));
            }
            '"' => return self.scan_string(start),
            '0'..='9' => return self.scan_number(start),
            '_' | 'a'..='z' | 'A'..='Z' => return self.scan_ident_or_keyword(start),
            _ => {}
        }

        // Invalid character
        Err(LexError::invalid_char(
            ch,
            Span::new(self.file, start, start + ch.len_utf8() as u32),
        ))
    }

    /// Create a token with the given kind and length in characters.
    fn make_token(&mut self, kind: TokenKind, char_len: usize) -> Token {
        let start = self.byte_offset;
        for _ in 0..char_len {
            self.advance();
        }
        let end = self.byte_offset;
        Token::new(kind, Span::new(self.file, start, end))
    }

    /// Scan a string literal.
    ///
    /// 0.0.10 T04 (D4): a `$` immediately followed by `{` opens an embedded
    /// expression — the lexer switches to expression mode (token scanning
    /// with brace-depth tracking, [`Self::scan_interp_expr`]) until the
    /// matching `}`; a literal containing `${` therefore produces a
    /// [`TokenKind::StringInterp`] parts token. A `$` in any other position
    /// is an ordinary character, and a literal without `${` still produces a
    /// plain [`TokenKind::StringLit`] — its pre-0.0.10 compile path stays
    /// byte-identical (PLAN D6). `\n` terminates the literal in both modes
    /// (a string literal is a single-line construct — unterminated-string
    /// handling unchanged); EOF inside an embedded expression is
    /// `UnclosedInterpolation`. A nested string literal inside an embedded
    /// expression works by recursion (`scan_string` is re-entered through
    /// `next_token`).
    fn scan_string(&mut self, start: u32) -> Result<Token, LexError> {
        // Consume opening quote
        self.advance();

        // Preallocate (SPEC §9): most strings have no interpolation and
        // return on the plain `StringLit` path, so start small rather than
        // pay for a chunk-per-`${}` guess.
        let mut parts: Vec<StringPart> = Vec::with_capacity(2);
        let mut lit = String::new();

        while let Some(ch) = self.current_char {
            match ch {
                '"' => {
                    // Closing quote found
                    self.advance();
                    let kind = if parts.is_empty() {
                        // No interpolation: the plain literal path, unchanged.
                        TokenKind::StringLit(lit)
                    } else {
                        if !lit.is_empty() {
                            parts.push(StringPart::Lit(lit));
                        }
                        TokenKind::StringInterp(parts)
                    };
                    return Ok(Token::new(
                        kind,
                        Span::new(self.file, start, self.byte_offset),
                    ));
                }
                '\\' => {
                    self.advance();
                    match self.current_char {
                        Some('n') => {
                            lit.push('\n');
                            self.advance();
                        }
                        Some('t') => {
                            lit.push('\t');
                            self.advance();
                        }
                        Some('r') => {
                            lit.push('\r');
                            self.advance();
                        }
                        Some('\\') => {
                            lit.push('\\');
                            self.advance();
                        }
                        Some('"') => {
                            lit.push('"');
                            self.advance();
                        }
                        Some('\'') => {
                            lit.push('\'');
                            self.advance();
                        }
                        // 0.0.10 T04 (D4): `\$` is the one new escape — a
                        // literal `$`. Escaping is only needed before `{`.
                        Some('$') => {
                            lit.push('$');
                            self.advance();
                        }
                        // Extended string escapes (\0, \b, \f, \xHH, \uXXXX)
                        // are tracked on the 0.1.0 roadmap (DESIGN §18).
                        Some(bad) => {
                            let err_span = Span::new(
                                self.file,
                                self.byte_offset - 1,
                                self.byte_offset + bad.len_utf8() as u32,
                            );
                            self.advance();
                            return Err(LexError::invalid_escape(bad, err_span));
                        }
                        None => {
                            let err_span =
                                Span::new(self.file, self.byte_offset - 1, self.byte_offset);
                            return Err(LexError::invalid_escape('\0', err_span));
                        }
                    }
                }
                '\n' => {
                    // Unterminated string (newline in string not allowed) —
                    // unchanged for literal chunks and embedded expressions
                    // alike (T04: a string literal is a single-line construct).
                    let err_span = Span::new(self.file, start, self.byte_offset);
                    return Err(LexError::unterminated_string(err_span));
                }
                '$' if self.next_char == Some('{') => {
                    // `${` opens an embedded expression; flush the literal
                    // chunk accumulated so far (empty chunks are never
                    // pushed, so adjacent Lit parts cannot occur).
                    if !lit.is_empty() {
                        parts.push(StringPart::Lit(std::mem::take(&mut lit)));
                    }
                    let dollar_start = self.byte_offset;
                    let expr = self.scan_interp_expr(dollar_start)?;
                    parts.push(expr);
                }
                ch => {
                    lit.push(ch);
                    self.advance();
                }
            }
        }

        // EOF reached without closing quote
        Err(LexError::unterminated_string(Span::new(
            self.file,
            start,
            self.byte_offset,
        )))
    }

    /// Scan an embedded interpolation expression `${ ... }` (0.0.10 T04, D4).
    ///
    /// Called positioned at the opening `$` (with `next_char == '{'`);
    /// consumes through the matching `}`. The delimiters are excluded from
    /// the token run; the run's tokens carry absolute file spans. Brace
    /// depth is tracked on the *token* level, so braces inside a nested
    /// string literal never leak (the nested literal is scanned recursively
    /// by [`Self::scan_string`] through [`Self::next_token`]). Whitespace
    /// (spaces/tabs/CR) separates tokens — `next_token` itself skips none,
    /// that being the `tokenize` loop's job. Comments are not recognized
    /// inside an embedded expression, and a newline still terminates the
    /// string literal. EOF inside the expression is `UnclosedInterpolation`
    /// with a span covering `${` through EOF.
    fn scan_interp_expr(&mut self, dollar_start: u32) -> Result<StringPart, LexError> {
        // Consume `${`
        self.advance(); // $
        self.advance(); // {

        // Preallocate (SPEC §9): the run's token count isn't knowable up
        // front, so seed a small capacity to avoid the first re-allocations
        // on this hot path.
        let mut tokens: Vec<Token> = Vec::with_capacity(4);
        let mut depth: u32 = 0;

        loop {
            while matches!(self.current_char, Some(' ') | Some('\t') | Some('\r')) {
                self.advance();
            }
            if self.current_char == Some('\n') {
                // The string literal is a single-line construct — same
                // unterminated-string rule as literal chunks.
                return Err(LexError::unterminated_string(Span::new(
                    self.file,
                    dollar_start,
                    self.byte_offset,
                )));
            }
            if self.is_eof() {
                return Err(LexError::unclosed_interpolation(Span::new(
                    self.file,
                    dollar_start,
                    self.byte_offset,
                )));
            }

            let tok = self.next_token()?;
            match tok.kind {
                TokenKind::LBrace => {
                    depth += 1;
                    tokens.push(tok);
                }
                TokenKind::RBrace => {
                    if depth == 0 {
                        // The matching `}` closes the interpolation; it is
                        // consumed but not part of the run.
                        let span = Span::new(self.file, dollar_start, self.byte_offset);
                        return Ok(StringPart::Expr { tokens, span });
                    }
                    depth -= 1;
                    tokens.push(tok);
                }
                _ => tokens.push(tok),
            }
        }
    }

    /// Scan a number literal (int or float).
    fn scan_number(&mut self, start: u32) -> Result<Token, LexError> {
        let mut has_dot = false;
        let mut digits = String::new();

        while let Some(ch) = self.current_char {
            if ch.is_ascii_digit() {
                digits.push(ch);
                self.advance();
            } else if ch == '.' && !has_dot && self.next_char.is_some_and(|c| c.is_ascii_digit()) {
                has_dot = true;
                digits.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let span = Span::new(self.file, start, self.byte_offset);

        // Check for invalid number (e.g., just ".")
        if digits.is_empty() {
            return Err(LexError::invalid_number("empty number".to_string(), span));
        }

        if has_dot {
            // Float literal
            match digits.parse::<f64>() {
                Ok(val) if val.is_finite() => Ok(Token::new(TokenKind::FloatLit(val), span)),
                Ok(_) => Err(LexError::float_overflow(span)),
                Err(_) => Err(LexError::invalid_number(digits, span)),
            }
        } else {
            // Integer literal
            match digits.parse::<i64>() {
                Ok(val) => Ok(Token::new(TokenKind::IntLit(val), span)),
                Err(_) => Err(LexError::int_overflow(span)),
            }
        }
    }

    /// Scan an identifier or keyword.
    fn scan_ident_or_keyword(&mut self, start: u32) -> Result<Token, LexError> {
        let mut ident = String::new();

        while let Some(ch) = self.current_char {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let span = Span::new(self.file, start, self.byte_offset);

        // Check for keywords
        let kind = match ident.as_str() {
            "func" => TokenKind::Func,
            "const" => TokenKind::Const,
            "if" => TokenKind::If,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "choose" => TokenKind::Choose,
            "when" => TokenKind::When,
            "otherwise" => TokenKind::Otherwise,
            "import" => TokenKind::Import,
            "struct" => TokenKind::Struct,
            "pub" => TokenKind::Pub,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "int" => TokenKind::IntType,
            "float" => TokenKind::FloatType,
            "bool" => TokenKind::BoolType,
            "string" => TokenKind::StringType,
            "unit" => TokenKind::UnitType,
            "Result" => TokenKind::ResultType,
            "box" => TokenKind::BoxType,
            "ref" => TokenKind::RefType,
            "move" => TokenKind::Move,
            "clone" => TokenKind::Clone,
            "deref" => TokenKind::Deref,
            "as" => TokenKind::As,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "channel" => TokenKind::Channel,
            "ptr" => TokenKind::Ptr,
            "addr" => TokenKind::Addr,
            "deref_ptr" => TokenKind::DerefPtr,
            "unsafe" => TokenKind::Unsafe,
            "trusted" => TokenKind::Trusted,
            // 0.0.9 T03 (D4): the loop-jump keywords are reserved words (the
            // source breaking set of 0.0.9) — using them as identifiers stops
            // compiling, the 0.0.5/0.0.6 precedent.
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "enum" => TokenKind::Enum,
            // 0.0.11 T01 (D1): `map` is a reserved word (the source breaking
            // set of 0.0.11) — using it as an identifier stops compiling, the
            // 0.0.5/0.0.9/0.0.10 reserved-word precedent.
            "map" => TokenKind::Map,
            "or" => TokenKind::Or,
            "and" => TokenKind::And,
            "not" => TokenKind::Not,
            _ => TokenKind::Ident(ident),
        };

        Ok(Token::new(kind, span))
    }

    /// Skip whitespace and comments. Returns whether a line break was crossed
    /// (line-comment-terminating newlines count; newlines inside block
    /// comments do not).
    fn skip_whitespace_and_comments(&mut self, errors: &mut Vec<LexError>) -> bool {
        let mut saw_newline = false;
        loop {
            match self.current_char {
                Some(' ') | Some('\t') | Some('\r') => {
                    self.advance();
                }
                Some('\n') => {
                    saw_newline = true;
                    self.advance();
                }
                Some('/') if self.next_char == Some('/') => {
                    // Single-line comment
                    self.advance(); // consume first /
                    self.advance(); // consume second /
                    while let Some(ch) = self.current_char {
                        if ch == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                Some('/') if self.next_char == Some('*') => {
                    // Multi-line comment (no nesting)
                    let comment_start = self.byte_offset;
                    self.advance(); // consume /
                    self.advance(); // consume *
                    let mut terminated = false;
                    while !self.is_eof() {
                        match self.current_char {
                            Some('*') if self.next_char == Some('/') => {
                                self.advance(); // consume *
                                self.advance(); // consume /
                                terminated = true;
                                break;
                            }
                            _ => self.advance(),
                        }
                    }
                    if !terminated {
                        // Report unterminated comment error
                        let err_span = Span::new(self.file, comment_start, self.byte_offset);
                        errors.push(LexError::unterminated_comment(err_span));
                    }
                }
                _ => break,
            }
        }
        saw_newline
    }

    /// Advance to the next character.
    fn advance(&mut self) {
        if let Some(ch) = self.current_char {
            let old_offset = self.byte_offset;
            self.byte_offset += ch.len_utf8() as u32;
            // Sanity check: byte_offset should not overflow
            debug_assert!(self.byte_offset >= old_offset, "byte_offset overflow");
            self.char_offset += 1;
        }
        self.current_char = self.next_char;
        self.next_char = self.chars.next();
    }

    /// Check if we're at EOF.
    fn is_eof(&self) -> bool {
        self.current_char.is_none()
    }
}

/// Tokenize `source` under `file`: every produced span carries that id.
pub fn tokenize_in_file(file: FileId, source: &str) -> Result<Vec<Token>, Vec<LexError>> {
    let mut lexer = Lexer::new(file, source);
    lexer.tokenize()
}

/// Convenience function to tokenize a single source string.
///
/// Single-file convention (0.0.3 V01): the file is registered as `FileId(0)`,
/// matching [`crate::compile`].
pub fn tokenize(source: &str) -> Result<Vec<Token>, Vec<LexError>> {
    tokenize_in_file(FileId(0), source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_empty() {
        let tokens = tokenize("").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn tokenize_single_tokens() {
        let tokens = tokenize("(){}[]").unwrap();
        assert_eq!(tokens.len(), 7); // 6 tokens + EOF
        assert_eq!(tokens[0].kind, TokenKind::LParen);
        assert_eq!(tokens[1].kind, TokenKind::RParen);
        assert_eq!(tokens[2].kind, TokenKind::LBrace);
        assert_eq!(tokens[3].kind, TokenKind::RBrace);
        assert_eq!(tokens[4].kind, TokenKind::LBracket);
        assert_eq!(tokens[5].kind, TokenKind::RBracket);
        assert_eq!(tokens[6].kind, TokenKind::Eof);
    }

    #[test]
    fn tokenize_operators() {
        let tokens = tokenize("+ - * / % = == != < > <= >=").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Plus);
        assert_eq!(tokens[1].kind, TokenKind::Minus);
        assert_eq!(tokens[2].kind, TokenKind::Star);
        assert_eq!(tokens[3].kind, TokenKind::Slash);
        assert_eq!(tokens[4].kind, TokenKind::Percent);
        assert_eq!(tokens[5].kind, TokenKind::Assign);
        assert_eq!(tokens[6].kind, TokenKind::Eq);
        assert_eq!(tokens[7].kind, TokenKind::Ne);
        assert_eq!(tokens[8].kind, TokenKind::Lt);
        assert_eq!(tokens[9].kind, TokenKind::Gt);
        assert_eq!(tokens[10].kind, TokenKind::Le);
        assert_eq!(tokens[11].kind, TokenKind::Ge);
    }

    #[test]
    fn tokenize_arrow() {
        let tokens = tokenize("->").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Arrow);
    }

    #[test]
    fn tokenize_keywords() {
        let tokens =
            tokenize("func const if elif else while choose when otherwise import struct pub")
                .unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Func);
        assert_eq!(tokens[1].kind, TokenKind::Const);
        assert_eq!(tokens[2].kind, TokenKind::If);
        assert_eq!(tokens[3].kind, TokenKind::Elif);
        assert_eq!(tokens[4].kind, TokenKind::Else);
        assert_eq!(tokens[5].kind, TokenKind::While);
        assert_eq!(tokens[6].kind, TokenKind::Choose);
        assert_eq!(tokens[7].kind, TokenKind::When);
        assert_eq!(tokens[8].kind, TokenKind::Otherwise);
        assert_eq!(tokens[9].kind, TokenKind::Import);
        assert_eq!(tokens[10].kind, TokenKind::Struct);
        assert_eq!(tokens[11].kind, TokenKind::Pub);
    }

    #[test]
    fn tokenize_loop_keywords() {
        // 0.0.3 V04: `for` / `in` are reserved words (Breaking — they no
        // longer lex as identifiers).
        let tokens = tokenize("for x in a").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::For);
        assert_eq!(tokens[1].kind, TokenKind::Ident("x".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::In);
        assert_eq!(tokens[3].kind, TokenKind::Ident("a".to_string()));
        assert_eq!(TokenKind::For.keyword_str(), Some("for"));
        assert_eq!(TokenKind::In.keyword_str(), Some("in"));
        // Neither word can be an identifier any more: the lexer classifies
        // them as keywords, so `for = 1` / `in = 1` fail in the parser, not
        // by silently reading `for`/`in` as a variable name.
        let tokens = tokenize("for = 1").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::For);
        assert_eq!(tokens[1].kind, TokenKind::Assign);
    }

    #[test]
    fn tokenize_bool_keywords() {
        let tokens = tokenize("true false").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::True);
        assert_eq!(tokens[1].kind, TokenKind::False);
    }

    #[test]
    fn tokenize_type_keywords() {
        let tokens = tokenize("int float bool string unit Result box ref").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::IntType);
        assert_eq!(tokens[1].kind, TokenKind::FloatType);
        assert_eq!(tokens[2].kind, TokenKind::BoolType);
        assert_eq!(tokens[3].kind, TokenKind::StringType);
        assert_eq!(tokens[4].kind, TokenKind::UnitType);
        assert_eq!(tokens[5].kind, TokenKind::ResultType);
        assert_eq!(tokens[6].kind, TokenKind::BoxType);
        assert_eq!(tokens[7].kind, TokenKind::RefType);
    }

    #[test]
    fn tokenize_logical_keywords() {
        let tokens = tokenize("or and not").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Or);
        assert_eq!(tokens[1].kind, TokenKind::And);
        assert_eq!(tokens[2].kind, TokenKind::Not);
    }

    #[test]
    fn tokenize_ownership_keywords() {
        let tokens = tokenize("move clone deref").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Move);
        assert_eq!(tokens[1].kind, TokenKind::Clone);
        assert_eq!(tokens[2].kind, TokenKind::Deref);
    }

    #[test]
    fn tokenize_async_keywords() {
        // 0.0.4: `async` / `await` / `channel` are reserved words. They no
        // longer lex as identifiers.
        let tokens = tokenize("async await channel").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Async);
        assert_eq!(tokens[1].kind, TokenKind::Await);
        assert_eq!(tokens[2].kind, TokenKind::Channel);
        assert_eq!(TokenKind::Async.keyword_str(), Some("async"));
        assert_eq!(TokenKind::Await.keyword_str(), Some("await"));
        assert_eq!(TokenKind::Channel.keyword_str(), Some("channel"));
        // Metadata stays consistent with the reserved-word spelling.
        assert!(TokenKind::Async.is_keyword());
        assert!(TokenKind::Await.is_keyword());
        assert!(TokenKind::Channel.is_keyword());
        assert_eq!(TokenKind::Async.to_string(), "async");
        assert_eq!(TokenKind::Await.to_string(), "await");
        assert_eq!(TokenKind::Channel.to_string(), "channel");
    }

    #[test]
    fn tokenize_async_keyword_prefix_identifiers() {
        // 0.0.4 keywords must keep from swallowing longer identifiers that
        // merely start with them (`channel_id` etc. stay identifiers).
        let tokens = tokenize("asyncx awaiting channel_id awaitable").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "asyncx"));
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "awaiting"));
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "channel_id"));
        assert!(matches!(tokens[3].kind, TokenKind::Ident(ref s) if s == "awaitable"));
    }

    #[test]
    fn tokenize_deref_ptr_keyword() {
        // 0.0.5 T02: `deref_ptr` is the ptr read/write spelling (settled in
        // T02, folded back into PLAN §3.2) — a distinct reserved word, *not*
        // an overload of `deref` (which stays box-only). It lexes as a
        // keyword, and longer identifiers starting with it stay identifiers.
        let tokens = tokenize("deref_ptr").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::DerefPtr);
        assert_eq!(TokenKind::DerefPtr.keyword_str(), Some("deref_ptr"));
        assert!(TokenKind::DerefPtr.is_keyword());
        assert_eq!(TokenKind::DerefPtr.to_string(), "deref_ptr");
        let tokens = tokenize("deref_ptrx deref_ptr_2").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "deref_ptrx"));
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "deref_ptr_2"));
    }

    #[test]
    fn tokenize_ptr_keywords() {
        // 0.0.5: `ptr` / `addr` / `unsafe` / `trusted` are reserved words.
        // They no longer lex as identifiers.
        let tokens = tokenize("ptr addr unsafe trusted").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ptr);
        assert_eq!(tokens[1].kind, TokenKind::Addr);
        assert_eq!(tokens[2].kind, TokenKind::Unsafe);
        assert_eq!(tokens[3].kind, TokenKind::Trusted);
        assert_eq!(TokenKind::Ptr.keyword_str(), Some("ptr"));
        assert_eq!(TokenKind::Addr.keyword_str(), Some("addr"));
        assert_eq!(TokenKind::Unsafe.keyword_str(), Some("unsafe"));
        assert_eq!(TokenKind::Trusted.keyword_str(), Some("trusted"));
        // Metadata stays consistent with the reserved-word spelling.
        assert!(TokenKind::Ptr.is_keyword());
        assert!(TokenKind::Addr.is_keyword());
        assert!(TokenKind::Unsafe.is_keyword());
        assert!(TokenKind::Trusted.is_keyword());
        assert_eq!(TokenKind::Ptr.to_string(), "ptr");
        assert_eq!(TokenKind::Addr.to_string(), "addr");
        assert_eq!(TokenKind::Unsafe.to_string(), "unsafe");
        assert_eq!(TokenKind::Trusted.to_string(), "trusted");
    }

    #[test]
    fn tokenize_ptr_keyword_prefix_identifiers() {
        // 0.0.5 keywords must keep from swallowing longer identifiers that
        // merely start with them (`ptr_count` etc. stay identifiers), and
        // keyword spellings are case-sensitive (`Ptr` stays an identifier,
        // like `Result` vs `result`).
        let tokens = tokenize("ptrx addr_of unsafe_id trusted1 Ptr ADDR Unsafe Trusted").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "ptrx"));
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "addr_of"));
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "unsafe_id"));
        assert!(matches!(tokens[3].kind, TokenKind::Ident(ref s) if s == "trusted1"));
        assert!(matches!(tokens[4].kind, TokenKind::Ident(ref s) if s == "Ptr"));
        assert!(matches!(tokens[5].kind, TokenKind::Ident(ref s) if s == "ADDR"));
        assert!(matches!(tokens[6].kind, TokenKind::Ident(ref s) if s == "Unsafe"));
        assert!(matches!(tokens[7].kind, TokenKind::Ident(ref s) if s == "Trusted"));
    }

    #[test]
    fn tokenize_enum_keyword() {
        // 0.0.6: `enum` is a reserved word; it no longer lexes as an identifier.
        let tokens = tokenize("enum").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Enum);
        // Metadata stays consistent with the reserved-word spelling. The
        // to_string assertion is load-bearing: `Display` renders keywords via
        // `keyword_str`, and a keyword variant missing there hits an
        // `unreachable!` panic on user-error paths (the catch-all `other`
        // arm makes the omission invisible to the compiler).
        assert_eq!(TokenKind::Enum.keyword_str(), Some("enum"));
        assert!(TokenKind::Enum.is_keyword());
        assert_eq!(TokenKind::Enum.to_string(), "enum");
    }

    #[test]
    fn tokenize_break_continue_keywords() {
        // 0.0.9 T03 (D4): `break` / `continue` are reserved words; they no
        // longer lex as identifiers (the source breaking set of 0.0.9).
        let tokens = tokenize("break continue").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Break);
        assert_eq!(tokens[1].kind, TokenKind::Continue);
        // Same metadata consistency contract as the `enum` test: `Display`
        // renders keywords via `keyword_str`, and a keyword variant missing
        // there hits an `unreachable!` panic on user-error paths.
        assert_eq!(TokenKind::Break.keyword_str(), Some("break"));
        assert_eq!(TokenKind::Continue.keyword_str(), Some("continue"));
        assert!(TokenKind::Break.is_keyword());
        assert!(TokenKind::Continue.is_keyword());
        assert_eq!(TokenKind::Break.to_string(), "break");
        assert_eq!(TokenKind::Continue.to_string(), "continue");
    }

    #[test]
    fn tokenize_break_continue_prefix_identifiers() {
        // The keywords must not swallow longer identifiers that merely start
        // with them (`breakx`, `continue_2` stay identifiers; the loop-jump
        // words are case-sensitive like every other keyword spelling).
        let tokens = tokenize("breaks breakx continue_2 ContinueX break continue").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "breaks"));
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "breakx"));
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "continue_2"));
        assert!(matches!(tokens[3].kind, TokenKind::Ident(ref s) if s == "ContinueX"));
        assert_eq!(tokens[4].kind, TokenKind::Break);
        assert_eq!(tokens[5].kind, TokenKind::Continue);
    }

    #[test]
    fn tokenize_enum_keyword_prefix_identifiers() {
        // The keyword must not swallow longer identifiers that merely start
        // with it, and keyword spellings are case-sensitive (`Enum` stays an
        // identifier, like `Result` vs `result`).
        let tokens = tokenize("enums enum_id EnumX enumx Enum").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "enums"));
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "enum_id"));
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "EnumX"));
        assert!(matches!(tokens[3].kind, TokenKind::Ident(ref s) if s == "enumx"));
        assert!(matches!(tokens[4].kind, TokenKind::Ident(ref s) if s == "Enum"));
    }

    #[test]
    fn tokenize_as_keyword() {
        let tokens = tokenize("as").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::As);
        assert_eq!(tokens[1].kind, TokenKind::Eof);
    }

    #[test]
    fn tokenize_question() {
        let tokens = tokenize("?").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Question);
        assert_eq!(tokens[1].kind, TokenKind::Eof);
    }

    #[test]
    fn tokenize_hash() {
        // 0.0.8 T01: `#` is a token, unconditionally, in every file.
        let tokens = tokenize("#").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Hash);
        assert_eq!(tokens[1].kind, TokenKind::Eof);
    }

    #[test]
    fn tokenize_feature_directive_tokens() {
        // `#feature curry` lexes as Hash + two identifiers: `feature` is NOT
        // a keyword (it stays a legal binding name elsewhere), and `curry`
        // is a feature *name*, not a reserved word.
        let tokens = tokenize("#feature curry").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Hash);
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "feature"));
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "curry"));
        assert_eq!(tokens[3].kind, TokenKind::Eof);
    }

    #[test]
    fn hash_is_not_a_comment_and_comments_are_unchanged() {
        // `#` never starts a comment; the `//` / `/* */` syntax is untouched.
        let tokens = tokenize("x = 1 // note\n#feature curry\ny = 2 /* mid */ + 3").unwrap();
        // The line comment ends at the newline (preserved for ASI); the `#`
        // line lexes as Hash + idents; the block comment vanishes.
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Hash));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Newline));
        let kinds: Vec<&TokenKind> = tokens.iter().map(|t| &t.kind).collect();
        assert!(kinds.contains(&&TokenKind::Plus));
    }

    #[test]
    fn hash_is_not_a_keyword() {
        assert!(!TokenKind::Hash.is_keyword());
        assert_eq!(TokenKind::Hash.keyword_str(), None);
        assert_eq!(TokenKind::Hash.to_string(), "#");
    }

    #[test]
    fn tokenize_question_adjacent_to_expr() {
        // `x?` is the typical `?` suffix form (0.0.2 Result propagation)
        let tokens = tokenize("x?").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "x"));
        assert_eq!(tokens[1].kind, TokenKind::Question);
        assert_eq!(tokens[2].kind, TokenKind::Eof);
    }

    #[test]
    fn tokenize_double_question() {
        // `??` lexes as two separate tokens (chained `?` on nested Result)
        let tokens = tokenize("f(x)??").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "f"));
        assert_eq!(tokens[1].kind, TokenKind::LParen);
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "x"));
        assert_eq!(tokens[3].kind, TokenKind::RParen);
        assert_eq!(tokens[4].kind, TokenKind::Question);
        assert_eq!(tokens[5].kind, TokenKind::Question);
        assert_eq!(tokens[6].kind, TokenKind::Eof);
    }

    #[test]
    fn tokenize_keyword_prefix_identifiers() {
        // Keywords must not swallow longer identifiers that start with them
        let tokens = tokenize("moved clone_x deref2 moveit as_x asb").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "moved"));
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "clone_x"));
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "deref2"));
        assert!(matches!(tokens[3].kind, TokenKind::Ident(ref s) if s == "moveit"));
        assert!(matches!(tokens[4].kind, TokenKind::Ident(ref s) if s == "as_x"));
        assert!(matches!(tokens[5].kind, TokenKind::Ident(ref s) if s == "asb"));
    }

    #[test]
    fn tokenize_map_keyword() {
        // 0.0.11 T01: `map` is a reserved word (D1), lexing to TokenKind::Map.
        let tokens = tokenize("map").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Map);
        assert_eq!(tokens[1].kind, TokenKind::Eof);
        // Longer identifiers starting with "map" stay identifiers.
        let tokens2 = tokenize("mapped map_x mapx").unwrap();
        assert!(matches!(tokens2[0].kind, TokenKind::Ident(ref s) if s == "mapped"));
        assert!(matches!(tokens2[1].kind, TokenKind::Ident(ref s) if s == "map_x"));
        assert!(matches!(tokens2[2].kind, TokenKind::Ident(ref s) if s == "mapx"));
    }

    #[test]
    fn map_keyword_metadata() {
        assert!(TokenKind::Map.is_keyword());
        assert_eq!(TokenKind::Map.keyword_str(), Some("map"));
        assert_eq!(TokenKind::Map.to_string(), "map");
    }

    #[test]
    fn ownership_keyword_metadata() {
        assert!(TokenKind::Move.is_keyword());
        assert!(TokenKind::Clone.is_keyword());
        assert!(TokenKind::Deref.is_keyword());
        assert!(TokenKind::As.is_keyword());
        // `?` is punctuation, not a keyword
        assert!(!TokenKind::Question.is_keyword());
        assert_eq!(TokenKind::Move.keyword_str(), Some("move"));
        assert_eq!(TokenKind::Clone.keyword_str(), Some("clone"));
        assert_eq!(TokenKind::Deref.keyword_str(), Some("deref"));
        assert_eq!(TokenKind::As.keyword_str(), Some("as"));
        assert_eq!(TokenKind::Question.keyword_str(), None);
        assert_eq!(TokenKind::Move.to_string(), "move");
        assert_eq!(TokenKind::Clone.to_string(), "clone");
        assert_eq!(TokenKind::Deref.to_string(), "deref");
        assert_eq!(TokenKind::As.to_string(), "as");
        assert_eq!(TokenKind::Question.to_string(), "?");
    }

    #[test]
    fn tokenize_identifiers() {
        let tokens = tokenize("foo bar baz123 _private").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "foo"));
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "bar"));
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "baz123"));
        assert!(matches!(tokens[3].kind, TokenKind::Ident(ref s) if s == "_private"));
    }

    #[test]
    fn tokenize_int_literals() {
        let tokens = tokenize("0 42 123456789").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::IntLit(0));
        assert_eq!(tokens[1].kind, TokenKind::IntLit(42));
        assert_eq!(tokens[2].kind, TokenKind::IntLit(123456789));
    }

    #[test]
    fn tokenize_float_literals() {
        let tokens = tokenize("0.0 3.1 123.456").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::FloatLit(0.0));
        assert!(matches!(tokens[1].kind, TokenKind::FloatLit(f) if (f - 3.1f64).abs() < 0.005));
        assert_eq!(tokens[2].kind, TokenKind::FloatLit(123.456));
    }

    #[test]
    fn tokenize_string_literals() {
        let tokens = tokenize(r#""hello" "world""#).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLit("hello".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::StringLit("world".to_string()));
    }

    #[test]
    fn tokenize_string_escapes() {
        let tokens = tokenize(r#""hello\nworld\t!""#).unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::StringLit("hello\nworld\t!".to_string())
        );
    }

    #[test]
    fn tokenize_comments() {
        let tokens = tokenize("foo // comment\nbar").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "foo"));
        // 0.0.2: line-comment-terminating newline is preserved for the ASI pass
        assert_eq!(tokens[1].kind, TokenKind::Newline);
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "bar"));
    }

    #[test]
    fn tokenize_multiline_comments() {
        let tokens = tokenize("foo /* comment */ bar").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "foo"));
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "bar"));
    }

    #[test]
    fn newline_not_emitted_at_file_start() {
        // ASI contract (ASI.md §2): newlines at the start of the source
        // produce no Newline token.
        let tokens = tokenize("\n\n  foo").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "foo"));
        assert!(!tokens.iter().any(|t| t.kind == TokenKind::Newline));
    }

    #[test]
    fn newline_collapses_blank_lines() {
        // Runs of whitespace/blank lines collapse into a single Newline
        // (ASI.md §2).
        let tokens = tokenize("foo\n\n  \nbar").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Newline);
        assert_eq!(tokens[2].kind, TokenKind::Ident("bar".to_string()));
    }

    #[test]
    fn crlf_is_single_newline() {
        // `\r\n` counts as a single newline (ASI.md §2).
        let tokens = tokenize("foo\r\nbar").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Newline);
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "bar"));
    }

    #[test]
    fn newline_inside_block_comment_not_emitted() {
        // Newlines inside block comments are not emitted; ones at the end of
        // line comments still are (see tokenize_comments).
        let tokens = tokenize("foo /* a\nb */ bar").unwrap();
        assert!(!tokens.iter().any(|t| t.kind == TokenKind::Newline));
    }

    #[test]
    fn tokenize_complex() {
        let source = r#"
func fib(n: int): int {
    if n < 2 {
        n
    } elif n == 2 {
        1
    } else {
        fib(n - 1) + fib(n - 2)
    }
}
"#;
        let tokens = tokenize(source).unwrap();
        // Just verify it tokenizes without errors
        assert!(!tokens.is_empty());
        assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
    }

    #[test]
    fn error_invalid_char() {
        let result = tokenize("@");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0].kind,
            crate::lexer::error::LexErrorKind::InvalidChar('@')
        ));
    }

    #[test]
    fn error_unterminated_string() {
        let result = tokenize(r#""hello"#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0].kind,
            crate::lexer::error::LexErrorKind::UnterminatedString
        ));
    }

    #[test]
    fn error_invalid_escape() {
        let result = tokenize(r#""hello\xworld""#);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        // After invalid escape, lexer continues and also reports unterminated string
        assert_eq!(errors.len(), 2);
        assert!(matches!(
            errors[0].kind,
            crate::lexer::error::LexErrorKind::InvalidEscape('x')
        ));
        assert!(matches!(
            errors[1].kind,
            crate::lexer::error::LexErrorKind::UnterminatedString
        ));
    }

    #[test]
    fn error_unterminated_comment() {
        let result = tokenize("foo /* unterminated");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0].kind,
            crate::lexer::error::LexErrorKind::UnterminatedComment
        ));
    }

    #[test]
    fn spans_carry_the_lexed_file_id() {
        // Same source, two file ids: offsets identical, file identity
        // follows the id passed to the lexer (0.0.3 V01).
        let a = tokenize_in_file(FileId(0), "x = 1;").unwrap();
        let b = tokenize_in_file(FileId(2), "x = 1;").unwrap();
        assert_eq!(a.len(), b.len());
        for (ta, tb) in a.iter().zip(&b) {
            assert_eq!(ta.span.file, FileId(0));
            assert_eq!(tb.span.file, FileId(2));
            assert_eq!((ta.span.start, ta.span.end), (tb.span.start, tb.span.end));
        }
    }

    #[test]
    fn lex_error_span_carries_the_file_id() {
        let errors = tokenize_in_file(FileId(3), "@").unwrap_err();
        assert_eq!(errors[0].span.file, FileId(3));
    }

    #[test]
    fn float_literal_with_dot() {
        // "1." should be parsed as IntLit(1) + Dot (property access, not incomplete float)
        let tokens = tokenize("1.").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::IntLit(1));
        assert_eq!(tokens[1].kind, TokenKind::Dot);
    }

    // ===== 0.0.10 T04: string interpolation + the `\$` escape (D4) =====

    #[test]
    fn dollar_escape_produces_literal_dollar() {
        // `\$` is the one new escape (D4). The literals contain no `${`, so
        // they stay plain StringLit tokens (D6: unchanged compile path).
        let tokens = tokenize(r#""a\$b" "100\$""#).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLit("a$b".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::StringLit("100$".to_string()));
    }

    #[test]
    fn dollar_elsewhere_is_an_ordinary_character() {
        // `$` is special only immediately before `{`; in every other
        // position it is a plain character (D4). `$` at the very end of a
        // string needs no escape either.
        let tokens = tokenize(r#""a$b" "100$" "$ {""#).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::StringLit("a$b".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::StringLit("100$".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::StringLit("$ {".to_string()));
    }

    #[test]
    fn interp_string_parts_shape() {
        // `"a${b}c"` → Lit("a") + Expr(Ident b) + Lit("c"); run tokens and
        // the part span carry absolute file spans. Offsets in `"a${b}c"`:
        // `$`=2, `{`=3, `b`=4, `}`=5 — the part span covers `${` (2) through
        // the matching `}` (6).
        let tokens = tokenize(r#""a${b}c""#).unwrap();
        assert_eq!(tokens.len(), 2); // the string + EOF
        let TokenKind::StringInterp(parts) = &tokens[0].kind else {
            panic!("expected StringInterp, got {:?}", tokens[0].kind)
        };
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], StringPart::Lit("a".to_string()));
        match &parts[1] {
            StringPart::Expr { tokens: run, span } => {
                assert_eq!(run.len(), 1);
                assert!(matches!(run[0].kind, TokenKind::Ident(ref s) if s == "b"));
                assert_eq!((run[0].span.start, run[0].span.end), (4, 5));
                assert_eq!((span.start, span.end), (2, 6));
            }
            other => panic!("expected an Expr part, got {other:?}"),
        }
        assert_eq!(parts[2], StringPart::Lit("c".to_string()));
    }

    #[test]
    fn interp_empty_chunks_are_never_pushed() {
        // `"${x}"` — the interpolation is the only part (no empty leading or
        // trailing literal chunk), and `"${}"` — an empty run is captured by
        // the lexer; the parser (T05) rejects it.
        let tokens = tokenize(r#""${x}""#).unwrap();
        let TokenKind::StringInterp(parts) = &tokens[0].kind else {
            panic!("expected StringInterp, got {:?}", tokens[0].kind)
        };
        assert_eq!(parts.len(), 1);

        let tokens = tokenize(r#""${}""#).unwrap();
        let TokenKind::StringInterp(parts) = &tokens[0].kind else {
            panic!("expected StringInterp, got {:?}", tokens[0].kind)
        };
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            StringPart::Expr { tokens: run, span } => {
                assert!(run.is_empty());
                assert_eq!((span.start, span.end), (1, 4));
            }
            other => panic!("expected an Expr part, got {other:?}"),
        }
    }

    #[test]
    fn interp_nested_braces_track_depth() {
        // A `{` inside the embedded expression opens a brace level; only the
        // matching top-level `}` closes the interpolation. `"${ {a} }"`:
        // `$`=1, `{`=2, ` {`=4, `a`=5, `}`=6, ` }`=8 — the part span ends
        // after the depth-0 `}` at 9.
        let tokens = tokenize(r#""${ {a} }""#).unwrap();
        let TokenKind::StringInterp(parts) = &tokens[0].kind else {
            panic!("expected StringInterp, got {:?}", tokens[0].kind)
        };
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            StringPart::Expr { tokens: run, span } => {
                assert_eq!(run.len(), 3); // `{` `a` `}`
                assert_eq!(run[0].kind, TokenKind::LBrace);
                assert!(matches!(run[1].kind, TokenKind::Ident(ref s) if s == "a"));
                assert_eq!(run[2].kind, TokenKind::RBrace);
                assert_eq!((span.start, span.end), (1, 9));
            }
            other => panic!("expected an Expr part, got {other:?}"),
        }

        // A `{` *after* the interpolation is an ordinary literal chunk.
        let tokens = tokenize(r#""${x}{""#).unwrap();
        let TokenKind::StringInterp(parts) = &tokens[0].kind else {
            panic!("expected StringInterp, got {:?}", tokens[0].kind)
        };
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], StringPart::Expr { .. }));
        assert_eq!(parts[1], StringPart::Lit("{".to_string()));
    }

    #[test]
    fn interp_nested_string_literal() {
        // Recursive scanning (D4): a nested string literal — itself
        // interpolated — is one token in the outer run, and its braces
        // never leak into the outer brace-depth tracking. `"a${ "x${1}" }b"`:
        // the inner string spans 5..12, the outer part 2..14.
        let tokens = tokenize(r#""a${ "x${1}" }b""#).unwrap();
        let TokenKind::StringInterp(parts) = &tokens[0].kind else {
            panic!("expected StringInterp, got {:?}", tokens[0].kind)
        };
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], StringPart::Lit("a".to_string()));
        match &parts[1] {
            StringPart::Expr { tokens: run, span } => {
                assert_eq!(run.len(), 1);
                let TokenKind::StringInterp(inner) = &run[0].kind else {
                    panic!("expected a nested StringInterp, got {:?}", run[0].kind)
                };
                assert_eq!(inner.len(), 2);
                assert_eq!(inner[0], StringPart::Lit("x".to_string()));
                match &inner[1] {
                    StringPart::Expr {
                        tokens: inner_run, ..
                    } => {
                        assert_eq!(inner_run.len(), 1);
                        assert_eq!(inner_run[0].kind, TokenKind::IntLit(1));
                    }
                    other => panic!("expected an Expr part, got {other:?}"),
                }
                assert_eq!((run[0].span.start, run[0].span.end), (5, 12));
                assert_eq!((span.start, span.end), (2, 14));
            }
            other => panic!("expected an Expr part, got {other:?}"),
        }
        assert_eq!(parts[2], StringPart::Lit("b".to_string()));
    }

    #[test]
    fn interp_free_string_stays_plain_string_lit() {
        // D6: a literal without `${` keeps the pre-0.0.10 token (and
        // therefore the byte-identical compile path).
        let tokens = tokenize(r#""hello\nworld\t!""#).unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::StringLit("hello\nworld\t!".to_string())
        );
    }

    #[test]
    fn interp_part_tokens_carry_the_file_id() {
        // "tokens carry absolute file spans" (T04): run tokens and the part
        // span are stamped with the lexed file (0.0.3 V01).
        let tokens = tokenize_in_file(FileId(5), r#""${x}""#).unwrap();
        let TokenKind::StringInterp(parts) = &tokens[0].kind else {
            panic!("expected StringInterp, got {:?}", tokens[0].kind)
        };
        match &parts[0] {
            StringPart::Expr { tokens: run, span } => {
                assert_eq!(run[0].span.file, FileId(5));
                assert_eq!(span.file, FileId(5));
            }
            other => panic!("expected an Expr part, got {other:?}"),
        }
    }

    #[test]
    fn error_unclosed_interpolation_at_eof() {
        // EOF inside the embedded expression (no matching `}`) is the new
        // lexer error, with a span covering the `${` through EOF.
        let errors = tokenize(r#""${abc"#).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0].kind,
            crate::lexer::error::LexErrorKind::UnclosedInterpolation
        ));
        assert_eq!((errors[0].span.start, errors[0].span.end), (1, 6));
    }

    #[test]
    fn error_newline_inside_interpolation_is_unterminated_string() {
        // A string literal is a single-line construct — a newline inside the
        // embedded expression terminates it (unterminated-string handling
        // unchanged, T04).
        let errors = tokenize("\"${a\nb}\"").unwrap_err();
        assert!(matches!(
            errors[0].kind,
            crate::lexer::error::LexErrorKind::UnterminatedString
        ));
    }

    #[test]
    fn error_backslash_at_eof_unchanged() {
        // `\` at EOF keeps the invalid-escape('\0') path — T04 changes
        // nothing there.
        let errors = tokenize("\"abc\\").unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0].kind,
            crate::lexer::error::LexErrorKind::InvalidEscape('\0')
        ));
    }
}
