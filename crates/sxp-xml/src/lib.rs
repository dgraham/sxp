mod grammar;
pub use grammar::grammar;

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use sxp::{ParseError, Parser, Result, Sexp, SymbolPool};

/// Configuration options for an XML parser.
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
pub struct XmlParser {
    pool: SymbolPool,
    options: ParserOptions,
}

impl XmlParser {
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
        self.parse_document(input)
    }
}

impl Parser for XmlParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        XmlParser::parse(self, input)
    }
}

impl XmlParser {
    fn parse_document(&mut self, input: &str) -> Result<Sexp> {
        let mut reader = Reader::from_str(input);
        reader.config_mut().trim_text(false);

        let mut stack: Vec<Vec<Sexp>> = Vec::new();
        // Whether whitespace is significant in each open element, per inherited
        // xml:space="preserve" attributes.
        let mut preserved: Vec<bool> = Vec::new();
        let mut roots = Vec::new();

        loop {
            match reader
                .read_event()
                .map_err(|error| ParseError::new(error.to_string(), 0))?
            {
                Event::Start(element) => {
                    preserved.push(preserves_space(
                        &element,
                        preserved.last().copied().unwrap_or(false),
                    ));
                    stack.push(self.element_values(&element)?);
                }
                Event::Empty(element) => append(
                    Sexp::list(self.element_values(&element)?),
                    &mut stack,
                    &mut roots,
                ),
                Event::End(_) => {
                    preserved.pop();
                    let values = stack.pop().ok_or_else(|| {
                        ParseError::new("XML parser emitted an unmatched end tag", 0)
                    })?;
                    append(Sexp::list(values), &mut stack, &mut roots);
                }
                Event::Text(text) => {
                    let text = text.xml10_content();
                    if preserved.last().copied().unwrap_or(false)
                        || self.options.preserve_whitespace
                        || !text.chars().all(is_xml_whitespace)
                    {
                        append(Sexp::string(text.into_owned()), &mut stack, &mut roots);
                    }
                }
                Event::CData(text) => append(
                    Sexp::string(text.xml10_content().into_owned()),
                    &mut stack,
                    &mut roots,
                ),
                Event::Comment(comment) => {
                    if self.options.preserve_comments {
                        append(
                            Sexp::list([
                                self.symbol("!comment"),
                                Sexp::string(comment.into_inner().into_owned()),
                            ]),
                            &mut stack,
                            &mut roots,
                        );
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }

        Ok(Sexp::list(roots))
    }
}

// xml:space="preserve" applies to the element and all of its descendants,
// until an xml:space="default" attribute overrides it.
fn preserves_space(element: &BytesStart, inherited: bool) -> bool {
    for attribute in element.attributes().flatten() {
        if attribute.key.as_ref() == "xml:space" {
            return attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| value == "preserve")
                .unwrap_or(false);
        }
    }
    inherited
}

fn is_xml_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n')
}

impl XmlParser {
    fn element_values(&mut self, element: &BytesStart<'_>) -> Result<Vec<Sexp>> {
        let mut values = vec![self.symbol(element.name().as_ref())];
        let attributes = element
            .attributes()
            .map(|attribute| -> std::result::Result<Sexp, quick_xml::Error> {
                let attribute = attribute?;
                Ok(Sexp::list([
                    self.symbol(attribute.key.as_ref()),
                    Sexp::string(
                        attribute
                            .normalized_value(XmlVersion::Implicit1_0)?
                            .into_owned(),
                    ),
                ]))
            })
            .collect::<std::result::Result<Vec<_>, quick_xml::Error>>()
            .map_err(|error| ParseError::new(error.to_string(), 0))?;
        if !attributes.is_empty() {
            values.push(Sexp::list(
                std::iter::once(self.symbol("@")).chain(attributes),
            ));
        }
        Ok(values)
    }
}

fn append(value: Sexp, stack: &mut [Vec<Sexp>], roots: &mut Vec<Sexp>) {
    if let Some(parent) = stack.last_mut() {
        parent.push(value);
    } else {
        roots.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{ParserOptions, XmlParser};
    use sxp::{Result, Sexp};

    fn parse(input: &str) -> Result<Sexp> {
        XmlParser::new().parse(input)
    }

    fn lowers(input: &str, expected: &str) -> sxp::Sexp {
        let value = parse(input).expect("should parse");
        assert_eq!(value.to_string(), expected);
        value
    }

    #[test]
    fn parses_elements_attributes_and_text() {
        lowers(
            r#"<p class="intro">Hello</p>"#,
            r#"((p (@ (class "intro")) "Hello"))"#,
        );
    }

    #[test]
    fn removes_whitespace_between_nodes() {
        lowers(
            "<list>\n  <item>one</item>\n  <item>two</item>\n</list>",
            r#"((list (item "one") (item "two")))"#,
        );
    }

    #[test]
    fn removes_whitespace_only_text_nodes() {
        lowers("<p>   </p>", "((p))");
    }

    #[test]
    fn keeps_other_text_untouched() {
        lowers(
            "<p>\n  Hello\n  world\n</p>",
            r#"((p "\n  Hello\n  world\n"))"#,
        );
    }

    #[test]
    fn preserves_whitespace_with_xml_space() {
        lowers(
            r#"<pre xml:space="preserve"> </pre>"#,
            r#"((pre (@ (xml:space "preserve")) " "))"#,
        );
    }

    #[test]
    fn inherits_xml_space_until_overridden() {
        lowers(
            r#"<a xml:space="preserve">
  <b> </b>
  <c xml:space="default">
    <d> </d>
  </c>
</a>"#,
            r#"((a (@ (xml:space "preserve")) "\n  " (b " ") "\n  " (c (@ (xml:space "default")) (d)) "\n"))"#,
        );
    }

    #[test]
    fn preserves_cdata_whitespace() {
        lowers("<p><![CDATA[   ]]></p>", r#"((p "   "))"#);
    }

    #[test]
    fn elides_comments_by_default() {
        lowers("<a>\n  <!-- x -->\n</a>", "((a))");
    }

    #[test]
    fn preserves_comments_with_option() {
        let options = ParserOptions::default().preserve_comments(true);
        assert_eq!(
            XmlParser::with_options(options)
                .parse("<a><!-- hi --></a>")
                .map(|sexp| sexp.to_string()),
            Ok("((a (!comment \" hi \")))".to_owned())
        );
    }

    #[test]
    fn keeps_comment_content_verbatim() {
        let options = ParserOptions::default().preserve_comments(true);
        assert_eq!(
            XmlParser::with_options(options)
                .parse("<!-- &amp; -->")
                .map(|sexp| sexp.to_string()),
            Ok("((!comment \" &amp; \"))".to_owned())
        );
    }

    #[test]
    fn preserves_whitespace_between_nodes_with_option() {
        let options = ParserOptions::default().preserve_whitespace(true);
        assert_eq!(
            XmlParser::with_options(options)
                .parse("<list>\n  <item>one</item>\n</list>")
                .map(|sexp| sexp.to_string()),
            Ok("((list \"\\n  \" (item \"one\") \"\\n\"))".to_owned())
        );
    }
}
