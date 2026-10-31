mod grammar;
pub use grammar::grammar;

use sxp::{ParseError, Parser, Result, Sexp, SymbolPool};
use toml::Value;

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
pub struct TomlParser {
    pool: SymbolPool,
}

impl TomlParser {
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
        let value = toml::from_str(input).map_err(|error: toml::de::Error| {
            let offset = error.span().map_or(0, |span| span.start);
            ParseError::new(error.to_string(), offset)
        })?;
        self.lower_value(value)
    }

    fn symbol(&mut self, text: &str) -> Sexp {
        self.pool.symbol(text)
    }

    fn lower_value(&mut self, value: Value) -> Result<Sexp> {
        match value {
            Value::String(value) => Ok(Sexp::string(value)),
            Value::Integer(value) => Ok(Sexp::integer(value.to_string())),
            Value::Float(value) => Ok(Sexp::float(float_text(value))),
            Value::Boolean(value) => Ok(Sexp::boolean(value)),
            Value::Datetime(value) => Ok(Sexp::list([
                self.symbol("datetime"),
                Sexp::string(value.to_string()),
            ])),
            Value::Array(values) => {
                let array = self.symbol("array");
                let values = values
                    .into_iter()
                    .map(|value| self.lower_value(value))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Sexp::list(std::iter::once(array).chain(values)))
            }
            Value::Table(values) => {
                let table = self.symbol("table");
                let fields = values
                    .into_iter()
                    .map(|(key, value)| {
                        Ok(Sexp::list([Sexp::string(key), self.lower_value(value)?]))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Sexp::list(std::iter::once(table).chain(fields)))
            }
        }
    }
}

impl Parser for TomlParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        TomlParser::parse(self, input)
    }
}

fn float_text(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value == f64::INFINITY {
        "inf".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_owned()
    } else {
        format!("{value:?}")
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::TomlParser;
    use sxp::{Atom, Result, Sexp};

    fn parse(input: &str) -> Result<Sexp> {
        TomlParser::new().parse(input)
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
    fn reuses_symbols_across_parses() {
        let mut parser = TomlParser::new();
        let first = parser.parse("value = 1").unwrap();
        let second = parser.parse("value = 2").unwrap();

        assert!(Rc::ptr_eq(first_symbol(&first), first_symbol(&second)));
    }

    #[test]
    fn parses_tables_and_arrays() {
        lowers(
            "title = \"Example\"\nactive = true\nscores = [3, 5]\n",
            r#"(table ("active" true) ("scores" (array 3 5)) ("title" "Example"))"#,
        );
    }

    #[test]
    fn preserves_scalar_types_and_datetimes() {
        let value =
            parse("count = 42\nratio = 1.5\nenabled = false\ncreated = 2024-01-02T03:04:05Z\n")
                .unwrap();

        assert!(value.iter().any(
            |node| matches!(node, Sexp::Atom(Atom::Integer(value)) if value.as_ref() == "42")
        ));
        assert!(
            value.iter().any(
                |node| matches!(node, Sexp::Atom(Atom::Float(value)) if value.as_ref() == "1.5")
            )
        );
        assert!(
            value
                .iter()
                .any(|node| matches!(node, Sexp::Atom(Atom::Boolean(false))))
        );
        assert_eq!(
            value.to_string(),
            r#"(table ("count" 42) ("created" (datetime "2024-01-02T03:04:05Z")) ("enabled" false) ("ratio" 1.5))"#
        );
    }

    #[test]
    fn preserves_float_categories() {
        lowers(
            "whole = 1.0\npositive = inf\nnegative = -inf\nnot_a_number = nan\n",
            r#"(table ("negative" -inf) ("not_a_number" nan) ("positive" inf) ("whole" 1.0))"#,
        );
    }

    #[test]
    fn rejects_invalid_toml_with_an_offset() {
        let error = parse("value = [\n").unwrap_err();
        assert!(error.offset() > 0);
    }
}
