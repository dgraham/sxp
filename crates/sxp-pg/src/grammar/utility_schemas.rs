use sxp::Sexp;
use sxp::pattern::{AtomType, Pattern};

pub(super) const STRICT_STATEMENT_TAGS: &[&str] = &[
    "alter-system-reset",
    "alter-system-set",
    "analyze",
    "begin",
    "call",
    "checkpoint",
    "close",
    "cluster",
    "commit",
    "commit-prepared",
    "copy",
    "deallocate",
    "declare",
    "discard",
    "do",
    "execute",
    "explain",
    "fetch",
    "listen",
    "load",
    "lock",
    "move",
    "notify",
    "prepare",
    "prepare-transaction",
    "reindex-database",
    "reindex-index",
    "reindex-schema",
    "reindex-system",
    "reindex-table",
    "refresh-materialized-view",
    "release-savepoint",
    "reset",
    "return",
    "rollback",
    "rollback-prepared",
    "rollback-to-savepoint",
    "savepoint",
    "set",
    "set-constraints",
    "set-session-characteristics",
    "set-transaction",
    "set-transaction-snapshot",
    "show",
    "start-transaction",
    "unlisten",
    "vacuum",
];

pub(super) fn rules() -> Vec<(String, Pattern)> {
    use Pattern as P;

    let atom = P::Atom;
    let exact = |name: &str| P::Exact(Sexp::symbol(name));
    let optional = |pattern| P::Optional(Box::new(pattern));
    let repeat = |pattern| P::Repeat(Box::new(pattern));
    let reference = |name: &str| P::Reference(name.to_owned());
    let string = || atom(AtomType::String);
    let symbol = || atom(AtomType::Symbol);
    let value = reference("value");
    let table = reference("table");
    let query = reference("query");

    let optional_sqlcommenter = optional(reference("sqlcommenter"));
    let root = |name: &str, fields: Vec<P>| {
        super::tagged(
            name,
            std::iter::once(optional_sqlcommenter.clone()).chain(fields),
        )
    };
    let one_or_more = |pattern: P| P::OneOrMore(Box::new(pattern));
    let named = |name: &str, fields: Vec<P>| super::tagged(name, fields);
    let enum_symbols =
        |names: &[&str]| P::OneOf(names.iter().map(|name| exact(name)).collect::<Vec<_>>());
    let mode = P::OneOf(vec![
        named(
            "isolation",
            vec![enum_symbols(&[
                "read-uncommitted",
                "read-committed",
                "repeatable-read",
                "serializable",
            ])],
        ),
        exact("read-only"),
        exact("read-write"),
        exact("deferrable"),
        exact("not-deferrable"),
    ]);
    let maintenance_option = P::OneOf(vec![
        symbol(),
        named("option", vec![string(), value.clone()]),
    ]);
    let reindex_option = P::OneOf(vec![
        exact("concurrently"),
        exact("verbose"),
        named("tablespace", vec![string()]),
        named("option", vec![string(), value.clone()]),
    ]);
    let query_clause = named("query", vec![query.clone()]);
    let option_value = P::Optional(Box::new(value.clone()));
    let generic_option = named("option", vec![string(), option_value]);
    let copy_options = named("options", vec![one_or_more(generic_option)]);
    let copy_source = P::OneOf(vec![table.clone(), query_clause.clone()]);
    let copy_destination = P::OneOf(vec![
        named("program", vec![string()]),
        named("file", vec![string()]),
        exact("stdin"),
        exact("stdout"),
    ]);
    let explain_option = P::OneOf(vec![
        enum_symbols(&[
            "analyze",
            "verbose",
            "settings",
            "buffers",
            "wal",
            "generic-plan",
            "memory",
            "summary",
        ]),
        P::OneOf(vec![
            named("costs", vec![exact("false")]),
            named("timing", vec![exact("false")]),
            named("summary", vec![exact("false")]),
        ]),
        named("format", vec![enum_symbols(&["xml", "json", "yaml"])]),
        named("serialize", vec![enum_symbols(&["none", "text", "binary"])]),
    ]);
    let transaction_modes = repeat(mode);
    let local = optional(exact("local"));
    let table_list = |tables: P| named("tables", vec![one_or_more(tables)]);
    let vacuum_relation = P::List(vec![
        table.clone(),
        optional(named("columns", vec![one_or_more(string())])),
    ]);
    let vacuum_tables = table_list(vacuum_relation);
    let constraints = P::OneOf(vec![
        exact("all"),
        named("constraints", vec![one_or_more(table.clone())]),
    ]);
    let fetch_forms = P::OneOf(vec![
        root("fetch", vec![symbol(), string()]),
        root("fetch", vec![exact("backward"), symbol(), string()]),
        root(
            "fetch",
            vec![enum_symbols(&["absolute", "relative"]), symbol(), string()],
        ),
    ]);
    let move_forms = P::OneOf(vec![
        root("move", vec![symbol(), string()]),
        root("move", vec![exact("backward"), symbol(), string()]),
        root(
            "move",
            vec![enum_symbols(&["absolute", "relative"]), symbol(), string()],
        ),
    ]);
    let cursor_flag = ["binary", "scroll", "no-scroll", "insensitive", "with-hold"]
        .into_iter()
        .map(|name| optional(exact(name)))
        .collect::<Vec<_>>();
    let call_fields = vec![
        string(),
        repeat(value.clone()),
        optional(named("outargs", vec![one_or_more(value.clone())])),
    ];

    let patterns = vec![
        ("begin", root("begin", vec![transaction_modes.clone()])),
        (
            "start-transaction",
            root("start-transaction", vec![transaction_modes.clone()]),
        ),
        ("commit", root("commit", vec![optional(exact("chain"))])),
        ("rollback", root("rollback", vec![optional(exact("chain"))])),
        (
            "rollback-to-savepoint",
            root("rollback-to-savepoint", vec![string()]),
        ),
        ("savepoint", root("savepoint", vec![string()])),
        (
            "release-savepoint",
            root("release-savepoint", vec![string()]),
        ),
        (
            "prepare-transaction",
            root("prepare-transaction", vec![string()]),
        ),
        ("commit-prepared", root("commit-prepared", vec![string()])),
        (
            "rollback-prepared",
            root("rollback-prepared", vec![string()]),
        ),
        (
            "set",
            root("set", vec![local.clone(), string(), repeat(value.clone())]),
        ),
        (
            "reset",
            root("reset", vec![P::OneOf(vec![string(), exact("all")])]),
        ),
        (
            "set-transaction",
            root(
                "set-transaction",
                vec![local.clone(), transaction_modes.clone()],
            ),
        ),
        (
            "set-session-characteristics",
            root(
                "set-session-characteristics",
                vec![local.clone(), transaction_modes.clone()],
            ),
        ),
        (
            "set-transaction-snapshot",
            root("set-transaction-snapshot", vec![local.clone(), string()]),
        ),
        (
            "set-constraints",
            root(
                "set-constraints",
                vec![constraints, enum_symbols(&["deferred", "immediate"])],
            ),
        ),
        ("show", root("show", vec![string()])),
        (
            "copy",
            root(
                "copy",
                vec![
                    copy_source,
                    optional(named("columns", vec![one_or_more(value.clone())])),
                    enum_symbols(&["from", "to"]),
                    copy_destination,
                    optional(named("where", vec![value.clone()])),
                    optional(copy_options),
                ],
            ),
        ),
        (
            "explain",
            root(
                "explain",
                vec![repeat(explain_option), query_clause.clone()],
            ),
        ),
        (
            "vacuum",
            root(
                "vacuum",
                vec![
                    repeat(maintenance_option.clone()),
                    optional(vacuum_tables.clone()),
                ],
            ),
        ),
        (
            "analyze",
            root(
                "analyze",
                vec![repeat(maintenance_option.clone()), optional(vacuum_tables)],
            ),
        ),
        (
            "cluster",
            root(
                "cluster",
                vec![
                    optional(table.clone()),
                    optional(named("using", vec![string()])),
                    repeat(maintenance_option),
                ],
            ),
        ),
        (
            "reindex-index",
            root(
                "reindex-index",
                vec![table.clone(), repeat(reindex_option.clone())],
            ),
        ),
        (
            "reindex-table",
            root(
                "reindex-table",
                vec![table.clone(), repeat(reindex_option.clone())],
            ),
        ),
        (
            "reindex-schema",
            root(
                "reindex-schema",
                vec![string(), repeat(reindex_option.clone())],
            ),
        ),
        (
            "reindex-system",
            root(
                "reindex-system",
                vec![optional(string()), repeat(reindex_option.clone())],
            ),
        ),
        (
            "reindex-database",
            root(
                "reindex-database",
                vec![optional(string()), repeat(reindex_option)],
            ),
        ),
        (
            "refresh-materialized-view",
            root(
                "refresh-materialized-view",
                vec![
                    optional(exact("concurrently")),
                    table.clone(),
                    optional(exact("with-no-data")),
                ],
            ),
        ),
        ("checkpoint", root("checkpoint", vec![])),
        ("load", root("load", vec![string()])),
        (
            "alter-system-set",
            root("alter-system-set", vec![string(), repeat(value.clone())]),
        ),
        (
            "alter-system-reset",
            root(
                "alter-system-reset",
                vec![P::OneOf(vec![string(), exact("all")])],
            ),
        ),
        (
            "discard",
            root(
                "discard",
                vec![enum_symbols(&["all", "plans", "sequences", "temp"])],
            ),
        ),
        (
            "prepare",
            root(
                "prepare",
                vec![
                    string(),
                    optional(named("parameters", vec![one_or_more(value.clone())])),
                    query_clause.clone(),
                ],
            ),
        ),
        (
            "execute",
            root(
                "execute",
                vec![
                    string(),
                    optional(named("parameters", vec![one_or_more(value.clone())])),
                ],
            ),
        ),
        (
            "deallocate",
            root("deallocate", vec![P::OneOf(vec![string(), exact("all")])]),
        ),
        (
            "declare",
            root(
                "declare",
                std::iter::once(string())
                    .chain(cursor_flag)
                    .chain(std::iter::once(query_clause.clone()))
                    .collect(),
            ),
        ),
        ("close", root("close", vec![string()])),
        ("fetch", fetch_forms),
        ("move", move_forms),
        (
            "lock",
            root(
                "lock",
                vec![
                    table.clone(),
                    repeat(table),
                    enum_symbols(&[
                        "access-share",
                        "row-share",
                        "row-exclusive",
                        "share-update-exclusive",
                        "share",
                        "share-row-exclusive",
                        "exclusive",
                        "access-exclusive",
                    ]),
                    optional(exact("nowait")),
                ],
            ),
        ),
        ("notify", root("notify", vec![string(), optional(string())])),
        ("listen", root("listen", vec![string()])),
        (
            "unlisten",
            root("unlisten", vec![P::OneOf(vec![string(), exact("*")])]),
        ),
        ("return", root("return", vec![value.clone()])),
        (
            "do",
            root(
                "do",
                vec![
                    optional(named("language", vec![string()])),
                    optional(named("sql", vec![string()])),
                ],
            ),
        ),
        ("call", root("call", call_fields)),
    ];

    patterns
        .into_iter()
        .map(|(tag, pattern)| (format!("pg-statement-{tag}"), pattern))
        .collect()
}
