use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::rc::Rc;
use std::str::FromStr;

pub mod cli;
pub mod color;
mod parser;
pub mod pattern;
mod reader;

pub use color::{ColorChoice, Colors, write, write_stdout};
pub use parser::SexpParser;
pub use reader::SexpReader;

use color::styled;
use termcolor::{Ansi, WriteColor};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    message: String,
    offset: usize,
}

/// Result alias for parsing functions, using [`ParseError`] as the error type.
pub type Result<T> = std::result::Result<T, ParseError>;

/// The output format for rendered S-expressions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// Every item on a single line, separated by one space.
    Compact,
    /// Omits optional whitespace, separating adjacent bare atoms only.
    Minified,
    /// Lists stay on one line while they fit within `width` columns,
    /// otherwise items break onto indented lines.
    Horizontal {
        /// Target line width.
        width: usize,
    },
    /// Every item on its own line, indented by nesting depth.
    Vertical,
    /// Prints strings without quotes, like Scheme's display procedure.
    Display,
    /// Each token (atom or parenthesis) on its own line, without color, for
    /// line-oriented tools such as awk.
    Token,
    /// One tab-separated line per node — depth, structural type, and value —
    /// for debugging and shell scripts. Never colorized.
    Inspect,
}

impl Default for Format {
    fn default() -> Self {
        Self::Horizontal {
            width: PRETTY_WIDTH,
        }
    }
}

/// Returned when a format name does not match a [`Format`] variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatParseError {
    text: String,
}

impl Display for FormatParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid format {:?}: expected compact, minified, horizontal, vertical, display, token, or inspect",
            self.text
        )
    }
}

impl std::error::Error for FormatParseError {}

impl Format {
    /// Replaces the target width of the horizontal format.
    pub fn with_width(self, width: usize) -> Self {
        match self {
            Self::Horizontal { .. } => Self::Horizontal { width },
            other => other,
        }
    }
}

impl FromStr for Format {
    type Err = FormatParseError;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        match text {
            "compact" => Ok(Self::Compact),
            "minified" => Ok(Self::Minified),
            "horizontal" => Ok(Self::Horizontal {
                width: PRETTY_WIDTH,
            }),
            "vertical" => Ok(Self::Vertical),
            "display" => Ok(Self::Display),
            "token" => Ok(Self::Token),
            "inspect" => Ok(Self::Inspect),
            _ => Err(FormatParseError {
                text: text.to_string(),
            }),
        }
    }
}

impl ParseError {
    pub fn new(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl Display for ParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

/// An error returned while parsing expressions from a reader.
#[derive(Debug)]
pub enum SexpReaderError {
    Io(io::Error),
    Parse(ParseError),
    Failed,
}

impl Display for SexpReaderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, formatter),
            Self::Parse(error) => Display::fmt(error, formatter),
            Self::Failed => formatter.write_str("reader parser cannot continue after an error"),
        }
    }
}

impl std::error::Error for SexpReaderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Failed => None,
        }
    }
}

impl From<io::Error> for SexpReaderError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ParseError> for SexpReaderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

/// Result type returned by [`SexpReader`].
pub type SexpReaderResult<T> = std::result::Result<T, SexpReaderError>;

pub trait Parser {
    fn parse(&mut self, input: &str) -> Result<Sexp>;
}

/// A node in an S-expression tree: an atom or a parenthesized list.
#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    Atom(Atom),
    List(Box<[Sexp]>),
}

/// A scalar S-expression value.
#[derive(Clone, Debug, PartialEq)]
pub enum Atom {
    Symbol(Rc<str>),
    Integer(Box<str>),
    Float(Box<str>),
    String(Box<str>),
    Boolean(bool),
    Nil,
}

impl Sexp {
    pub fn symbol(value: impl Into<String>) -> Self {
        Self::Atom(Atom::Symbol(Rc::from(value.into())))
    }

    pub fn integer(value: impl Into<String>) -> Self {
        Self::Atom(Atom::Integer(value.into().into_boxed_str()))
    }

    pub fn float(value: impl Into<String>) -> Self {
        Self::Atom(Atom::Float(value.into().into_boxed_str()))
    }

    pub fn boolean(value: bool) -> Self {
        Self::Atom(Atom::Boolean(value))
    }

    pub fn nil() -> Self {
        Self::Atom(Atom::Nil)
    }

    pub fn string(value: impl Into<String>) -> Self {
        Self::Atom(Atom::String(value.into().into_boxed_str()))
    }

    pub fn list(values: impl IntoIterator<Item = Sexp>) -> Self {
        Self::List(values.into_iter().collect())
    }

