# sxp-toml

Convert TOML documents from standard input with `sxp toml`. Tables and arrays
are represented as `(table ...)` and `(array ...)`, and datetimes retain
their type.

```console
$ printf 'title = "Example"\nactive = true\nscores = [3, 5]\n' | sxp toml
(table ("active" true) ("scores" (array 3 5)) ("title" "Example"))
```

The binary supports `--print-pattern`, `--validate`, and `--query PATTERN`.
Its grammar includes typed scalar values, datetimes, arrays, and nested
tables.

```console
$ printf 'scores = [3, 5]' | sxp toml | sxp toml --validate
$ printf 'scores = [3, 5]' | sxp toml | sxp toml --query '(capture arrays (list array (repeat (any))))'
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.
