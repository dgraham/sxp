mod grammar;
pub use grammar::grammar;

use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use sxp::{Parser, Result, Sexp, SymbolPool};

/// Configuration options for an HTML parser.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParserOptions {
    /// Preserve text nodes as they appear in the document, including
    /// whitespace between elements.
    preserve_whitespace: bool,
    /// Keep comment nodes in the tree.
    preserve_comments: bool,
    /// Maximum number of distinct symbols retained by the parser.
    symbol_pool_max_entries: Option<usize>,
}

impl ParserOptions {
    /// Sets whether text nodes preserve their original whitespace.
    pub fn preserve_whitespace(mut self, preserve: bool) -> Self {
        self.preserve_whitespace = preserve;
        self
    }

    /// Sets whether comment nodes are included in the parsed tree.
    pub fn preserve_comments(mut self, preserve: bool) -> Self {
        self.preserve_comments = preserve;
        self
    }

    /// Sets the maximum number of distinct symbols retained for reuse.
    /// `None` leaves the pool uncapped. `Some(0)` disables symbol caching.
    pub fn max_symbol_entries(mut self, max_entries: Option<usize>) -> Self {
        self.symbol_pool_max_entries = max_entries;
        self
    }
}

#[derive(Default)]
pub struct HtmlParser {
    pool: SymbolPool,
    options: ParserOptions,
}

impl HtmlParser {
    fn symbol(&mut self, text: &str) -> Sexp {
        self.pool.symbol(text)
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: ParserOptions) -> Self {
        let pool = options
            .symbol_pool_max_entries
            .map(SymbolPool::with_max_entries)
            .unwrap_or_default();
        Self { pool, options }
    }

    pub fn parse(&mut self, input: &str) -> Result<Sexp> {
        Ok(self.parse_html(input))
    }
}

impl Parser for HtmlParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        HtmlParser::parse(self, input)
    }
}

impl HtmlParser {
    fn parse_html(&mut self, input: &str) -> Sexp {
        let document = parse_document(RcDom::default(), Default::default())
            .one(input)
            .document;
        Sexp::list(self.children(&document, false))
    }
}

impl HtmlParser {
    fn children(&mut self, node: &Handle, preformatted: bool) -> Vec<Sexp> {
        let nodes = node.children.borrow();
        let significant: Vec<bool> = nodes
            .iter()
            .map(|node| match &node.data {
                NodeData::Text { contents } => {
                    preformatted
                        || self.options.preserve_whitespace
                        || !contents.borrow().chars().all(is_html_whitespace)
                }
                NodeData::Element { .. } => true,
                _ => false,
            })
            .collect();

        nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| match &node.data {
                NodeData::Text { contents }
                    if !preformatted && !self.options.preserve_whitespace =>
                {
                    let text = normalize_text(
                        &contents.borrow(),
                        significant[..index].iter().any(|&significant| significant),
                        significant[index + 1..]
                            .iter()
                            .any(|&significant| significant),
                    );
                    if text.is_empty() {
                        None
                    } else {
                        Some(Sexp::string(text))
                    }
                }
                _ => self.lower_node(node, preformatted),
            })
            .collect()
    }
}

// Collapse whitespace runs to a single space and trim the edges of the text
// that border no significant sibling content, so indentation and formatting
// between nodes disappears while spaces between inline content survive.
fn normalize_text(text: &str, keep_leading: bool, keep_trailing: bool) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut pending_space = false;

    for character in text.chars() {
        if is_html_whitespace(character) {
            pending_space = true;
        } else {
            if pending_space && (keep_leading || !normalized.is_empty()) {
                normalized.push(' ');
            }
            pending_space = false;
            normalized.push(character);
        }
    }

    if pending_space && keep_trailing && !normalized.is_empty() {
        normalized.push(' ');
    }

    normalized
}

fn is_html_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{0C}')
}

fn is_preformatted(local: &str) -> bool {
    matches!(local, "pre" | "textarea")
}

