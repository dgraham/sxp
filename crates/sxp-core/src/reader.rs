use memchr::memchr2;
use wide::u8x16;

const SIMD_BYTE_WIDTH: usize = 16;

/// The kind of token produced by [`Reader`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Open,
    Close,
    Atom,
    AtomStart,
    AtomEnd,
    StringStart,
    StringText,
    Escape(char),
    UnicodeEscape(char),
    StringEnd,
}

/// A token's byte range in the concatenated input stream.
///
/// The range covers the token or event and is half-open (`start..end`).
/// `StringStart` and `StringEnd` cover their quote. `StringText` covers raw
/// text. `Escape` includes the decoded character and covers a simple escape.
/// `UnicodeEscape` includes the decoded scalar and covers the complete
/// `\u{...}` escape.
///
/// `Atom` covers a complete atom. A split atom is reported as `AtomStart`
/// followed by `AtomEnd`. `AtomEnd` covers the remaining atom bytes and can
/// be empty when the atom ends at the next input boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

/// Result of one call to [`Reader::read`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadResult {
    InputEmpty,
    Token(Token),
    End,
    Error(Error),
}

/// Lexical errors detected by the range-based reader.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    UnterminatedString,
    UnterminatedEscape,
    InvalidEscape,
    ExpectedUnicodeBrace,
    InvalidUnicodeEscape,
    InvalidUnicodeScalar,
    Failed,
}

/// Accumulates the hex digits of a `\u{...}` escape into a scalar value.
///
/// At most six digits are accepted, matching the escape grammar. Callers pass
/// each digit as a nibble value (`0..=15`) and handle character conversion.
#[derive(Clone, Debug)]
pub struct UnicodeDigits {
    value: u32,
    digits: u8,
}

impl UnicodeDigits {
    /// Creates an empty accumulator.
    pub const fn new() -> Self {
        Self {
            value: 0,
            digits: 0,
        }
    }

    /// Discards all accumulated digits.
    #[inline]
    pub fn clear(&mut self) {
        self.value = 0;
        self.digits = 0;
    }

    /// Returns `true` if no digits have been pushed.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.digits == 0
    }

    /// Returns the accumulated value as a scalar, or `None` if the value is
    /// not a valid Unicode scalar.
    #[inline]
    pub fn as_char(&self) -> Option<char> {
        char::from_u32(self.value)
    }

    /// Returns the nibble value of an ASCII hex character (`0-9`, `a-f`,
    /// `A-F`), or `None` for any other character.
    #[inline]
    pub fn hex_value(source: impl Into<char>) -> Option<u8> {
        let ch = source.into();
        match ch {
            '0'..='9' => Some(ch as u8 - b'0'),
            'a'..='f' => Some(ch as u8 - b'a' + 10),
            'A'..='F' => Some(ch as u8 - b'A' + 10),
            _ => None,
        }
    }

    /// Adds a hex digit (0-15). Returns `false` if six digits have already
    /// been pushed.
    #[inline]
    pub fn push(&mut self, digit: u8) -> bool {
        debug_assert!(digit <= 15, "digit must be a hex nibble");
        if self.digits == 6 {
            return false;
        }
        self.value = (self.value << 4) | digit as u32;
        self.digits += 1;
        true
    }
}

impl Default for UnicodeDigits {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum State {
    #[default]
    Start,
    Atom,
    String,
    Escape,
    UnicodeOpen,
    UnicodeDigits,
    End,
    Failed,
}

/// Incremental, allocation-free lexer emitting source byte ranges.
///
/// Returned Token span positions are absolute offsets from the start
/// of the full input stream, not the current buffer. Pass unconsumed
/// input after `InputEmpty` is returned, indicating the lexer has
/// processed as much of the current input buffer as possible.
#[derive(Clone, Debug, Default)]
pub struct Reader {
    state: State,
    position: usize,
    token_start: usize,
    escape_start: usize,
    digits: UnicodeDigits,
    atom_start_end: Option<usize>,
}

impl Reader {
    /// Creates a lexer at byte position zero.
    pub const fn new() -> Self {
        Self {
            state: State::Start,
            position: 0,
            token_start: 0,
            escape_start: 0,
            digits: UnicodeDigits::new(),
            atom_start_end: None,
        }
    }

