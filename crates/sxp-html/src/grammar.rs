use sxp::Sexp;
use sxp::pattern::{AtomType, Grammar, Pattern};

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    let node = Pattern::OneOf(vec![
        Pattern::Atom(AtomType::String),
        Pattern::List(vec![
            Pattern::Exact(Sexp::symbol("!doctype")),
            Pattern::Atom(AtomType::String),
        ]),
        Pattern::List(vec![
            Pattern::Exact(Sexp::symbol("!comment")),
            Pattern::Atom(AtomType::String),
        ]),
        Pattern::Reference("element-with-attributes".to_owned()),
        Pattern::Reference("element".to_owned()),
    ]);
    Grammar::new(
        "sxp-html",
        "document",
        [
            (
                "document".to_owned(),
                Pattern::List(vec![
                    Pattern::Reference("node".to_owned()),
                    Pattern::Repeat(Box::new(Pattern::Reference("node".to_owned()))),
                ]),
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
    .expect("the built-in HTML output grammar is valid")
}

#[cfg(test)]
mod tests {
    use crate::{HtmlParser, ParserOptions};

    #[test]
    fn output_grammar_matches_html_output() {
        let grammar = crate::grammar();
        let options = ParserOptions::default()
            .preserve_whitespace(true)
            .preserve_comments(true);
        let mut parser = HtmlParser::with_options(options);
        let value = parser
            .parse("<!doctype html><!--note--><main id=\"top\"> Hello <b>world</b></main>")
            .unwrap();
        assert!(grammar.matches_root(&value));

        let round_trip = sxp::SexpParser::new().parse(&value.to_string()).unwrap();
        assert!(grammar.matches_root(&round_trip));

        let malformed = sxp::SexpParser::new()
            .parse(r#"((p (@ (class 1))))"#)
            .unwrap();
        assert!(!grammar.matches_root(&malformed));
        assert!(!grammar.matches_root(&sxp::SexpParser::new().parse("()").unwrap()));
    }
}
