# sxp-promql

Convert PromQL from standard input with `sxp promql`. The parser covers
covers selectors, matchers, aggregations, operators, subqueries,
and `@`/`offset` modifiers.

```console
$ printf 'sum by (job) (rate(http_requests_total[5m]))' | sxp promql
(aggregate
  (op sum)
  (grouping by "job")
  (expr
    (call
      (func "rate")
      (args
        (matrix-selector
          (expr (vector-selector (name "http_requests_total")))
          (range "5m"))))))
```

Numbers become `(float n)` atoms (`Inf` and `NaN` normalize to `inf` and
`nan`), strings stay quoted atoms, and metric names, matchers, durations, and
labels are preserved verbatim. The binary supports `--print-pattern`,
`--validate`, and `--query PATTERN`.

See the [`sxp` crate](../sxp/README.md) for shared query and formatting options.
