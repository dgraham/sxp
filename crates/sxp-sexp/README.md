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

Use `--first` or its `--car` alias to select the first item of each top-level
list. Use `--rest` or `--cdr` to select the remaining items. With `--stream`, the
selection is applied to every expression.


```console
$ printf '(1 2 3)\n(4 5 6)\n' | sxp sexp --stream --first
1
4

$ printf '(1 2 3)\n(4 5 6)\n' | sxp sexp --stream --rest
(2 3)
(5 6)
```

Use the common output format and color options documented in the
[`sxp` crate](../sxp/README.md).