    /// Resets the lexer as if it had not consumed any input.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Returns the total number of input bytes consumed.
    #[inline]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Reads at most one token from `input`.
    ///
    /// `eof` must be true only when `input` contains the final available bytes.
    /// The returned count is bytes consumed from this input slice.
    #[inline]
    pub fn read(&mut self, input: &[u8], eof: bool) -> (ReadResult, usize) {
        let mut consumed = 0;

        loop {
            match self.state {
                State::Start => {
                    let whitespace = count_ascii_whitespace(&input[consumed..]);
                    self.advance(&mut consumed, whitespace);

                    let Some(&byte) = input.get(consumed) else {
                        if eof {
                            self.state = State::End;
                            return (ReadResult::End, consumed);
                        }
                        return (ReadResult::InputEmpty, consumed);
                    };
                    self.token_start = self.position;
                    match byte {
                        b'(' => {
                            self.advance(&mut consumed, 1);
                            return (
                                ReadResult::Token(Token {
                                    kind: TokenKind::Open,
                                    start: self.token_start,
                                    end: self.position,
                                }),
                                consumed,
                            );
                        }
                        b')' => {
                            self.advance(&mut consumed, 1);
                            return (
                                ReadResult::Token(Token {
                                    kind: TokenKind::Close,
                                    start: self.token_start,
                                    end: self.position,
                                }),
                                consumed,
                            );
                        }
                        b'"' => {
                            self.advance(&mut consumed, 1);
                            self.state = State::String;
                            return (
                                ReadResult::Token(Token {
                                    kind: TokenKind::StringStart,
                                    start: self.token_start,
                                    end: self.position,
                                }),
                                consumed,
                            );
                        }
                        _ => {
                            self.atom_start_end = None;
                            self.state = State::Atom;
                        }
                    }
                }
                State::Atom => {
                    if consumed == input.len() {
                        if eof {
                            return (self.finish_atom(), consumed);
                        }
                        if self.atom_start_end.is_none() && self.position > self.token_start {
                            return (self.start_atom(), consumed);
                        }
                        return (ReadResult::InputEmpty, consumed);
                    }

                    match scan_atom_run(&input[consumed..]) {
                        AtomScan::Delimiter(length) => {
                            if length > 0 {
                                self.advance(&mut consumed, length);
                            }
                            return (self.finish_atom(), consumed);
                        }
                        AtomScan::End => {
                            let remaining = input.len() - consumed;
                            self.advance(&mut consumed, remaining);
                            if eof {
                                return (self.finish_atom(), consumed);
                            }
                            if self.atom_start_end.is_none() {
                                return (self.start_atom(), consumed);
                            }
                            return (ReadResult::InputEmpty, consumed);
                        }
                    }
                }
                State::String => {
                    let remaining = &input[consumed..];
                    let Some(special) = memchr2(b'"', b'\\', remaining) else {
                        if remaining.is_empty() {
                            if eof {
                                return self.fail(Error::UnterminatedString, consumed);
                            }
                            return (ReadResult::InputEmpty, consumed);
                        }
                        let start = self.position;
                        self.advance(&mut consumed, remaining.len());
                        return (
                            ReadResult::Token(Token {
                                kind: TokenKind::StringText,
                                start,
                                end: self.position,
                            }),
                            consumed,
                        );
                    };
                    if special > 0 {
                        let start = self.position;
                        self.advance(&mut consumed, special);
                        return (
                            ReadResult::Token(Token {
                                kind: TokenKind::StringText,
                                start,
                                end: self.position,
                            }),
                            consumed,
                        );
                    }
                    match input[consumed] {
                        b'"' => {
                            self.token_start = self.position;
                            self.advance(&mut consumed, 1);
                            self.state = State::Start;
                            return (
                                ReadResult::Token(Token {
                                    kind: TokenKind::StringEnd,
                                    start: self.token_start,
                                    end: self.position,
                                }),
                                consumed,
                            );
                        }
                        b'\\' => {
                            self.escape_start = self.position;
                            self.advance(&mut consumed, 1);
                            self.state = State::Escape;
                        }
                        _ => unreachable!(),
                    }
                }
                State::Escape => {
                    if consumed == input.len() {
                        if eof {
                            return self.fail(Error::UnterminatedEscape, consumed);
                        }
                        return (ReadResult::InputEmpty, consumed);
                    }
                    let escaped = match input[consumed] {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            self.advance(&mut consumed, 1);
                            self.state = State::UnicodeOpen;
                            continue;
                        }
                        _ => {
                            self.advance(&mut consumed, 1);
                            return self.fail(Error::InvalidEscape, consumed);
                        }
                    };
                    self.advance(&mut consumed, 1);
                    self.state = State::String;
                    return (
                        ReadResult::Token(Token {
                            kind: TokenKind::Escape(escaped),
                            start: self.escape_start,
                            end: self.position,
                        }),
                        consumed,
                    );
                }
                State::UnicodeOpen => {
                    let Some(&byte) = input.get(consumed) else {
                        if eof {
                            return self.fail(Error::ExpectedUnicodeBrace, consumed);
                        }
                        return (ReadResult::InputEmpty, consumed);
                    };
                    if byte != b'{' {
                        self.advance(&mut consumed, 1);
                        return self.fail(Error::ExpectedUnicodeBrace, consumed);
                    }
                    self.advance(&mut consumed, 1);
                    self.digits.clear();
                    self.state = State::UnicodeDigits;
                }
                State::UnicodeDigits => {
                    let Some(&byte) = input.get(consumed) else {
                        if eof {
                            return self.fail(Error::InvalidUnicodeEscape, consumed);
                        }
                        return (ReadResult::InputEmpty, consumed);
                    };
                    if byte == b'}' {
                        if self.digits.is_empty() {
                            return self.fail(Error::InvalidUnicodeEscape, consumed);
                        }
                        self.advance(&mut consumed, 1);
                        let Some(scalar) = self.digits.as_char() else {
                            return self.fail(Error::InvalidUnicodeScalar, consumed);
                        };
                        self.state = State::String;
                        return (
                            ReadResult::Token(Token {
                                kind: TokenKind::UnicodeEscape(scalar),
                                start: self.escape_start,
                                end: self.position,
                            }),
                            consumed,
                        );
                    }
                    if let Some(digit) = UnicodeDigits::hex_value(byte) {
                        if !self.digits.push(digit) {
                            self.advance(&mut consumed, 1);
                            return self.fail(Error::InvalidUnicodeEscape, consumed);
                        }
                        self.advance(&mut consumed, 1);
                    } else {
                        self.advance(&mut consumed, 1);
                        return self.fail(Error::InvalidUnicodeEscape, consumed);
                    }
                }
                State::End => return (ReadResult::End, consumed),
                State::Failed => {
                    return (ReadResult::Error(Error::Failed), consumed);
                }
            }
        }
    }

