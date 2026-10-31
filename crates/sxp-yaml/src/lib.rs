mod grammar;
pub use grammar::grammar;

use noyalib::{Number, Value};
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
pub struct YamlParser {
    pool: SymbolPool,
}

impl YamlParser {
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
        let documents: Vec<Value> =
            noyalib::parallel::parse(input).map_err(|error: noyalib::Error| {
                let offset = error.location().map_or(0, |location| location.index());
                ParseError::new(error.to_string(), offset)
            })?;
        match documents.as_slice() {
            [] => Ok(Sexp::nil()),
            [document] => self.lower_value(document.clone()),
            documents => {
                let values = documents
                    .iter()
                    .cloned()
                    .map(|value| self.lower_value(value))
                    .collect::<Result<Vec<_>>>()?;
                let documents = self.symbol("documents");
                Ok(Sexp::list(std::iter::once(documents).chain(values)))
            }
        }
    }

    fn symbol(&mut self, text: &str) -> Sexp {
        self.pool.symbol(text)
    }

    fn lower_value(&mut self, value: Value) -> Result<Sexp> {
        match value {
            Value::Null => Ok(Sexp::nil()),
            Value::Bool(value) => Ok(Sexp::boolean(value)),
            Value::Number(number) => match number {
                Number::Integer(value) => Ok(Sexp::integer(value.to_string())),
                Number::Float(value) => Ok(Sexp::float(float_text(value))),
                other if other.is_integer() => Ok(Sexp::integer(other.to_string())),
                other => Ok(Sexp::float(float_text(other.as_f64()))),
            },
            Value::String(value) => Ok(Sexp::string(value)),
            Value::Sequence(values) => {
                let seq = self.symbol("seq");
                let values = values
                    .into_iter()
                    .map(|value| self.lower_value(value))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Sexp::list(std::iter::once(seq).chain(values)))
            }
            Value::Mapping(values) => {
                let map = self.symbol("map");
                let fields = values
                    .into_iter()
                    .map(|(key, value)| {
                        Ok(Sexp::list([Sexp::string(key), self.lower_value(value)?]))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Sexp::list(std::iter::once(map).chain(fields)))
            }
            Value::Tagged(value) => {
                let (tag_name, inner_value) = value.into_parts();
                let tag = self.symbol("tag");
                let tag_name = Sexp::string(tag_name.as_str());
                let inner = self.lower_value(inner_value)?;
                Ok(Sexp::list([tag, tag_name, inner]))
            }
        }
    }
}

impl Parser for YamlParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        YamlParser::parse(self, input)
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

    use super::YamlParser;
    use sxp::{Atom, Result, Sexp};

    fn parse(input: &str) -> Result<Sexp> {
        YamlParser::new().parse(input)
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
        let mut parser = YamlParser::new();
        let first = parser.parse("value: 1").unwrap();
        let second = parser.parse("value: 2").unwrap();

        assert!(Rc::ptr_eq(first_symbol(&first), first_symbol(&second)));
    }

    #[test]
    fn parses_maps_and_sequences() {
        lowers(
            "title: Example\nscores:\n  - 3\n  - 5\n",
            r#"(map ("title" "Example") ("scores" (seq 3 5)))"#,
        );
    }

    #[test]
    fn preserves_scalar_types() {
        let value =
            parse("count: 42\nratio: 1.5\nenabled: false\nname: Ada\nnothing: null\n").unwrap();

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
            r#"(map ("count" 42) ("ratio" 1.5) ("enabled" false) ("name" "Ada") ("nothing" nil))"#
        );
    }

    #[test]
    fn preserves_float_categories() {
        lowers(
            "whole: 1.0\npositive: .inf\nnegative: -.inf\nnot_a_number: .nan\n",
            r#"(map ("whole" 1.0) ("positive" inf) ("negative" -inf) ("not_a_number" nan))"#,
        );
    }

    #[test]
    fn preserves_tags() {
        lowers(
            "temperature: !Celsius 42.0\nshape: !Point {x: 1}\n",
            r#"(map ("temperature" (tag "!Celsius" "42.0")) ("shape" (tag "!Point" (map ("x" 1)))))"#,
        );
    }

    #[test]
    fn parses_multiple_documents() {
        lowers(
            "---\na: 1\n---\n- x\n- y\n",
            r#"(documents (map ("a" 1)) (seq "x" "y"))"#,
        );
    }

    #[test]
    fn empty_input_is_nil() {
        lowers("", "nil");
    }

    #[test]
    fn empty_document_is_nil() {
        lowers("---\n", "nil");
    }

    #[test]
    fn rejects_invalid_yaml_with_an_offset() {
        let error = parse("a: [\n").unwrap_err();
        assert!(error.offset() > 0);
    }
}
