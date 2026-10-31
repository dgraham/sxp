# sxp-yaml

Convert YAML documents from standard input with `sxp yaml`. Mappings and
sequences are represented as `(map ...)` and `(seq ...)`, nulls as `nil`, and
custom tags as `(tag ...)`. A stream with multiple documents is wrapped in
`(documents ...)`.

```console
$ printf 'title: Example\nactive: true\nscores: [3, 5]\n' | sxp yaml
(map ("active" true) ("scores" (seq 3 5)) ("title" "Example"))
```

The binary supports `--print-pattern`, `--validate`, and `--query PATTERN`.
Its grammar includes typed scalars, tags, sequences, mappings, and
multi-document streams.

```console
$ printf 'scores: [3, 5]' | sxp yaml | sxp yaml --validate
$ printf 'scores: [3, 5]' | sxp yaml | sxp yaml --query '(capture arrays (list seq (repeat (any))))'
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.
