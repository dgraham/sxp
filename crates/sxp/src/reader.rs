use super::parser::lower_atom;
use super::{ParseError, ParserOptions, Sexp, SexpReaderError, SexpReaderResult, SymbolPool};
use std::io::{BufRead, BufReader, Read};
use sxp_core::{Error as LexerError, ReadResult, Reader, TokenKind};

struct SpanStringDecoder {
    output: Vec<u8>,
}

/// Reads consecutive S-expressions from a buffered byte stream using
/// `sxp-core::Reader`.
pub struct SexpReader<R: Read> {
    reader: BufReader<R>,
    lexer: Reader,
    atom: Vec<u8>,
    atom_pending: bool,
    string: SpanStringDecoder,
    lists: Vec<Vec<Sexp>>,
    pool: SymbolPool,
    typed_atoms: bool,
    typed_booleans: bool,
    failed: bool,
}

enum ReadStep {
    Continue,
    Expression(Sexp),
    End,
    Error(ParseError),
}

impl<R: Read> SexpReader<R> {
    /// Creates a streaming parser with default options.
    pub fn new(reader: R) -> Self {
        Self::with_options(reader, ParserOptions::default())
    }

    /// Creates a streaming parser with the given options.
    pub fn with_options(reader: R, options: ParserOptions) -> Self {
        let pool = options
            .max_symbol_entries
            .map(SymbolPool::with_max_entries)
            .unwrap_or_default();
        Self {
            reader: BufReader::new(reader),
            lexer: Reader::new(),
            atom: Vec::new(),
            atom_pending: false,
            string: SpanStringDecoder::new(),
            lists: Vec::new(),
            pool,
            typed_atoms: options.typed_atoms,
            typed_booleans: options.typed_booleans,
            failed: false,
        }
    }

    /// Reads the next top-level expression, returning `None` at EOF.
    ///
    /// After a parse or I/O error, the reader is left in an unrecoverable
    /// state and subsequent calls return [`SexpReaderError::Failed`].
    pub fn read_next(&mut self) -> SexpReaderResult<Option<Sexp>> {
        if self.failed {
            return Err(SexpReaderError::Failed);
        }
        self.atom.clear();
        self.atom_pending = false;
        self.string.reset();
        self.lists.clear();

        let result = self.read_next_inner();
        if result.is_err() {
            self.failed = true;
            drop_iteratively(&mut self.lists);
        }
        result
    }

