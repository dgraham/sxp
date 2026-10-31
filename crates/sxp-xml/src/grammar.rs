use sxp::Sexp;
use sxp::pattern::{AtomType, Grammar, Pattern};

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    let node = Pattern::OneOf(vec![
        Pattern::Atom(AtomType::String),
        Pattern::List(vec![
            Pattern::Exact(Sexp::symbol("!comment")),
            Pattern::Atom(AtomType::String),
        ]),
        Pattern::Reference("element-with-attributes".to_owned()),
        Pattern::Reference("element".to_owned()),
    ]);
    Grammar::new(
        "sxp-xml",
        "document",
        [
            (
                "document".to_owned(),
                Pattern::List(vec![Pattern::Repeat(Box::new(Pattern::Reference(
                    "node".to_owned(),
                )))]),
            ),
            ("node".to_owned(), node),
            (
                "element-with-attributes".to_owned(),
                Pattern::List(vec![
                    Pattern::Atom(AtomType::Symbol),
                    Pattern::Reference("attributes".to_owned()),
                    Pattern::Repeat(Box::new(Pattern::Reference("node".to_owned()))),
                ]),
            ),
            (
                "element".to_owned(),
                Pattern::List(vec![
                    Pattern::Atom(AtomType::Symbol),
                    Pattern::Repeat(Box::new(Pattern::Reference("node".to_owned()))),
                ]),
            ),
            (
                "attributes".to_owned(),
                Pattern::List(vec![
                    Pattern::Exact(Sexp::symbol("@")),
                    Pattern::List(vec![
                        Pattern::Atom(AtomType::Symbol),
                        Pattern::Atom(AtomType::String),
                    ]),
                    Pattern::Repeat(Box::new(Pattern::List(vec![
                        Pattern::Atom(AtomType::Symbol),
                        Pattern::Atom(AtomType::String),
                    ]))),
                ]),
            ),
        ],
    )
    .expect("the built-in XML output grammar is valid")
}

#[cfg(test)]
mod tests {
    use crate::{ParserOptions, XmlParser};

    #[test]
    fn output_grammar_matches_xml_output() {
        let grammar = crate::grammar();
        let options = ParserOptions::default()
            .preserve_whitespace(true)
            .preserve_comments(true);
        let value = XmlParser::with_options(options)
            .parse("<root id=\"top\"><child><![CDATA[ ]]></child><!--note--></root>")
            .unwrap();
        assert!(grammar.matches_root(&value));

        let round_trip = sxp::SexpParser::new().parse(&value.to_string()).unwrap();
        assert!(grammar.matches_root(&round_trip));

        let malformed = sxp::SexpParser::new()
            .parse(r#"((root (@ (id 1))))"#)
            .unwrap();
        assert!(!grammar.matches_root(&malformed));
    }
}
