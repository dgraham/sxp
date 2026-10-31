# sxp-csv

Convert CSV from standard input with `sxp csv`. By default, the
header is emitted as symbols and data records as lists. Integer and
float fields become typed numeric atoms with their original text,
`true` and `false` become boolean atoms, and other data fields
remain strings. Header values that cannot be represented safely as
bare symbols remain strings.

```console
$ printf 'name,role\nHubot,gifs\nBender,bending\nBaymax,doctor\n' | sxp csv
((name role) ("Hubot" "gifs") ("Bender" "bending") ("Baymax" "doctor"))
```

Use `--lose` to stream the header and each data record as its own
one-line S-expression. This adopts Scheme's LOSE (Line-Oriented
S-expressions) convention, described in [SRFI 176][lose]. Passing
`--no-headers` treats the first record as data, `--delimiter CHAR`
changes the field delimiter, and `--flexible` allows records with
differing field counts.

```console
$ printf 'name,role\nHubot,gifs\nBender,bending\nBaymax,doctor\n' | sxp csv --lose | sxp sexp --stream
(name role)
("Hubot" "gifs")
("Bender" "bending")
("Baymax" "doctor")
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.

[lose]: https://srfi.schemers.org/srfi-176/srfi-176.html#_line_oriented_s_expressions_lose