    /// Returns whether this expression is a list.
    pub fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }

    /// Returns whether this expression is an atom.
    pub fn is_atom(&self) -> bool {
        matches!(self, Self::Atom(_))
    }

    /// Returns the first item of a non-empty list.
    pub fn first(&self) -> Option<&Self> {
        match self {
            Self::List(values) => values.first(),
            Self::Atom(_) => None,
        }
    }

    /// Consumes a list and returns its first item.
    ///
    /// Returns `None` for atoms and empty lists.
    pub fn into_first(self) -> Option<Self> {
        match self {
            Self::List(values) => values.into_vec().into_iter().next(),
            Self::Atom(_) => None,
        }
    }

    /// Consumes a non-empty list and returns its remaining items as a list.
    ///
    /// Returns `None` for atoms and empty lists.
    pub fn into_rest(self) -> Option<Self> {
        let Self::List(values) = self else {
            return None;
        };
        let mut values = values.into_vec().into_iter();
        values.next()?;
        Some(Self::list(values))
    }

    pub fn iter(&self) -> SexpIter<'_> {
        self.into_iter()
    }

    /// Iterates over the tree in pre-order, pairing each node with its
    /// nesting depth. The root has depth 0 and every child is one deeper.
    pub fn iter_with_depth(&self) -> WithDepth<'_> {
        WithDepth::new(self)
    }
}

impl Atom {
    /// The variant name, as printed by the inspect format.
    fn type_name(&self) -> &'static str {
        match self {
            Self::Symbol(_) => "Symbol",
            Self::Integer(_) => "Integer",
            Self::Float(_) => "Float",
            Self::String(_) => "String",
            Self::Boolean(_) => "Boolean",
            Self::Nil => "Nil",
        }
    }

    /// Returns the value as an `i64` when the text fits.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => value.parse().ok(),
            _ => None,
        }
    }

    /// Returns the value as a `u64` when the text fits.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Integer(value) => value.parse().ok(),
            _ => None,
        }
    }

    /// Returns the value as an `f64`. Out-of-range text such
    /// as `1e567` yields the infinities rather than `None`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Integer(value) | Self::Float(value) => value.parse().ok(),
            _ => None,
        }
    }

    /// Returns the value as a `bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the value as a `&str`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// Whether the text fits in an `i64`.
    pub fn is_i64(&self) -> bool {
        self.as_i64().is_some()
    }

    /// Whether the text fits in a `u64`.
    pub fn is_u64(&self) -> bool {
        self.as_u64().is_some()
    }

    /// Whether the text is representable as an `f64`. True for
    /// integer text too, since integers convert losslessly.
    pub fn is_f64(&self) -> bool {
        self.as_f64().is_some()
    }

    /// Whether this is a boolean atom.
    pub fn is_boolean(&self) -> bool {
        matches!(self, Self::Boolean(_))
    }

    /// Whether this is a string atom.
    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    /// Whether this is the nil atom.
    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }

    /// Whether this is a symbol atom.
    pub fn is_symbol(&self) -> bool {
        matches!(self, Self::Symbol(_))
    }

    /// Whether this is an integer atom. Unlike `is_i64`/`is_u64`, this is
    /// a variant check and is true regardless of numeric range.
    pub fn is_integer(&self) -> bool {
        matches!(self, Self::Integer(_))
    }

    /// Whether this is a float atom. Unlike `is_f64`, this is a variant
    /// check and is false for integer atoms.
    pub fn is_float(&self) -> bool {
        matches!(self, Self::Float(_))
    }

    fn compact_width(&self) -> usize {
        match self {
            Self::Symbol(value) => value.chars().count(),
            Self::Integer(value) | Self::Float(value) => value.chars().count(),
            Self::String(value) => {
                2 + value
                    .chars()
                    .map(|ch| match ch {
                        '"' | '\\' | '\n' | '\r' | '\t' => 2,
                        ch if ch.is_control() => {
                            let codepoint = ch as u32;
                            let hex_digits =
                                (u32::BITS - codepoint.leading_zeros()).div_ceil(4).max(1);
                            4 + hex_digits as usize
                        }
                        _ => 1,
                    })
                    .sum::<usize>()
            }
            Self::Boolean(true) => 4,
            Self::Boolean(false) => 5,
            Self::Nil => 3,
        }
    }
}

impl Display for Atom {
    /// Renders the atom as it appears in S-expression output: strings are
    /// quoted and escaped, every other variant prints its bare text.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(value) => formatter.write_str(value),
            Self::Integer(value) | Self::Float(value) => formatter.write_str(value),
            Self::String(value) => {
                formatter.write_str("\"")?;
                let mut start = 0;
                for (index, ch) in value.char_indices() {
                    let escape = match ch {
                        '"' => Some("\\\""),
                        '\\' => Some("\\\\"),
                        '\n' => Some("\\n"),
                        '\r' => Some("\\r"),
                        '\t' => Some("\\t"),
                        _ => None,
                    };
                    if let Some(escape) = escape {
                        formatter.write_str(&value[start..index])?;
                        formatter.write_str(escape)?;
                        start = index + ch.len_utf8();
                    } else if ch.is_control() {
                        formatter.write_str(&value[start..index])?;
                        write!(formatter, "\\u{{{:x}}}", ch as u32)?;
                        start = index + ch.len_utf8();
                    }
                }
                formatter.write_str(&value[start..])?;
                formatter.write_str("\"")
            }
            Self::Boolean(value) => write!(formatter, "{value}"),
            Self::Nil => formatter.write_str("nil"),
        }
    }
}

