use super::{ParseError, ParserOptions, Result, Sexp, SymbolPool};
use sxp_core::{Error as LexerError, ReadResult, Reader, TokenKind};

/// Parses in-memory S-expressions using `sxp-core`'s reader.
pub struct SexpParser {
    pool: SymbolPool,
    lists: Vec<Vec<Sexp>>,
    text: String,
    typed_atoms: bool,
    typed_booleans: bool,
}

impl SexpParser {
    pub fn new() -> Self {
        Self::with_options(ParserOptions::default())
    }

    pub fn with_options(options: ParserOptions) -> Self {
        let pool = options
            .max_symbol_entries
            .map(SymbolPool::with_max_entries)
            .unwrap_or_default();
        Self {
            pool,
            lists: Vec::new(),
            text: String::new(),
            typed_atoms: options.typed_atoms,
            typed_booleans: options.typed_booleans,
        }
    }

    pub fn parse(&mut self, input: &str) -> Result<Sexp> {
        let Self {
            pool,
            lists,
            text,
            typed_atoms,
            typed_booleans,
        } = self;
        lists.clear();

        let bytes = input.as_bytes();
        let mut lexer = Reader::new();
        let mut offset = 0;
        let mut root = None;
        loop {
            let (result, consumed) = lexer.read(&bytes[offset..], true);
            offset += consumed;

            match result {
                ReadResult::Token(token) => {
                    if root.is_some() {
                        return Err(ParseError::new("unexpected trailing input", token.start));
                    }
                    match token.kind {
                        TokenKind::Open => lists.push(Vec::new()),
                        TokenKind::Close => {
                            let Some(values) = lists.pop() else {
                                return Err(ParseError::new(
                                    "unexpected closing parenthesis",
                                    token.start,
                                ));
                            };
                            let value = Sexp::list(values);
                            if let Some(parent) = lists.last_mut() {
                                parent.push(value);
                            } else {
                                root = Some(value);
                            }
                        }
                        TokenKind::Atom => {
                            let value = lower_atom(
                                pool,
                                &input[token.start..token.end],
                                *typed_atoms,
                                *typed_booleans,
                            );
                            if let Some(parent) = lists.last_mut() {
                                parent.push(value);
                            } else {
                                root = Some(value);
                            }
                        }
                        TokenKind::AtomStart | TokenKind::AtomEnd => {
                            return Err(ParseError::new("incomplete atom", token.start));
                        }
                        TokenKind::StringStart => {
                            text.clear();
                        }
                        TokenKind::StringText => {
                            text.push_str(&input[token.start..token.end]);
                        }
                        TokenKind::Escape(scalar) | TokenKind::UnicodeEscape(scalar) => {
                            text.push(scalar);
                        }
                        TokenKind::StringEnd => {
                            let value = Sexp::string(std::mem::take(text));
                            if let Some(parent) = lists.last_mut() {
                                parent.push(value);
                            } else {
                                root = Some(value);
                            }
                        }
                    }
                }
                ReadResult::InputEmpty => {
                    return Err(ParseError::new("incomplete input", lexer.position()));
                }
                ReadResult::End => {
                    if !lists.is_empty() {
                        return Err(ParseError::new("unterminated list", input.len()));
                    }
                    return root.ok_or_else(|| {
                        ParseError::new("expected an S-expression", lexer.position())
                    });
                }
                ReadResult::Error(LexerError::UnterminatedString) => {
                    return Err(ParseError::new("unterminated string", lexer.position()));
                }
                ReadResult::Error(LexerError::UnterminatedEscape) => {
                    return Err(ParseError::new(
                        "unterminated escape sequence",
                        lexer.position(),
                    ));
                }
                ReadResult::Error(LexerError::InvalidEscape) => {
                    return Err(ParseError::new("invalid escape sequence", lexer.position()));
                }
                ReadResult::Error(LexerError::ExpectedUnicodeBrace) => {
                    return Err(ParseError::new(
                        "expected `{` after `\\u`",
                        lexer.position(),
                    ));
                }
                ReadResult::Error(LexerError::InvalidUnicodeEscape) => {
                    return Err(ParseError::new("invalid Unicode escape", lexer.position()));
                }
                ReadResult::Error(LexerError::InvalidUnicodeScalar) => {
                    return Err(ParseError::new(
                        "invalid Unicode scalar value",
                        lexer.position(),
                    ));
                }
                ReadResult::Error(LexerError::Failed) => {
                    return Err(ParseError::new("lexer failed", lexer.position()));
                }
            }
        }
    }
}

impl Default for SexpParser {
    fn default() -> Self {
        Self::new()
    }
}

impl super::Parser for SexpParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        SexpParser::parse(self, input)
    }
}

pub(super) fn lower_atom(
    pool: &mut SymbolPool,
    text: &str,
    typed_atoms: bool,
    typed_booleans: bool,
) -> Sexp {
    if typed_atoms || typed_booleans {
        match text {
            "true" => return Sexp::boolean(true),
            "false" => return Sexp::boolean(false),
            _ => {}
        }
    }
    if typed_atoms {
        if text == "nil" {
            return Sexp::nil();
        }
        if let Some(number) = parse_typed_number(text) {
            return number;
        }
    }
    pool.symbol(text)
}

