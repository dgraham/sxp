use sxp::Sexp;
use sxp::pattern::{AtomType, Pattern};

pub(super) const STRICT_STATEMENT_TAGS: &[&str] = &[
    "create-access-method",
    "create-aggregate",
    "create-cast",
    "create-collation",
    "create-conversion",
    "create-database",
    "create-event-trigger",
    "create-extension",
    "create-foreign-data-wrapper",
    "create-foreign-server",
    "create-group",
    "create-language",
    "create-operator",
    "create-operator-class",
    "create-operator-family",
    "create-policy",
    "create-publication",
    "create-role",
    "create-rule",
    "create-statistics",
    "create-subscription",
    "create-tablespace",
    "create-text-search-configuration",
    "create-text-search-dictionary",
    "create-text-search-parser",
    "create-text-search-template",
    "create-transform",
    "create-trigger",
    "create-user",
    "create-user-mapping",
];

pub(super) fn rules() -> Vec<(String, Pattern)> {
    use Pattern as P;

    let string = || P::Atom(AtomType::String);
    let symbol = || P::Atom(AtomType::Symbol);
    let integer = || P::Atom(AtomType::Integer);
    let float = || P::Atom(AtomType::Float);
    let value = P::Reference("value".to_owned());
    let table = P::Reference("table".to_owned());
    let sqlcommenter = P::Reference("sqlcommenter".to_owned());
    let optional = |pattern| P::Optional(Box::new(pattern));
    let repeat = |pattern| P::Repeat(Box::new(pattern));
    let one_or_more = |pattern| P::OneOrMore(Box::new(pattern));
    let exact = |name: &str| P::Exact(Sexp::symbol(name));
    let tagged = |name: &str, fields: Vec<P>| {
        P::List(
            std::iter::once(exact(name))
                .chain(fields)
                .collect::<Vec<_>>(),
        )
    };
    let root = |name: &str, payload: Vec<P>| {
        let mut fields = vec![optional(sqlcommenter.clone())];
        fields.extend(payload);
        tagged(name, fields)
    };
    let name = || tagged("name", vec![one_or_more(string())]);
    let name_object = || {
        tagged(
            "name",
            vec![
                one_or_more(string()),
                optional(tagged("parameters", vec![repeat(value.clone())])),
            ],
        )
    };
    let type_field = |tag: &str| tagged(tag, vec![value.clone()]);
    let scalar = || {
        P::OneOf(vec![
            string(),
            symbol(),
            integer(),
            float(),
            P::Atom(AtomType::Boolean),
            P::Atom(AtomType::Nil),
        ])
    };
    let options = || {
        tagged(
            "options",
            vec![repeat(P::OneOf(vec![
                tagged("option", vec![string()]),
                tagged("option", vec![string(), value.clone()]),
            ]))],
        )
    };
    let option_with_scalar = |tag: &str| tagged(tag, vec![scalar()]);
    let role = || {
        tagged(
            "role",
            vec![P::OneOf(vec![
                string(),
                exact("all"),
                exact("current-role"),
                exact("current-user"),
                exact("session-user"),
                exact("public"),
            ])],
        )
    };
    let roles_clause = |tag: &str| tagged(tag, vec![one_or_more(role())]);

    let enum_values =
        |values: &[&str]| P::OneOf(values.iter().map(|value| exact(value)).collect::<Vec<_>>());

    let aggregate_option = P::OneOf(vec![
        type_field("basetype"),
        type_field("sfunc"),
        type_field("sfunc1"),
        type_field("stype"),
        type_field("stype1"),
        type_field("finalfunc"),
        type_field("func"),
        type_field("combinefunc"),
        type_field("serialfunc"),
        type_field("deserialfunc"),
        type_field("msfunc"),
        type_field("minvfunc"),
        type_field("mstype"),
        type_field("mstate"),
        type_field("mfinalfunc"),
        option_with_scalar("sspace"),
        option_with_scalar("msspace"),
        option_with_scalar("parallel"),
        tagged(
            "finalfunc-modify",
            vec![enum_values(&["read-only", "read-write", "shareable"])],
        ),
        tagged(
            "mfinalfunc-modify",
            vec![enum_values(&["read-only", "read-write", "shareable"])],
        ),
        tagged("initcond", vec![string()]),
        tagged("initcond1", vec![string()]),
        tagged("initcond2", vec![string()]),
        tagged("minitcond", vec![string()]),
        tagged("sortop", vec![string()]),
        exact("finalfunc-extra"),
        exact("no-finalfunc-extra"),
        exact("mfinalfunc-extra"),
        exact("no-mfinalfunc-extra"),
        exact("window"),
        exact("hypothetical"),
    ]);
    let operator_option = P::OneOf(vec![
        type_field("leftarg"),
        type_field("rightarg"),
        type_field("procedure"),
        type_field("function"),
        type_field("restrict"),
        type_field("join"),
        tagged("commutator", vec![one_or_more(string())]),
        tagged("negator", vec![one_or_more(string())]),
        exact("sort1"),
        exact("sort2"),
        exact("ltcmp"),
        exact("gtcmp"),
        tagged("sort1", vec![one_or_more(string())]),
        tagged("sort2", vec![one_or_more(string())]),
        tagged("ltcmp", vec![one_or_more(string())]),
        tagged("gtcmp", vec![one_or_more(string())]),
        exact("hashes"),
        exact("merges"),
        tagged("option", vec![string(), optional(value.clone())]),
    ]);
    let role_option = P::OneOf(vec![
        exact("superuser"),
        exact("no-superuser"),
        exact("createdb"),
        exact("no-createdb"),
        exact("createrole"),
        exact("no-createrole"),
        exact("inherit"),
        exact("no-inherit"),
        exact("login"),
        exact("no-login"),
        exact("replication"),
        exact("no-replication"),
        exact("bypassrls"),
        exact("no-bypassrls"),
        tagged(
            "connection-limit",
            vec![P::OneOf(vec![symbol(), integer()])],
        ),
        tagged(
            "password",
            vec![P::OneOf(vec![value.clone(), exact("null")])],
        ),
        tagged("valid-until", vec![value.clone()]),
        roles_clause("members"),
        roles_clause("in-role"),
        roles_clause("admins"),
    ]);
    let ts_config_option = P::OneOf(vec![
        tagged("copy", vec![name()]),
        tagged("parser", vec![name()]),
    ]);
    let ts_dictionary_option = P::OneOf(vec![
        tagged("template", vec![name()]),
        tagged("dictionary", vec![name()]),
        option_with_scalar("dictfile"),
        option_with_scalar("afffile"),
        option_with_scalar("stopwords"),
        option_with_scalar("synonyms"),
        option_with_scalar("initfile"),
        tagged("option", vec![string(), optional(value.clone())]),
    ]);
    let ts_parser_option = P::OneOf(
        ["start", "gettoken", "end", "lextypes", "headword"]
            .iter()
            .map(|tag| tagged(tag, vec![name()]))
            .collect(),
    );
    let ts_template_option = P::OneOf(vec![
        tagged("init", vec![name()]),
        tagged("lexize", vec![name()]),
    ]);
    let subscription_option = P::OneOf(vec![
        tagged("copy-data", vec![]),
        tagged("no-copy-data", vec![]),
        tagged("two-phase", vec![optional(value.clone())]),
        tagged("no-two-phase", vec![]),
        tagged("binary", vec![optional(value.clone())]),
        tagged("no-binary", vec![]),
        tagged("create-slot", vec![]),
        tagged("no-create-slot", vec![]),
        tagged("run-as-owner", vec![]),
        tagged("no-run-as-owner", vec![]),
        tagged("failover", vec![]),
        tagged("no-failover", vec![]),
        tagged("refresh", vec![]),
        tagged("no-refresh", vec![]),
        tagged("enabled", vec![]),
        tagged("no-enabled", vec![]),
        tagged("connect", vec![]),
        tagged("no-connect", vec![]),
        tagged("password-required", vec![]),
        tagged("no-password-required", vec![]),
        tagged("disable-on-error", vec![optional(value.clone())]),
        tagged("no-disable-on-error", vec![]),
        tagged("lsn", vec![string()]),
        tagged("slot-name", vec![string()]),
        tagged("streaming", vec![string()]),
        tagged("synchronous-commit", vec![string()]),
        tagged("origin", vec![string()]),
        tagged("option", vec![string(), optional(value.clone())]),
    ]);

    let trigger_event = P::OneOf(vec![
        exact("insert"),
        exact("delete"),
        exact("truncate"),
        exact("update"),
        tagged("update", vec![one_or_more(string())]),
    ]);
    let trigger_transition = P::OneOf(
        ["new-table", "old-table", "new-row", "old-row"]
            .iter()
            .map(|tag| tagged(tag, vec![string()]))
            .collect(),
    );
    let trigger_function = tagged(
        "function",
        vec![name(), optional(tagged("args", vec![repeat(string())]))],
    );
    let trigger_prefix = || {
        vec![
            string(),
            optional(exact("or-replace")),
            optional(exact("constraint")),
            enum_values(&["before", "after", "instead"]),
            tagged("on", vec![table.clone()]),
            optional(tagged("from", vec![table.clone()])),
            tagged("events", vec![one_or_more(trigger_event.clone())]),
            tagged("for-each", vec![enum_values(&["row", "statement"])]),
            optional(tagged("when", vec![value.clone()])),
            optional(tagged(
                "transitions",
                vec![one_or_more(trigger_transition.clone())],
            )),
        ]
    };
    let publication_table = P::List(vec![
        exact("table"),
        optional(tagged("only", vec![P::Exact(Sexp::symbol("true"))])),
        P::OneOf(vec![
            P::List(vec![string()]),
            P::List(vec![string(), string()]),
            P::List(vec![string(), string(), string()]),
        ]),
        optional(tagged("where", vec![value.clone()])),
        optional(tagged("columns", vec![repeat(string())])),
    ]);
    let rule_event = enum_values(&["on-select", "on-insert", "on-update", "on-delete"]);
    let aggregate_parameters = tagged(
        "parameters",
        vec![one_or_more(P::OneOf(vec![
            value.clone(),
            exact("*"),
            tagged("variadic", vec![value.clone()]),
        ]))],
    );
    let opclass_item = P::OneOf(vec![
        tagged(
            "operator",
            vec![
                symbol(),
                optional(name_object()),
                optional(tagged("types", vec![repeat(value.clone())])),
                optional(tagged("order-by", vec![name()])),
            ],
        ),
        tagged(
            "function",
            vec![
                symbol(),
                optional(name_object()),
                optional(tagged("types", vec![repeat(value.clone())])),
                optional(tagged("order-by", vec![name()])),
            ],
        ),
        tagged("storage", vec![value.clone()]),
    ]);

    let patterns = vec![
        (
            "create-access-method",
            root(
                "create-access-method",
                vec![
                    string(),
                    tagged("type", vec![enum_values(&["index", "table"])]),
                    tagged("handler", vec![name()]),
                ],
            ),
        ),
        (
            "create-aggregate",
            root(
                "create-aggregate",
                vec![
                    name(),
                    optional(exact("or-replace")),
                    optional(tagged("if-exists", vec![])),
                    optional(aggregate_parameters),
                    optional(tagged("order-by", vec![one_or_more(value.clone())])),
                    repeat(aggregate_option),
                ],
            ),
        ),
        (
            "create-cast",
            root(
                "create-cast",
                vec![
                    value.clone(),
                    value.clone(),
                    optional(exact("with-inout")),
                    optional(tagged("function", vec![name_object()])),
                    tagged(
                        "as",
                        vec![enum_values(&["implicit", "assignment", "explicit"])],
                    ),
                ],
            ),
        ),
        (
            "create-collation",
            root(
                "create-collation",
                vec![
                    name(),
                    optional(tagged("if-exists", vec![])),
                    repeat(P::OneOf(vec![
                        tagged("from", vec![name()]),
                        option_with_scalar("provider"),
                        option_with_scalar("locale"),
                        option_with_scalar("lc-collate"),
                        option_with_scalar("lc-ctype"),
                        option_with_scalar("collversion"),
                        option_with_scalar("version"),
                        option_with_scalar("rules"),
                        option_with_scalar("deterministic"),
                    ])),
                ],
            ),
        ),
        (
            "create-conversion",
            root(
                "create-conversion",
                vec![
                    name(),
                    tagged("for", vec![string()]),
                    tagged("to", vec![string()]),
                    tagged("from", vec![name()]),
                    optional(exact("default")),
                ],
            ),
        ),
        (
            "create-database",
            root(
                "create-database",
                vec![
                    string(),
                    repeat(P::OneOf(vec![
                        exact("allow-connections"),
                        exact("no-allow-connections"),
                        exact("is-template"),
                        exact("no-is-template"),
                        tagged(
                            "connection-limit",
                            vec![P::OneOf(vec![symbol(), integer()])],
                        ),
                        option_with_scalar("owner"),
                        option_with_scalar("template"),
                        option_with_scalar("encoding"),
                        option_with_scalar("locale"),
                        option_with_scalar("lc-collate"),
                        option_with_scalar("lc-type"),
                        option_with_scalar("tablespace"),
                        option_with_scalar("strategy"),
                        option_with_scalar("icu-locale"),
                        option_with_scalar("locale-provider"),
                        option_with_scalar("collation-version"),
                        option_with_scalar("builtin-locale"),
                        option_with_scalar("icu-rules"),
                        tagged("oid", vec![P::OneOf(vec![symbol(), integer()])]),
                    ])),
                ],
            ),
        ),
        (
            "create-event-trigger",
            root(
                "create-event-trigger",
                vec![
                    string(),
                    tagged("on", vec![string()]),
                    repeat(tagged(
                        "when",
                        vec![P::OneOf(vec![
                            P::List(vec![symbol(), repeat(value.clone())]),
                            tagged("event", vec![one_or_more(string())]),
                            tagged("tags", vec![one_or_more(string())]),
                        ])],
                    )),
                    tagged("execute", vec![tagged("function", vec![name()])]),
                ],
            ),
        ),
        (
            "create-extension",
            root(
                "create-extension",
                vec![
                    string(),
                    optional(tagged("if-not-exists", vec![])),
                    repeat(P::OneOf(vec![
                        tagged("version", vec![string()]),
                        tagged("schema", vec![string()]),
                        exact("cascade"),
                    ])),
                ],
            ),
        ),
        (
            "create-foreign-data-wrapper",
            root(
                "create-foreign-data-wrapper",
                vec![
                    string(),
                    repeat(P::OneOf(vec![
                        tagged("handler", vec![string()]),
                        tagged("validator", vec![string()]),
                        tagged("no-handler", vec![]),
                        tagged("no-validator", vec![]),
                    ])),
                    optional(options()),
                ],
            ),
        ),
        (
            "create-foreign-server",
            root(
                "create-foreign-server",
                vec![
                    optional(tagged("if-not-exists", vec![])),
                    string(),
                    optional(tagged("type", vec![string()])),
                    optional(tagged("version", vec![string()])),
                    tagged("foreign-data-wrapper", vec![string()]),
                    optional(options()),
                ],
            ),
        ),
        (
            "create-group",
            root("create-group", vec![string(), repeat(role_option.clone())]),
        ),
        (
            "create-language",
            root(
                "create-language",
                vec![
                    string(),
                    optional(exact("or-replace")),
                    optional(exact("trusted")),
                    repeat(P::OneOf(vec![
                        tagged("handler", vec![name()]),
                        tagged("inline", vec![name()]),
                        tagged("validator", vec![name()]),
                    ])),
                ],
            ),
        ),
        (
            "create-operator",
            root(
                "create-operator",
                vec![
                    name(),
                    optional(tagged("if-exists", vec![])),
                    repeat(operator_option),
                ],
            ),
        ),
        (
            "create-operator-class",
            root(
                "create-operator-class",
                vec![
                    name(),
                    optional(tagged("default", vec![])),
                    tagged("for", vec![value.clone()]),
                    tagged("using", vec![string()]),
                    optional(tagged("family", vec![name()])),
                    optional(tagged("as", vec![repeat(opclass_item)])),
                ],
            ),
        ),
        (
            "create-operator-family",
            root(
                "create-operator-family",
                vec![name(), tagged("using", vec![string()])],
            ),
        ),
        (
            "create-policy",
            root(
                "create-policy",
                vec![
                    string(),
                    tagged("on", vec![table.clone()]),
                    optional(tagged(
                        "for",
                        vec![enum_values(&["select", "insert", "update", "delete"])],
                    )),
                    optional(tagged("as", vec![exact("restrictive")])),
                    optional(roles_clause("to")),
                    optional(tagged("using", vec![value.clone()])),
                    optional(tagged("with-check", vec![value.clone()])),
                ],
            ),
        ),
        (
            "create-publication",
            root(
                "create-publication",
                vec![
                    string(),
                    repeat(P::OneOf(vec![
                        tagged("publish", vec![string()]),
                        tagged("publish-via-partition-root", vec![]),
                        tagged("no-publish-via-partition-root", vec![]),
                        tagged("option", vec![string(), optional(value.clone())]),
                    ])),
                    optional(P::OneOf(vec![
                        tagged("all-tables", vec![]),
                        tagged(
                            "tables",
                            vec![one_or_more(P::OneOf(vec![
                                publication_table.clone(),
                                tagged("schema", vec![P::OneOf(vec![string(), exact("current")])]),
                            ]))],
                        ),
                    ])),
                ],
            ),
        ),
        (
            "create-role",
            root("create-role", vec![string(), repeat(role_option.clone())]),
        ),
        (
            "create-rule",
            root(
                "create-rule",
                vec![
                    string(),
                    optional(exact("or-replace")),
                    rule_event,
                    tagged("to", vec![table.clone()]),
                    optional(exact("instead")),
                    optional(tagged("when", vec![value.clone()])),
                    P::OneOf(vec![
                        tagged("nothing", vec![]),
                        tagged("actions", vec![one_or_more(value.clone())]),
                    ]),
                ],
            ),
        ),
        (
            "create-statistics",
            root(
                "create-statistics",
                vec![
                    name(),
                    optional(tagged("kinds", vec![one_or_more(symbol())])),
                    tagged("on", vec![one_or_more(value.clone())]),
                    tagged("from", vec![one_or_more(value.clone())]),
                ],
            ),
        ),
        (
            "create-subscription",
            root(
                "create-subscription",
                vec![
                    string(),
                    tagged("connection", vec![string()]),
                    tagged("publications", vec![one_or_more(string())]),
                    repeat(subscription_option),
                ],
            ),
        ),
        (
            "create-tablespace",
            root(
                "create-tablespace",
                vec![
                    string(),
                    optional(tagged("owner", vec![role()])),
                    tagged("location", vec![string()]),
                    optional(options()),
                ],
            ),
        ),
        (
            "create-text-search-configuration",
            root(
                "create-text-search-configuration",
                vec![
                    name(),
                    optional(tagged("if-exists", vec![])),
                    repeat(ts_config_option),
                ],
            ),
        ),
        (
            "create-text-search-dictionary",
            root(
                "create-text-search-dictionary",
                vec![
                    name(),
                    optional(tagged("if-exists", vec![])),
                    repeat(ts_dictionary_option),
                ],
            ),
        ),
        (
            "create-text-search-parser",
            root(
                "create-text-search-parser",
                vec![
                    name(),
                    optional(tagged("if-exists", vec![])),
                    repeat(ts_parser_option),
                ],
            ),
        ),
        (
            "create-text-search-template",
            root(
                "create-text-search-template",
                vec![
                    name(),
                    optional(tagged("if-exists", vec![])),
                    repeat(ts_template_option),
                ],
            ),
        ),
        (
            "create-transform",
            root(
                "create-transform",
                vec![
                    optional(exact("or-replace")),
                    tagged("for", vec![value.clone()]),
                    tagged("language", vec![string()]),
                    tagged("from-sql", vec![name_object()]),
                    tagged("to-sql", vec![name_object()]),
                ],
            ),
        ),
        (
            "create-trigger",
            P::OneOf(vec![
                root(
                    "create-trigger",
                    trigger_prefix()
                        .into_iter()
                        .chain(std::iter::once(trigger_function.clone()))
                        .collect(),
                ),
                root(
                    "create-trigger",
                    trigger_prefix()
                        .into_iter()
                        .chain([
                            exact("deferrable"),
                            enum_values(&["initially-deferred", "initially-immediate"]),
                            trigger_function.clone(),
                        ])
                        .collect(),
                ),
            ]),
        ),
        (
            "create-user",
            root("create-user", vec![string(), repeat(role_option)]),
        ),
        (
            "create-user-mapping",
            root(
                "create-user-mapping",
                vec![
                    optional(tagged("if-not-exists", vec![])),
                    tagged("for", vec![role()]),
                    tagged("server", vec![string()]),
                    optional(options()),
                ],
            ),
        ),
    ];

    patterns
        .into_iter()
        .map(|(tag, pattern)| (format!("pg-statement-{tag}"), pattern))
        .collect()
}
