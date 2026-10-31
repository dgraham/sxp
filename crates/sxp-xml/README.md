# sxp-xml

Convert XML from standard input with `sxp xml`. The output grammar covers
document nodes, elements, optional attribute lists, text, and comments when
comment preservation is enabled.

```console
$ printf '<root><item>one</item></root>' | sxp xml | sxp xml --validate
$ printf '<root><item>one</item></root>' | sxp xml | sxp xml --query '(capture items (list item (repeat (any))))'
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.