    fn read_next_inner(&mut self) -> SexpReaderResult<Option<Sexp>> {
        let Self {
            reader,
            lexer,
            atom,
            atom_pending,
            string,
            lists,
            pool,
            typed_atoms,
            typed_booleans,
            ..
        } = self;
        loop {
            let (consumed, step) = {
                let input = reader.fill_buf()?;
                let base = lexer.position();
                let was_atom_pending = *atom_pending;
                let (result, consumed) = lexer.read(input, input.is_empty());

                if was_atom_pending {
                    atom.extend_from_slice(&input[..consumed]);
                }

                let step = match result {
                    ReadResult::Token(token) => match token.kind {
                        TokenKind::Open => {
                            lists.push(Vec::new());
                            ReadStep::Continue
                        }
                        TokenKind::Close => {
                            if let Some(values) = lists.pop() {
                                let value = Sexp::list(values);
                                if let Some(parent) = lists.last_mut() {
                                    parent.push(value);
                                    ReadStep::Continue
                                } else {
                                    ReadStep::Expression(value)
                                }
                            } else {
                                ReadStep::Error(ParseError::new(
                                    "unexpected closing parenthesis",
                                    token.start,
                                ))
                            }
                        }
                        TokenKind::Atom => {
                            let start = token.start.saturating_sub(base);
                            let end = token.end.saturating_sub(base);
                            match lower_atom_bytes(
                                pool,
                                &input[start..end],
                                token.start,
                                *typed_atoms,
                                *typed_booleans,
                            ) {
                                Ok(value) => {
                                    if let Some(parent) = lists.last_mut() {
                                        parent.push(value);
                                        ReadStep::Continue
                                    } else {
                                        ReadStep::Expression(value)
                                    }
                                }
                                Err(error) => ReadStep::Error(error),
                            }
                        }
                        TokenKind::AtomStart => {
                            let start = token.start.saturating_sub(base);
                            let end = token.end.saturating_sub(base);
                            atom.extend_from_slice(&input[start..end]);
                            *atom_pending = true;
                            ReadStep::Continue
                        }
                        TokenKind::AtomEnd => {
                            let offset = lexer.position().saturating_sub(atom.len());
                            match lower_atom_bytes(
                                pool,
                                atom,
                                offset,
                                *typed_atoms,
                                *typed_booleans,
                            ) {
                                Ok(value) => {
                                    atom.clear();
                                    *atom_pending = false;
                                    if let Some(parent) = lists.last_mut() {
                                        parent.push(value);
                                        ReadStep::Continue
                                    } else {
                                        ReadStep::Expression(value)
                                    }
                                }
                                Err(error) => ReadStep::Error(error),
                            }
                        }
                        TokenKind::StringStart => {
                            string.reset();
                            ReadStep::Continue
                        }
                        TokenKind::StringText => {
                            let start = token.start.saturating_sub(base);
                            let end = token.end.saturating_sub(base);
                            string.append_text(&input[start..end]);
                            ReadStep::Continue
                        }
                        TokenKind::Escape(scalar) | TokenKind::UnicodeEscape(scalar) => {
                            string.append_char(scalar);
                            ReadStep::Continue
                        }
                        TokenKind::StringEnd => match string.finish(token.end) {
                            Ok(text) => {
                                let value = Sexp::string(text);
                                string.output.clear();
                                if let Some(parent) = lists.last_mut() {
                                    parent.push(value);
                                    ReadStep::Continue
                                } else {
                                    ReadStep::Expression(value)
                                }
                            }
                            Err(error) => ReadStep::Error(error),
                        },
                    },
                    ReadResult::InputEmpty => ReadStep::Continue,
                    ReadResult::End => {
                        if lists.is_empty() {
                            ReadStep::End
                        } else {
                            ReadStep::Error(ParseError::new("unterminated list", lexer.position()))
                        }
                    }
                    ReadResult::Error(error) => {
                        let parse_error = match error {
                            LexerError::UnterminatedString | LexerError::UnterminatedEscape => {
                                string.error(
                                    lexer.position(),
                                    error == LexerError::UnterminatedEscape,
                                )
                            }
                            LexerError::InvalidEscape => {
                                ParseError::new("invalid escape sequence", lexer.position())
                            }
                            LexerError::ExpectedUnicodeBrace => {
                                ParseError::new("expected `{` after `\\u`", lexer.position())
                            }
                            LexerError::InvalidUnicodeEscape => {
                                ParseError::new("invalid Unicode escape", lexer.position())
                            }
                            LexerError::InvalidUnicodeScalar => {
                                ParseError::new("invalid Unicode scalar value", lexer.position())
                            }
                            LexerError::Failed => ParseError::new("lexer failed", lexer.position()),
                        };
                        ReadStep::Error(parse_error)
                    }
                };
                (consumed, step)
            };
            reader.consume(consumed);
            match step {
                ReadStep::Continue => {}
                ReadStep::Expression(value) => return Ok(Some(value)),
                ReadStep::End => return Ok(None),
                ReadStep::Error(error) => return Err(error.into()),
            }
        }
    }
}

fn drop_iteratively(lists: &mut Vec<Vec<Sexp>>) {
    let mut pending = Vec::new();
    for values in lists.drain(..) {
        pending.extend(values);
    }
    while let Some(value) = pending.pop() {
        if let Sexp::List(values) = value {
            pending.extend(values.into_vec());
        }
    }
}

fn lower_atom_bytes(
    pool: &mut SymbolPool,
    bytes: &[u8],
    offset: usize,
    typed_atoms: bool,
    typed_booleans: bool,
) -> Result<Sexp, ParseError> {
    let text = if bytes.is_ascii() {
        // SAFETY: The `is_ascii()` check guarantees these bytes are valid UTF-8.
        unsafe { std::str::from_utf8_unchecked(bytes) }
    } else {
        std::str::from_utf8(bytes).map_err(|error| {
            ParseError::new("invalid UTF-8 in atom", offset + error.valid_up_to())
        })?
    };
    Ok(lower_atom(pool, text, typed_atoms, typed_booleans))
}

impl SpanStringDecoder {
    fn new() -> Self {
        Self { output: Vec::new() }
    }

    fn reset(&mut self) {
        self.output.clear();
    }

