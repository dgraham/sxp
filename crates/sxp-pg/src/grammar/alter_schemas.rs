use sxp::Sexp;
use sxp::pattern::{AtomType, Pattern};

pub(super) const STRICT_STATEMENT_TAGS: &[&str] = &[
    "alter-aggregate",
    "alter-collation",
    "alter-column-options",
    "alter-column-type",
    "alter-constraint",
    "alter-database",
    "alter-default-privileges",
    "alter-event-trigger",
    "alter-extension",
    "alter-foreign-data-wrapper",
    "alter-large-object",
    "alter-mapping",
    "alter-operator",
    "alter-operator-class",
    "alter-operator-family",
    "alter-policy",
    "alter-publication",
    "alter-role",
    "alter-server",
    "alter-statistics",
    "alter-subscription",
    "alter-tablespace",
    "alter-text-search-configuration",
    "alter-text-search-dictionary",
    "alter-user-mapping",
];

pub(super) fn rules() -> Vec<(String, Pattern)> {
    use Pattern as P;

    let atom = P::Atom;
    let string = || atom(AtomType::String);
    let symbol = || atom(AtomType::Symbol);
    let integer = || atom(AtomType::Integer);
    let float = || atom(AtomType::Float);
    let boolean = || atom(AtomType::Boolean);
    let nil = || atom(AtomType::Nil);
    let exact = |value: &str| P::Exact(Sexp::symbol(value));
    let optional = |pattern| P::Optional(Box::new(pattern));
    let repeat = |pattern| P::Repeat(Box::new(pattern));
    let one_or_more = |pattern| P::OneOrMore(Box::new(pattern));
    let reference = |name: &str| P::Reference(name.to_owned());
    let named = |name: &str, fields: Vec<P>| super::tagged(name, fields);
    let enum_symbols =
        |values: &[&str]| P::OneOf(values.iter().map(|value| exact(value)).collect::<Vec<_>>());
    let scalar = P::OneOf(vec![
        symbol(),
        string(),
        integer(),
        float(),
        boolean(),
        nil(),
    ]);
    let numeric_symbol = symbol();
    let expression = reference("expression");
    let value = reference("value");
    let table = reference("table");
    let sqlcommenter = optional(reference("sqlcommenter"));
    let qualified_name = named("name", vec![string(), repeat(string())]);
    let qualified_parts = P::List(vec![string(), repeat(string())]);
    let parameters = named("parameters", vec![repeat(reference("pg-node-type"))]);
    let signature_name = P::OneOf(vec![
        qualified_name.clone(),
        named("name", vec![string(), parameters.clone()]),
        named("name", vec![string(), exact("*")]),
        named("name", vec![string(), string(), parameters.clone()]),
        named(
            "name",
            vec![string(), string(), string(), parameters.clone()],
        ),
        named("name", vec![string(), string(), exact("*")]),
        named("name", vec![string(), string(), string(), exact("*")]),
    ]);
    let role_value = P::OneOf(vec![
        string(),
        enum_symbols(&[
            "public",
            "current-role",
            "current-user",
            "session-user",
            "all",
        ]),
    ]);
    let role = named("role", vec![role_value]);
    let roles = one_or_more(role.clone());
    let owner = named("owner", vec![role.clone()]);
    let rename = named("rename", vec![string()]);
    let set_schema = named("set-schema", vec![string()]);
    let type_node = reference("pg-node-type");
    let plain_type = P::OneOf(vec![
        type_node.clone(),
        named("type", vec![one_or_more(string())]),
    ]);

    let option = |tag: &str| named(tag, vec![string(), scalar.clone()]);
    let generic_option = P::OneOf(vec![
        option("option"),
        option("set-option"),
        option("add-option"),
        named("drop-option", vec![string()]),
    ]);
    let storage_option = P::OneOf(vec![
        named("option", vec![string()]),
        named("option", vec![string(), scalar.clone()]),
        named("set-option", vec![string(), scalar.clone()]),
        named("add-option", vec![string(), scalar.clone()]),
        named("drop-option", vec![string()]),
    ]);
    let role_setting = P::OneOf(vec![
        named("set", vec![string(), repeat(reference("value"))]),
        named("set", vec![string(), exact("default")]),
        named("set", vec![string(), exact("from-current")]),
        named("reset", vec![P::OneOf(vec![string(), exact("all")])]),
    ]);
    let role_option = P::OneOf(vec![
        enum_symbols(&[
            "superuser",
            "no-superuser",
            "createdb",
            "no-createdb",
            "createrole",
            "no-createrole",
            "inherit",
            "no-inherit",
            "login",
            "no-login",
            "replication",
            "no-replication",
            "bypassrls",
            "no-bypassrls",
        ]),
        named("connection-limit", vec![numeric_symbol.clone()]),
        named("password", vec![P::OneOf(vec![string(), nil()])]),
        named("valid-until", vec![reference("value")]),
        named("members", vec![roles.clone()]),
        named("drop-members", vec![roles.clone()]),
        named("in-role", vec![roles.clone()]),
        named("admins", vec![roles.clone()]),
    ]);
    let publication_table = named(
        "table",
        vec![
            qualified_parts,
            optional(named("where", vec![expression.clone()])),
            optional(named("columns", vec![repeat(reference("value"))])),
        ],
    );
    let publication_object = P::OneOf(vec![
        publication_table,
        named("schema", vec![P::OneOf(vec![string(), exact("current")])]),
    ]);
    let publication_objects = one_or_more(publication_object);
    let publication_option = P::OneOf(vec![
        named("publish", vec![string()]),
        named("publish-via-partition-root", vec![]),
        named("no-publish-via-partition-root", vec![]),
        generic_option.clone(),
    ]);
    let subscription_option = P::OneOf(vec![
        P::OneOf(
            [
                "copy-data",
                "no-copy-data",
                "two-phase",
                "no-two-phase",
                "binary",
                "no-binary",
                "create-slot",
                "no-create-slot",
                "run-as-owner",
                "no-run-as-owner",
                "failover",
                "no-failover",
                "refresh",
                "no-refresh",
                "enabled",
                "no-enabled",
                "connect",
                "no-connect",
                "password-required",
                "no-password-required",
                "disable-on-error",
                "no-disable-on-error",
            ]
            .into_iter()
            .map(|tag| {
                if tag == "binary" || tag == "two-phase" || tag == "disable-on-error" {
                    named(tag, vec![optional(value.clone())])
                } else {
                    named(tag, vec![])
                }
            })
            .collect(),
        ),
        named("lsn", vec![string()]),
        named("slot-name", vec![string()]),
        named("streaming", vec![string()]),
        named("synchronous-commit", vec![string()]),
        named("origin", vec![string()]),
        generic_option.clone(),
    ]);
    let operator_item = P::OneOf(vec![
        named(
            "operator",
            vec![
                numeric_symbol.clone(),
                optional(signature_name.clone()),
                optional(named("types", vec![one_or_more(reference("pg-node-type"))])),
                optional(named("order-by", vec![signature_name.clone()])),
            ],
        ),
        named(
            "function",
            vec![
                numeric_symbol.clone(),
                optional(signature_name.clone()),
                optional(named("types", vec![one_or_more(reference("pg-node-type"))])),
                optional(named("order-by", vec![signature_name.clone()])),
            ],
        ),
        named("storage", vec![type_node.clone()]),
    ]);
    let object_name = P::OneOf(vec![
        signature_name.clone(),
        table.clone(),
        plain_type.clone(),
        string(),
    ]);

    let alter_generic_object = |tag: &str, names: P, actions: Vec<P>| {
        let action = P::OneOf(actions);
        root_pattern(
            tag,
            sqlcommenter.clone(),
            vec![names, optional(named("if-exists", vec![])), action],
        )
    };

    let patterns = vec![
        (
            "alter-aggregate",
            alter_generic_object(
                "alter-aggregate",
                signature_name.clone(),
                vec![rename.clone(), owner.clone(), set_schema.clone()],
            ),
        ),
        (
            "alter-collation",
            P::OneOf(vec![
                root_pattern(
                    "alter-collation",
                    sqlcommenter.clone(),
                    vec![qualified_name.clone(), named("refresh-version", vec![])],
                ),
                alter_generic_object(
                    "alter-collation",
                    signature_name.clone(),
                    vec![rename.clone(), owner.clone(), set_schema.clone()],
                ),
            ]),
        ),
        (
            "alter-column-options",
            root_pattern(
                "alter-column-options",
                sqlcommenter.clone(),
                vec![
                    string(),
                    repeat(P::OneOf(vec![
                        generic_option.clone(),
                        named("option", vec![string(), reference("value")]),
                    ])),
                ],
            ),
        ),
        (
            "alter-column-type",
            root_pattern(
                "alter-column-type",
                sqlcommenter.clone(),
                vec![
                    string(),
                    type_node.clone(),
                    optional(named("collate", vec![string()])),
                    optional(named("compression", vec![string()])),
                    optional(named("storage", vec![string()])),
                    optional(named("using", vec![expression.clone()])),
                    optional(exact("cascade")),
                ],
            ),
        ),
        (
            "alter-constraint",
            root_pattern(
                "alter-constraint",
                sqlcommenter.clone(),
                vec![
                    string(),
                    repeat(enum_symbols(&[
                        "not-valid",
                        "no-inherit",
                        "deferrable",
                        "initially-deferred",
                    ])),
                ],
            ),
        ),
        (
            "alter-database",
            root_pattern(
                "alter-database",
                sqlcommenter.clone(),
                vec![
                    string(),
                    repeat(P::OneOf(vec![
                        enum_symbols(&["allow-connections", "no-allow-connections"]),
                        enum_symbols(&["is-template", "no-is-template"]),
                        named("connection-limit", vec![numeric_symbol.clone()]),
                        named("in-tablespace", vec![string()]),
                        named("set", vec![string(), repeat(reference("value"))]),
                        named("set", vec![string(), exact("default")]),
                        named("set", vec![string(), exact("from-current")]),
                        named("reset", vec![P::OneOf(vec![string(), exact("all")])]),
                        named("refresh-collation-version", vec![]),
                        rename.clone(),
                        owner.clone(),
                    ])),
                ],
            ),
        ),
        (
            "alter-default-privileges",
            P::OneOf(vec![
                default_privilege_root(sqlcommenter.clone(), "grant", "to", roles.clone()),
                default_privilege_root(sqlcommenter.clone(), "revoke", "from", roles.clone()),
            ]),
        ),
        (
            "alter-event-trigger",
            root_pattern(
                "alter-event-trigger",
                sqlcommenter.clone(),
                vec![
                    P::OneOf(vec![string(), named("name", vec![string()])]),
                    optional(named("if-exists", vec![])),
                    P::OneOf(vec![
                        named("disable", vec![]),
                        named("enable", vec![]),
                        named("enable", vec![enum_symbols(&["replica", "always"])]),
                        rename.clone(),
                        owner.clone(),
                        set_schema.clone(),
                    ]),
                ],
            ),
        ),
        (
            "alter-extension",
            root_pattern(
                "alter-extension",
                sqlcommenter.clone(),
                vec![
                    string(),
                    optional(P::OneOf(vec![
                        named("update-to", vec![string()]),
                        named("add", vec![object_name.clone()]),
                        named("drop", vec![object_name.clone()]),
                    ])),
                ],
            ),
        ),
        (
            "alter-foreign-data-wrapper",
            root_pattern(
                "alter-foreign-data-wrapper",
                sqlcommenter.clone(),
                vec![
                    string(),
                    repeat(P::OneOf(vec![
                        named("handler", vec![string()]),
                        named("validator", vec![string()]),
                        named("no-handler", vec![]),
                        named("no-validator", vec![]),
                        named("options", vec![one_or_more(generic_option.clone())]),
                    ])),
                ],
            ),
        ),
        (
            "alter-large-object",
            root_pattern(
                "alter-large-object",
                sqlcommenter.clone(),
                vec![named("oid", vec![numeric_symbol.clone()]), owner.clone()],
            ),
        ),
        (
            "alter-mapping",
            root_pattern(
                "alter-mapping",
                sqlcommenter.clone(),
                vec![
                    named("tokens", vec![one_or_more(string())]),
                    named("dictionaries", vec![repeat(signature_name.clone())]),
                ],
            ),
        ),
        (
            "alter-operator",
            root_pattern(
                "alter-operator",
                sqlcommenter.clone(),
                vec![
                    signature_name.clone(),
                    repeat(P::OneOf(vec![
                        owner.clone(),
                        rename.clone(),
                        set_schema.clone(),
                        named(
                            "restrict",
                            vec![P::OneOf(vec![exact("none"), plain_type.clone(), string()])],
                        ),
                        named(
                            "join",
                            vec![P::OneOf(vec![exact("none"), plain_type.clone(), string()])],
                        ),
                        named("commutator", vec![string()]),
                        named("negator", vec![string()]),
                        named("merges", vec![exact("true")]),
                        named("merges", vec![exact("false")]),
                        named("hashes", vec![exact("true")]),
                        named("hashes", vec![exact("false")]),
                        exact("merges"),
                        exact("hashes"),
                    ])),
                ],
            ),
        ),
        (
            "alter-operator-class",
            root_pattern(
                "alter-operator-class",
                sqlcommenter.clone(),
                vec![
                    named("using", vec![string()]),
                    qualified_name.clone(),
                    optional(named("if-exists", vec![])),
                    P::OneOf(vec![rename.clone(), owner.clone(), set_schema.clone()]),
                ],
            ),
        ),
        (
            "alter-operator-family",
            root_pattern(
                "alter-operator-family",
                sqlcommenter.clone(),
                vec![
                    qualified_name.clone(),
                    named("using", vec![string()]),
                    P::OneOf(vec![
                        named("add", vec![one_or_more(operator_item.clone())]),
                        named("drop", vec![one_or_more(operator_item)]),
                    ]),
                ],
            ),
        ),
        (
            "alter-policy",
            root_pattern(
                "alter-policy",
                sqlcommenter.clone(),
                vec![
                    string(),
                    named("on", vec![table.clone()]),
                    optional(named("to", vec![roles.clone()])),
                    optional(named("using", vec![expression.clone()])),
                    optional(named("with-check", vec![expression.clone()])),
                ],
            ),
        ),
        (
            "alter-publication",
            root_pattern(
                "alter-publication",
                sqlcommenter.clone(),
                vec![
                    string(),
                    repeat(P::OneOf(vec![
                        named("add", vec![publication_objects.clone()]),
                        named("drop", vec![publication_objects.clone()]),
                        named("set-tables", vec![publication_objects.clone()]),
                        publication_option.clone(),
                        rename.clone(),
                        owner.clone(),
                    ])),
                ],
            ),
        ),
        (
            "alter-role",
            root_pattern(
                "alter-role",
                sqlcommenter.clone(),
                vec![
                    role.clone(),
                    optional(named("in-database", vec![string()])),
                    repeat(P::OneOf(vec![role_option.clone(), role_setting.clone()])),
                ],
            ),
        ),
        (
            "alter-server",
            root_pattern(
                "alter-server",
                sqlcommenter.clone(),
                vec![
                    string(),
                    optional(named("version", vec![string()])),
                    optional(named("options", vec![one_or_more(generic_option.clone())])),
                ],
            ),
        ),
        (
            "alter-statistics",
            root_pattern(
                "alter-statistics",
                sqlcommenter.clone(),
                vec![
                    qualified_name.clone(),
                    optional(named("if-exists", vec![])),
                    P::OneOf(vec![
                        named("set-statistics", vec![numeric_symbol.clone()]),
                        set_schema.clone(),
                        owner.clone(),
                    ]),
                ],
            ),
        ),
        (
            "alter-subscription",
            root_pattern(
                "alter-subscription",
                sqlcommenter.clone(),
                vec![
                    string(),
                    repeat(P::OneOf(vec![
                        named("connection", vec![string()]),
                        named("publications", vec![one_or_more(string())]),
                        named("add-publications", vec![one_or_more(string())]),
                        named("drop-publications", vec![one_or_more(string())]),
                        named("set", vec![repeat(subscription_option.clone())]),
                        named("skip", vec![repeat(subscription_option.clone())]),
                        named("refresh", vec![]),
                        P::OneOf(vec![
                            exact("enable"),
                            exact("disable"),
                            named("enable", vec![]),
                            named("disable", vec![]),
                        ]),
                        subscription_option.clone(),
                        rename.clone(),
                        owner.clone(),
                    ])),
                ],
            ),
        ),
        (
            "alter-tablespace",
            root_pattern(
                "alter-tablespace",
                sqlcommenter.clone(),
                vec![
                    string(),
                    P::OneOf(vec![
                        named("set", vec![one_or_more(storage_option.clone())]),
                        named("reset", vec![one_or_more(storage_option.clone())]),
                        rename.clone(),
                        owner.clone(),
                        set_schema.clone(),
                    ]),
                ],
            ),
        ),
        (
            "alter-text-search-configuration",
            root_pattern(
                "alter-text-search-configuration",
                sqlcommenter.clone(),
                vec![
                    qualified_name.clone(),
                    P::OneOf(vec![
                        named(
                            "add-mapping",
                            vec![
                                named("tokens", vec![one_or_more(string())]),
                                named("dictionaries", vec![repeat(signature_name.clone())]),
                            ],
                        ),
                        named(
                            "alter-mapping",
                            vec![
                                named("tokens", vec![one_or_more(string())]),
                                named("dictionaries", vec![repeat(signature_name.clone())]),
                            ],
                        ),
                        named(
                            "replace-dictionary",
                            vec![
                                optional(named("tokens", vec![one_or_more(string())])),
                                named("from", vec![signature_name.clone()]),
                                named("to", vec![signature_name.clone()]),
                            ],
                        ),
                        named(
                            "drop-mapping",
                            vec![
                                named("tokens", vec![one_or_more(string())]),
                                optional(named("if-exists", vec![])),
                            ],
                        ),
                    ]),
                ],
            ),
        ),
        (
            "alter-text-search-dictionary",
            root_pattern(
                "alter-text-search-dictionary",
                sqlcommenter.clone(),
                vec![
                    qualified_name,
                    named("options", vec![repeat(storage_option)]),
                ],
            ),
        ),
        (
            "alter-user-mapping",
            root_pattern(
                "alter-user-mapping",
                sqlcommenter,
                vec![
                    named("for", vec![role]),
                    named("server", vec![string()]),
                    optional(named("options", vec![one_or_more(generic_option)])),
                ],
            ),
        ),
    ];

    let mut rules = patterns
        .into_iter()
        .map(|(tag, pattern)| (format!("pg-statement-{tag}"), pattern))
        .collect::<Vec<_>>();
    rules.push(("name".to_owned(), signature_name));
    rules
}

