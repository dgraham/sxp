use sxp::Sexp;
use sxp::pattern::{AtomType, Pattern};

pub(super) const STRICT_STATEMENT_TAGS: &[&str] = &[
    "grant",
    "grant-role",
    "import-foreign-schema",
    "move-all-in-tablespace",
    "reassign-owned",
    "rename-policy",
    "rename-role",
    "rename-rule",
    "rename-trigger",
    "revoke",
    "revoke-role",
    "security-label",
];

pub(super) fn rules() -> Vec<(String, Pattern)> {
    use Pattern as P;

    let atom = P::Atom;
    let exact = |name: &str| P::Exact(Sexp::symbol(name));
    let optional = |pattern| P::Optional(Box::new(pattern));
    let repeated = |pattern| P::Repeat(Box::new(pattern));
    let one_or_more = |pattern| P::OneOrMore(Box::new(pattern));
    let reference = |name: &str| P::Reference(name.to_owned());
    let string = || atom(AtomType::String);

    let integer_or_symbol = || P::OneOf(vec![atom(AtomType::Integer), atom(AtomType::Symbol)]);
    let tagged = |name: &str, fields: Vec<P>| super::tagged(name, fields);
    let named_values = |name: &str, pattern: P| tagged(name, vec![one_or_more(pattern)]);
    let sqlcommenter = optional(reference("sqlcommenter"));
    let root = |name: &str, fields: Vec<P>| {
        tagged(
            name,
            std::iter::once(sqlcommenter.clone())
                .chain(fields)
                .collect(),
        )
    };

    let role_value = P::OneOf(vec![
        string(),
        P::OneOf(
            ["public", "current-role", "current-user", "session-user"]
                .map(exact)
                .into(),
        ),
    ]);
    let role = tagged("role", vec![role_value]);
    let role_list = |name: &str| named_values(name, role.clone());
    let roles = one_or_more(role.clone());
    let granted_by = tagged("granted-by", vec![role.clone()]);
    let name = reference("ddl-name-with-signature");
    let table = reference("table");
    let privilege_columns = tagged("columns", vec![one_or_more(string())]);
    let privilege = P::OneOf(vec![
        P::OneOf(
            [
                "all",
                "alter-system",
                "delete",
                "execute",
                "insert",
                "references",
                "select",
                "trigger",
                "truncate",
                "update",
                "usage",
            ]
            .map(exact)
            .into(),
        ),
        tagged("all", vec![privilege_columns.clone()]),
        P::OneOf(
            ["insert", "references", "select", "update"]
                .map(|name| tagged(name, vec![privilege_columns.clone()]))
                .into(),
        ),
    ]);
    let privileges = named_values("privileges", privilege);
    let grant_object = P::OneOf(vec![
        table.clone(),
        name.clone(),
        tagged("large-object", vec![integer_or_symbol()]),
        P::OneOf(
            [
                "all-tables-in-schema",
                "all-sequences-in-schema",
                "all-functions-in-schema",
            ]
            .map(|tag| tagged(tag, vec![string()]))
            .into(),
        ),
    ]);
    let on = named_values("on", grant_object);
    let grant_fields = vec![
        privileges.clone(),
        on.clone(),
        role_list("to"),
        optional(exact("with-grant-option")),
        optional(granted_by.clone()),
    ];
    let revoke_fields = vec![
        privileges,
        on,
        role_list("from"),
        optional(exact("grant-option-for")),
        optional(granted_by.clone()),
        optional(exact("cascade")),
    ];

    let role_option = P::OneOf(
        [
            "with-admin-option",
            "without-admin-option",
            "admin-option-for",
            "with-inherit-option",
            "without-inherit-option",
            "inherit-option-for",
            "with-set-option",
            "without-set-option",
            "set-option-for",
            "with-grant-option",
            "without-grant-option",
            "grant-option-for",
        ]
        .map(exact)
        .into(),
    );
    let options = repeated(role_option);
    let import_option = tagged("option", vec![string(), optional(reference("value"))]);
    let import_options = optional(named_values("options", import_option));
    let import_filter = optional(P::OneOf(vec![
        named_values("limit-to", table.clone()),
        named_values("except", table.clone()),
    ]));

    let patterns = vec![
        ("grant", root("grant", grant_fields)),
        ("revoke", root("revoke", revoke_fields)),
        (
            "grant-role",
            root(
                "grant-role",
                vec![
                    roles.clone(),
                    role_list("to"),
                    options.clone(),
                    optional(granted_by.clone()),
                    optional(exact("cascade")),
                ],
            ),
        ),
        (
            "revoke-role",
            root(
                "revoke-role",
                vec![
                    roles.clone(),
                    role_list("from"),
                    options,
                    optional(granted_by),
                    optional(exact("cascade")),
                ],
            ),
        ),
        (
            "import-foreign-schema",
            root(
                "import-foreign-schema",
                vec![
                    tagged("remote-schema", vec![string()]),
                    import_filter,
                    tagged("server", vec![string()]),
                    tagged("local-schema", vec![string()]),
                    import_options,
                ],
            ),
        ),
        (
            "move-all-in-tablespace",
            root(
                "move-all-in-tablespace",
                vec![
                    tagged(
                        "type",
                        vec![P::OneOf(
                            ["table", "index", "materialized-view"].map(exact).into(),
                        )],
                    ),
                    tagged("from-tablespace", vec![string()]),
                    optional(named_values("owner", role.clone())),
                    tagged("to-tablespace", vec![string()]),
                    optional(exact("nowait")),
                ],
            ),
        ),
        (
            "reassign-owned",
            root(
                "reassign-owned",
                vec![roles.clone(), tagged("to", vec![role.clone()])],
            ),
        ),
        (
            "rename-policy",
            root(
                "rename-policy",
                vec![
                    string(),
                    tagged("on", vec![table.clone()]),
                    optional(tagged("if-exists", vec![])),
                    string(),
                ],
            ),
        ),
        ("rename-role", root("rename-role", vec![string(), string()])),
        (
            "rename-rule",
            root(
                "rename-rule",
                vec![
                    string(),
                    tagged("on", vec![table.clone()]),
                    tagged("rename", vec![string()]),
                ],
            ),
        ),
        (
            "rename-trigger",
            root(
                "rename-trigger",
                vec![
                    string(),
                    tagged("on", vec![table]),
                    tagged("rename", vec![string()]),
                ],
            ),
        ),
        (
            "security-label",
            root(
                "security-label",
                vec![
                    optional(tagged("provider", vec![string()])),
                    P::OneOf(vec![
                        name,
                        tagged("large-object", vec![integer_or_symbol()]),
                    ]),
                    P::OneOf(vec![string(), exact("drop")]),
                ],
            ),
        ),
    ];

    patterns
        .into_iter()
        .map(|(tag, pattern)| (format!("pg-statement-{tag}"), pattern))
        .collect()
}