    #[inline]
    fn advance(&mut self, consumed: &mut usize, count: usize) {
        *consumed += count;
        self.position = self.position.saturating_add(count);
    }

    #[inline]
    fn start_atom(&mut self) -> ReadResult {
        self.atom_start_end = Some(self.position);
        ReadResult::Token(Token {
            kind: TokenKind::AtomStart,
            start: self.token_start,
            end: self.position,
        })
    }

    #[inline]
    fn finish_atom(&mut self) -> ReadResult {
        self.state = State::Start;
        let token = match self.atom_start_end.take() {
            Some(start) => Token {
                kind: TokenKind::AtomEnd,
                start,
                end: self.position,
            },
            None => Token {
                kind: TokenKind::Atom,
                start: self.token_start,
                end: self.position,
            },
        };
        ReadResult::Token(token)
    }

    fn fail(&mut self, error: Error, consumed: usize) -> (ReadResult, usize) {
        self.state = State::Failed;
        (ReadResult::Error(error), consumed)
    }
}

#[inline]
fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

#[inline]
fn is_delimiter(byte: u8) -> bool {
    is_whitespace(byte) || matches!(byte, b'(' | b')' | b'"')
}

enum AtomScan {
    Delimiter(usize),
    End,
}

fn scan_atom_run(bytes: &[u8]) -> AtomScan {
    let mut index = 0;
    while index < bytes.len().min(SIMD_BYTE_WIDTH) {
        let byte = bytes[index];
        if is_delimiter(byte) {
            return AtomScan::Delimiter(index);
        }
        index += 1;
    }

    while bytes.len() - index >= SIMD_BYTE_WIDTH {
        let block = u8x16::new(
            bytes[index..index + SIMD_BYTE_WIDTH]
                .try_into()
                .expect("SIMD block has exactly 16 bytes"),
        );
        let delimiters = simd_ascii_whitespace_mask(block)
            | block.simd_eq(u8x16::splat(b'('))
            | block.simd_eq(u8x16::splat(b')'))
            | block.simd_eq(u8x16::splat(b'"'));
        let delimiter_index = delimiters.to_bitmask().trailing_zeros() as usize;

        if delimiter_index < SIMD_BYTE_WIDTH {
            return AtomScan::Delimiter(index + delimiter_index);
        }
        index += SIMD_BYTE_WIDTH;
    }

    for (offset, byte) in bytes[index..].iter().copied().enumerate() {
        if is_delimiter(byte) {
            return AtomScan::Delimiter(index + offset);
        }
    }
    AtomScan::End
}

