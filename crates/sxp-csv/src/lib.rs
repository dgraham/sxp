use std::io::Read;

mod grammar;
pub use grammar::grammar;

use csv::{ReaderBuilder, StringRecord};
use sxp::{ParseError, Parser, Result, Sexp, SymbolPool};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParserOptions {
    max_symbol_entries: Option<usize>,
    has_headers: bool,
    delimiter: char,
    flexible: bool,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            max_symbol_entries: None,
            has_headers: true,
            delimiter: ',',
            flexible: false,
        }
    }
}

impl ParserOptions {
    /// Sets the maximum number of distinct header symbols retained for reuse.
    /// `None` leaves the pool uncapped. `Some(0)` disables symbol caching.
    pub fn max_symbol_entries(mut self, max_entries: Option<usize>) -> Self {
        self.max_symbol_entries = max_entries;
        self
    }

    /// Sets whether the first record is a header row. Defaults to `true`.
    pub fn has_headers(mut self, yes: bool) -> Self {
        self.has_headers = yes;
        self
    }

    /// Sets the field delimiter. Defaults to `,`.
    pub fn delimiter(mut self, delimiter: char) -> Self {
        self.delimiter = delimiter;
        self
    }

    /// Sets whether records of differing lengths are allowed. Defaults to `false`.
    pub fn flexible(mut self, yes: bool) -> Self {
        self.flexible = yes;
        self
    }
}

#[derive(Default)]
pub struct CsvParser {
    pool: SymbolPool,
    options: ParserOptions,
}

impl CsvParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: ParserOptions) -> Self {
        let pool = options
            .max_symbol_entries
            .map(SymbolPool::with_max_entries)
            .unwrap_or_default();
        Self { pool, options }
    }

    pub fn parse(&mut self, input: &str) -> Result<Sexp> {
        let mut stream = self.stream(input.as_bytes())?;
        let mut records = Vec::new();
        while let Some(record) = stream.read_next()? {
            records.push(record);
        }
        Ok(Sexp::list(records))
    }

    /// Creates a reader that yields the header and each data record
    /// as separate list expressions.
    pub fn stream<R: Read>(&mut self, input: R) -> Result<CsvStream<R>> {
        self.validate_delimiter()?;
        let mut reader = ReaderBuilder::new()
            .has_headers(self.options.has_headers)
            .delimiter(self.options.delimiter as u8)
            .flexible(self.options.flexible)
            .from_reader(input);

        let header = if self.options.has_headers {
            let header = reader.headers().map_err(csv_error)?;
            if header.is_empty() {
                None
            } else {
                let fields = header
                    .iter()
                    .map(|field| csv_header_field(field, &mut self.pool))
                    .collect::<Vec<_>>();
                Some(Sexp::list(fields))
            }
        } else {
            None
        };

        Ok(CsvStream {
            reader,
            header,
            record: StringRecord::new(),
        })
    }

    fn validate_delimiter(&self) -> Result<()> {
        if !self.options.delimiter.is_ascii() || matches!(self.options.delimiter, '"' | '\r' | '\n')
        {
            return Err(ParseError::new(
                format!("invalid delimiter {:?}", self.options.delimiter),
                0,
            ));
        }
        Ok(())
    }
}

/// Reads a CSV header and records as individual S-expression lists.
pub struct CsvStream<R: Read> {
    reader: csv::Reader<R>,
    header: Option<Sexp>,
    record: StringRecord,
}

impl<R: Read> CsvStream<R> {
    /// Reads the next header or data record, returning `None` at end of input.
    pub fn read_next(&mut self) -> Result<Option<Sexp>> {
        if let Some(header) = self.header.take() {
            return Ok(Some(header));
        }
        if !self
            .reader
            .read_record(&mut self.record)
            .map_err(csv_error)?
        {
            return Ok(None);
        }
        let fields = self.record.iter().map(csv_field).collect::<Vec<_>>();
        Ok(Some(Sexp::list(fields)))
    }
}

fn csv_field(value: &str) -> Sexp {
    match value {
        "true" => Sexp::boolean(true),
        "false" => Sexp::boolean(false),
        _ => numeric_atom(value).unwrap_or_else(|| Sexp::string(value)),
    }
}

fn csv_header_field(value: &str, pool: &mut SymbolPool) -> Sexp {
    if is_safe_symbol(value) {
        pool.symbol(value)
    } else {
        Sexp::string(value)
    }
}

fn is_safe_symbol(value: &str) -> bool {
    !value.is_empty()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || matches!(byte, b' ' | b'(' | b')' | b'"'))
        && !matches!(value, "true" | "false" | "nil")
        && numeric_atom(value).is_none()
}

// Match typed S-expression number syntax without converting the field text.
fn numeric_atom(text: &str) -> Option<Sexp> {
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

impl Parser for CsvParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        CsvParser::parse(self, input)
    }
}

