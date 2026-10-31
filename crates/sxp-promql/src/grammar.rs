use sxp::Sexp;
use sxp::pattern::{AtomType, Grammar, Pattern};

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    use Pattern as P;

    let expr = one_of([
        reference("aggregate"),
        reference("unary"),
        reference("binary"),
        reference("paren"),
        reference("subquery"),
        reference("float"),
        reference("vector-selector"),
        reference("matrix-selector"),
        reference("call"),
        reference("extension"),
        P::Atom(AtomType::String),
    ]);
    let rules = vec![
        rule("expr", expr.clone()),
        rule(
            "aggregate",
            node(
                "aggregate",
                [
                    field("op", P::Atom(AtomType::Symbol)),
                    optional(reference("grouping")),
                    optional(field("param", expr.clone())),
                    field("expr", expr.clone()),
                ],
            ),
        ),
        rule(
            "unary",
            node(
                "unary",
                [
                    field("op", P::Atom(AtomType::Symbol)),
                    field("expr", expr.clone()),
                ],
            ),
        ),
        rule(
            "binary",
            node(
                "binary",
                [
                    field("left", expr.clone()),
                    field("op", P::Atom(AtomType::Symbol)),
                    field("right", expr.clone()),
                    optional(reference("modifier")),
                ],
            ),
        ),
        rule("paren", node("paren", [field("expr", expr.clone())])),
        rule(
            "subquery",
            node(
                "subquery",
                [
                    field("expr", expr.clone()),
                    field("range", P::Atom(AtomType::String)),
                    optional(field("step", P::Atom(AtomType::String))),
                    optional(reference("at")),
                    optional(field("offset", P::Atom(AtomType::String))),
                ],
            ),
        ),
        rule(
            "float",
            node(
                "float",
                [one_of([
                    P::Atom(AtomType::Float),
                    exact_symbol("inf"),
                    exact_symbol("-inf"),
                    exact_symbol("nan"),
                ])],
            ),
        ),
        rule(
            "vector-selector",
            node(
                "vector-selector",
                [
                    optional(field("name", P::Atom(AtomType::String))),
                    optional(reference("matchers")),
                    optional(field("offset", P::Atom(AtomType::String))),
                    optional(reference("at")),
                ],
            ),
        ),
        rule(
            "matrix-selector",
            node(
                "matrix-selector",
                [
                    field("expr", reference("vector-selector")),
                    field("range", P::Atom(AtomType::String)),
                ],
            ),
        ),
        rule(
            "call",
            node(
                "call",
                [field("func", P::Atom(AtomType::String)), reference("args")],
            ),
        ),
        rule("args", node("args", [repeat(expr.clone())])),
        rule(
            "extension",
            node("extension", [field("name", P::Atom(AtomType::String))]),
        ),
        rule(
            "matchers",
            node(
                "matchers",
                [repeat(one_of([reference("match"), reference("or")]))],
            ),
        ),
        rule(
            "match",
            node(
                "match",
                [
                    P::Atom(AtomType::Symbol),
                    P::Atom(AtomType::String),
                    P::Atom(AtomType::String),
                ],
            ),
        ),
        rule("or", node("or", [one_or_more(reference("match"))])),
        rule(
            "grouping",
            node(
                "grouping",
                [
                    one_of([exact_symbol("by"), exact_symbol("without")]),
                    repeat(P::Atom(AtomType::String)),
                ],
            ),
        ),
        rule(
            "modifier",
            node(
                "modifier",
                [
                    optional(reference("matching")),
                    optional(reference("cardinality")),
                    optional(P::List(vec![exact_symbol("bool")])),
                    optional(reference("fill")),
                ],
            ),
        ),
        rule(
            "matching",
            node(
                "matching",
                [
                    one_of([exact_symbol("on"), exact_symbol("ignoring")]),
                    repeat(P::Atom(AtomType::String)),
                ],
            ),
        ),
        rule(
            "cardinality",
            node(
                "cardinality",
                [P::Atom(AtomType::Symbol), repeat(P::Atom(AtomType::String))],
            ),
        ),
        rule(
            "fill",
            node(
                "fill",
                [
                    optional(field("left", P::Atom(AtomType::Float))),
                    optional(field("right", P::Atom(AtomType::Float))),
                ],
            ),
        ),
        rule(
            "at",
            node(
                "at",
                [one_of([
                    exact_symbol("start"),
                    exact_symbol("end"),
                    P::Atom(AtomType::Float),
                ])],
            ),
        ),
    ];
    Grammar::new("sxp-promql", "expr", rules).expect("the built-in PromQL output grammar is valid")
}

fn rule(name: &str, pattern: Pattern) -> (String, Pattern) {
    (name.to_owned(), pattern)
}

fn reference(name: &str) -> Pattern {
    Pattern::Reference(name.to_owned())
}

fn exact_symbol(name: &str) -> Pattern {
    Pattern::Exact(Sexp::symbol(name))
}

fn one_of(patterns: impl IntoIterator<Item = Pattern>) -> Pattern {
    Pattern::OneOf(patterns.into_iter().collect())
}

fn node(tag: &str, fields: impl IntoIterator<Item = Pattern>) -> Pattern {
    Pattern::List(std::iter::once(exact_symbol(tag)).chain(fields).collect())
}

fn field(name: &str, value: Pattern) -> Pattern {
    Pattern::List(vec![exact_symbol(name), value])
}

fn repeat(pattern: Pattern) -> Pattern {
    Pattern::Repeat(Box::new(pattern))
}

fn one_or_more(pattern: Pattern) -> Pattern {
    Pattern::OneOrMore(Box::new(pattern))
}

fn optional(pattern: Pattern) -> Pattern {
    Pattern::Optional(Box::new(pattern))
}

#[cfg(test)]
mod tests {
    use crate::PromqlParser;
    use sxp::{ParserOptions, Sexp, SexpParser};

    fn parse(input: &str) -> sxp::Result<Sexp> {
        let value = PromqlParser::new().parse(input)?;
        assert!(
            crate::grammar().matches_root(&value),
            "output does not match the built-in grammar: {value}"
        );
        let text = value.to_string();
        let round_tripped =
            SexpParser::with_options(ParserOptions::default().typed_atoms(true)).parse(&text)?;
        assert!(
            crate::grammar().matches_root(&round_tripped),
            "round-tripped output does not match the built-in grammar: {round_tripped}"
        );
        Ok(value)
    }

    #[test]
    fn validates_against_the_grammar() {
        parse("up").unwrap();
        parse(r#"http_requests_total{environment=~"staging|testing|development",method!="GET"} offset 5m"#)
            .unwrap();
        parse("sum by (job) (rate(http_requests_total[5m])) > bool 0.9").unwrap();
    }

    #[test]
    fn validates_modifiers_and_subqueries() {
        parse("a + on(instance) group_left (job) b").unwrap();
        parse("max_over_time(up[5m:1m] @ end())").unwrap();
        parse("topk(3, sum without (instance) (rate(errors_total[10m])))").unwrap();
        parse("-(1 + 2)").unwrap();
        parse(r#"absent(nonexistent{job="myjob"})"#).unwrap();
    }

    #[test]
    fn rejects_trees_from_other_formats() {
        let value = SexpParser::with_options(ParserOptions::default().typed_atoms(true))
            .parse("(map (title nil))")
            .unwrap();
        assert!(!crate::grammar().matches_root(&value));
    }
}