fn count_ascii_whitespace(bytes: &[u8]) -> usize {
    let mut index = 0;
    while index < bytes.len().min(SIMD_BYTE_WIDTH) {
        if !is_whitespace(bytes[index]) {
            return index;
        }
        index += 1;
    }

    while bytes.len() - index >= SIMD_BYTE_WIDTH {
        let block = u8x16::new(
            bytes[index..index + SIMD_BYTE_WIDTH]
                .try_into()
                .expect("SIMD block has exactly 16 bytes"),
        );
        let whitespace_mask = simd_ascii_whitespace_mask(block).to_bitmask();
        let non_whitespace_mask = !whitespace_mask & 0xffff;
        if non_whitespace_mask != 0 {
            return index + non_whitespace_mask.trailing_zeros() as usize;
        }
        index += SIMD_BYTE_WIDTH;
    }

    for &byte in &bytes[index..] {
        if !is_whitespace(byte) {
            break;
        }
        index += 1;
    }
    index
}

fn simd_ascii_whitespace_mask(bytes: u8x16) -> u8x16 {
    ((bytes.simd_ge(u8x16::splat(b'\t')) & bytes.simd_le(u8x16::splat(b'\n')))
        | bytes.simd_eq(u8x16::splat(b'\x0b'))
        | (bytes.simd_ge(u8x16::splat(b'\x0c')) & bytes.simd_le(u8x16::splat(b'\r'))))
        | bytes.simd_eq(u8x16::splat(b' '))
}

#[cfg(test)]
mod tests {
    use super::{Error, ReadResult, Reader, Token, TokenKind};
    use std::format;
    use std::vec::Vec;

