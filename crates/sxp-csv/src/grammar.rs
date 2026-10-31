use sxp::pattern::{AtomType, Grammar, Pattern};

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    Grammar::new(
        "sxp-csv",
        "csv",
        [
            (
                "csv".to_owned(),
                Pattern::List(vec![
                    Pattern::Optional(Box::new(Pattern::Reference("header".to_owned()))),
                    Pattern::Repeat(Box::new(Pattern::Reference("record".to_owned()))),
                ]),
            ),
            (
                "header".to_owned(),
                Pattern::List(vec![Pattern::Repeat(Box::new(Pattern::OneOf(vec![
                    Pattern::Atom(AtomType::Symbol),
                    Pattern::Atom(AtomType::String),
                ])))]),
            ),
            (
                "record".to_owned(),
                Pattern::List(vec![Pattern::Repeat(Box::new(Pattern::OneOf(vec![
                    Pattern::Atom(AtomType::String),
                    Pattern::Atom(AtomType::Integer),
                    Pattern::Atom(AtomType::Float),
                    Pattern::Atom(AtomType::Boolean),
                ])))]),
            ),
        ],
    )
    .expect("the built-in CSV output grammar is valid")
}

#[cfg(test)]
mod tests {
    use crate::CsvParser;
    use sxp::Sexp;

    fn parse(input: &str) -> sxp::Result<Sexp> {
        CsvParser::new().parse(input)
    }

    #[test]
    fn output_grammar_matches_csv_records() {
        let grammar = crate::grammar();
        let value = parse("name,age,score,active\nAda,36,1.5,true\n").unwrap();
        assert!(grammar.matches_root(&value));

        let mut sexp_parser =
            sxp::SexpParser::with_options(sxp::ParserOptions::default().typed_atoms(true));
        let round_trip = sexp_parser.parse(&value.to_string()).unwrap();
        assert!(grammar.matches_root(&round_trip));
        assert!(
            !grammar.matches_root(
                &sexp_parser
                    .parse(r#"(csv (header "name" "age") (row "Ada" "36"))"#)
                    .unwrap()
            )
        );
    }

    #[test]
    fn output_grammar_matches_rows_without_header_mode() {
        let grammar = crate::grammar();
        let mut parser =
            crate::CsvParser::with_options(crate::ParserOptions::default().has_headers(false));
        let value = parser.parse("a,b\n1,2\n").unwrap();
        assert!(grammar.matches_root(&value));

        let mut sexp_parser =
            sxp::SexpParser::with_options(sxp::ParserOptions::default().typed_atoms(true));
        let round_trip = sexp_parser.parse(&value.to_string()).unwrap();
        assert!(grammar.matches_root(&round_trip));
    }
}
