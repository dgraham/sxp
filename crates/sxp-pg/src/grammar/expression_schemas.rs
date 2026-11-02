use sxp::Sexp;
use sxp::pattern::{AtomType, Pattern};

pub(super) const STRICT_NODE_TAGS: &[&str] = &[
    "column",
    "parameter",
    "bit-string",
    "op",
    "any-op",
    "all-op",
    "in",
    "like",
    "ilike",
    "is-distinct-from",
    "is-not-distinct-from",
    "nullif",
    "between",
    "not-between",
    "between-symmetric",
    "not-between-symmetric",
    "similar-to",
    "indirection",
    "field",
    "case",
    "argument",
    "when",
    "else",
    "any-array",
    "all-array",
    "row-compare",
    "exists",
    "scalar-subquery",
    "array-subquery",
    "in-subquery",
    "any-subquery",
    "all-subquery",
    "row-compare-subquery",
    "and",
    "or",
    "not",
    "is-null",
    "is-not-null",
    "is-true",
    "is-not-true",
    "is-false",
    "is-not-false",
    "is-unknown",
    "is-not-unknown",
    "call",
    "named-arg",
    "variadic",
    "filter",
    "over",
    "frame",
    "exclude",
    "unbounded-preceding",
    "unbounded-following",
    "current-row",
    "preceding",
    "following",
    "array",
    "coalesce",
    "cast",
    "type",
    "setof",
    "modifiers",
    "collate",
    "grouping",
    "current-of",
    "current-date",
    "current-time",
    "current-timestamp",
    "localtime",
    "local-timestamp",
    "current-role",
    "current-user",
    "user",
    "session-user",
    "current-catalog",
    "current-schema",
    "precision",
    "xml-concat",
    "xml-element",
    "xml-forest",
    "xml-parse",
    "xml-pi",
    "xml-root",
    "xml-serialize",
    "is-document",
    "name",
    "attributes",
    "attr",
    "elem",
    "version",
    "standalone",
    "json-parse",
    "json-scalar",
    "json-serialize",
    "json-object",
    "json-array",
    "json-arrayagg",
    "json-objectagg",
    "json-exists",
    "json-query",
    "json-value",
    "is-json",
    "format",
    "oid",
    "typmod",
    "null",
    "error",
    "empty",
    "true",
    "false",
    "unknown",
    "empty-array",
    "empty-object",
    "default",
    "on-empty",
    "on-error",
    "entry",
    "key",
    "value",
    "arg",
    "items",
    "path",
    "agg",
    "passing",
    "merge-action",
];

pub(super) fn expression_pattern() -> Pattern {
    let expression_tags = [
        "column",
        "parameter",
        "bit-string",
        "op",
        "any-op",
        "all-op",
        "in",
        "like",
        "ilike",
        "is-distinct-from",
        "is-not-distinct-from",
        "nullif",
        "between",
        "not-between",
        "between-symmetric",
        "not-between-symmetric",
        "similar-to",
        "indirection",
        "case",
        "any-array",
        "all-array",
        "row-compare",
        "exists",
        "scalar-subquery",
        "array-subquery",
        "in-subquery",
        "any-subquery",
        "all-subquery",
        "row-compare-subquery",
        "and",
        "or",
        "not",
        "is-null",
        "is-not-null",
        "is-true",
        "is-not-true",
        "is-false",
        "is-not-false",
        "is-unknown",
        "is-not-unknown",
        "call",
        "array",
        "coalesce",
        "cast",
        "collate",
        "grouping",
        "current-of",
        "current-date",
        "current-time",
        "current-timestamp",
        "localtime",
        "local-timestamp",
        "current-role",
        "current-user",
        "user",
        "session-user",
        "current-catalog",
        "current-schema",
        "xml-concat",
        "xml-element",
        "xml-forest",
        "xml-parse",
        "xml-pi",
        "xml-root",
        "xml-serialize",
        "is-document",
        "json-parse",
        "json-scalar",
        "json-serialize",
        "json-object",
        "json-array",
        "json-arrayagg",
        "json-objectagg",
        "json-exists",
        "json-query",
        "json-value",
        "is-json",
        "merge-action",
    ];
    let mut alternatives = [
        AtomType::Symbol,
        AtomType::String,
        AtomType::Integer,
        AtomType::Float,
        AtomType::Boolean,
        AtomType::Nil,
    ]
    .map(Pattern::Atom)
    .into_iter()
    .collect::<Vec<_>>();
    alternatives.extend(
        expression_tags
            .into_iter()
            .map(|tag| Pattern::Reference(format!("pg-node-{tag}"))),
    );
    alternatives.push(Pattern::Reference("as".to_owned()));
    alternatives.push(Pattern::Reference("row".to_owned()));
    Pattern::OneOf(alternatives)
}

