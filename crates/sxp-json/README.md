# sxp-json

Convert JSON values from standard input with `sxp json`. Arrays and objects
are represented as `(array ...)` and `(object ...)`. Scalar values retain
their JSON types.

```console
$ printf '{"name":"Hubot","active":true,"scores":[3,5]}' | sxp json
(object ("active" true) ("name" "Hubot") ("scores" (array 3 5)))
```

The binary supports `--print-pattern`, `--validate`, and `--query PATTERN`.
Its printed grammar describes the JSON value forms and can validate an
existing S-expression:

```console
$ sxp json --print-pattern
(grammar (name "sxp-json") (root value) (rule value (or (null) (boolean) (integer) (float) (string) (list (exact array) (repeat (ref value))) (list (exact object) (repeat (list (string) (ref value)))))))
$ printf '(array 1 (object ("active" true)))' | sxp json --validate
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.