    #[test]
    fn returns_ranges_for_atoms_structure_and_raw_strings() {
        let input = br#"(alpha "a\"b\t" beta)"#;
        let mut lexer = Reader::new();
        let mut offset = 0;
        let mut tokens = Vec::new();

        loop {
            let (result, consumed) = lexer.read(&input[offset..], true);
            offset += consumed;
            match result {
                ReadResult::Token(token) => tokens.push(token),
                ReadResult::End => break,
                other => panic!("unexpected result: {other:?}"),
            }
        }

        assert_eq!(
            tokens,
            [
                Token {
                    kind: TokenKind::Open,
                    start: 0,
                    end: 1,
                },
                Token {
                    kind: TokenKind::Atom,
                    start: 1,
                    end: 6,
                },
                Token {
                    kind: TokenKind::StringStart,
                    start: 7,
                    end: 8,
                },
                Token {
                    kind: TokenKind::StringText,
                    start: 8,
                    end: 9,
                },
                Token {
                    kind: TokenKind::Escape('"'),
                    start: 9,
                    end: 11,
                },
                Token {
                    kind: TokenKind::StringText,
                    start: 11,
                    end: 12,
                },
                Token {
                    kind: TokenKind::Escape('\t'),
                    start: 12,
                    end: 14,
                },
                Token {
                    kind: TokenKind::StringEnd,
                    start: 14,
                    end: 15,
                },
                Token {
                    kind: TokenKind::Atom,
                    start: 16,
                    end: 20,
                },
                Token {
                    kind: TokenKind::Close,
                    start: 20,
                    end: 21,
                },
            ]
        );
        assert_eq!(&input[7..15], br#""a\"b\t""#);
    }

    #[test]
    fn scans_long_ascii_whitespace_and_atoms_with_correct_ranges() {
        let atom = "a".repeat(49);
        let next = "b".repeat(33);
        let input = format!("{}({atom} {next})", " ".repeat(48));
        let mut lexer = Reader::new();

        assert_eq!(
            lexer.read(input.as_bytes(), true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::Open,
                    start: 48,
                    end: 49,
                }),
                49,
            )
        );
        assert_eq!(
            lexer.read(&input.as_bytes()[49..], true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::Atom,
                    start: 49,
                    end: 98,
                }),
                49,
            )
        );
        assert_eq!(
            lexer.read(&input.as_bytes()[98..], true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::Atom,
                    start: 99,
                    end: 132,
                }),
                34,
            )
        );
    }

    #[test]
    fn resumes_after_split_escape_and_preserves_absolute_ranges() {
        let mut lexer = Reader::new();
        assert_eq!(
            lexer.read(br#""a\"#, false),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::StringStart,
                    start: 0,
                    end: 1,
                }),
                1,
            )
        );
        assert_eq!(
            lexer.read(b"a\\", false),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::StringText,
                    start: 1,
                    end: 2,
                }),
                1,
            )
        );
        assert_eq!(lexer.read(b"\\", false), (ReadResult::InputEmpty, 1));
        assert_eq!(
            lexer.read(br#""b""#, true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::Escape('"'),
                    start: 2,
                    end: 4,
                }),
                1,
            )
        );
        assert_eq!(
            lexer.read(b"b\"", true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::StringText,
                    start: 4,
                    end: 5,
                }),
                1,
            )
        );
        assert_eq!(
            lexer.read(b"\"", true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::StringEnd,
                    start: 5,
                    end: 6,
                }),
                1,
            )
        );
        assert_eq!(lexer.position(), 6);
    }

    #[test]
    fn unicode_escape_emits_full_range_across_chunks() {
        let mut lexer = Reader::new();
        assert_eq!(
            lexer.read(b"\"", false).0,
            ReadResult::Token(Token {
                kind: TokenKind::StringStart,
                start: 0,
                end: 1,
            })
        );
        assert_eq!(lexer.read(b"\\u{4", false), (ReadResult::InputEmpty, 4));
        assert_eq!(
            lexer.read(b"1}\"", true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::UnicodeEscape('A'),
                    start: 1,
                    end: 7,
                }),
                2,
            )
        );
        assert_eq!(
            lexer.read(b"\"", true).0,
            ReadResult::Token(Token {
                kind: TokenKind::StringEnd,
                start: 7,
                end: 8,
            })
        );
    }

    #[test]
    fn rejects_invalid_escape_grammar() {
        for (input, expected) in [
            (br#""\q""#.as_slice(), Error::InvalidEscape),
            (br#""\uX""#.as_slice(), Error::ExpectedUnicodeBrace),
            (br#""\u{}""#.as_slice(), Error::InvalidUnicodeEscape),
            (br#""\u{g}""#.as_slice(), Error::InvalidUnicodeEscape),
            (br#""\u{110000}""#.as_slice(), Error::InvalidUnicodeScalar),
            (br#""\u{1234567}""#.as_slice(), Error::InvalidUnicodeEscape),
        ] {
            let mut lexer = Reader::new();
            let mut offset = 0;
            loop {
                let (result, consumed) = lexer.read(&input[offset..], true);
                offset += consumed;
                match result {
                    ReadResult::Error(error) => {
                        assert_eq!(error, expected, "input: {input:?}");
                        break;
                    }
                    ReadResult::Token(_) => {}
                    other => panic!("unexpected result: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn emits_one_start_and_end_for_atoms_spanning_multiple_reads() {
        let mut lexer = Reader::new();
        assert_eq!(
            lexer.read(b"ab", false),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::AtomStart,
                    start: 0,
                    end: 2,
                }),
                2,
            )
        );
        assert_eq!(lexer.read(b"cd", false), (ReadResult::InputEmpty, 2));
        assert_eq!(
            lexer.read(b"ef ", false),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::AtomEnd,
                    start: 2,
                    end: 6,
                }),
                2,
            )
        );
    }

    #[test]
    fn ends_a_split_atom_at_eof_with_an_empty_range() {
        let mut lexer = Reader::new();
        assert_eq!(
            lexer.read(b"atom", false),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::AtomStart,
                    start: 0,
                    end: 4,
                }),
                4,
            )
        );
        assert_eq!(
            lexer.read(b"", true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::AtomEnd,
                    start: 4,
                    end: 4,
                }),
                0,
            )
        );
    }

    #[test]
    fn treats_non_ascii_whitespace_as_atom_content_across_chunks() {
        let input = "foo\u{a0}bar".as_bytes();
        let mut lexer = Reader::new();
        assert_eq!(
            lexer.read(&input[..4], false),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::AtomStart,
                    start: 0,
                    end: 4,
                }),
                4,
            )
        );
        assert_eq!(
            lexer.read(&input[4..], true),
            (
                ReadResult::Token(Token {
                    kind: TokenKind::AtomEnd,
                    start: 4,
                    end: 8,
                }),
                4,
            )
        );
        assert_eq!(lexer.read(b"", true), (ReadResult::End, 0));
    }

    #[test]
    fn reports_unterminated_strings_and_fails_after_error() {
        let mut lexer = Reader::new();
        let input = b"\"unfinished\\";
        let mut offset = 0;
        loop {
            let (result, consumed) = lexer.read(&input[offset..], true);
            offset += consumed;
            if let ReadResult::Error(error) = result {
                assert_eq!(error, Error::UnterminatedEscape);
                break;
            }
            assert!(matches!(result, ReadResult::Token(_)));
        }
        assert_eq!(lexer.read(b"", true).0, ReadResult::Error(Error::Failed));
    }

    #[test]
    fn reports_unterminated_strings() {
        let mut lexer = Reader::new();
        let input = b"\"unfinished";
        let mut offset = 0;
        loop {
            let (result, consumed) = lexer.read(&input[offset..], true);
            offset += consumed;
            if let ReadResult::Error(error) = result {
                assert_eq!(error, Error::UnterminatedString);
                break;
            }
            assert!(matches!(result, ReadResult::Token(_)));
        }
        assert_eq!(lexer.read(b"", true).0, ReadResult::Error(Error::Failed));
    }
}
