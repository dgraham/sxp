mod grammar;
pub use grammar::grammar;

use serde_json::{Number, Value};
use sxp::{ParseError, Parser, Result, Sexp, SymbolPool};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParserOptions {
    max_symbol_entries: Option<usize>,
}

impl ParserOptions {
    /// Sets the maximum number of distinct symbols retained for reuse.
    /// `None` leaves the pool uncapped. `Some(0)` disables symbol caching.
    pub fn max_symbol_entries(mut self, max_entries: Option<usize>) -> Self {
        self.max_symbol_entries = max_entries;
        self
    }
}

#[derive(Default)]
pub struct JsonParser {
    pool: SymbolPool,
}

impl JsonParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: ParserOptions) -> Self {
        let pool = options
            .max_symbol_entries
            .map(SymbolPool::with_max_entries)
            .unwrap_or_default();
        Self { pool }
    }

    pub fn parse(&mut self, input: &str) -> Result<Sexp> {
        let value = serde_json::from_str(input).map_err(|error| {
            let offset = json_error_offset(input, error.line(), error.column());
            ParseError::new(error.to_string(), offset)
        })?;
        self.lower_value(value)
    }

    fn symbol(&mut self, text: &str) -> Sexp {
        self.pool.symbol(text)
    }

    fn lower_value(&mut self, value: Value) -> Result<Sexp> {
        match value {
            Value::Null => Ok(Sexp::nil()),
            Value::Bool(value) => Ok(Sexp::boolean(value)),
            Value::Number(value) => lower_number(value),
            Value::String(value) => Ok(Sexp::string(value)),
            Value::Array(values) => {
                let array = self.symbol("array");
                let values = values
                    .into_iter()
                    .map(|value| self.lower_value(value))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Sexp::list(std::iter::once(array).chain(values)))
            }
            Value::Object(values) => {
                let object = self.symbol("object");
                let fields = values
                    .into_iter()
                    .map(|(key, value)| {
                        Ok(Sexp::list([Sexp::string(key), self.lower_value(value)?]))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Sexp::list(std::iter::once(object).chain(fields)))
            }
        }
    }
}

fn json_error_offset(input: &str, line: usize, column: usize) -> usize {
    let line_start = input
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    line_start
        .saturating_add(column.saturating_sub(1))
        .min(input.len())
}

impl Parser for JsonParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        JsonParser::parse(self, input)
    }
}

fn lower_number(number: Number) -> Result<Sexp> {
    let text = number.to_string();
    if text.contains(['.', 'e', 'E']) {
        Ok(Sexp::float(text))
    } else {
        Ok(Sexp::integer(text))
    }
}

#[cfg(test)]
mod tests {
    use super::JsonParser;
    use sxp::{Atom, Result, Sexp};

    fn parse(input: &str) -> Result<Sexp> {
        JsonParser::new().parse(input)
    }

    fn lowers(input: &str, expected: &str) -> sxp::Sexp {
        let value = parse(input).expect("should parse");
        assert_eq!(value.to_string(), expected);
        value
    }

    #[test]
    fn parses_nested_json_values() {
        lowers(
            r#"{"name":"Ada","active":true,"scores":[3,5],"missing":null}"#,
            r#"(object ("active" true) ("missing" nil) ("name" "Ada") ("scores" (array 3 5)))"#,
        );
    }

    #[test]
    fn preserves_json_scalar_types() {
        let value = parse(r#"[true,false,null,42,3.5,"text"]"#).unwrap();
        let atoms: Vec<_> = value
            .iter()
            .filter_map(|node| match node {
                Sexp::Atom(atom) => Some(atom),
                Sexp::List(_) => None,
            })
            .collect();

        assert!(atoms.contains(&&Atom::Boolean(true)));
        assert!(atoms.contains(&&Atom::Boolean(false)));
        assert!(atoms.contains(&&Atom::Nil));
        assert!(atoms.contains(&&Atom::Integer("42".into())));
        assert!(atoms.contains(&&Atom::Float("3.5".into())));
        assert!(atoms.contains(&&Atom::String("text".into())));
    }

    #[test]
    fn round_trips_extreme_numbers() {
        lowers(
            "[9223372036854775808, 18446744073709551616, 123456789012345678901234567890, -0, 1e450, 1e567, 1.50]",
            "(array 9223372036854775808 18446744073709551616 123456789012345678901234567890 -0 1e+450 1e+567 1.50)",
        );
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse(r#"{"unterminated":"value""#).is_err());
    }

    #[test]
    fn reports_json_errors_as_byte_offsets_after_multibyte_text() {
        let input = "{\n  \"ok\":\"é\",\n  \"x\": }";
        let error = parse(input).unwrap_err();
        assert_eq!(error.offset(), input.find('}').unwrap());
    }
}
