use sxp::Sexp;
use sxp::pattern::{AtomType, Grammar, Pattern};

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    let value = Pattern::OneOf(vec![
        Pattern::Atom(AtomType::Boolean),
        Pattern::Atom(AtomType::Integer),
        Pattern::OneOf(vec![
            Pattern::Atom(AtomType::Float),
            Pattern::Exact(Sexp::symbol("nan")),
            Pattern::Exact(Sexp::symbol("inf")),
            Pattern::Exact(Sexp::symbol("-inf")),
        ]),
        Pattern::Atom(AtomType::String),
        Pattern::List(vec![
            Pattern::Exact(Sexp::symbol("datetime")),
            Pattern::Atom(AtomType::String),
        ]),
        Pattern::Reference("array".to_owned()),
        Pattern::Reference("table".to_owned()),
    ]);
    Grammar::new(
        "sxp-toml",
        "table",
        [
            (
                "table".to_owned(),
                Pattern::List(vec![
                    Pattern::Exact(Sexp::symbol("table")),
                    Pattern::Repeat(Box::new(Pattern::List(vec![
                        Pattern::Atom(AtomType::String),
                        Pattern::Reference("value".to_owned()),
                    ]))),
                ]),
            ),
            (
                "array".to_owned(),
                Pattern::List(vec![
                    Pattern::Exact(Sexp::symbol("array")),
                    Pattern::Repeat(Box::new(Pattern::Reference("value".to_owned()))),
                ]),
            ),
            ("value".to_owned(), value),
        ],
    )
    .expect("the built-in TOML output grammar is valid")
}

#[cfg(test)]
mod tests {
    use crate::TomlParser;
    use sxp::Sexp;

    fn parse(input: &str) -> sxp::Result<Sexp> {
        TomlParser::new().parse(input)
    }

    #[test]
    fn output_grammar_matches_toml_values() {
        let grammar = crate::grammar();
        let value = parse(
            "title = \"Example\"\nenabled = true\nvalues = [1, -2, 3.5, inf, -inf, nan]\ncreated = 2024-01-02T03:04:05Z\nnested = { key = \"value\" }\n",
        )
        .unwrap();
        assert!(grammar.matches_root(&value));

        let mut sexp_parser =
            sxp::SexpParser::with_options(sxp::ParserOptions::default().typed_atoms(true));
        let round_trip = sexp_parser.parse(&value.to_string()).unwrap();
        assert!(grammar.matches_root(&round_trip));
        assert!(!grammar.matches_root(&sexp_parser.parse("(table (title nil))").unwrap()));
    }
}
