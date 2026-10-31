# sxp

The `sxp` command dispatches each input type to a matching, independently
installable `sxp-*` binary found on `PATH` or `XDG_BIN_HOME` (default:
`~/.local/bin`). See the [main usage example](../../README.md#usage) and
per-crate docs for parser details.

Commands share options for output style (`--format`), width (`-w/--width`),
and color (`--color`, `--colors`), plus `--print-pattern`, `--validate`, and
`--query PATTERN`.

The crate provides `SexpParser` for in-memory input and `SexpReader` for
consecutive expressions from a `Read`. `SexpReader` buffers internally, so avoid
wrapping its input in another `BufReader`. See [`sxp-core`](../sxp-core/README.md)
for the low-level span-based lexer.
