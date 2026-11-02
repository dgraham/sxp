mod grammar;
pub use grammar::grammar;

use sxp::{Atom, ParseError, Parser, Result, Sexp, SymbolPool};

mod parser;

/// Configuration options for a PostgreSQL parser.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParserOptions {
    sqlcommenter: bool,
    symbol_pool_max_entries: Option<usize>,
}

impl ParserOptions {
    /// Sets whether sqlcommenter tags are represented in the parsed tree.
    pub fn sqlcommenter(mut self, enabled: bool) -> Self {
        self.sqlcommenter = enabled;
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
pub struct PostgresParser {
    pool: SymbolPool,
    options: ParserOptions,
}

impl PostgresParser {
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
        parser::parse(self, input)
    }
}

impl Parser for PostgresParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        PostgresParser::parse(self, input)
    }
}
