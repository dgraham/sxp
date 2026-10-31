# sxp-sexp

Parse and format S-expressions from standard input. Process a single
expression with `sxp sexp` or a stream of consecutive expressions
with `--stream`.

```console
$ printf '(define x 42)' | sxp sexp
(define x 42)

$ printf '(define x 42)' | sxp sexp --format=inspect
0	List	3
1	Symbol	define
1	Symbol	x
1	Integer	42
```

Use the common output format and color options documented in the
[`sxp` crate](../sxp/README.md).