fn root_pattern(tag: &str, sqlcommenter: Pattern, fields: Vec<Pattern>) -> Pattern {
    super::tagged(tag, std::iter::once(sqlcommenter).chain(fields))
}

fn default_privilege_root(
    sqlcommenter: Pattern,
    action: &str,
    grantee_tag: &str,
    roles: Pattern,
) -> Pattern {
    use Pattern as P;

    let string = P::Atom(AtomType::String);
    let symbol = P::Atom(AtomType::Symbol);
    let qualifiers = P::Repeat(Box::new(P::OneOf(vec![
        super::tagged("in-schema", [P::OneOrMore(Box::new(string.clone()))]),
        super::tagged("for-role", [roles.clone()]),
    ])));
    let privileges = super::tagged(
        "privileges",
        [P::OneOrMore(Box::new(P::OneOf(vec![
            P::Exact(Sexp::symbol("all")),
            symbol,
        ])))],
    );
    super::tagged(
        "alter-default-privileges",
        [
            sqlcommenter,
            qualifiers,
            P::Exact(Sexp::symbol(action)),
            privileges,
            P::OneOf(
                [
                    "on-tables",
                    "on-sequences",
                    "on-functions",
                    "on-schemas",
                    "on-types",
                    "on-routines",
                    "on-large-objects",
                ]
                .into_iter()
                .map(|name| P::Exact(Sexp::symbol(name)))
                .collect(),
            ),
            super::tagged(grantee_tag, [roles]),
            P::Optional(Box::new(P::Exact(Sexp::symbol(if action == "grant" {
                "with-grant-option"
            } else {
                "grant-option-for"
            })))),
        ],
    )
}
