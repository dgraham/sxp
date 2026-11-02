use sxp::Sexp;
use sxp::pattern::{AtomType, Pattern};

pub(super) const STRICT_STATEMENT_TAGS: &[&str] = &[
    "drop-access-method",
    "drop-aggregate",
    "drop-cast",
    "drop-collation",
    "drop-constraint",
    "drop-database",
    "drop-default",
    "drop-event-trigger",
    "drop-expression",
    "drop-extension",
    "drop-foreign-data-wrapper",
    "drop-identity",
    "drop-language",
    "drop-mapping",
    "drop-not-null",
    "drop-operator",
    "drop-operator-class",
    "drop-option",
    "drop-owned",
    "drop-policy",
    "drop-publications",
    "drop-publication",
    "drop-role",
    "drop-rule",
    "drop-server",
    "drop-statistics",
    "drop-subscription",
    "drop-tablespace",
    "drop-text-search-configuration",
    "drop-transform",
    "drop-trigger",
    "drop-user-mapping",
];

pub(super) fn rules() -> Vec<(String, Pattern)> {
    use Pattern as P;

    let atom = P::Atom;
    let exact = |name: &str| P::Exact(Sexp::symbol(name));
    let optional = |pattern| P::Optional(Box::new(pattern));
    let repeat = |pattern| P::Repeat(Box::new(pattern));
    let one_or_more = |pattern| P::OneOrMore(Box::new(pattern));
    let reference = |name: &str| P::Reference(name.to_owned());
    let string = || atom(AtomType::String);

    let optional_sqlcommenter = optional(reference("sqlcommenter"));
    let root = |name: &str, fields: Vec<P>| {
        super::tagged(
            name,
            std::iter::once(optional_sqlcommenter.clone()).chain(fields),
        )
    };
    let named = |name: &str, fields: Vec<P>| super::tagged(name, fields);
    let if_exists = || named("if-exists", vec![]);
    let qualified_name = || named("name", vec![string(), repeat(string())]);
    let aggregate_signature = || {
        named(
            "name",
            vec![
                string(),
                repeat(string()),
                named("parameters", vec![repeat(reference("value"))]),
            ],
        )
    };
    let operator_signature = || {
        named(
            "name",
            vec![
                string(),
                repeat(string()),
                named("parameters", vec![reference("value"), reference("value")]),
            ],
        )
    };
    let role = || {
        named(
            "role",
            vec![P::OneOf(vec![
                string(),
                exact("current-role"),
                exact("current-user"),
                exact("public"),
                exact("session-user"),
            ])],
        )
    };
    let table_path = || P::List(vec![string(), repeat(string())]);
    let on_table = || named("on", vec![named("table", vec![table_path()])]);
    let drop_behavior = || optional(exact("cascade"));
    let list_if_exists = || optional(if_exists());

    let generic_name_drop = |tag: &str, behavior: bool| {
        let mut fields = vec![list_if_exists(), one_or_more(qualified_name())];
        if behavior {
            fields.push(drop_behavior());
        }
        root(tag, fields)
    };

    let patterns = vec![
        (
            "drop-access-method",
            generic_name_drop("drop-access-method", true),
        ),
        (
            "drop-aggregate",
            root(
                "drop-aggregate",
                vec![
                    list_if_exists(),
                    one_or_more(aggregate_signature()),
                    drop_behavior(),
                ],
            ),
        ),
        (
            "drop-cast",
            root(
                "drop-cast",
                vec![
                    list_if_exists(),
                    named("source", vec![qualified_name()]),
                    named("target", vec![qualified_name()]),
                    drop_behavior(),
                ],
            ),
        ),
        ("drop-collation", generic_name_drop("drop-collation", true)),
        (
            "drop-constraint",
            root(
                "drop-constraint",
                vec![string(), optional(exact("if-exists")), drop_behavior()],
            ),
        ),
        (
            "drop-database",
            root(
                "drop-database",
                vec![
                    optional(exact("if-exists")),
                    string(),
                    optional(exact("force")),
                ],
            ),
        ),
        (
            "drop-default",
            P::OneOf(vec![
                exact("drop-default"),
                root("drop-default", vec![string()]),
            ]),
        ),
        (
            "drop-event-trigger",
            generic_name_drop("drop-event-trigger", true),
        ),
        (
            "drop-expression",
            root(
                "drop-expression",
                vec![string(), optional(exact("if-exists")), drop_behavior()],
            ),
        ),
        ("drop-extension", generic_name_drop("drop-extension", true)),
        (
            "drop-foreign-data-wrapper",
            generic_name_drop("drop-foreign-data-wrapper", true),
        ),
        (
            "drop-identity",
            root(
                "drop-identity",
                vec![string(), optional(exact("if-exists")), drop_behavior()],
            ),
        ),
        ("drop-language", generic_name_drop("drop-language", true)),
        (
            "drop-mapping",
            root(
                "drop-mapping",
                vec![
                    named("tokens", vec![one_or_more(string())]),
                    optional(if_exists()),
                ],
            ),
        ),
        (
            "drop-not-null",
            P::OneOf(vec![
                exact("drop-not-null"),
                root("drop-not-null", vec![string()]),
            ]),
        ),
        (
            "drop-operator",
            root(
                "drop-operator",
                vec![
                    list_if_exists(),
                    one_or_more(operator_signature()),
                    drop_behavior(),
                ],
            ),
        ),
        (
            "drop-operator-class",
            root(
                "drop-operator-class",
                vec![
                    list_if_exists(),
                    qualified_name(),
                    named("using", vec![string()]),
                    drop_behavior(),
                ],
            ),
        ),
        ("drop-option", root("drop-option", vec![string()])),
        (
            "drop-owned",
            root(
                "drop-owned",
                vec![one_or_more(role()), optional(exact("cascade"))],
            ),
        ),
        (
            "drop-policy",
            root(
                "drop-policy",
                vec![list_if_exists(), string(), on_table(), drop_behavior()],
            ),
        ),
        (
            "drop-publications",
            root("drop-publications", vec![one_or_more(string())]),
        ),
        (
            "drop-publication",
            root(
                "drop-publication",
                vec![list_if_exists(), one_or_more(string()), drop_behavior()],
            ),
        ),
        (
            "drop-role",
            root(
                "drop-role",
                vec![
                    optional(exact("if-exists")),
                    one_or_more(P::OneOf(vec![string(), role()])),
                ],
            ),
        ),
        (
            "drop-rule",
            root(
                "drop-rule",
                vec![list_if_exists(), string(), on_table(), drop_behavior()],
            ),
        ),
        ("drop-server", generic_name_drop("drop-server", true)),
        (
            "drop-statistics",
            generic_name_drop("drop-statistics", true),
        ),
        (
            "drop-subscription",
            root(
                "drop-subscription",
                vec![list_if_exists(), string(), drop_behavior()],
            ),
        ),
        (
            "drop-tablespace",
            root(
                "drop-tablespace",
                vec![optional(exact("if-exists")), string()],
            ),
        ),
        (
            "drop-text-search-configuration",
            generic_name_drop("drop-text-search-configuration", true),
        ),
        (
            "drop-transform",
            root(
                "drop-transform",
                vec![
                    list_if_exists(),
                    named("type", vec![qualified_name()]),
                    named("language", vec![string()]),
                    drop_behavior(),
                ],
            ),
        ),
        (
            "drop-trigger",
            root(
                "drop-trigger",
                vec![list_if_exists(), string(), on_table(), drop_behavior()],
            ),
        ),
        (
            "drop-user-mapping",
            root(
                "drop-user-mapping",
                vec![
                    list_if_exists(),
                    named("for", vec![role()]),
                    named("server", vec![string()]),
                ],
            ),
        ),
    ];

    patterns
        .into_iter()
        .map(|(tag, pattern)| (format!("pg-statement-{tag}"), pattern))
        .collect()
}
