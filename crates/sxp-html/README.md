# sxp-html

Convert HTML from standard input with `sxp html`. The output grammar covers
document nodes, elements, optional attribute lists, text, and comments when
comment preservation is enabled.

```console
$ printf '<ul><li>one</li></ul>' | sxp html | sxp html --validate
$ printf '<ul><li>one</li></ul>' | sxp html | sxp html --query '(capture items (list li (repeat (any))))'
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.