pub(super) fn rules() -> Vec<(String, Pattern)> {
    use AtomType as A;
    use Pattern as P;

    let atom = |kind| P::Atom(kind);
    let symbol = || atom(A::Symbol);
    let string = || atom(A::String);
    let value = || P::Reference("value".to_owned());
    let expression = || P::Reference("expression".to_owned());
    let named_ref = |name: &str| P::Reference(name.to_owned());
    let exact_symbol = |name: &str| P::Exact(Sexp::symbol(name));
    let optional = |pattern| P::Optional(Box::new(pattern));
    let repeated = |pattern| P::Repeat(Box::new(pattern));
    let one_or_more = |pattern: P| P::OneOrMore(Box::new(pattern));
    let tagged = |name: &str, fields: Vec<P>| super::tagged(name, fields);
    let rule = |name: &str, pattern: P| (format!("pg-node-{name}"), pattern);
    let fields = |name: &str, parts: Vec<P>| tagged(name, parts);
    let order_by = || {
        fields(
            "order-by",
            vec![one_or_more(fields(
                "sort",
                vec![
                    expression(),
                    P::OneOf(vec![
                        exact_symbol("asc"),
                        exact_symbol("desc"),
                        fields("using", vec![string()]),
                    ]),
                    optional(P::OneOf(vec![
                        exact_symbol("nulls-first"),
                        exact_symbol("nulls-last"),
                    ])),
                ],
            ))],
        )
    };
    let json_value_expr = || {
        P::OneOf(vec![
            expression(),
            P::List(vec![expression(), named_ref("pg-node-format")]),
        ])
    };
    let call_argument = || {
        P::OneOf(vec![
            expression(),
            named_ref("pg-node-named-arg"),
            named_ref("pg-node-variadic"),
        ])
    };

    let mut rules = Vec::new();

    rules.push(rule(
        "column",
        fields(
            "column",
            vec![one_or_more(P::OneOf(vec![string(), exact_symbol("*")]))],
        ),
    ));
    rules.push(rule("parameter", fields("parameter", vec![symbol()])));
    rules.push(rule("bit-string", fields("bit-string", vec![string()])));

    for name in ["op", "any-op", "all-op"] {
        rules.push(rule(
            name,
            P::OneOf(vec![
                fields(name, vec![string(), expression()]),
                fields(name, vec![string(), expression(), expression()]),
            ]),
        ));
    }
    rules.push(rule(
        "in",
        fields("in", vec![expression(), one_or_more(expression())]),
    ));
    for name in [
        "like",
        "ilike",
        "is-distinct-from",
        "is-not-distinct-from",
        "nullif",
        "any-array",
        "all-array",
    ] {
        rules.push(rule(name, fields(name, vec![expression(), expression()])));
    }
    for name in [
        "between",
        "not-between",
        "between-symmetric",
        "not-between-symmetric",
    ] {
        let mut alternatives = vec![fields(name, vec![expression(), expression(), expression()])];
        if name == "between" {
            alternatives.push(fields(
                name,
                vec![
                    P::OneOf(vec![
                        named_ref("pg-node-unbounded-preceding"),
                        named_ref("pg-node-current-row"),
                        named_ref("pg-node-preceding"),
                        named_ref("pg-node-following"),
                    ]),
                    P::OneOf(vec![
                        named_ref("pg-node-unbounded-preceding"),
                        named_ref("pg-node-unbounded-following"),
                        named_ref("pg-node-current-row"),
                        named_ref("pg-node-preceding"),
                        named_ref("pg-node-following"),
                    ]),
                ],
            ));
        }
        rules.push(rule(name, P::OneOf(alternatives)));
    }
    rules.push(rule(
        "similar-to",
        P::OneOf(vec![
            fields("similar-to", vec![expression(), expression()]),
            fields("similar-to", vec![expression(), expression(), expression()]),
        ]),
    ));

    rules.push(rule(
        "indirection",
        fields(
            "indirection",
            vec![
                expression(),
                repeated(P::OneOf(vec![
                    named_ref("pg-node-field"),
                    fields("index", vec![expression()]),
                    fields(
                        "slice",
                        vec![optional(expression()), optional(expression())],
                    ),
                    exact_symbol("*"),
                ])),
            ],
        ),
    ));
    rules.push(rule("field", fields("field", vec![string()])));
    rules.push(rule(
        "case",
        fields(
            "case",
            vec![
                optional(named_ref("pg-node-argument")),
                one_or_more(named_ref("pg-node-when")),
                optional(named_ref("pg-node-else")),
            ],
        ),
    ));
    rules.push(rule("argument", fields("argument", vec![expression()])));
    rules.push(rule(
        "when",
        fields("when", vec![expression(), expression()]),
    ));
    rules.push(rule("else", fields("else", vec![expression()])));

    rules.push(rule(
        "row-compare",
        fields(
            "row-compare",
            vec![
                P::OneOf(
                    ["lt", "le", "eq", "ge", "gt", "ne"]
                        .into_iter()
                        .map(exact_symbol)
                        .collect(),
                ),
                fields("left", vec![one_or_more(expression())]),
                fields("right", vec![one_or_more(expression())]),
            ],
        ),
    ));
    rules.push(rule("exists", fields("exists", vec![named_ref("query")])));
    rules.push(rule(
        "scalar-subquery",
        fields("scalar-subquery", vec![named_ref("query")]),
    ));
    rules.push(rule(
        "array-subquery",
        fields("array-subquery", vec![named_ref("query")]),
    ));
    rules.push(rule(
        "in-subquery",
        fields("in-subquery", vec![expression(), named_ref("query")]),
    ));
    for name in ["any-subquery", "all-subquery", "row-compare-subquery"] {
        rules.push(rule(
            name,
            fields(name, vec![string(), expression(), named_ref("query")]),
        ));
    }

    for name in ["and", "or"] {
        rules.push(rule(name, fields(name, vec![one_or_more(expression())])));
    }
    rules.push(rule("not", fields("not", vec![expression()])));
    for name in ["is-null", "is-not-null"] {
        rules.push(rule(name, fields(name, vec![expression()])));
    }
    for name in [
        "is-true",
        "is-not-true",
        "is-false",
        "is-not-false",
        "is-unknown",
        "is-not-unknown",
    ] {
        rules.push(rule(name, fields(name, vec![expression()])));
    }

    let filter = fields("filter", vec![expression()]);
    let over = fields(
        "over",
        vec![repeated(P::OneOf(vec![
            fields("name", vec![string()]),
            fields("window", vec![string()]),
            fields("partition-by", vec![one_or_more(expression())]),
            order_by(),
            named_ref("pg-node-frame"),
        ]))],
    );
    rules.push(rule("filter", filter.clone()));
    rules.push(rule("over", over.clone()));

    let call_suffix = || {
        vec![
            optional(exact_symbol("within-group")),
            optional(order_by()),
            optional(named_ref("pg-node-filter")),
            optional(named_ref("pg-node-over")),
        ]
    };
    let mut call_alternatives = vec![fields("call", vec![string(), repeated(call_argument())])];
    for count in 0..=4 {
        let mut args = vec![string()];
        args.extend((0..count).map(|_| call_argument()));
        args.extend(call_suffix());
        call_alternatives.push(fields("call", args));
    }
    let mut distinct_args = vec![
        string(),
        fields("distinct", vec![one_or_more(call_argument())]),
    ];
    distinct_args.extend(call_suffix());
    call_alternatives.push(fields("call", distinct_args));
    let mut star_args = vec![string(), exact_symbol("*")];
    star_args.extend(call_suffix());
    call_alternatives.push(fields("call", star_args));
    for name in ["greatest", "least"] {
        call_alternatives.push(fields(
            "call",
            vec![P::Exact(Sexp::string(name)), one_or_more(expression())],
        ));
    }
    rules.push(rule("call", P::OneOf(call_alternatives)));
    rules.push(rule(
        "named-arg",
        fields("named-arg", vec![string(), expression()]),
    ));
    rules.push(rule("variadic", fields("variadic", vec![expression()])));

    rules.push(rule(
        "frame",
        P::OneOf(vec![
            fields(
                "frame",
                vec![
                    P::OneOf(["range", "rows", "groups"].map(exact_symbol).into()),
                    P::OneOf(vec![
                        named_ref("pg-node-unbounded-preceding"),
                        named_ref("pg-node-current-row"),
                        named_ref("pg-node-preceding"),
                        named_ref("pg-node-following"),
                    ]),
                    optional(named_ref("pg-node-exclude")),
                ],
            ),
            fields(
                "frame",
                vec![
                    P::OneOf(["range", "rows", "groups"].map(exact_symbol).into()),
                    fields(
                        "between",
                        vec![
                            P::OneOf(vec![
                                named_ref("pg-node-unbounded-preceding"),
                                named_ref("pg-node-current-row"),
                                named_ref("pg-node-preceding"),
                                named_ref("pg-node-following"),
                            ]),
                            P::OneOf(vec![
                                named_ref("pg-node-unbounded-preceding"),
                                named_ref("pg-node-unbounded-following"),
                                named_ref("pg-node-current-row"),
                                named_ref("pg-node-preceding"),
                                named_ref("pg-node-following"),
                            ]),
                        ],
                    ),
                    optional(named_ref("pg-node-exclude")),
                ],
            ),
        ]),
    ));
    for name in ["unbounded-preceding", "unbounded-following", "current-row"] {
        rules.push(rule(name, fields(name, vec![])));
    }
    for name in ["preceding", "following"] {
        rules.push(rule(name, fields(name, vec![expression()])));
    }
    rules.push(rule(
        "exclude",
        fields(
            "exclude",
            vec![P::OneOf(
                ["current-row", "group", "ties"]
                    .into_iter()
                    .map(exact_symbol)
                    .collect(),
            )],
        ),
    ));

    rules.push(rule("array", fields("array", vec![repeated(expression())])));
    rules.push(rule(
        "coalesce",
        fields("coalesce", vec![one_or_more(expression())]),
    ));
    rules.push(rule(
        "cast",
        fields("cast", vec![expression(), named_ref("pg-node-type")]),
    ));
    rules.push(rule(
        "type",
        fields(
            "type",
            vec![
                one_or_more(string()),
                optional(exact_symbol("%-type")),
                optional(fields("modifiers", vec![one_or_more(value())])),
                repeated(fields("array", vec![optional(symbol())])),
            ],
        ),
    ));
    rules.push(rule(
        "setof",
        fields("setof", vec![named_ref("pg-node-type")]),
    ));
    rules.push(rule(
        "modifiers",
        fields("modifiers", vec![one_or_more(value())]),
    ));
    rules.push(rule(
        "collate",
        fields("collate", vec![expression(), string()]),
    ));
    rules.push(rule(
        "grouping",
        fields("grouping", vec![repeated(expression())]),
    ));
    rules.push(rule("current-of", fields("current-of", vec![string()])));
    for name in [
        "current-date",
        "current-time",
        "current-timestamp",
        "localtime",
        "local-timestamp",
        "current-role",
        "current-user",
        "user",
        "session-user",
        "current-catalog",
        "current-schema",
    ] {
        rules.push(rule(
            name,
            fields(name, vec![optional(named_ref("pg-node-precision"))]),
        ));
    }
    rules.push(rule("precision", fields("precision", vec![symbol()])));

    rules.push(rule(
        "xml-concat",
        fields("xml-concat", vec![one_or_more(expression())]),
    ));
    rules.push(rule(
        "xml-element",
        fields(
            "xml-element",
            vec![
                named_ref("pg-node-name"),
                optional(named_ref("pg-node-attributes")),
                repeated(expression()),
            ],
        ),
    ));
    rules.push(rule(
        "xml-forest",
        fields("xml-forest", vec![one_or_more(named_ref("pg-node-elem"))]),
    ));
    rules.push(rule(
        "xml-parse",
        fields(
            "xml-parse",
            vec![
                P::OneOf([exact_symbol("document"), exact_symbol("content")].into()),
                expression(),
                optional(exact_symbol("preserve-whitespace")),
            ],
        ),
    ));
    rules.push(rule(
        "xml-pi",
        fields(
            "xml-pi",
            vec![named_ref("pg-node-name"), optional(expression())],
        ),
    ));
    rules.push(rule(
        "xml-root",
        fields(
            "xml-root",
            vec![
                expression(),
                named_ref("pg-node-version"),
                optional(named_ref("pg-node-standalone")),
            ],
        ),
    ));
    rules.push(rule(
        "xml-serialize",
        P::OneOf(vec![
            fields(
                "xml-serialize",
                vec![
                    P::OneOf([exact_symbol("document"), exact_symbol("content")].into()),
                    expression(),
                ],
            ),
            fields(
                "xml-serialize",
                vec![
                    P::OneOf([exact_symbol("document"), exact_symbol("content")].into()),
                    expression(),
                    optional(fields("as", vec![named_ref("pg-node-type")])),
                    optional(exact_symbol("indent")),
                ],
            ),
        ]),
    ));
    rules.push(rule(
        "is-document",
        fields("is-document", vec![one_or_more(expression())]),
    ));
    rules.push(rule("name", fields("name", vec![one_or_more(string())])));
    rules.push(rule(
        "attributes",
        fields("attributes", vec![one_or_more(named_ref("pg-node-attr"))]),
    ));
    rules.push(rule(
        "attr",
        P::OneOf(vec![
            fields("attr", vec![expression()]),
            fields("attr", vec![named_ref("pg-node-name"), expression()]),
        ]),
    ));
    rules.push(rule(
        "elem",
        fields("elem", vec![named_ref("pg-node-name"), expression()]),
    ));
    rules.push(rule("version", fields("version", vec![expression()])));
    rules.push(rule(
        "standalone",
        fields(
            "standalone",
            vec![P::OneOf(
                ["yes", "no", "no-value"]
                    .into_iter()
                    .map(exact_symbol)
                    .collect(),
            )],
        ),
    ));

    let format = fields(
        "format",
        vec![
            P::OneOf([exact_symbol("json"), exact_symbol("jsonb")].into()),
            optional(P::OneOf(
                ["utf8", "utf16", "utf32"]
                    .into_iter()
                    .map(exact_symbol)
                    .collect(),
            )),
        ],
    );
    rules.push(rule("format", format));
    rules.push(rule("oid", fields("oid", vec![symbol()])));
    rules.push(rule("typmod", fields("typmod", vec![symbol()])));
    for name in [
        "null",
        "error",
        "empty",
        "true",
        "false",
        "unknown",
        "empty-array",
        "empty-object",
        "default",
    ] {
        rules.push(rule(name, fields(name, vec![optional(expression())])));
    }
    let json_behavior = P::OneOf(
        [
            "null",
            "error",
            "empty",
            "true",
            "false",
            "unknown",
            "empty-array",
            "empty-object",
            "default",
        ]
        .into_iter()
        .map(|name| named_ref(&format!("pg-node-{name}")))
        .collect(),
    );
    rules.push(rule(
        "on-empty",
        fields("on-empty", vec![json_behavior.clone()]),
    ));
    rules.push(rule("on-error", fields("on-error", vec![json_behavior])));
    rules.push(rule(
        "entry",
        fields(
            "entry",
            vec![named_ref("pg-node-key"), named_ref("pg-node-value")],
        ),
    ));
    rules.push(rule("key", fields("key", vec![expression()])));
    rules.push(rule("value", fields("value", vec![json_value_expr()])));
    rules.push(rule(
        "arg",
        fields("arg", vec![string(), json_value_expr()]),
    ));
    rules.push(rule("items", fields("items", vec![repeated(expression())])));
    rules.push(rule(
        "path",
        fields(
            "path",
            vec![expression(), optional(named_ref("pg-node-name"))],
        ),
    ));
    rules.push(rule(
        "agg",
        fields(
            "agg",
            vec![P::OneOf(vec![
                json_value_expr(),
                named_ref("pg-node-entry"),
            ])],
        ),
    ));
    rules.push(rule(
        "passing",
        fields("passing", vec![one_or_more(named_ref("pg-node-arg"))]),
    ));

    let returning_clause = || optional(value());
    let json_key_entry = named_ref("pg-node-entry");
    let json_array_items = fields("items", vec![repeated(expression())]);
    let json_query = fields("query", vec![named_ref("query")]);
    let json_passing = optional(named_ref("pg-node-passing"));
    let json_format = optional(named_ref("pg-node-format"));
    let json_returning = returning_clause();
    let json_on_empty = optional(named_ref("pg-node-on-empty"));
    let json_on_error = optional(named_ref("pg-node-on-error"));

    rules.push(rule(
        "json-parse",
        fields(
            "json-parse",
            vec![
                json_value_expr(),
                optional(exact_symbol("unique-keys")),
                json_returning.clone(),
            ],
        ),
    ));
    rules.push(rule(
        "json-scalar",
        fields("json-scalar", vec![expression(), json_returning.clone()]),
    ));
    rules.push(rule(
        "json-serialize",
        fields(
            "json-serialize",
            vec![json_value_expr(), json_returning.clone()],
        ),
    ));
    rules.push(rule(
        "json-object",
        fields(
            "json-object",
            vec![
                repeated(P::OneOf(vec![expression(), json_key_entry])),
                optional(exact_symbol("absent-on-null")),
                optional(exact_symbol("unique-keys")),
                json_returning.clone(),
            ],
        ),
    ));
    rules.push(rule(
        "json-array",
        P::OneOf(vec![
            fields(
                "json-array",
                vec![
                    json_array_items.clone(),
                    optional(exact_symbol("absent-on-null")),
                    json_returning.clone(),
                ],
            ),
            fields(
                "json-array",
                vec![
                    json_query,
                    json_format.clone(),
                    optional(exact_symbol("absent-on-null")),
                    json_returning.clone(),
                ],
            ),
        ]),
    ));

    let aggregate_suffix = || {
        vec![
            optional(order_by()),
            optional(named_ref("pg-node-filter")),
            optional(named_ref("pg-node-over")),
            json_returning.clone(),
        ]
    };
    let mut json_array_agg = vec![named_ref("pg-node-agg")];
    json_array_agg.extend(aggregate_suffix());
    json_array_agg.push(optional(exact_symbol("absent-on-null")));
    rules.push(rule(
        "json-arrayagg",
        fields("json-arrayagg", json_array_agg),
    ));
    let mut json_object_agg = vec![named_ref("pg-node-agg")];
    json_object_agg.extend(aggregate_suffix());
    json_object_agg.push(optional(exact_symbol("absent-on-null")));
    json_object_agg.push(optional(exact_symbol("unique-keys")));
    rules.push(rule(
        "json-objectagg",
        fields("json-objectagg", json_object_agg),
    ));

    let json_func = |name: &str| {
        fields(
            name,
            vec![
                json_value_expr(),
                P::OneOf(vec![string(), named_ref("pg-node-path")]),
                json_passing.clone(),
                json_returning.clone(),
                optional(P::OneOf(vec![
                    exact_symbol("without-wrapper"),
                    exact_symbol("with-conditional-wrapper"),
                    exact_symbol("with-unconditional-wrapper"),
                ])),
                optional(P::OneOf(vec![
                    exact_symbol("keep-quotes"),
                    exact_symbol("omit-quotes"),
                ])),
                json_on_empty.clone(),
                json_on_error.clone(),
            ],
        )
    };
    for name in ["json-exists", "json-query", "json-value"] {
        rules.push(rule(name, json_func(name)));
    }
    rules.push(rule(
        "is-json",
        fields(
            "is-json",
            vec![
                expression(),
                optional(P::OneOf(
                    ["object", "array", "scalar"]
                        .into_iter()
                        .map(exact_symbol)
                        .collect(),
                )),
                json_format,
                optional(exact_symbol("with-unique-keys")),
            ],
        ),
    ));
    rules.push(rule("merge-action", fields("merge-action", vec![])));

    rules
}