/// Interns symbol strings so repeated keywords and names share one allocation.
#[derive(Default)]
pub struct SymbolPool {
    cache: HashSet<Rc<str>>,
    max_entries: Option<usize>,
}

impl SymbolPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a pool that retains at most `max_entries` distinct symbols.
    ///
    /// Symbols already in the pool are still reused after the limit is reached.
    /// New symbols beyond the limit are returned without being cached. A
    /// limit of zero disables caching.
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            cache: HashSet::new(),
            max_entries: Some(max_entries),
        }
    }

    pub fn intern(&mut self, text: &str) -> Rc<str> {
        if let Some(cached) = self.cache.get(text) {
            Rc::clone(cached)
        } else {
            let atom = Rc::from(text);
            let can_cache = match self.max_entries {
                Some(max_entries) => self.cache.len() < max_entries,
                None => true,
            };
            if can_cache {
                self.cache.insert(Rc::clone(&atom));
            }
            atom
        }
    }

    pub fn symbol(&mut self, text: &str) -> Sexp {
        Sexp::Atom(Atom::Symbol(self.intern(text)))
    }

    /// Clears cached symbols while retaining the pool's allocated capacity.
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParserOptions {
    max_symbol_entries: Option<usize>,
    typed_atoms: bool,
    typed_booleans: bool,
}

impl ParserOptions {
    /// Sets the maximum number of distinct symbols retained for reuse.
    /// `None` leaves the pool uncapped. `Some(0)` disables symbol caching.
    pub fn max_symbol_entries(mut self, max_entries: Option<usize>) -> Self {
        self.max_symbol_entries = max_entries;
        self
    }

    /// Parses `true`, `false`, `nil`, and numbers as typed atoms.
    ///
    /// This is disabled by default so generic S-expression parsing continues
    /// to treat every bare token as a symbol.
    pub fn typed_atoms(mut self, enabled: bool) -> Self {
        self.typed_atoms = enabled;
        self
    }

    /// Parses `true` and `false` as boolean atoms without parsing numeric
    /// tokens or `nil` as typed atoms.
    pub fn typed_booleans(mut self, enabled: bool) -> Self {
        self.typed_booleans = enabled;
        self
    }
}

/// Target line width the horizontal format tries to fit lists within.
pub const PRETTY_WIDTH: usize = 80;

impl Display for Sexp {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let mut buffer = Ansi::new(Vec::new());

        let result = if formatter.alternate() {
            self.fmt_pretty(&mut buffer, None, 0, 0)
        } else {
            self.fmt_compact(&mut buffer, None)
        };
        result.map_err(|_| fmt::Error)?;

        let text = String::from_utf8(buffer.into_inner()).map_err(|_| fmt::Error)?;
        formatter.write_str(&text)
    }
}

