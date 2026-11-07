# sxp-pg

Convert PostgreSQL SQL from standard input with `sxp pg`. The binary supports
`--print-pattern`, `--validate`, and `--query PATTERN`. Validation recognizes
known statement roots and node tags, with strict schemas for expressions,
query and DML statements, DDL, utility, and privilege families.

```console
$ printf 'SELECT 1' | sxp pg | sxp pg --validate
$ printf 'SELECT 1' | sxp pg --query '(capture selects (list select (repeat (any))))'
```

Use `--lose` to stream each SQL statement as its own one-line
S-expression. This adopts Scheme's LOSE (Line-Oriented S-expressions)
convention, described in [SRFI 176][lose].

```console
$ printf 'SELECT 1; SELECT 2;' | sxp pg --lose
(select (columns 1))
(select (columns 2))
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.

[lose]: https://srfi.schemers.org/srfi-176/srfi-176.html#_line_oriented_s_expressions_lose
