use sxp::Sexp;
use sxp::pattern::{AtomType, Pattern};

pub(super) const STRICT_STATEMENT_TAGS: &[&str] = &[
    "comment-on-access-method",
    "comment-on-aggregate",
    "comment-on-cast",
    "comment-on-collation",
    "comment-on-column",
    "comment-on-constraint",
    "comment-on-conversion",
    "comment-on-domain",
    "comment-on-domain-constraint",
    "comment-on-event-trigger",
    "comment-on-extension",
    "comment-on-foreign-data-wrapper",
    "comment-on-foreign-table",
    "comment-on-language",
    "comment-on-large-object",
    "comment-on-operator",
    "comment-on-operator-class",
    "comment-on-policy",
    "comment-on-procedure",
    "comment-on-publication",
    "comment-on-role",
    "comment-on-rule",
    "comment-on-server",
    "comment-on-statistics",
    "comment-on-subscription",
    "comment-on-table",
    "comment-on-tablespace",
    "comment-on-text-search-configuration",
    "comment-on-text-search-dictionary",
    "comment-on-text-search-parser",
    "comment-on-text-search-template",
    "comment-on-transform",
    "comment-on-trigger",
    "comment-on-type",
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
    let tagged = |name: &str, fields: Vec<P>| super::tagged(name, fields);
    let name_of_size = |count: usize| tagged("name", vec![string(); count]);
    let qualified_name =
        |minimum: usize, maximum: usize| P::OneOf((minimum..=maximum).map(name_of_size).collect());

    let general_name = qualified_name(1, 2);
    let object_name = qualified_name(1, 2);
    let qualified_target = qualified_name(2, 3);
    let unqualified_name = name_of_size(1);

    let modifiers = tagged(
        "modifiers",
        vec![P::OneOrMore(Box::new(reference("value")))],
    );
    let array_bound = P::OneOf(vec![
        tagged("array", vec![]),
        tagged("array", vec![symbol()]),
    ]);
    let signature_type_core = |name_count: usize| {
        let mut fields = vec![string(); name_count];
        fields.extend([
            optional(exact("%-type")),
            optional(modifiers.clone()),
            repeat(array_bound.clone()),
        ]);
        tagged("type", fields)
    };
    let signature_type = P::OneOf(vec![
        signature_type_core(1),
        signature_type_core(2),
        signature_type_core(3),
        tagged(
            "setof",
            vec![P::OneOf(vec![
                signature_type_core(1),
                signature_type_core(2),
                signature_type_core(3),
            ])],
        ),
    ]);
    let parameters = |item: P| tagged("parameters", vec![repeat(item)]);
    let routine_name = || {
        let mut forms = Vec::new();
        for count in 1..=2 {
            let mut fields = vec![string(); count];
            forms.push(tagged("name", fields.clone()));
            fields.push(parameters(signature_type.clone()));
            forms.push(tagged("name", fields));
            let mut wildcard_fields = vec![string(); count];
            wildcard_fields.push(exact("*"));
            forms.push(tagged("name", wildcard_fields));
        }
        P::OneOf(forms)
    };
    let operator_name = || {
        let argument = P::OneOf(vec![signature_type.clone(), exact("any")]);
        let parameters = tagged("parameters", vec![argument.clone(), argument]);
        P::OneOf(vec![
            tagged("name", vec![string(), parameters.clone()]),
            tagged("name", vec![string(), string(), parameters]),
        ])
    };
    let comment_text = P::OneOf(vec![string(), exact("drop")]);
    let sqlcommenter = optional(reference("sqlcommenter"));
    let root = |tag: &str, mut fields: Vec<P>| {
        fields.insert(0, sqlcommenter.clone());
        tagged(tag, fields)
    };

    let qualified_object = |tag: &str| root(tag, vec![general_name.clone(), comment_text.clone()]);
    let unqualified_object =
        |tag: &str| root(tag, vec![unqualified_name.clone(), comment_text.clone()]);
    let target_object = |tag: &str| root(tag, vec![qualified_target.clone(), comment_text.clone()]);
    let routine_object = |tag: &str| root(tag, vec![routine_name(), comment_text.clone()]);

    let patterns = vec![
        (
            "comment-on-access-method",
            unqualified_object("comment-on-access-method"),
        ),
        (
            "comment-on-aggregate",
            routine_object("comment-on-aggregate"),
        ),
        (
            "comment-on-cast",
            root(
                "comment-on-cast",
                vec![
                    tagged("source", vec![object_name.clone()]),
                    tagged("target", vec![object_name.clone()]),
                    comment_text.clone(),
                ],
            ),
        ),
        (
            "comment-on-collation",
            qualified_object("comment-on-collation"),
        ),
        ("comment-on-column", target_object("comment-on-column")),
        (
            "comment-on-constraint",
            target_object("comment-on-constraint"),
        ),
        (
            "comment-on-conversion",
            qualified_object("comment-on-conversion"),
        ),
        ("comment-on-domain", qualified_object("comment-on-domain")),
        (
            "comment-on-domain-constraint",
            target_object("comment-on-domain-constraint"),
        ),
        (
            "comment-on-event-trigger",
            unqualified_object("comment-on-event-trigger"),
        ),
        (
            "comment-on-extension",
            unqualified_object("comment-on-extension"),
        ),
        (
            "comment-on-foreign-data-wrapper",
            unqualified_object("comment-on-foreign-data-wrapper"),
        ),
        (
            "comment-on-foreign-table",
            qualified_object("comment-on-foreign-table"),
        ),
        (
            "comment-on-language",
            unqualified_object("comment-on-language"),
        ),
        (
            "comment-on-large-object",
            root(
                "comment-on-large-object",
                vec![tagged("large-object", vec![symbol()]), comment_text.clone()],
            ),
        ),
        (
            "comment-on-operator",
            root(
                "comment-on-operator",
                vec![operator_name(), comment_text.clone()],
            ),
        ),
        (
            "comment-on-operator-class",
            root(
                "comment-on-operator-class",
                vec![
                    general_name.clone(),
                    tagged("using", vec![string()]),
                    comment_text.clone(),
                ],
            ),
        ),
        ("comment-on-policy", target_object("comment-on-policy")),
        (
            "comment-on-procedure",
            routine_object("comment-on-procedure"),
        ),
        (
            "comment-on-publication",
            unqualified_object("comment-on-publication"),
        ),
        ("comment-on-role", unqualified_object("comment-on-role")),
        ("comment-on-rule", target_object("comment-on-rule")),
        ("comment-on-server", unqualified_object("comment-on-server")),
        (
            "comment-on-statistics",
            qualified_object("comment-on-statistics"),
        ),
        (
            "comment-on-subscription",
            unqualified_object("comment-on-subscription"),
        ),
        ("comment-on-table", qualified_object("comment-on-table")),
        (
            "comment-on-tablespace",
            unqualified_object("comment-on-tablespace"),
        ),
        (
            "comment-on-text-search-configuration",
            qualified_object("comment-on-text-search-configuration"),
        ),
        (
            "comment-on-text-search-dictionary",
            qualified_object("comment-on-text-search-dictionary"),
        ),
        (
            "comment-on-text-search-parser",
            qualified_object("comment-on-text-search-parser"),
        ),
        (
            "comment-on-text-search-template",
            qualified_object("comment-on-text-search-template"),
        ),
        (
            "comment-on-transform",
            root(
                "comment-on-transform",
                vec![
                    tagged("type", vec![object_name.clone()]),
                    tagged("language", vec![string()]),
                    comment_text.clone(),
                ],
            ),
        ),
        ("comment-on-trigger", target_object("comment-on-trigger")),
        ("comment-on-type", qualified_object("comment-on-type")),
    ];

    patterns
        .into_iter()
        .map(|(tag, pattern)| (format!("pg-statement-{tag}"), pattern))
        .collect()
}