impl Sexp {
    fn fmt_display(&self, out: &mut dyn WriteColor) -> io::Result<()> {
        match self {
            Self::Atom(Atom::String(value)) => out.write_all(value.as_bytes()),
            Self::Atom(atom) => write!(out, "{atom}"),
            Self::List(values) => {
                out.write_all(b"(")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        out.write_all(b" ")?;
                    }
                    value.fmt_display(out)?;
                }
                out.write_all(b")")
            }
        }
    }

    fn fmt_compact(&self, out: &mut dyn WriteColor, colors: Option<&Colors>) -> io::Result<()> {
        match self {
            Self::Atom(atom @ Atom::String(_)) => {
                styled(out, colors.map(|colors| &colors.string), |out| {
                    write!(out, "{atom}")
                })
            }
            Self::Atom(atom) => styled(out, colors.map(|colors| &colors.atom), |out| {
                write!(out, "{atom}")
            }),
            Self::List(values) => {
                styled(out, colors.map(|colors| &colors.punct), |out| {
                    out.write_all(b"(")
                })?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        out.write_all(b" ")?;
                    }
                    value.fmt_compact(out, colors)?;
                }
                styled(out, colors.map(|colors| &colors.punct), |out| {
                    out.write_all(b")")
                })
            }
        }
    }

    fn fmt_minified(&self, out: &mut dyn WriteColor, colors: Option<&Colors>) -> io::Result<()> {
        match self {
            Self::Atom(atom @ Atom::String(_)) => {
                styled(out, colors.map(|colors| &colors.string), |out| {
                    write!(out, "{atom}")
                })
            }
            Self::Atom(atom) => styled(out, colors.map(|colors| &colors.atom), |out| {
                write!(out, "{atom}")
            }),
            Self::List(values) => {
                styled(out, colors.map(|colors| &colors.punct), |out| {
                    out.write_all(b"(")
                })?;
                let mut previous_was_bare_atom = false;
                for value in values {
                    let is_bare_atom =
                        matches!(value, Self::Atom(atom) if !matches!(atom, Atom::String(_)));
                    if previous_was_bare_atom && is_bare_atom {
                        out.write_all(b" ")?;
                    }
                    value.fmt_minified(out, colors)?;
                    previous_was_bare_atom = is_bare_atom;
                }
                styled(out, colors.map(|colors| &colors.punct), |out| {
                    out.write_all(b")")
                })
            }
        }
    }

    fn fmt_pretty(
        &self,
        out: &mut dyn WriteColor,
        colors: Option<&Colors>,
        indent: usize,
        width: usize,
    ) -> io::Result<()> {
        match self {
            Self::Atom(atom @ Atom::String(_)) => {
                styled(out, colors.map(|colors| &colors.string), |out| {
                    write!(out, "{atom}")
                })
            }
            Self::Atom(atom) => styled(out, colors.map(|colors| &colors.atom), |out| {
                write!(out, "{atom}")
            }),
            Self::List(values) if values.is_empty() => {
                styled(out, colors.map(|colors| &colors.punct), |out| {
                    out.write_all(b"()")
                })
            }
            Self::List(values) => {
                // Keep the list on one line when it fits within the width
                // budget. The compact rendering is the horizontal layout.
                if indent + self.compact_width() <= width {
                    return self.fmt_compact(out, colors);
                }
                styled(out, colors.map(|colors| &colors.punct), |out| {
                    out.write_all(b"(")
                })?;
                values[0].fmt_pretty(out, colors, indent, width)?;
                for value in &values[1..] {
                    out.write_all(b"\n")?;
                    for _ in 0..indent + 2 {
                        out.write_all(b" ")?;
                    }
                    value.fmt_pretty(out, colors, indent + 2, width)?;
                }
                styled(out, colors.map(|colors| &colors.punct), |out| {
                    out.write_all(b")")
                })
            }
        }
    }

    /// Writes each token (atom or parenthesis) on its own line so
    /// line-oriented tools such as awk can process the stream.
    fn fmt_token(&self, out: &mut dyn WriteColor) -> io::Result<()> {
        match self {
            Self::Atom(atom) => writeln!(out, "{atom}"),
            Self::List(values) => {
                writeln!(out, "(")?;
                for value in values {
                    value.fmt_token(out)?;
                }
                writeln!(out, ")")
            }
        }
    }

    /// Writes one tab-separated line per node in pre-order: nesting depth,
    /// structural type, and value. Lists report their item count; atoms
    /// print their S-expression text, with strings quoted and escaped so
    /// values never contain tabs or newlines. Output is never colorized so
    /// the columns stay machine-readable.
    fn fmt_inspect(&self, out: &mut dyn WriteColor) -> io::Result<()> {
        for (depth, node) in self.iter_with_depth() {
            let (kind, value) = match node {
                Self::List(values) => ("List", values.len().to_string()),
                Self::Atom(atom) => (atom.type_name(), atom.to_string()),
            };
            writeln!(out, "{depth}\t{kind}\t{value}")?;
        }
        Ok(())
    }

    fn compact_width(&self) -> usize {
        match self {
            Self::Atom(atom) => atom.compact_width(),
            Self::List(values) => {
                2 + values.len().saturating_sub(1)
                    + values.iter().map(Self::compact_width).sum::<usize>()
            }
        }
    }
}

pub struct SexpIter<'a> {
    stack: Vec<&'a Sexp>,
}

impl<'a> SexpIter<'a> {
    fn new(sexp: &'a Sexp) -> Self {
        Self { stack: vec![sexp] }
    }
}

impl<'a> Iterator for SexpIter<'a> {
    type Item = &'a Sexp;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        if let Sexp::List(values) = node {
            self.stack.extend(values.iter().rev());
        }
        Some(node)
    }
}

impl<'a> IntoIterator for &'a Sexp {
    type Item = &'a Sexp;
    type IntoIter = SexpIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        SexpIter::new(self)
    }
}

/// Iterates over a [`Sexp`] tree in pre-order, yielding each node with its
/// nesting depth. Created by [`Sexp::iter_with_depth`].
pub struct WithDepth<'a> {
    stack: Vec<(usize, &'a Sexp)>,
}