fn parse_typed_number(text: &str) -> Option<Sexp> {
    let bytes = text.as_bytes();
    let mut index = usize::from(bytes.first() == Some(&b'-'));
    if index == bytes.len() {
        return None;
    }

    match bytes[index] {
        b'0' => {
            index += 1;
            if matches!(bytes.get(index), Some(b'0'..=b'9')) {
                return None;
            }
        }
        b'1'..=b'9' => {
            index += 1;
            while matches!(bytes.get(index), Some(b'0'..=b'9')) {
                index += 1;
            }
        }
        _ => return None,
    }

    let mut is_float = false;
    if bytes.get(index) == Some(&b'.') {
        is_float = true;
        index += 1;
        let start = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == start {
            return None;
        }
    }

    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        is_float = true;
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == start {
            return None;
        }
    }

    if index != bytes.len() {
        return None;
    }
    Some(if is_float {
        Sexp::float(text)
    } else {
        Sexp::integer(text)
    })
}

#[cfg(test)]
mod tests {
    use super::{Sexp, SexpParser};
    use crate::ParserOptions;

    #[test]
    fn parser_parses_typed_atoms() {
        let input = "(foo 42 \"a\\n\\u{1f642}\")";
        let options = ParserOptions::default().typed_atoms(true);
        let mut parser = SexpParser::with_options(options);
        let expected = Sexp::list([
            Sexp::symbol("foo"),
            Sexp::integer("42"),
            Sexp::string("a\n\u{1f642}"),
        ]);

        assert_eq!(parser.parse(input).unwrap(), expected);
    }

    #[test]
    fn parser_parses_multiple_unicode_escapes_in_a_string() {
        let value = SexpParser::new().parse(r#""a\u{41}b\u{1f642}c""#).unwrap();

        assert_eq!(value, Sexp::string("aAb\u{1f642}c"));
    }

    #[test]
    fn parser_scans_long_string_spans_and_unicode() {
        let first = "a".repeat(8191);
        let last = "b".repeat(16);
        let input = format!("\"{first}\\n{last}é\"");
        let expected = Sexp::string(format!("{first}\n{last}é"));
        let value = SexpParser::new().parse(&input).unwrap();

        assert_eq!(value, expected);
    }

    #[test]
    fn parser_scans_long_ascii_atoms_and_whitespace() {
        let first = "a".repeat(32);
        let second = "b".repeat(24);
        let input = format!("({first}{}{second})", " ".repeat(40));
        let expected = Sexp::list([Sexp::symbol(first), Sexp::symbol(second)]);

        let actual = SexpParser::new().parse(&input).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn parser_scans_ascii_atom_through_end_of_input() {
        let actual = SexpParser::new().parse("last-atom").unwrap();

        assert_eq!(actual, Sexp::symbol("last-atom"));
    }

    #[test]
    fn parser_uses_ascii_whitespace_and_keeps_non_ascii_whitespace_in_atoms() {
        let expected = Sexp::list([Sexp::symbol("foo"), Sexp::symbol("bar")]);
        assert_eq!(SexpParser::new().parse("(foo\x0bbar)").unwrap(), expected);
        assert_eq!(
            SexpParser::new().parse("(foo\u{a0}bar)").unwrap(),
            Sexp::list([Sexp::symbol("foo\u{a0}bar")])
        );
    }

    #[test]
    fn parser_applies_parser_options() {
        let options = ParserOptions::default().typed_atoms(true);
        let mut parser = SexpParser::with_options(options);
        assert_eq!(
            parser.parse("(true nil 42)").unwrap(),
            Sexp::list([Sexp::boolean(true), Sexp::nil(), Sexp::integer("42"),])
        );
    }

    #[test]
    fn parser_rejects_invalid_inputs() {
        for input in [
            "",
            " \t\n",
            ")",
            "(",
            "\"",
            "\"\\q\"",
            r#""\é""#,
            "\"\\u{}\"",
            "\"\\u{110000}\"",
            r#""\uX""#,
            r#""\u{g}""#,
            r#""\u{1g}""#,
            r#""\u{1""#,
            "foo bar",
        ] {
            assert!(SexpParser::new().parse(input).is_err(), "input: {input:?}");
        }
    }

    #[test]
    fn parser_parses_deep_lists_without_call_stack_recursion() {
        let depth = 30_000;
        let input = format!("{}x{}", "(".repeat(depth), ")".repeat(depth));
        let value = SexpParser::new().parse(&input).unwrap();

        let mut node = &value;
        for _ in 0..depth {
            match node {
                Sexp::List(items) if items.len() == 1 => node = &items[0],
                _ => panic!("unexpected node in nested list"),
            }
        }
        assert_eq!(node, &Sexp::symbol("x"));
        drop_iteratively(value);
    }

    fn drop_iteratively(value: Sexp) {
        let mut pending = vec![value];
        while let Some(value) = pending.pop() {
            if let Sexp::List(items) = value {
                pending.extend(items.into_vec());
            }
        }
    }
}
