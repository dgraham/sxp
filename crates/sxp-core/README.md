# sxp-core

The core S-expression lexer used by the parser and streaming reader. Its
incremental, span-based reader returns byte ranges, rather than copying token
bytes, and supports reading consecutive top-level expressions.

See the [`sxp` crate](../sxp/README.md) for the `SexpParser` and `SexpReader`
types.
