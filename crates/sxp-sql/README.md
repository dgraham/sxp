# sxp-sql

Convert SQL from standard input with `sxp sql`. Pass `--sqlcommenter` to
represent [sqlcommenter](https://google.github.io/sqlcommenter/spec/) tag/value
pairs as a `(sqlcommenter ...)` clause.

```console
$ printf "SELECT * FROM users /*app='web',route='%%2Fapi%%2Fusers'*/;" | sxp sql --sqlcommenter
(select
  (sqlcommenter
    (tag
      "app"
      "web")
    (tag
      "route"
      "/api/users"))
  (columns
    (column
      *))
  (from
    (table
      ("users"))))
```

See the [`sqlfmt` crate](../sqlfmt/README.md) to split SQL input into
statements, and the [`sxp` crate](../sxp/README.md) for shared formatting
options.
