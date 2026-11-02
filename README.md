# sxp

Convert structured text to pretty-printed S-expressions.

## Usage

Format an S-expression from standard input with `sxp sexp`:

```console
$ printf '%s' \
  '(robots (robot (name "Hubot") (role gifs)) ' \
  '(robot (name "Bender") (role bending)) ' \
  '(robot (name "Baymax") (role doctor)))' \
  | sxp sexp
(robots
  (robot (name "Hubot") (role gifs))
  (robot (name "Bender") (role bending))
  (robot (name "Baymax") (role doctor)))
```

## Crates

| Crate | Description |
|---|---
| [sxp](crates/sxp/README.md) | `sxp` command and shared parser types |
| [sxp-core](crates/sxp-core/README.md) | Incremental, zero-copy, allocation-free, `no_std` S-expression lexer |
| [sxp-csv](crates/sxp-csv/README.md) | CSV → S-expression |
| [sxp-html](crates/sxp-html/README.md) | HTML → S-expression |
| [sxp-json](crates/sxp-json/README.md) | JSON → S-expression |
| [sxp-md](crates/sxp-md/README.md) | Markdown → S-expression |
| [sxp-pg](crates/sxp-pg/README.md) | PostgreSQL → S-expression |
| [sxp-promql](crates/sxp-promql/README.md) | PromQL → S-expression |
| [sxp-sexp](crates/sxp-sexp/README.md) | S-expression → S-expression |
| [sxp-toml](crates/sxp-toml/README.md) | TOML → S-expression |
| [sxp-xml](crates/sxp-xml/README.md) | XML → S-expression |
| [sxp-yaml](crates/sxp-yaml/README.md) | YAML → S-expression |

## Development

```console
$ cargo test
$ cargo build --release
```

## License

Distributed under the MIT license. See [LICENSE](LICENSE).
