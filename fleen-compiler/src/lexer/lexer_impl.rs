//! Fleen lexer: converts source code into a stream of tokens.

use crate::lexer::error::LexError;
use crate::lexer::token::{Span, Token, TokenKind};

/// Lexer state for tokenizing Fleen source code.
pub struct Lexer<'a> {
    chars: std::str::Chars<'a>,
    byte_offset: u32,
    char_offset: usize,
    current_char: Option<char>,
    next_char: Option<char>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source code.
    pub fn new(source: &'a str) -> Self {
        let mut chars = source.chars();
        let current_char = chars.next();
        let next_char = chars.next();
        Self {
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
                    Span::new(ws_start, self.byte_offset),
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
            Span::new(self.byte_offset, self.byte_offset),
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
            Span::new(start, start + ch.len_utf8() as u32),
        ))
    }

    /// Create a token with the given kind and length in characters.
    fn make_token(&mut self, kind: TokenKind, char_len: usize) -> Token {
        let start = self.byte_offset;
        for _ in 0..char_len {
            self.advance();
        }
        let end = self.byte_offset;
        Token::new(kind, Span::new(start, end))
    }

    /// Scan a string literal.
    fn scan_string(&mut self, start: u32) -> Result<Token, LexError> {
        // Consume opening quote
        self.advance();

        let mut value = String::new();

        while let Some(ch) = self.current_char {
            if ch == '"' {
                // Closing quote found
                self.advance();
                return Ok(Token::new(
                    TokenKind::StringLit(value),
                    Span::new(start, self.byte_offset),
                ));
            }

            if ch == '\\' {
                self.advance();
                match self.current_char {
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('r') => value.push('\r'),
                    Some('\\') => value.push('\\'),
                    Some('"') => value.push('"'),
                    Some('\'') => value.push('\''),
                    // TODO: Future versions may support additional escapes:
                    // \0 (null), \b (backspace), \f (form feed),
                    // \xHH (hex), \uXXXX (unicode)
                    Some(ch) => {
                        let err_span = Span::new(
                            self.byte_offset - 1,
                            self.byte_offset + ch.len_utf8() as u32,
                        );
                        self.advance();
                        return Err(LexError::invalid_escape(ch, err_span));
                    }
                    None => {
                        let err_span = Span::new(self.byte_offset - 1, self.byte_offset);
                        return Err(LexError::invalid_escape('\0', err_span));
                    }
                }
            } else if ch == '\n' {
                // Unterminated string (newline in string not allowed)
                let err_span = Span::new(start, self.byte_offset);
                return Err(LexError::unterminated_string(err_span));
            } else {
                value.push(ch);
            }

            self.advance();
        }

        // EOF reached without closing quote
        Err(LexError::unterminated_string(Span::new(
            start,
            self.byte_offset,
        )))
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

        let span = Span::new(start, self.byte_offset);

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

        let span = Span::new(start, self.byte_offset);

        // Check for keywords
        let kind = match ident.as_str() {
            "func" => TokenKind::Func,
            "const" => TokenKind::Const,
            "if" => TokenKind::If,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "choose" => TokenKind::Choose,
            "when" => TokenKind::When,
            "otherwise" => TokenKind::Otherwise,
            "import" => TokenKind::Import,
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
                        let err_span = Span::new(comment_start, self.byte_offset);
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

/// Convenience function to tokenize a source string.
pub fn tokenize(source: &str) -> Result<Vec<Token>, Vec<LexError>> {
    let mut lexer = Lexer::new(source);
    lexer.tokenize()
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
            tokenize("func const if elif else while choose when otherwise import").unwrap();
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
        // ASI 契约（ASI.md §2）：源码开头的换行不发 Newline。
        let tokens = tokenize("\n\n  foo").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "foo"));
        assert!(!tokens.iter().any(|t| t.kind == TokenKind::Newline));
    }

    #[test]
    fn newline_collapses_blank_lines() {
        // 连续空白/空行折叠为单个 Newline（ASI.md §2）。
        let tokens = tokenize("foo\n\n  \nbar").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Newline);
        assert_eq!(tokens[2].kind, TokenKind::Ident("bar".to_string()));
    }

    #[test]
    fn crlf_is_single_newline() {
        // `\r\n` 视为一个换行（ASI.md §2）。
        let tokens = tokenize("foo\r\nbar").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Newline);
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "bar"));
    }

    #[test]
    fn newline_inside_block_comment_not_emitted() {
        // 块注释内的换行不发；行注释末尾的照发（见 tokenize_comments）。
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
    fn float_literal_with_dot() {
        // "1." should be parsed as IntLit(1) + Dot (property access, not incomplete float)
        let tokens = tokenize("1.").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::IntLit(1));
        assert_eq!(tokens[1].kind, TokenKind::Dot);
    }
}
