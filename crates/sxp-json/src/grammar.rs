use sxp::Sexp;
use sxp::pattern::{AtomType, Grammar, Pattern};

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    let value = Pattern::OneOf(vec![
        Pattern::Atom(AtomType::Nil),
        Pattern::Atom(AtomType::Boolean),
        Pattern::Atom(AtomType::Integer),
        Pattern::Atom(AtomType::Float),
        Pattern::Atom(AtomType::String),
        Pattern::List(vec![
            Pattern::Exact(Sexp::symbol("array")),
            Pattern::Repeat(Box::new(Pattern::Reference("value".to_owned()))),
        ]),
        Pattern::List(vec![
            Pattern::Exact(Sexp::symbol("object")),
            Pattern::Repeat(Box::new(Pattern::List(vec![
                Pattern::Atom(AtomType::String),
                Pattern::Reference("value".to_owned()),
            ]))),
        ]),
    ]);
    Grammar::new("sxp-json", "value", [("value".to_owned(), value)])
        .expect("the built-in JSON output grammar is valid")
}

#[cfg(test)]
mod tests {
    use crate::JsonParser;
    use sxp::Sexp;

    fn parse(input: &str) -> sxp::Result<Sexp> {
        JsonParser::new().parse(input)
    }

    fn parse_sexp(input: &str) -> Sexp {
        sxp::SexpParser::with_options(sxp::ParserOptions::default().typed_atoms(true))
            .parse(input)
            .unwrap()
    }

    #[test]
    fn output_grammar_matches_json_values() {
        let grammar = crate::grammar();
        assert!(grammar.matches_root(&parse(r#"{"a":[null,true,3.5]}"#).unwrap()));
        assert!(!grammar.matches_root(&parse_sexp("(array missing)")));
        assert!(!grammar.matches_root(&parse_sexp(r#"(object (1 "value"))"#)));
    }
}
