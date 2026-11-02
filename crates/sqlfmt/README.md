# sqlfmt

`sqlfmt split` reads SQL statements from standard input, splits them with the
PostgreSQL parser, and prints each statement terminated by a null byte.

Pair it with `xargs -0` to pass statements to `sxp sql`:

```console
$ sqlfmt split < schema.sql | xargs -0 -n1 sh -c 'printf %s "$1" | sxp sql' _
```

Or use a loop to avoid spawning a shell for each statement:

```sh
sqlfmt split < schema.sql | while IFS= read -r -d '' stmt; do
  printf %s "$stmt" | sxp sql
done
```