fn csv_error(error: csv::Error) -> ParseError {
    let offset = match error.kind() {
        csv::ErrorKind::UnequalLengths { pos, .. } | csv::ErrorKind::Utf8 { pos, .. } => {
            pos.as_ref().map_or(0, |position| position.byte() as usize)
        }
        _ => 0,
    };
    ParseError::new(error.to_string(), offset)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::rc::Rc;

    use super::{CsvParser, ParserOptions};
    use sxp::{Atom, Result, Sexp};

    fn parse(input: &str) -> Result<Sexp> {
        CsvParser::new().parse(input)
    }

    fn lowers(input: &str, expected: &str) -> sxp::Sexp {
        let value = parse(input).expect("should parse");
        assert_eq!(value.to_string(), expected);
        value
    }

    fn first_symbol(value: &Sexp) -> &Rc<str> {
        value
            .iter()
            .find_map(|node| match node {
                Sexp::Atom(Atom::Symbol(symbol)) => Some(symbol),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn reuses_header_symbols_across_parses() {
        let mut parser = CsvParser::new();
        let first = parser.parse("name,age\nAda,36\n").unwrap();
        let second = parser.parse("name,age\nGrace,45\n").unwrap();

        assert!(Rc::ptr_eq(first_symbol(&first), first_symbol(&second)));
    }

    #[test]
    fn parses_headers_and_rows_as_lists() {
        lowers(
            "name,age\nAda,36\nGrace,45\n",
            r#"((name age) ("Ada" 36) ("Grace" 45))"#,
        );
    }

    #[test]
    fn keeps_headers_as_strings_when_they_are_not_safe_symbols() {
        let value = parse("\"first name\",true,007\nAda,yes,12\n").unwrap();
        assert_eq!(
            value.to_string(),
            r#"(("first name" "true" 007) ("Ada" "yes" 12))"#
        );

        let grammar = crate::grammar();
        assert!(grammar.matches_root(&value));
        let mut sexp_parser =
            sxp::SexpParser::with_options(sxp::ParserOptions::default().typed_atoms(true));
        let round_trip = sexp_parser.parse(&value.to_string()).unwrap();
        assert!(grammar.matches_root(&round_trip));
    }

    #[test]
    fn classifies_numeric_and_boolean_fields() {
        lowers(
            "integer,negative,float,exponent,large,padded,invalid,true,false,other\n36,-7,3.50,1e+300,999999999999999999999,007,12x,true,false,True\n",
            r#"((integer negative float exponent large padded invalid "true" "false" other) (36 -7 3.50 1e+300 999999999999999999999 "007" "12x" true false "True"))"#,
        );
    }

    #[test]
    fn parses_rows_without_headers() {
        let mut parser = CsvParser::with_options(ParserOptions::default().has_headers(false));
        assert_eq!(
            parser.parse("a,b\n1,2\n").map(|value| value.to_string()),
            Ok("((\"a\" \"b\") (1 2))".to_owned())
        );
    }

    #[test]
    fn preserves_quoted_fields() {
        lowers(
            "a,b\n\"x,\"\"y\"\",\nl\",\"\"\n",
            r#"((a b) ("x,\"y\",\nl" ""))"#,
        );
    }

    #[test]
    fn parses_custom_delimiter() {
        let mut parser = CsvParser::with_options(ParserOptions::default().delimiter(';'));
        assert_eq!(
            parser.parse("a;b\n1;x\n").map(|value| value.to_string()),
            Ok("((a b) (1 \"x\"))".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_delimiters() {
        for delimiter in ['"', '\r', '\n'] {
            let mut parser = CsvParser::with_options(ParserOptions::default().delimiter(delimiter));
            assert!(parser.parse("a,b\n").is_err());
        }
    }

    #[test]
    fn skips_blank_lines() {
        lowers("a,b\n\n1,x\n", r#"((a b) (1 "x"))"#);
    }

    #[test]
    fn accepts_crlf_and_missing_trailing_newline() {
        lowers("a,b\r\n1,x", r#"((a b) (1 "x"))"#);
    }

    #[test]
    fn empty_input_is_empty_csv() {
        lowers("", "()");
    }

    #[test]
    fn streams_header_and_rows_as_separate_lists() {
        let mut parser = CsvParser::new();
        let mut stream = parser
            .stream(Cursor::new("name,age\nAda,36\nGrace,45\n"))
            .unwrap();

        assert_eq!(
            stream.read_next().unwrap().unwrap().to_string(),
            "(name age)"
        );
        assert_eq!(
            stream.read_next().unwrap().unwrap().to_string(),
            r#"("Ada" 36)"#
        );
        assert_eq!(
            stream.read_next().unwrap().unwrap().to_string(),
            r#"("Grace" 45)"#
        );
        assert_eq!(stream.read_next().unwrap(), None);
    }

    #[test]
    fn streams_data_rows_without_headers() {
        let mut parser = CsvParser::with_options(ParserOptions::default().has_headers(false));
        let mut stream = parser.stream(Cursor::new("Ada,36\nGrace,45\n")).unwrap();

        assert_eq!(
            stream.read_next().unwrap().unwrap().to_string(),
            r#"("Ada" 36)"#
        );
        assert_eq!(
            stream.read_next().unwrap().unwrap().to_string(),
            r#"("Grace" 45)"#
        );
        assert_eq!(stream.read_next().unwrap(), None);
    }

    #[test]
    fn allows_unequal_rows_when_flexible() {
        let mut parser = CsvParser::with_options(ParserOptions::default().flexible(true));
        assert_eq!(
            parser.parse("a,b\n1\n").map(|value| value.to_string()),
            Ok("((a b) (1))".to_owned())
        );
    }

    #[test]
    fn rejects_unequal_rows_with_an_offset() {
        let error = parse("a,b\n1\n").unwrap_err();
        assert!(error.offset() > 0);
    }
}