    fn append_text(&mut self, bytes: &[u8]) {
        self.output.extend_from_slice(bytes);
    }

    fn append_char(&mut self, ch: char) {
        let mut encoded = [0; 4];
        self.output
            .extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
    }

    fn finish(&self, offset: usize) -> Result<&str, ParseError> {
        if self.output.is_ascii() {
            // SAFETY: The `is_ascii()` check guarantees these bytes are valid UTF-8.
            Ok(unsafe { std::str::from_utf8_unchecked(&self.output) })
        } else {
            std::str::from_utf8(&self.output)
                .map_err(|_| ParseError::new("invalid UTF-8 in string", offset.saturating_sub(1)))
        }
    }

    fn error(&self, offset: usize, escape_pending: bool) -> ParseError {
        if escape_pending {
            ParseError::new("unterminated escape sequence", offset)
        } else {
            ParseError::new("unterminated string", offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SexpReader;
    use crate::{Atom, ParserOptions, Sexp, SexpParser, SexpReaderError};
    use std::io::{self, Cursor, Read};
    use std::rc::Rc;

    struct ChunkedReader<R> {
        reader: R,
        chunk_size: usize,
    }

    impl<R: Read> Read for ChunkedReader<R> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let length = buffer.len().min(self.chunk_size);
            self.reader.read(&mut buffer[..length])
        }
    }

    struct FailingReader {
        bytes: Cursor<&'static [u8]>,
    }

    impl Read for FailingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let read = self.bytes.read(buffer)?;
            if read == 0 {
                Err(io::Error::other("reader failed"))
            } else {
                Ok(read)
            }
        }
    }

    struct InterruptOnceReader<R> {
        reader: R,
        interrupted: bool,
    }

    impl<R: Read> Read for InterruptOnceReader<R> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.reader.read(buffer)
        }
    }

    #[test]
    fn reader_parses_consecutive_expressions_across_single_byte_chunks() {
        let input = "(alpha 123) \"a\\n\\u{41}\" tail";
        let chunked = ChunkedReader {
            reader: Cursor::new(input.as_bytes()),
            chunk_size: 1,
        };
        let mut reader = SexpReader::new(chunked);

        assert_eq!(
            reader.read_next().unwrap(),
            Some(Sexp::list([Sexp::symbol("alpha"), Sexp::symbol("123")]))
        );
        assert_eq!(reader.read_next().unwrap(), Some(Sexp::string("a\nA")));
        assert_eq!(reader.read_next().unwrap(), Some(Sexp::symbol("tail")));
        assert_eq!(reader.read_next().unwrap(), None);
    }

    #[test]
    fn reader_preserves_utf8_split_across_string_chunks() {
        let input = "\"café 雪\"";
        let chunked = ChunkedReader {
            reader: Cursor::new(input.as_bytes()),
            chunk_size: 1,
        };
        let mut reader = SexpReader::new(chunked);

        assert_eq!(reader.read_next().unwrap(), Some(Sexp::string("café 雪")));
    }

    #[test]
    fn reader_matches_in_memory_parser() {
        let inputs = [
            "(foo (bar 42) \"a\\\"b\\n\\u{1f642}\")",
            "\"plain café\"",
            "(café 雪)",
        ];
        let input = inputs.join("\n");
        let mut reader = SexpReader::new(Cursor::new(input.as_bytes()));
        let mut parser = SexpParser::new();
        for input in inputs {
            assert_eq!(
                parser.parse(input).unwrap(),
                reader.read_next().unwrap().unwrap(),
                "input: {input:?}",
            );
        }
        assert!(reader.read_next().unwrap().is_none());
    }

    #[test]
    fn reader_keeps_non_ascii_whitespace_in_atoms_across_small_chunks() {
        let input = " \u{a0}abé🦀\u{a0}bar";
        for chunk_size in [1, 2, 3] {
            let chunked = ChunkedReader {
                reader: Cursor::new(input.as_bytes()),
                chunk_size,
            };
            let mut reader = SexpReader::new(chunked);

            assert_eq!(
                reader.read_next().unwrap(),
                Some(Sexp::symbol("\u{a0}abé🦀\u{a0}bar"))
            );
            assert_eq!(reader.read_next().unwrap(), None);
        }
    }

    #[test]
    fn reader_rejects_truncated_utf8_atom_across_single_byte_chunks() {
        let chunked = ChunkedReader {
            reader: Cursor::new(b"foo\xc3".as_slice()),
            chunk_size: 1,
        };
        let mut reader = SexpReader::new(chunked);

        assert!(matches!(reader.read_next(), Err(SexpReaderError::Parse(_))));
    }

    #[test]
    fn reader_rejects_non_ascii_escape_across_single_byte_chunks() {
        let chunked = ChunkedReader {
            reader: Cursor::new("\"\\é\"".as_bytes()),
            chunk_size: 1,
        };
        let mut reader = SexpReader::new(chunked);

        assert!(matches!(reader.read_next(), Err(SexpReaderError::Parse(_))));
    }

    #[test]
    fn reader_applies_options_and_retains_capped_symbols() {
        let options = ParserOptions::default()
            .typed_atoms(true)
            .max_symbol_entries(Some(1));
        let mut reader = SexpReader::with_options(Cursor::new("true 42 foo foo bar"), options);

        assert_eq!(reader.read_next().unwrap(), Some(Sexp::boolean(true)));
        assert_eq!(reader.read_next().unwrap(), Some(Sexp::integer("42")));
        let symbols = (0..3)
            .map(|_| match reader.read_next().unwrap().unwrap() {
                Sexp::Atom(Atom::Symbol(symbol)) => symbol,
                _ => panic!("expected a symbol"),
            })
            .collect::<Vec<_>>();
        assert!(Rc::ptr_eq(&symbols[0], &symbols[1]));
        assert!(!Rc::ptr_eq(&symbols[1], &symbols[2]));
    }

    #[test]
    fn reader_rejects_invalid_input_and_cannot_continue_after_errors() {
        for input in [
            ")",
            "\"unterminated",
            "\"bad\\q\"",
            "\"\\uX\"",
            "\"\\u{g}\"",
            "\"\\u{110000}\"",
        ] {
            let mut reader = SexpReader::new(Cursor::new(input));
            assert!(matches!(reader.read_next(), Err(SexpReaderError::Parse(_))));
            assert!(matches!(reader.read_next(), Err(SexpReaderError::Failed)));
        }

        let mut reader = SexpReader::new(Cursor::new(b"\"x\xe2\"".as_slice()));
        assert!(matches!(reader.read_next(), Err(SexpReaderError::Parse(_))));

        let mut reader = SexpReader::new(FailingReader {
            bytes: Cursor::new(b"(foo "),
        });
        assert!(matches!(reader.read_next(), Err(SexpReaderError::Io(_))));
        assert!(matches!(reader.read_next(), Err(SexpReaderError::Failed)));
    }

    #[test]
    fn reader_reports_invalid_utf8() {
        let mut reader = SexpReader::new(Cursor::new(b"abc\xff"));
        let error = reader.read_next().unwrap_err();
        assert!(matches!(
            error,
            SexpReaderError::Parse(ref error) if error.offset() == 3
        ));

        for input in [
            b"\"a\xff\"".as_slice(),
            b"\"\xe2\\u{41}\"".as_slice(),
            b"\"\\u{41}\xff\"".as_slice(),
        ] {
            let mut reader = SexpReader::new(Cursor::new(input));
            assert!(matches!(reader.read_next(), Err(SexpReaderError::Parse(_))));
        }
    }

    #[test]
    fn reader_reports_incomplete_expressions() {
        let mut reader = SexpReader::new(Cursor::new("("));

        assert!(matches!(reader.read_next(), Err(SexpReaderError::Parse(_))));
    }

    #[test]
    fn reader_drops_partial_deep_trees_without_recursion() {
        let depth = 10_000;
        let input = format!("{}x{}", "(".repeat(depth), ")".repeat(depth - 1));
        let mut reader = SexpReader::new(Cursor::new(input));

        assert!(matches!(reader.read_next(), Err(SexpReaderError::Parse(_))));
    }

    #[test]
    fn reader_surfaces_interrupted_reads_and_rejects_continuing_after_error() {
        let input = InterruptOnceReader {
            reader: Cursor::new(b"value".as_slice()),
            interrupted: false,
        };
        let mut reader = SexpReader::new(input);

        assert!(matches!(
            reader.read_next(),
            Err(SexpReaderError::Io(ref error))
                if error.kind() == io::ErrorKind::Interrupted
        ));
        assert!(matches!(reader.read_next(), Err(SexpReaderError::Failed)));
    }
}