impl<'a> WithDepth<'a> {
    fn new(sexp: &'a Sexp) -> Self {
        Self {
            stack: vec![(0, sexp)],
        }
    }
}

impl<'a> Iterator for WithDepth<'a> {
    type Item = (usize, &'a Sexp);

    fn next(&mut self) -> Option<Self::Item> {
        let (depth, node) = self.stack.pop()?;

        if let Sexp::List(values) = node {
            self.stack
                .extend(values.iter().rev().map(|value| (depth + 1, value)));
        }

        Some((depth, node))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Atom, Format, PRETTY_WIDTH, ParserOptions, Result, Sexp, SexpParser, SymbolPool, write,
    };
    use std::rc::Rc;
    use termcolor::NoColor;

    fn parse(input: &str) -> Result<Sexp> {
        SexpParser::new().parse(input)
    }

    fn render(value: &Sexp, format: Format) -> String {
        let mut buffer = Vec::new();
        write(value, &mut NoColor::new(&mut buffer), format, None).unwrap();
        String::from_utf8(buffer).unwrap()
    }

    #[test]
    fn with_width_only_affects_horizontal() {
        assert_eq!(Format::Compact.with_width(40), Format::Compact);
        assert_eq!(Format::Vertical.with_width(40), Format::Vertical);
        assert_eq!(
            Format::Horizontal { width: 80 }.with_width(40),
            Format::Horizontal { width: 40 }
        );
    }

    #[test]
    fn vertical_tab_separates_atoms() {
        assert_eq!(
            parse("(foo\x0bbar)"),
            Ok(Sexp::list([Sexp::symbol("foo"), Sexp::symbol("bar")]))
        );
    }

    #[test]
    fn default_format_is_horizontal() {
        assert_eq!(
            Format::default(),
            Format::Horizontal {
                width: PRETTY_WIDTH,
            }
        );
    }

    #[test]
    fn is_list_identifies_list_expressions() {
        assert!(Sexp::list([]).is_list());
        assert!(!Sexp::symbol("atom").is_list());
    }

    #[test]
    fn is_atom_identifies_atom_expressions() {
        assert!(Sexp::symbol("atom").is_atom());
        assert!(!Sexp::list([]).is_atom());
    }

    #[test]
    fn first_borrows_the_first_list_item() {
        let list = Sexp::list([Sexp::symbol("first"), Sexp::symbol("rest")]);
        assert_eq!(list.first(), Some(&Sexp::symbol("first")));
        assert_eq!(Sexp::list([]).first(), None);
        assert_eq!(Sexp::symbol("atom").first(), None);
    }

    #[test]
    fn into_first_selects_the_first_list_item() {
        assert_eq!(
            Sexp::list([Sexp::symbol("first"), Sexp::symbol("rest")]).into_first(),
            Some(Sexp::symbol("first"))
        );
        assert_eq!(Sexp::list([]).into_first(), None);
        assert_eq!(Sexp::symbol("atom").into_first(), None);
    }

    #[test]
    fn into_rest_selects_the_remaining_list_items() {
        assert_eq!(
            Sexp::list([Sexp::symbol("first"), Sexp::symbol("rest")]).into_rest(),
            Some(Sexp::list([Sexp::symbol("rest")]))
        );
        assert_eq!(
            Sexp::list([Sexp::symbol("only")]).into_rest(),
            Some(Sexp::list([]))
        );
        assert_eq!(Sexp::list([]).into_rest(), None);
        assert_eq!(Sexp::symbol("atom").into_rest(), None);
    }

    #[test]
    fn symbol_pool_stops_caching_new_entries_at_its_limit() {
        let mut pool = SymbolPool::with_max_entries(1);
        let cached = pool.intern("cached");
        let cached_again = pool.intern("cached");
        let uncached = pool.intern("uncached");
        let uncached_again = pool.intern("uncached");
        let cached_after_limit = pool.intern("cached");

        assert!(Rc::ptr_eq(&cached, &cached_again));
        assert!(!Rc::ptr_eq(&uncached, &uncached_again));
        assert!(Rc::ptr_eq(&cached, &cached_after_limit));
    }

    #[test]
    fn symbol_pool_with_zero_entries_does_not_cache_symbols() {
        let mut pool = SymbolPool::with_max_entries(0);
        let first = pool.intern("symbol");
        let second = pool.intern("symbol");

        assert!(!Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn format_parses_names() {
        assert_eq!("compact".parse::<Format>(), Ok(Format::Compact));
        assert_eq!("minified".parse::<Format>(), Ok(Format::Minified));
        assert_eq!(
            "horizontal".parse::<Format>(),
            Ok(Format::Horizontal {
                width: PRETTY_WIDTH,
            })
        );
        assert_eq!("vertical".parse::<Format>(), Ok(Format::Vertical));
        assert_eq!("display".parse::<Format>(), Ok(Format::Display));
        assert_eq!("token".parse::<Format>(), Ok(Format::Token));
        assert_eq!("inspect".parse::<Format>(), Ok(Format::Inspect));
        assert_eq!(
            "quick".parse::<Format>().map_err(|e| e.to_string()),
            Err(
                "invalid format \"quick\": expected compact, minified, horizontal, vertical, display, token, or inspect"
                    .to_owned()
            )
        );
    }

    #[test]
    fn display_omits_string_quotes_recursively() {
        let value = Sexp::list([
            Sexp::string("SELECT 1"),
            Sexp::list([Sexp::string("two"), Sexp::symbol("three")]),
        ]);

        assert_eq!(render(&value, Format::Display), "(SELECT 1 (two three))");
    }

    #[test]
    fn horizontal_keeps_fitting_lists_on_one_line() {
        let value = Sexp::list([
            Sexp::symbol("select"),
            Sexp::list([
                Sexp::symbol("columns"),
                Sexp::symbol("id"),
                Sexp::symbol("name"),
            ]),
        ]);

        assert_eq!(
            render(
                &value,
                Format::Horizontal {
                    width: PRETTY_WIDTH
                }
            ),
            "(select (columns id name))"
        );
        assert_eq!(
            render(
                &value,
                Format::Horizontal {
                    width: PRETTY_WIDTH
                }
            ),
            render(&value, Format::Compact)
        );
    }

    #[test]
    fn minified_omits_optional_whitespace_and_stays_parseable() {
        let value = Sexp::list([
            Sexp::symbol("select"),
            Sexp::list([
                Sexp::symbol("columns"),
                Sexp::integer("1"),
                Sexp::string("hi"),
            ]),
        ]);
        let output = render(&value, Format::Minified);

        assert_eq!(output, "(select(columns 1\"hi\"))");
        let mut parser = SexpParser::with_options(ParserOptions::default().typed_atoms(true));
        assert_eq!(parser.parse(&output).unwrap(), value);
    }

    #[test]
    fn horizontal_breaks_lists_over_the_width() {
        let value = Sexp::list([
            Sexp::symbol("select"),
            Sexp::symbol("id"),
            Sexp::symbol("name"),
            Sexp::symbol("email_address"),
            Sexp::symbol("created_at_timestamp"),
            Sexp::symbol("updated_at_timestamp"),
            Sexp::symbol("deleted_at_timestamp"),
        ]);

        assert_eq!(
            render(
                &value,
                Format::Horizontal {
                    width: PRETTY_WIDTH
                }
            ),
            "(select\n  id\n  name\n  email_address\n  created_at_timestamp\n  updated_at_timestamp\n  deleted_at_timestamp)"
        );
    }

    #[test]
    fn horizontal_keeps_short_inner_lists_inline() {
        let value = Sexp::list([
            Sexp::symbol("select"),
            Sexp::list([
                Sexp::symbol("columns"),
                Sexp::symbol("id"),
                Sexp::symbol("name"),
            ]),
            Sexp::list([Sexp::symbol("from"), Sexp::symbol("users")]),
            Sexp::list([
                Sexp::symbol("where"),
                Sexp::list([Sexp::symbol(">"), Sexp::symbol("age"), Sexp::integer("18")]),
            ]),
            Sexp::list([
                Sexp::symbol("order-by"),
                Sexp::symbol("created_at"),
                Sexp::symbol("updated_at"),
            ]),
            Sexp::list([Sexp::symbol("limit"), Sexp::integer("100")]),
        ]);

        assert_eq!(
            render(
                &value,
                Format::Horizontal {
                    width: PRETTY_WIDTH
                }
            ),
            "(select\n  (columns id name)\n  (from users)\n  (where (> age 18))\n  (order-by created_at updated_at)\n  (limit 100))"
        );
    }

    #[test]
    fn horizontal_respects_width() {
        let value = Sexp::list([
            Sexp::symbol("select"),
            Sexp::symbol("id"),
            Sexp::symbol("name"),
        ]);

        assert_eq!(
            render(&value, Format::Horizontal { width: 16 }),
            "(select id name)"
        );
        assert_eq!(
            render(&value, Format::Horizontal { width: 15 }),
            "(select\n  id\n  name)"
        );
    }

    #[test]
    fn horizontal_counts_escaped_unicode_string_width_without_rendering() {
        let value = Sexp::list([Sexp::string("é\u{1}\n"), Sexp::symbol("x")]);

        assert_eq!(
            render(&value, Format::Horizontal { width: 14 }),
            "(\"é\\u{1}\\n\" x)"
        );
        assert_eq!(
            render(&value, Format::Horizontal { width: 13 }),
            "(\"é\\u{1}\\n\"\n  x)"
        );
    }

    #[test]
    fn token_prints_each_token_on_its_own_line() {
        let value = Sexp::list([
            Sexp::symbol("p"),
            Sexp::list([
                Sexp::symbol("@"),
                Sexp::list([Sexp::symbol("class"), Sexp::string("a\"b")]),
            ]),
        ]);

        assert_eq!(
            render(&value, Format::Token),
            "(\np\n(\n@\n(\nclass\n\"a\\\"b\"\n)\n)\n)\n"
        );
    }

    #[test]
    fn prints_compact_single_line_and_escapes_strings() {
        let value = Sexp::list([
            Sexp::symbol("p"),
            Sexp::list([
                Sexp::symbol("@"),
                Sexp::list([Sexp::symbol("class"), Sexp::string("a\"b")]),
            ]),
            Sexp::string("Hello\nworld"),
        ]);

        assert_eq!(
            value.to_string(),
            "(p (@ (class \"a\\\"b\")) \"Hello\\nworld\")"
        );
    }

    #[test]
    fn writes_escaped_unicode_and_control_characters() {
        let value = Sexp::list([Sexp::string("é\"\\\n\r\t\u{0}雪")]);

        assert_eq!(render(&value, Format::Compact), r#"("é\"\\\n\r\t\u{0}雪")"#);
    }

    #[test]
    fn alternate_prints_indented_multiline() {
        let value = Sexp::list([
            Sexp::symbol("p"),
            Sexp::list([
                Sexp::symbol("@"),
                Sexp::list([Sexp::symbol("class"), Sexp::string("a\"b")]),
            ]),
            Sexp::string("Hello\nworld"),
        ]);

        assert_eq!(
            format!("{value:#}"),
            "(p\n  (@\n    (class\n      \"a\\\"b\"))\n  \"Hello\\nworld\")"
        );
    }

    #[test]
    fn parses_lists_atoms_and_escaped_strings() {
        assert_eq!(
            parse(r#"(p (@ (class "a\"b")) "Hello\nworld")"#),
            Ok(Sexp::list([
                Sexp::symbol("p"),
                Sexp::list([
                    Sexp::symbol("@"),
                    Sexp::list([Sexp::symbol("class"), Sexp::string("a\"b")]),
                ]),
                Sexp::string("Hello\nworld"),
            ]))
        );
    }

    #[test]
    fn parses_unicode_and_ascii_whitespace() {
        let mut parser = SexpParser::new();

        assert_eq!(
            parser.parse("\t(é a\u{a0}b \"café\\n雪\") "),
            Ok(Sexp::list([
                Sexp::symbol("é"),
                Sexp::symbol("a\u{a0}b"),
                Sexp::string("café\n雪"),
            ]))
        );
    }

    #[test]
    fn prints_scalar_atoms() {
        let value = Sexp::list([
            Sexp::integer("42"),
            Sexp::float("3.5"),
            Sexp::boolean(true),
            Sexp::nil(),
            Sexp::symbol("sym"),
        ]);

        assert_eq!(value.to_string(), "(42 3.5 true nil sym)");
    }

    #[test]
    fn atom_accessors_report_typed_values() {
        let integer = Atom::Integer("42".into());
        let big = Atom::Integer("18446744073709551616".into());
        let float = Atom::Float("3.5".into());
        let string = Atom::String("text".into());
        let boolean = Atom::Boolean(true);
        let symbol = Atom::Symbol("sym".into());

        assert!(integer.is_i64() && integer.as_i64() == Some(42));
        assert!(integer.is_u64() && integer.as_u64() == Some(42));
        assert!(integer.is_f64() && integer.as_f64() == Some(42.0));
        assert!(!big.is_i64() && !big.is_u64());
        assert!(big.as_f64().is_some());
        assert!(!float.is_i64() && !float.is_u64());
        assert!(float.is_f64() && float.as_f64() == Some(3.5));
        assert_eq!(string.as_str(), Some("text"));
        assert!(string.is_string());
        assert_eq!(boolean.as_bool(), Some(true));
        assert!(boolean.is_boolean());
        assert!(!symbol.is_string());
        assert!(!symbol.is_boolean());
        assert!(Atom::Nil.is_nil());
        assert!(!boolean.is_nil());
        assert!(symbol.is_symbol());
        assert!(!string.is_symbol());
        assert!(integer.is_integer());
        assert!(!float.is_integer());
        assert!(float.is_float());
        assert!(!integer.is_float());
        assert_eq!(symbol.as_str(), None);
        assert_eq!(symbol.as_bool(), None);
    }

    #[test]
    fn parses_numbers_as_symbols() {
        assert_eq!(parse("42"), Ok(Sexp::symbol("42")));
        assert_eq!(parse("3.5"), Ok(Sexp::symbol("3.5")));
        assert_eq!(parse("true"), Ok(Sexp::symbol("true")));
    }

    #[test]
    fn parses_typed_atoms_when_enabled() {
        let mut parser = SexpParser::with_options(ParserOptions::default().typed_atoms(true));
        assert_eq!(
            parser.parse("(true false nil 42 -1.50 1e+450)"),
            Ok(Sexp::list([
                Sexp::boolean(true),
                Sexp::boolean(false),
                Sexp::nil(),
                Sexp::integer("42"),
                Sexp::float("-1.50"),
                Sexp::float("1e+450"),
            ]))
        );
        assert_eq!(
            parser.parse("01").map(|value| value.to_string()),
            Ok("01".to_owned())
        );
    }

    #[test]
    fn parses_typed_booleans_without_changing_other_bare_atoms() {
        let mut parser = SexpParser::with_options(ParserOptions::default().typed_booleans(true));
        assert_eq!(
            parser.parse("(true false 42 nil)"),
            Ok(Sexp::list([
                Sexp::boolean(true),
                Sexp::boolean(false),
                Sexp::symbol("42"),
                Sexp::symbol("nil"),
            ]))
        );
    }

    #[test]
    fn rejects_unterminated_lists() {
        assert_eq!(
            parse("(p").map_err(|error| error.to_string()),
            Err("unterminated list at byte 2".to_owned())
        );
    }

    #[test]
    fn iter_yields_nodes_in_preorder() {
        let value = Sexp::list([
            Sexp::symbol("select"),
            Sexp::list([Sexp::symbol("id"), Sexp::symbol("name")]),
            Sexp::list([Sexp::symbol("from"), Sexp::symbol("users")]),
        ]);

        assert_eq!(
            value.iter().cloned().collect::<Vec<_>>(),
            vec![
                Sexp::list([
                    Sexp::symbol("select"),
                    Sexp::list([Sexp::symbol("id"), Sexp::symbol("name")]),
                    Sexp::list([Sexp::symbol("from"), Sexp::symbol("users")]),
                ]),
                Sexp::symbol("select"),
                Sexp::list([Sexp::symbol("id"), Sexp::symbol("name")]),
                Sexp::symbol("id"),
                Sexp::symbol("name"),
                Sexp::list([Sexp::symbol("from"), Sexp::symbol("users")]),
                Sexp::symbol("from"),
                Sexp::symbol("users"),
            ]
        );
    }

    #[test]
    fn iter_yields_atom_or_empty_list_as_single_node() {
        assert_eq!(
            Sexp::symbol("x").iter().cloned().collect::<Vec<_>>(),
            vec![Sexp::symbol("x")]
        );
        assert_eq!(
            Sexp::integer("7").iter().cloned().collect::<Vec<_>>(),
            vec![Sexp::integer("7")]
        );
        assert_eq!(
            Sexp::list([]).iter().cloned().collect::<Vec<_>>(),
            vec![Sexp::list([])]
        );
    }

    #[test]
    fn iter_stays_exhausted() {
        let value = Sexp::list([Sexp::symbol("a")]);
        let mut iter = value.iter();

        assert!(iter.next().is_some());
        assert!(iter.next().is_some());
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn sexp_is_iterable_by_reference() {
        let value = Sexp::list([Sexp::symbol("a"), Sexp::list([Sexp::symbol("b")])]);

        let mut count = 0;
        for _ in &value {
            count += 1;
        }

        assert_eq!(count, 4);
    }

    #[test]
    fn iter_with_depth_pairs_each_node_with_its_depth() {
        let value = Sexp::list([
            Sexp::symbol("a"),
            Sexp::list([Sexp::list([Sexp::symbol("b")])]),
        ]);

        let depths: Vec<_> = value.iter_with_depth().map(|(depth, _)| depth).collect();

        assert_eq!(depths, [0, 1, 1, 2, 3]);
    }

    #[test]
    fn inspect_prints_depth_type_and_value_per_node() {
        let value = Sexp::list([
            Sexp::symbol("define"),
            Sexp::symbol("x"),
            Sexp::integer("42"),
        ]);

        assert_eq!(
            render(&value, Format::Inspect),
            "0\tList\t3\n\
             1\tSymbol\tdefine\n\
             1\tSymbol\tx\n\
             1\tInteger\t42\n"
        );
    }

    #[test]
    fn inspect_prints_every_atom_kind_and_escapes_tabs() {
        let value = Sexp::list([
            Sexp::list([]),
            Sexp::float("3.5"),
            Sexp::boolean(true),
            Sexp::nil(),
            Sexp::string("a\tb"),
        ]);

        assert_eq!(
            render(&value, Format::Inspect),
            "0\tList\t5\n\
             1\tList\t0\n\
             1\tFloat\t3.5\n\
             1\tBoolean\ttrue\n\
             1\tNil\tnil\n\
             1\tString\t\"a\\tb\"\n"
        );
    }
}