impl HtmlParser {
    fn lower_node(&mut self, node: &Handle, preformatted: bool) -> Option<Sexp> {
        match &node.data {
            NodeData::Document => Some(Sexp::list(self.children(node, preformatted))),
            NodeData::Doctype { name, .. } => {
                Some(Sexp::list([self.symbol("!doctype"), Sexp::string(name)]))
            }
            NodeData::Text { contents } => Some(Sexp::string(contents.borrow().to_string())),
            NodeData::Comment { contents } if self.options.preserve_comments => Some(Sexp::list([
                self.symbol("!comment"),
                Sexp::string(contents),
            ])),
            NodeData::Element { name, attrs, .. } => {
                let preformatted = preformatted || is_preformatted(&name.local);
                let mut values = vec![self.symbol(&name.local)];
                let attributes = attrs
                    .borrow()
                    .iter()
                    .map(|attribute| {
                        Sexp::list([
                            self.symbol(&attribute.name.local),
                            Sexp::string(attribute.value.to_string()),
                        ])
                    })
                    .collect::<Vec<_>>();
                if !attributes.is_empty() {
                    values.push(Sexp::list(
                        std::iter::once(self.symbol("@")).chain(attributes),
                    ));
                }
                values.extend(self.children(node, preformatted));
                Some(Sexp::list(values))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HtmlParser, ParserOptions};
    use sxp::{Result, Sexp};

    fn parse(input: &str) -> Result<Sexp> {
        HtmlParser::new().parse(input)
    }

    fn lowers(input: &str, expected: &str) -> sxp::Sexp {
        let value = parse(input).expect("should parse");
        assert_eq!(value.to_string(), expected);
        value
    }

    #[test]
    fn parses_html_into_elements_and_text() {
        lowers(
            "<p class=intro>Hello</p>",
            r#"((html (head) (body (p (@ (class "intro")) "Hello"))))"#,
        );
    }

    #[test]
    fn removes_whitespace_between_nodes() {
        lowers(
            "<ul>\n  <li>one</li>\n  <li>two</li>\n</ul>",
            r#"((html (head) (body (ul (li "one") (li "two")))))"#,
        );
    }

    #[test]
    fn removes_whitespace_inside_nodes() {
        lowers(
            "<p>\n  Hello\n  world\n</p>",
            r#"((html (head) (body (p "Hello world"))))"#,
        );
    }

    #[test]
    fn keeps_whitespace_between_inline_content() {
        lowers(
            "<p>Hello <b>world</b> again</p>",
            r#"((html (head) (body (p "Hello " (b "world") " again"))))"#,
        );
    }

    #[test]
    fn preserves_whitespace_in_pre_and_textarea() {
        lowers(
            "<pre>\n  one\n  two\n</pre>",
            r#"((html (head) (body (pre "  one\n  two\n"))))"#,
        );
        lowers(
            "<textarea>\n  one\n</textarea>",
            r#"((html (head) (body (textarea "  one\n"))))"#,
        );
    }

    #[test]
    fn preserves_whitespace_between_nodes_with_option() {
        let options = ParserOptions::default().preserve_whitespace(true);
        assert_eq!(
            HtmlParser::with_options(options)
                .parse("<ul>\n  <li>one</li>\n</ul>")
                .map(|sexp| sexp.to_string()),
            Ok("((html (head) (body (ul \"\\n  \" (li \"one\") \"\\n\"))))".to_owned())
        );
    }

    #[test]
    fn elides_comments_by_default() {
        lowers(
            "<p>a<!-- note -->b</p>",
            r#"((html (head) (body (p "a" "b"))))"#,
        );
    }

    #[test]
    fn preserves_comments_with_option() {
        let options = ParserOptions::default().preserve_comments(true);
        assert_eq!(
            HtmlParser::with_options(options)
                .parse("<p><!-- note --></p>")
                .map(|sexp| sexp.to_string()),
            Ok("((html (head) (body (p (!comment \" note \")))))".to_owned())
        );
    }
}
