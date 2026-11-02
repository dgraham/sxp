use sxp::Sexp;
use sxp::pattern::{AtomType, Pattern};

pub(super) const STRICT_STATEMENT_TAGS: &[&str] = &[
    "create-table",
    "create-table-as",
    "create-foreign-table",
    "alter-table",
    "alter-foreign-table",
    "drop-table",
    "drop-foreign-table",
    "create-index",
    "alter-index",
    "drop-index",
    "create-view",
    "alter-view",
    "drop-view",
    "create-materialized-view",
    "alter-materialized-view",
    "drop-materialized-view",
    "create-schema",
    "alter-schema",
    "drop-schema",
    "create-domain",
    "alter-domain",
    "drop-domain",
    "create-type",
    "alter-type",
    "drop-type",
    "create-enum",
    "alter-enum",
    "create-sequence",
    "alter-sequence",
    "drop-sequence",
    "create-function",
    "alter-function",
    "alter-function-depends",
    "drop-function",
    "create-procedure",
    "alter-procedure",
    "alter-procedure-depends",
    "drop-procedure",
    "alter-routine",
    "drop-routine",
];

pub(super) fn rules() -> Vec<(String, Pattern)> {
    use Pattern as P;

    let string = P::Atom(AtomType::String);
    let value = P::Reference("value".to_owned());
    let table = P::Reference("table".to_owned());
    let query = P::Reference("query".to_owned());
    let optional = |pattern: P| P::Optional(Box::new(pattern));
    let repeated = |pattern: P| P::Repeat(Box::new(pattern));
    let strings = || {
        P::OneOf(vec![
            P::List(vec![string.clone()]),
            P::List(vec![string.clone(), string.clone()]),
            P::List(vec![string.clone(), string.clone(), string.clone()]),
        ])
    };
    let name = tagged("name", [string.clone(), repeated(string.clone())]);
    let parameters = tagged("parameters", [repeated(value.clone())]);
    let name_with_signature = P::OneOf(vec![
        name.clone(),
        tagged("name", [string.clone(), parameters.clone()]),
        tagged("name", [string.clone(), P::Exact(Sexp::symbol("*"))]),
        tagged("name", [string.clone(), string.clone(), parameters.clone()]),
        tagged(
            "name",
            [
                string.clone(),
                string.clone(),
                string.clone(),
                parameters.clone(),
            ],
        ),
        tagged(
            "name",
            [string.clone(), string.clone(), P::Exact(Sexp::symbol("*"))],
        ),
        tagged(
            "name",
            [
                string.clone(),
                string.clone(),
                string.clone(),
                P::Exact(Sexp::symbol("*")),
            ],
        ),
    ]);
    let object_name = P::OneOf(vec![name_with_signature.clone(), table.clone(), strings()]);
    let if_exists = tagged("if-exists", []);
    let if_not_exists = tagged("if-not-exists", []);
    let ddl_value = P::Reference("ddl-value".to_owned());
    let column_definition = tagged(
        "column",
        [string.clone(), P::OneOrMore(Box::new(ddl_value.clone()))],
    );
    let ddl_value_pattern = P::OneOf(vec![
        column_definition.clone(),
        tagged("collate", [string.clone()]),
        tagged("like", [table.clone(), repeated(P::Atom(AtomType::Symbol))]),
        tagged("exclude", [repeated(ddl_value.clone())]),
        tagged("in", [P::OneOrMore(Box::new(ddl_value.clone()))]),
        value.clone(),
    ]);
    let option = P::OneOf(vec![
        tagged("option", [string.clone()]),
        tagged("option", [string.clone(), value.clone()]),
    ]);
    let columns = tagged("columns", [repeated(ddl_value.clone())]);
    let column_names = tagged("columns", [repeated(string.clone())]);
    let query_clause = tagged("query", [query.clone()]);
    let owner = tagged("owner", [tagged("role", [value.clone()])]);
    let alter_generic = |allowed_heads: &[&str]| {
        let heads = P::OneOf(
            allowed_heads
                .iter()
                .map(|head| P::Exact(Sexp::symbol(*head)))
                .collect(),
        );
        P::List(vec![heads, repeated(ddl_value.clone())])
    };
    let drop_statement = |tag: &str, name_pattern: P, concurrent: bool| {
        let mut fields = vec![
            optional(if_exists.clone()),
            name_pattern.clone(),
            repeated(name_pattern),
        ];
        if concurrent {
            fields.push(optional(P::Exact(Sexp::symbol("concurrently"))));
        }
        fields.push(optional(P::Exact(Sexp::symbol("cascade"))));
        tagged(tag, fields)
    };
    let create_table = |tag: &str, foreign: bool| {
        let mut fields = vec![
            optional(tagged(
                "persistence",
                [P::OneOf(vec![
                    P::Exact(Sexp::symbol("temporary")),
                    P::Exact(Sexp::symbol("unlogged")),
                ])],
            )),
            optional(if_not_exists.clone()),
            table.clone(),
            optional(tagged("of", [value.clone()])),
            columns.clone(),
            optional(tagged("inherits", [value.clone(), repeated(value.clone())])),
            optional(tagged("constraints", [repeated(value.clone())])),
            optional(tagged("options", [repeated(option.clone())])),
            optional(tagged("using", [string.clone()])),
            optional(tagged(
                "on-commit",
                [P::OneOf(
                    ["preserve-rows", "delete-rows", "drop"]
                        .into_iter()
                        .map(|value| P::Exact(Sexp::symbol(value)))
                        .collect(),
                )],
            )),
            optional(tagged("tablespace", [string.clone()])),
            optional(tagged("partition-of", [table.clone()])),
            optional(tagged(
                "bounds",
                [P::OneOrMore(Box::new(ddl_value.clone()))],
            )),
            optional(tagged("partition-by", [repeated(ddl_value.clone())])),
        ];
        if foreign {
            fields.push(tagged("server", [string.clone()]));
            fields.push(optional(tagged("options", [repeated(option.clone())])));
        }
        tagged(tag, fields)
    };
    let table_actions = P::OneOf(vec![
        alter_generic(&[
            "add-column",
            "add-constraint",
            "add-identity",
            "add-mapping",
            "add-publications",
            "alter-column-options",
            "alter-column-type",
            "alter-constraint",
            "attach-partition",
            "disable-row-level-security",
            "disable-trigger",
            "disable-trigger-all",
            "disable-trigger-user",
            "disable-rule",
            "disable-replica-rule",
            "drop-column",
            "drop-constraint",
            "drop-default",
            "drop-expression",
            "drop-identity",
            "drop-not-null",
            "drop-rule",
            "drop-trigger",
            "drop-replica-rule",
            "enable-row-level-security",
            "enable-always-trigger",
            "enable-always-rule",
            "enable-replica-trigger",
            "enable-replica-rule",
            "enable-trigger",
            "enable-trigger-all",
            "enable-trigger-user",
            "enable-rule",
            "force-row-level-security",
            "no-force-row-level-security",
            "no-inherit",
            "rename-column",
            "rename-constraint",
            "rename-table",
            "set-access-method",
            "set-compression",
            "set-default",
            "set-expression",
            "set-identity",
            "set-logged",
            "set-not-null",
            "set-options",
            "set-statistics",
            "set-storage",
            "set-tablespace",
            "set-unlogged",
            "set-without-cluster",
            "set-without-oids",
            "validate-constraint",
            "cluster-on",
            "owner",
            "set-schema",
            "set",
            "options",
            "reset",
            "reset-options",
            "inherit",
            "of",
            "replica-identity",
            "detach-partition",
        ]),
        tagged("not-of", []),
    ]);
    let alter_table = |tag: &str| {
        tagged(
            tag,
            [
                table.clone(),
                optional(if_exists.clone()),
                table_actions.clone(),
                repeated(table_actions.clone()),
            ],
        )
    };

    let index_column = P::OneOf(vec![
        tagged(
            "index-column",
            [string.clone(), repeated(ddl_value.clone())],
        ),
        value.clone(),
    ]);
    let create_index = tagged(
        "create-index",
        [
            optional(P::Exact(Sexp::symbol("unique"))),
            optional(P::Exact(Sexp::symbol("nulls-not-distinct"))),
            optional(P::Exact(Sexp::symbol("concurrently"))),
            optional(if_not_exists.clone()),
            optional(tagged("name", [string.clone()])),
            tagged("on", [table.clone()]),
            optional(tagged("using", [string.clone()])),
            tagged("columns", [repeated(index_column)]),
            optional(tagged("include", [repeated(value.clone())])),
            optional(tagged("options", [repeated(option.clone())])),
            optional(tagged("tablespace", [string.clone()])),
            optional(tagged("where", [value.clone()])),
        ],
    );
    let alter_index = P::OneOf(vec![
        tagged(
            "alter-index",
            [
                table.clone(),
                optional(if_exists.clone()),
                alter_generic(&[
                    "rename-table",
                    "set-options",
                    "set",
                    "set-tablespace",
                    "set-schema",
                    "owner",
                ]),
                repeated(alter_generic(&[
                    "rename-table",
                    "set-options",
                    "set",
                    "set-tablespace",
                    "set-schema",
                    "owner",
                ])),
            ],
        ),
        tagged(
            "alter-index",
            [
                name_with_signature.clone(),
                optional(if_exists.clone()),
                alter_generic(&[
                    "rename",
                    "set-options",
                    "set",
                    "set-tablespace",
                    "set-schema",
                    "owner",
                ]),
                repeated(alter_generic(&[
                    "rename",
                    "set-options",
                    "set",
                    "set-tablespace",
                    "set-schema",
                    "owner",
                ])),
            ],
        ),
    ]);

    let view_persistence = tagged(
        "persistence",
        [P::OneOf(vec![
            P::Exact(Sexp::symbol("temporary")),
            P::Exact(Sexp::symbol("unlogged")),
        ])],
    );
    let view_columns = optional(tagged("columns", [repeated(string.clone())]));
    let create_view = tagged(
        "create-view",
        [
            optional(P::Exact(Sexp::symbol("or-replace"))),
            optional(P::Exact(Sexp::symbol("recursive"))),
            optional(view_persistence.clone()),
            table.clone(),
            view_columns.clone(),
            query_clause.clone(),
            optional(tagged("options", [repeated(option.clone())])),
            optional(tagged(
                "check-option",
                [P::OneOf(vec![
                    P::Exact(Sexp::symbol("local")),
                    P::Exact(Sexp::symbol("cascaded")),
                ])],
            )),
        ],
    );
    let create_materialized_view = tagged(
        "create-materialized-view",
        [
            optional(if_not_exists.clone()),
            table.clone(),
            view_columns.clone(),
            optional(tagged("using", [string.clone()])),
            optional(tagged("options", [repeated(option.clone())])),
            optional(tagged("tablespace", [string.clone()])),
            query_clause.clone(),
            optional(P::Exact(Sexp::symbol("with-no-data"))),
        ],
    );
    let alter_view = |tag: &str| {
        tagged(
            tag,
            [
                P::OneOf(vec![table.clone(), name_with_signature.clone()]),
                optional(if_exists.clone()),
                alter_generic(&[
                    "owner",
                    "rename",
                    "set",
                    "reset",
                    "set-options",
                    "set-schema",
                    "set-tablespace",
                ]),
                repeated(alter_generic(&[
                    "owner",
                    "rename",
                    "set",
                    "reset",
                    "set-options",
                    "set-schema",
                    "set-tablespace",
                ])),
            ],
        )
    };

    let create_schema = tagged(
        "create-schema",
        [
            optional(if_not_exists.clone()),
            optional(string.clone()),
            optional(tagged("authorization", [value.clone()])),
            optional(tagged("elements", [repeated(value.clone())])),
        ],
    );
    let alter_schema = tagged(
        "alter-schema",
        [
            string.clone(),
            P::OneOf(vec![
                tagged("rename", [string.clone()]),
                owner.clone(),
                tagged("set-schema", [string.clone()]),
            ]),
        ],
    );
    let domain_action = P::OneOf(vec![
        tagged("set-default", [value.clone()]),
        P::Exact(Sexp::symbol("drop-default")),
        P::Exact(Sexp::symbol("drop-not-null")),
        P::Exact(Sexp::symbol("set-not-null")),
        tagged("add-constraint", [value.clone()]),
        tagged(
            "drop-constraint",
            [
                string.clone(),
                optional(P::Exact(Sexp::symbol("if-exists"))),
                optional(P::Exact(Sexp::symbol("cascade"))),
            ],
        ),
        tagged("validate-constraint", [string.clone()]),
        tagged("rename-constraint", [string.clone(), string.clone()]),
        owner.clone(),
        tagged("set-schema", [string.clone()]),
        tagged("rename", [string.clone()]),
    ]);
    let create_domain = tagged(
        "create-domain",
        [
            name.clone(),
            value.clone(),
            optional(tagged("collate", [string.clone()])),
            tagged("constraints", [repeated(value.clone())]),
        ],
    );
    let alter_domain = tagged("alter-domain", [name.clone(), domain_action.clone()]);

    let create_enum = tagged(
        "create-enum",
        [
            name.clone(),
            tagged("values", [string.clone(), repeated(string.clone())]),
        ],
    );
    let alter_enum = tagged(
        "alter-enum",
        [
            name.clone(),
            P::OneOf(vec![
                tagged(
                    "add-value",
                    [
                        string.clone(),
                        optional(P::OneOf(vec![
                            tagged("before", [string.clone()]),
                            tagged("after", [string.clone()]),
                        ])),
                        optional(if_not_exists.clone()),
                    ],
                ),
                tagged("rename-value", [string.clone(), string.clone()]),
            ]),
        ],
    );

    let type_option = P::OneOf(vec![
        P::Exact(Sexp::symbol("passedbyvalue")),
        tagged("internallength", [value.clone()]),
        tagged("input", [value.clone()]),
        tagged("output", [value.clone()]),
        tagged("receive", [value.clone()]),
        tagged("send", [value.clone()]),
        tagged("typmod-in", [value.clone()]),
        tagged("typmod-out", [value.clone()]),
        tagged("analyze", [value.clone()]),
        tagged("element", [value.clone()]),
        tagged("like", [value.clone()]),
        tagged("category", [value.clone()]),
        tagged("preferred", [value.clone()]),
        tagged("default", [value.clone()]),
        tagged("delimiter", [value.clone()]),
        tagged("collatable", [value.clone()]),
        tagged("alignment", [value.clone()]),
        tagged("storage", [value.clone()]),
        tagged("subtype-opclass", [value.clone()]),
        tagged("canonical", [value.clone()]),
        tagged("subtype-diff", [value.clone()]),
        tagged("multirange-type-name", [value.clone()]),
    ]);
    let create_type = P::OneOf(vec![
        tagged(
            "create-type",
            [
                name.clone(),
                optional(if_not_exists.clone()),
                repeated(type_option.clone()),
            ],
        ),
        tagged(
            "create-type",
            [
                name.clone(),
                tagged("attributes", [repeated(ddl_value.clone())]),
            ],
        ),
        tagged(
            "create-type",
            [
                name.clone(),
                tagged(
                    "as-range",
                    [
                        tagged("subtype", [value.clone()]),
                        optional(tagged("subtype-opclass", [value.clone()])),
                        optional(tagged("collation", [value.clone()])),
                        optional(tagged("canonical", [value.clone()])),
                        optional(tagged("subtype-diff", [value.clone()])),
                        optional(tagged("multirange-type-name", [value.clone()])),
                    ],
                ),
            ],
        ),
    ]);
    let alter_type_action = P::OneOf(vec![
        tagged("rename-attribute", [string.clone(), string.clone()]),
        tagged("set-options", [repeated(option.clone())]),
        owner.clone(),
        tagged("rename", [string.clone()]),
        tagged("set-schema", [string.clone()]),
        P::Exact(Sexp::symbol("cascade")),
    ]);
    let alter_type = tagged(
        "alter-type",
        [
            name.clone(),
            alter_type_action.clone(),
            repeated(alter_type_action.clone()),
        ],
    );
    let alter_domain_or_type = alter_generic(&[
        "rename-constraint",
        "set-default",
        "drop-default",
        "drop-not-null",
        "set-not-null",
        "add-constraint",
        "drop-constraint",
        "validate-constraint",
        "rename-attribute",
        "set-options",
        "owner",
        "rename",
        "set-schema",
    ]);

    let sequence_option = P::OneOf(vec![
        option.clone(),
        tagged("option", [string.clone(), value.clone()]),
    ]);
    let create_sequence = tagged(
        "create-sequence",
        [
            table.clone(),
            optional(if_not_exists.clone()),
            optional(tagged("options", [repeated(sequence_option.clone())])),
        ],
    );
    let alter_sequence = tagged(
        "alter-sequence",
        [
            table.clone(),
            optional(if_exists.clone()),
            optional(tagged("options", [repeated(sequence_option)])),
        ],
    );

    let parameter = tagged(
        "parameter",
        [
            optional(P::OneOf(
                ["in", "out", "inout", "variadic", "table"]
                    .into_iter()
                    .map(|mode| P::Exact(Sexp::symbol(mode)))
                    .collect(),
            )),
            optional(string.clone()),
            value.clone(),
            optional(tagged("default", [value.clone()])),
        ],
    );
    let function_parameters = tagged("parameters", [repeated(parameter)]);
    let function_name = P::OneOf(vec![
        name.clone(),
        tagged("name", [string.clone(), string.clone()]),
    ]);
    let function_option = P::OneOf(vec![
        tagged("language", [string.clone()]),
        tagged("as", [string.clone(), repeated(string.clone())]),
        tagged("sql-body", [repeated(value.clone())]),
        tagged(
            "parallel",
            [P::OneOf(
                ["safe", "unsafe", "restricted"]
                    .into_iter()
                    .map(|mode| P::Exact(Sexp::symbol(mode)))
                    .collect(),
            )],
        ),
        tagged("cost", [value.clone()]),
        tagged("rows", [value.clone()]),
        tagged("set", [repeated(value.clone())]),
        tagged("reset", [repeated(value.clone())]),
        tagged("transform", [repeated(value.clone())]),
        tagged("support", [value.clone()]),
        P::OneOf(
            [
                "immutable",
                "stable",
                "volatile",
                "strict",
                "called-on-null-input",
                "security-definer",
                "security-invoker",
                "leakproof",
                "not-leakproof",
                "window",
            ]
            .into_iter()
            .map(|name| P::Exact(Sexp::symbol(name)))
            .collect(),
        ),
    ]);
    let create_function = tagged(
        "create-function",
        [
            optional(P::Exact(Sexp::symbol("or-replace"))),
            function_name.clone(),
            optional(function_parameters.clone()),
            optional(tagged("returns", [value.clone()])),
            repeated(function_option.clone()),
        ],
    );
    let create_procedure = tagged(
        "create-procedure",
        [
            optional(P::Exact(Sexp::symbol("or-replace"))),
            function_name.clone(),
            optional(function_parameters.clone()),
            repeated(function_option.clone()),
        ],
    );
    let alter_routine_action = P::OneOf(vec![
        function_option.clone(),
        tagged("rename", [string.clone()]),
        owner.clone(),
        tagged("set-schema", [string.clone()]),
        tagged("reset", [P::Exact(Sexp::symbol("all"))]),
        tagged("depends-on-extension", [string.clone()]),
        P::Exact(Sexp::symbol("remove")),
    ]);
    let alter_routine = |tag: &str| {
        tagged(
            tag,
            [
                name_with_signature.clone(),
                alter_routine_action.clone(),
                repeated(alter_routine_action.clone()),
            ],
        )
    };
    let routine_depends = |tag: &str| {
        tagged(
            tag,
            [
                name_with_signature.clone(),
                tagged("depends-on-extension", [string.clone()]),
                optional(P::Exact(Sexp::symbol("remove"))),
            ],
        )
    };
    let routine_drop = |tag: &str| drop_statement(tag, name_with_signature.clone(), false);

    let mut rules = vec![
        ("ddl-value".to_owned(), ddl_value_pattern),
        ("ddl-name".to_owned(), name.clone()),
        (
            "ddl-name-with-signature".to_owned(),
            name_with_signature.clone(),
        ),
        ("ddl-object-name".to_owned(), object_name),
        ("ddl-option".to_owned(), option.clone()),
        ("ddl-function-option".to_owned(), function_option),
        ("ddl-alter-table-action".to_owned(), table_actions.clone()),
        (
            "ddl-alter-domain-or-type-action".to_owned(),
            alter_domain_or_type.clone(),
        ),
        ("ddl-alter-type-action".to_owned(), alter_type_action),
    ];

    let mut add = |tag: &str, pattern: Pattern| {
        rules.push((
            format!("pg-statement-{tag}"),
            with_optional_sqlcommenter(pattern),
        ));
    };
    add("create-table", create_table("create-table", false));
    add(
        "create-table-as",
        tagged(
            "create-table-as",
            [
                optional(if_not_exists.clone()),
                optional(tagged(
                    "persistence",
                    [P::OneOf(vec![
                        P::Exact(Sexp::symbol("temporary")),
                        P::Exact(Sexp::symbol("unlogged")),
                    ])],
                )),
                table.clone(),
                optional(column_names.clone()),
                optional(tagged("options", [repeated(option.clone())])),
                optional(tagged("using", [string.clone()])),
                optional(tagged(
                    "on-commit",
                    [P::OneOf(
                        ["preserve-rows", "delete-rows", "drop"]
                            .into_iter()
                            .map(|value| P::Exact(Sexp::symbol(value)))
                            .collect(),
                    )],
                )),
                optional(tagged("tablespace", [string.clone()])),
                query_clause.clone(),
                optional(P::Exact(Sexp::symbol("with-no-data"))),
            ],
        ),
    );
    add(
        "create-foreign-table",
        create_table("create-foreign-table", true),
    );
    add("alter-table", alter_table("alter-table"));
    add("alter-foreign-table", alter_table("alter-foreign-table"));
    add(
        "drop-table",
        drop_statement("drop-table", name_with_signature.clone(), false),
    );
    add(
        "drop-foreign-table",
        drop_statement("drop-foreign-table", name_with_signature.clone(), false),
    );
    add("create-index", create_index);
    add("alter-index", alter_index);
    add(
        "drop-index",
        drop_statement("drop-index", name_with_signature.clone(), true),
    );
    add("create-view", create_view);
    add("alter-view", alter_view("alter-view"));
    add(
        "drop-view",
        drop_statement("drop-view", name_with_signature.clone(), false),
    );
    add("create-materialized-view", create_materialized_view);
    add(
        "alter-materialized-view",
        alter_view("alter-materialized-view"),
    );
    add(
        "drop-materialized-view",
        drop_statement("drop-materialized-view", name_with_signature.clone(), false),
    );
    add("create-schema", create_schema);
    add("alter-schema", alter_schema);
    add(
        "drop-schema",
        drop_statement("drop-schema", name_with_signature.clone(), false),
    );
    add("create-domain", create_domain);
    add(
        "alter-domain",
        P::OneOf(vec![
            alter_domain,
            tagged("alter-domain", [name.clone(), alter_domain_or_type.clone()]),
        ]),
    );
    add(
        "drop-domain",
        drop_statement("drop-domain", name_with_signature.clone(), false),
    );
    add("create-type", create_type);
    add(
        "alter-type",
        P::OneOf(vec![
            alter_type,
            tagged("alter-type", [name.clone(), alter_domain_or_type]),
        ]),
    );
    add(
        "drop-type",
        drop_statement("drop-type", name_with_signature.clone(), false),
    );
    add("create-enum", create_enum);
    add("alter-enum", alter_enum);
    add("create-sequence", create_sequence);
    add("alter-sequence", alter_sequence);
    add(
        "drop-sequence",
        drop_statement("drop-sequence", name_with_signature.clone(), false),
    );
    add("create-function", create_function);
    add("alter-function", alter_routine("alter-function"));
    add(
        "alter-function-depends",
        routine_depends("alter-function-depends"),
    );
    add("drop-function", routine_drop("drop-function"));
    add("create-procedure", create_procedure);
    add("alter-procedure", alter_routine("alter-procedure"));
    add(
        "alter-procedure-depends",
        routine_depends("alter-procedure-depends"),
    );
    add("drop-procedure", routine_drop("drop-procedure"));
    add("alter-routine", alter_routine("alter-routine"));
    add("drop-routine", routine_drop("drop-routine"));

    rules
}

fn tagged(name: &str, fields: impl IntoIterator<Item = Pattern>) -> Pattern {
    Pattern::List(
        std::iter::once(Pattern::Exact(Sexp::symbol(name)))
            .chain(fields)
            .collect(),
    )
}

fn with_optional_sqlcommenter(pattern: Pattern) -> Pattern {
    match pattern {
        Pattern::OneOf(alternatives) => Pattern::OneOf(
            alternatives
                .into_iter()
                .map(with_optional_sqlcommenter)
                .collect(),
        ),
        Pattern::List(mut fields) => {
            fields.insert(
                1,
                Pattern::Optional(Box::new(Pattern::Reference("sqlcommenter".to_owned()))),
            );
            Pattern::List(fields)
        }
        _ => unreachable!("statement schema must be a list or a list alternative"),
    }
}
