mod grammar;
pub use grammar::grammar;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use sxp::{ParseError, Parser as SexpParser, Result, Sexp, SymbolPool};

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
pub struct MarkdownParser {
    pool: SymbolPool,
}

impl MarkdownParser {
    fn symbol(&mut self, text: &str) -> Sexp {
        self.pool.symbol(text)
    }

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
        self.parse_document(input)
    }
}

impl SexpParser for MarkdownParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        MarkdownParser::parse(self, input)
    }
}

impl MarkdownParser {
    fn parse_document(&mut self, input: &str) -> Result<Sexp> {
        let mut stack: Vec<Vec<Sexp>> = Vec::new();
        let mut roots = Vec::new();

        for event in Parser::new_ext(input, Options::all()) {
            match event {
                Event::Start(tag) => stack.push(self.tag_values(tag)),
                Event::End(_) => {
                    let values = stack.pop().ok_or_else(|| {
                        ParseError::new("Markdown parser emitted an unmatched end tag", 0)
                    })?;
                    append(Sexp::list(values), &mut stack, &mut roots);
                }
                Event::Text(text) => append(Sexp::string(text), &mut stack, &mut roots),
                Event::Code(code) => append(
                    Sexp::list([self.symbol("code"), Sexp::string(code)]),
                    &mut stack,
                    &mut roots,
                ),
                Event::Html(html) | Event::InlineHtml(html) => append(
                    Sexp::list([self.symbol("html"), Sexp::string(html)]),
                    &mut stack,
                    &mut roots,
                ),
                Event::SoftBreak => append(self.symbol("soft-break"), &mut stack, &mut roots),
                Event::HardBreak => append(self.symbol("hard-break"), &mut stack, &mut roots),
                Event::Rule => append(Sexp::list([self.symbol("rule")]), &mut stack, &mut roots),
                Event::TaskListMarker(checked) => append(
                    Sexp::list([self.symbol("task"), Sexp::boolean(checked)]),
                    &mut stack,
                    &mut roots,
                ),
                Event::FootnoteReference(label) => append(
                    Sexp::list([self.symbol("footnote-reference"), Sexp::string(label)]),
                    &mut stack,
                    &mut roots,
                ),
                _ => {}
            }
        }

        Ok(Sexp::list(roots))
    }
}

impl MarkdownParser {
    fn tag_values(&mut self, tag: Tag<'_>) -> Vec<Sexp> {
        match tag {
            Tag::Paragraph => vec![self.symbol("paragraph")],
            Tag::Heading { level, .. } => vec![
                self.symbol("heading"),
                Sexp::list([self.symbol("level"), self.symbol(&level.to_string())]),
            ],
            Tag::BlockQuote(_) => vec![self.symbol("blockquote")],
            Tag::CodeBlock(kind) => {
                let mut values = vec![self.symbol("code-block")];
                if let CodeBlockKind::Fenced(language) = kind {
                    values.push(Sexp::list([
                        self.symbol("language"),
                        Sexp::string(language),
                    ]));
                }
                values
            }
            Tag::HtmlBlock => vec![self.symbol("html-block")],
            Tag::List(start) => {
                let mut values = vec![self.symbol("list")];
                if let Some(start) = start {
                    values.push(Sexp::list([
                        self.symbol("start"),
                        self.symbol(&start.to_string()),
                    ]));
                }
                values
            }
            Tag::Item => vec![self.symbol("item")],
            Tag::FootnoteDefinition(label) => {
                vec![self.symbol("footnote-definition"), Sexp::string(label)]
            }
            Tag::Table(_) => vec![self.symbol("table")],
            Tag::TableHead => vec![self.symbol("table-head")],
            Tag::TableRow => vec![self.symbol("table-row")],
            Tag::TableCell => vec![self.symbol("table-cell")],
            Tag::Emphasis => vec![self.symbol("emphasis")],
            Tag::Strong => vec![self.symbol("strong")],
            Tag::Strikethrough => vec![self.symbol("strikethrough")],
            Tag::Link {
                dest_url, title, ..
            } => vec![
                self.symbol("link"),
                Sexp::list([
                    self.symbol("@"),
                    Sexp::list([self.symbol("href"), Sexp::string(dest_url)]),
                    Sexp::list([self.symbol("title"), Sexp::string(title)]),
                ]),
            ],
            Tag::Image {
                dest_url, title, ..
            } => vec![
                self.symbol("image"),
                Sexp::list([
                    self.symbol("@"),
                    Sexp::list([self.symbol("src"), Sexp::string(dest_url)]),
                    Sexp::list([self.symbol("title"), Sexp::string(title)]),
                ]),
            ],
            _ => vec![self.symbol("unknown")],
        }
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
    use super::MarkdownParser;
    use sxp::{Atom, Result, Sexp};

    fn parse(input: &str) -> Result<Sexp> {
        MarkdownParser::new().parse(input)
    }

    fn lowers(input: &str, expected: &str) -> sxp::Sexp {
        let value = parse(input).expect("should parse");
        assert_eq!(value.to_string(), expected);
        value
    }

    #[test]
    fn parses_markdown_blocks_and_inline_content() {
        lowers(
            "# Hello *world*",
            r#"((heading (level h1) "Hello " (emphasis "world")))"#,
        );
    }

    #[test]
    fn preserves_typed_task_markers() {
        let task = parse("- [x] checked\n- [ ] unchecked").unwrap();
        let task_atoms: Vec<_> = task
            .iter()
            .filter_map(|node| match node {
                Sexp::Atom(atom) => Some(atom),
                Sexp::List(_) => None,
            })
            .collect();
        assert!(task_atoms.contains(&&Atom::Boolean(true)));
        assert!(task_atoms.contains(&&Atom::Boolean(false)));
    }
}
