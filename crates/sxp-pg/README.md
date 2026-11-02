# sxp-pg

Convert PostgreSQL SQL from standard input with `sxp pg`. The binary supports
`--print-pattern`, `--validate`, and `--query PATTERN`. Validation recognizes
known statement roots and node tags, with strict schemas for expressions,
query and DML statements, DDL, utility, and privilege families.

```console
$ printf 'SELECT 1' | sxp pg | sxp pg --validate
$ printf 'SELECT 1' | sxp pg --query '(capture selects (list select (repeat (any))))'
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.
