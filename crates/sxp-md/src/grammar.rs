use sxp::Sexp;
use sxp::pattern::{AtomType, Grammar, Pattern};

fn symbol_pattern(value: &str) -> Pattern {
    Pattern::Exact(Sexp::symbol(value))
}

fn tagged(name: &str, children: impl IntoIterator<Item = Pattern>) -> Pattern {
    Pattern::List(
        std::iter::once(symbol_pattern(name))
            .chain(children)
            .collect(),
    )
}

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    let text = || Pattern::Atom(AtomType::String);
    let node = Pattern::Reference("node".to_owned());
    let repeated_nodes = || Pattern::Repeat(Box::new(node.clone()));
    let link_attributes = tagged("@", [tagged("href", [text()]), tagged("title", [text()])]);
    let image_attributes = tagged("@", [tagged("src", [text()]), tagged("title", [text()])]);
    let heading_level = tagged(
        "level",
        [Pattern::OneOf(
            (1..=6)
                .map(|level| symbol_pattern(&format!("h{level}")))
                .collect(),
        )],
    );
    let fenced_code_block = tagged(
        "code-block",
        [
            tagged("language", [text()]),
            Pattern::Repeat(Box::new(text())),
        ],
    );
    let code_block = tagged("code-block", [Pattern::Repeat(Box::new(text()))]);

    let node_pattern = Pattern::OneOf(vec![
        text(),
        symbol_pattern("soft-break"),
        symbol_pattern("hard-break"),
        tagged("code", [text()]),
        tagged("html", [text()]),
        tagged("rule", []),
        tagged("task", [Pattern::Atom(AtomType::Boolean)]),
        tagged("footnote-reference", [text()]),
        tagged("paragraph", [repeated_nodes()]),
        tagged("heading", [heading_level, repeated_nodes()]),
        tagged("blockquote", [repeated_nodes()]),
        code_block,
        fenced_code_block,
        tagged("html-block", [repeated_nodes()]),
        tagged("list", [repeated_nodes()]),
        tagged(
            "list",
            [
                tagged("start", [Pattern::Atom(AtomType::Symbol)]),
                repeated_nodes(),
            ],
        ),
        tagged("item", [repeated_nodes()]),
        tagged("footnote-definition", [text(), repeated_nodes()]),
        tagged("table", [repeated_nodes()]),
        tagged("table-head", [repeated_nodes()]),
        tagged("table-row", [repeated_nodes()]),
        tagged("table-cell", [repeated_nodes()]),
        tagged("emphasis", [repeated_nodes()]),
        tagged("strong", [repeated_nodes()]),
        tagged("strikethrough", [repeated_nodes()]),
        tagged("link", [link_attributes, repeated_nodes()]),
        tagged("image", [image_attributes, repeated_nodes()]),
        tagged("unknown", [repeated_nodes()]),
    ]);

    Grammar::new(
        "sxp-md",
        "document",
        [
            (
                "document".to_owned(),
                Pattern::List(vec![Pattern::Repeat(Box::new(node))]),
            ),
            ("node".to_owned(), node_pattern),
        ],
    )
    .expect("the built-in Markdown output grammar is valid")
}

#[cfg(test)]
mod tests {
    use crate::MarkdownParser;
    use sxp::Sexp;

    fn parse(input: &str) -> sxp::Result<Sexp> {
        MarkdownParser::new().parse(input)
    }

    #[test]
    fn output_grammar_matches_emitted_markdown_forms() {
        let grammar = crate::grammar();
        for input in [
            "# Heading *emphasis*\n",
            "- [x] checked\n- [ ] unchecked\n",
            "2. first\n3. second\n",
            "[link](https://example.com \"title\") and ![image](image.png \"alt\")",
            "```rust\nfn main() {}\n```\n",
            "| left | right |\n| --- | --- |\n| a | b |\n",
            "> quoted\n\n---\n",
            "note[^1]\n\n[^1]: footnote\n",
        ] {
            let value = parse(input).unwrap();
            assert!(
                grammar.matches_root(&value),
                "grammar rejected Markdown output for {input:?}: {value}"
            );
            let round_trip =
                sxp::SexpParser::with_options(sxp::ParserOptions::default().typed_booleans(true))
                    .parse(&value.to_string())
                    .unwrap();
            assert!(
                grammar.matches_root(&round_trip),
                "grammar rejected serialized Markdown output for {input:?}: {round_trip}"
            );
        }
    }
    #[test]
    fn output_grammar_rejects_invalid_task_markers() {
        let invalid = Sexp::list([Sexp::list([Sexp::symbol("task"), Sexp::string("true")])]);
        assert!(!crate::grammar().matches_root(&invalid));
    }
}
