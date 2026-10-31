# sxp-md

Convert Markdown from standard input with `sxp md`.

```console
$ printf '# Heading\n\n- [x] checked' | sxp md | sxp md --validate
$ printf '# Heading\n\n## Another' | sxp md | sxp md --query '(capture headings (list heading (repeat (any))))'
```

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.
