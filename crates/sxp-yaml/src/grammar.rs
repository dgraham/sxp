use sxp::Sexp;
use sxp::pattern::{AtomType, Grammar, Pattern};

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    let value = Pattern::OneOf(vec![
        Pattern::Atom(AtomType::Nil),
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
            Pattern::Exact(Sexp::symbol("tag")),
            Pattern::Atom(AtomType::String),
            Pattern::Reference("value".to_owned()),
        ]),
        Pattern::Reference("seq".to_owned()),
        Pattern::Reference("map".to_owned()),
        Pattern::Reference("documents".to_owned()),
    ]);
    Grammar::new(
        "sxp-yaml",
        "value",
        [
            ("value".to_owned(), value),
            (
                "seq".to_owned(),
                Pattern::List(vec![
                    Pattern::Exact(Sexp::symbol("seq")),
                    Pattern::Repeat(Box::new(Pattern::Reference("value".to_owned()))),
                ]),
            ),
            (
                "map".to_owned(),
                Pattern::List(vec![
                    Pattern::Exact(Sexp::symbol("map")),
                    Pattern::Repeat(Box::new(Pattern::List(vec![
                        Pattern::Atom(AtomType::String),
                        Pattern::Reference("value".to_owned()),
                    ]))),
                ]),
            ),
            (
                "documents".to_owned(),
                Pattern::List(vec![
                    Pattern::Exact(Sexp::symbol("documents")),
                    Pattern::OneOrMore(Box::new(Pattern::Reference("value".to_owned()))),
                ]),
            ),
        ],
    )
    .expect("the built-in YAML output grammar is valid")
}

#[cfg(test)]
mod tests {
    use crate::YamlParser;
    use sxp::Sexp;

    fn parse(input: &str) -> sxp::Result<Sexp> {
        YamlParser::new().parse(input)
    }

    #[test]
    fn output_grammar_matches_yaml_values() {
        let grammar = crate::grammar();
        let value = parse(
            "title: Example\nenabled: true\nscores: [1, -2, 3.5, .inf, -.inf, .nan]\nnothing: null\ncreated: !Celsius 20.0\n",
        )
        .unwrap();
        assert!(grammar.matches_root(&value));

        let mut sexp_parser =
            sxp::SexpParser::with_options(sxp::ParserOptions::default().typed_atoms(true));
        let round_trip = sexp_parser.parse(&value.to_string()).unwrap();
        assert!(grammar.matches_root(&round_trip));
        assert!(!grammar.matches_root(&sexp_parser.parse("(map (title nil))").unwrap()));
    }

    #[test]
    fn output_grammar_matches_multiple_documents() {
        let grammar = crate::grammar();
        let value = parse("---\na: 1\n---\n- x\n").unwrap();
        assert!(grammar.matches_root(&value));

        let mut sexp_parser =
            sxp::SexpParser::with_options(sxp::ParserOptions::default().typed_atoms(true));
        let round_trip = sexp_parser.parse(&value.to_string()).unwrap();
        assert!(grammar.matches_root(&round_trip));
    }
}
