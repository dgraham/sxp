use sxp::Sexp;
use sxp::pattern::{AtomType, Grammar, Pattern};

mod alter_schemas;
mod comment_schemas;
mod create_schemas;
mod ddl_schemas;
mod drop_schemas;
mod expression_schemas;
mod misc_schemas;
mod utility_schemas;

const PG_OUTPUT_TAGS: &str = r#"
%-type * absent-on-null absolute actions add-column add-constraint add-identity add-mapping add-value
add-publications afffile after alignment all-tables-in-schema and basetype cast coalesce collation
combinefunc cost create-slot current-timestamp deserialfunc deterministic dictfile drop-option elements
enable-trigger finalfunc finalfunc-modify food from-tablespace func grouping gtcmp hashes having
inherit inherits init initcond inline input internallength json-query kinds lc-collate leftarg
lexize limit locale ltcmp members merge-action mfinalfunc minitcond minvfunc msfunc mstate
named-arg negator new-table no-connect no-copy-data not not-between-symmetric offset old-table operator
output parser preceding procedure publish-via-partition-root rename-attribute rename-column rename-constraint rename-value
reset-options restrict rightarg rows rows-from rules serialfunc set-option sfunc sfunc1 slot-name
sspace streaming stype stype1 subtype tags template to-tablespace two-phase typmod-in typmod-out validator
with-check add-option canonical category commutator current-date disable-on-error drop-publications
enable-always-trigger encoding end failover gettoken json-value lc-ctype lextypes multirange-type-name
new-row no-create-slot no-publish-via-partition-root old-row preferred run-as-owner session-user start
subtype-diff subtype-opclass synonyms unbounded-following unbounded-preceding
contained-by enforced new-alias not-enforced old-alias overlaps period stored virtual without-overlaps
add builtin-locale collation-version dictionary drop-identity enable-replica-trigger following icu-locale
icu-rules json-exists locale-provider origin synchronous-commit
disable-trigger drop-expression no-refresh set-tables stopwords
admins agg alias all all-subquery all-tables alter-collation alter-column-options alter-column-type alter-constraint
alter-database alter-default-privileges alter-domain alter-enum alter-extension alter-foreign-data-wrapper alter-mapping alter-operator alter-operator-family alter-policy
alter-publication alter-role alter-sequence alter-server alter-statistics alter-subscription alter-system-reset alter-system-set alter-table alter-tablespace
alter-text-search-configuration alter-text-search-dictionary alter-type alter-user-mapping always any-subquery arg args argument array
array-subquery as as-range asc assignment attach-partition attr attributes authorization backward
begin between binary bit-string bounds by call cascade case chain
check check-option checkpoint close cluster cluster-on collate column columns commit
commit-prepared compression concurrently condition connection connection-limit constraint constraints copy create-aggregate
create-cast create-collation create-conversion create-database create-domain create-enum create-event-trigger create-extension create-foreign-data-wrapper create-foreign-server
create-foreign-table create-index create-language create-materialized-view create-operator create-operator-class create-operator-family create-policy create-publication create-rule
create-schema create-sequence create-statistics create-subscription create-table create-table-as create-tablespace create-transform create-trigger create-type
create-user-mapping create-view cte cube current current-of current-row cycle deallocate declare
default default-values deferrable delete delete-rows depends-on-extension desc detach-partition dictionaries disable
disable-row-level-security discard distinct distinct-on do do-nothing drop drop-cast drop-constraint drop-database
drop-default drop-mapping drop-not-null drop-owned drop-publication drop-role drop-subscription drop-tablespace drop-transform drop-user-mapping
elem element else enable enable-row-level-security entry error-on-error events except exclude
execute exists explain false family field file filter finalize for
for-each for-ordinality for-role force force-row-level-security foreign-data-wrapper foreign-key format formatted frame
from from-current from-sql full function generated granted-by group group-by grouping-set
grouping-sets handler hypothetical identity if-exists if-not-exists import-foreign-schema in in-database in-role
in-schema in-subquery in-tablespace include indent index index-column indirection inference initially-deferred
initially-immediate insensitive insert instead into is-document is-json isolation items join
json-array json-arrayagg json-column json-object json-objectagg json-parse json-scalar json-serialize json-table json-table-path-scan
json-table-sibling-join key language large-object lateral left like limit-to listen load
local local-schema location lock locking maxvalue merge minvalue modifiers modulus
move-all-in-tablespace name namespace namespaces natural nested no no-force-row-level-security no-handler no-inherit op
no-scroll no-validator no-value no-wait none not-deferrable not-null not-of not-valid nothing
notify nowait null nulls-not-distinct object of oid omit-quotes on on-commit
on-conflict on-delete on-empty on-error on-large-objects on-update only opclass operators option options
or-replace order-by outargs over overriding owner parallel parameter parameters partition-by
partition-column partition-of passing password path persistence precision prepare prepare-transaction preserve-rows
preserve-whitespace primary-key privileges program provider publications publish query reassign-owned recursive
references refresh refresh-collation-version refresh-materialized-view refresh-version relative remainder remote-schema remove rename
rename-policy rename-table repeatable replace-dictionary replica replica-identity reset restart restart-identity restrictive
return returning returns right role rollback rollback-prepared rollup row row-compare
savepoint scalar scalar-subquery schema scroll search security-label select serialize server
set set-access-method set-compression set-constraints set-default set-expression set-identity set-logged set-not-null set-options
set-schema set-statistics set-storage set-tablespace set-unlogged set-without-cluster set-without-oids setof similar-to skip
skip-locked slice sort sortop source sql sql-body sqlcommenter standalone start-transaction
storage subquery support table table-sample tables tablespace tag target temporary
ties to to-sql tokens transform transitions true truncate trusted type
types typmod unique unique-keys unlisten unlogged update update-to using using-alias
using-index valid-until validate-constraint value values variadic verbose version when where
window with with-hold with-inout with-no-data with-ordinality with-unique-keys within-group xml-column xml-concat
xml-element xml-forest xml-parse xml-pi xml-root xml-serialize xmltable yes
disable-trigger-all disable-trigger-user disable-rule disable-replica-rule enable-always-rule
enable-replica-rule enable-trigger-all enable-trigger-user enable-rule lsn
"#;

const PG_STATEMENT_TAGS: &str = r#"
alter-aggregate alter-collation alter-database alter-default-privileges alter-domain alter-enum
alter-event-trigger alter-extension alter-foreign-data-wrapper alter-foreign-table alter-function alter-function-depends
alter-index alter-large-object alter-materialized-view alter-operator alter-operator-class alter-policy
alter-procedure alter-procedure-depends alter-publication alter-role alter-routine alter-schema
alter-sequence alter-server alter-statistics alter-subscription alter-system-reset alter-system-set
alter-table alter-tablespace alter-text-search-configuration alter-text-search-dictionary alter-type alter-user-mapping
alter-view analyze begin call checkpoint close cluster comment-on-access-method comment-on-aggregate
comment-on-cast comment-on-collation comment-on-column comment-on-constraint comment-on-conversion comment-on-domain
comment-on-domain-constraint comment-on-event-trigger comment-on-extension comment-on-foreign-data-wrapper comment-on-foreign-table comment-on-language
comment-on-large-object comment-on-operator comment-on-operator-class comment-on-policy comment-on-procedure comment-on-publication
comment-on-role comment-on-rule comment-on-server comment-on-statistics comment-on-subscription comment-on-table
comment-on-tablespace comment-on-text-search-configuration comment-on-text-search-dictionary comment-on-text-search-parser comment-on-text-search-template comment-on-transform
comment-on-trigger comment-on-type commit commit-prepared copy create-access-method create-aggregate
create-cast create-collation create-conversion create-database create-domain create-enum create-event-trigger
create-extension create-foreign-data-wrapper create-foreign-server create-foreign-table create-function create-group create-index
create-language create-materialized-view create-operator create-policy create-procedure create-publication create-role
create-rule create-schema create-sequence create-statistics create-subscription create-table create-table-as
create-tablespace create-text-search-configuration create-text-search-dictionary create-text-search-parser create-text-search-template create-transform
create-trigger create-type create-user create-user-mapping create-view declare delete discard
do drop-access-method drop-aggregate drop-cast drop-collation drop-database drop-domain
drop-event-trigger drop-extension drop-foreign-data-wrapper drop-foreign-table drop-function drop-index drop-language
drop-operator drop-operator-class drop-owned drop-policy drop-procedure drop-publication drop-role
drop-rule drop-server drop-statistics drop-subscription drop-table drop-tablespace drop-text-search-configuration
drop-transform drop-trigger drop-type drop-user-mapping except explain fetch grant grant-role
import-foreign-schema insert into listen load lock merge move move-all-in-tablespace
notify prepare prepare-transaction reassign-owned refresh-materialized-view reindex-database reindex-index
reindex-schema reindex-system reindex-table release-savepoint rename-policy rename-role rename-rule
rename-trigger reset return revoke revoke-role rollback rollback-prepared rollback-to-savepoint savepoint
security-label select set set-constraints set-session-characteristics set-transaction set-transaction-snapshot sqlcommenter
start-transaction statements truncate union unlisten update vacuum with
"#;

/// Returns the output grammar for this parser.
pub fn grammar() -> Grammar {
    use Pattern as P;
    use std::collections::BTreeSet;

    let mut strict_statement_tags = BTreeSet::from([
        "delete",
        "except",
        "insert",
        "intersect",
        "merge",
        "select",
        "truncate",
        "union",
        "update",
        "values",
        "with",
    ]);
    strict_statement_tags.extend(ddl_schemas::STRICT_STATEMENT_TAGS.iter().copied());
    strict_statement_tags.extend(utility_schemas::STRICT_STATEMENT_TAGS.iter().copied());
    strict_statement_tags.extend(alter_schemas::STRICT_STATEMENT_TAGS.iter().copied());
    strict_statement_tags.extend(comment_schemas::STRICT_STATEMENT_TAGS.iter().copied());
    strict_statement_tags.extend(create_schemas::STRICT_STATEMENT_TAGS.iter().copied());
    strict_statement_tags.extend(drop_schemas::STRICT_STATEMENT_TAGS.iter().copied());
    strict_statement_tags.extend(misc_schemas::STRICT_STATEMENT_TAGS.iter().copied());
    let strict_node_tags = expression_schemas::STRICT_NODE_TAGS
        .iter()
        .copied()
        .chain(["as", "assignment", "index", "slice", "sort"])
        .chain(strict_statement_tags.iter().copied())
        .collect::<BTreeSet<_>>();
    let output_tags = PG_OUTPUT_TAGS
        .split_whitespace()
        .chain(PG_STATEMENT_TAGS.split_whitespace())
        .collect::<BTreeSet<_>>();
    let non_statement_tags = ["create-slot", "into", "sqlcommenter", "statements"];
    let tags = output_tags
        .iter()
        .filter(|tag| !strict_node_tags.contains(**tag))
        .map(|tag| P::Exact(Sexp::symbol(*tag)))
        .collect::<Vec<_>>();
    let statement_tags = PG_STATEMENT_TAGS
        .split_whitespace()
        .filter(|tag| !strict_statement_tags.contains(tag) && !non_statement_tags.contains(tag))
        .map(|tag| P::Exact(Sexp::symbol(tag)))
        .chain(
            output_tags
                .iter()
                .filter(|tag| {
                    !strict_statement_tags.contains(**tag)
                        && !non_statement_tags.contains(*tag)
                        && ["create-", "alter-", "drop-"]
                            .iter()
                            .any(|prefix| tag.starts_with(prefix))
                })
                .map(|tag| P::Exact(Sexp::symbol(*tag))),
        )
        .collect::<Vec<_>>();
    let scalar = P::OneOf(
        [
            AtomType::Symbol,
            AtomType::String,
            AtomType::Integer,
            AtomType::Float,
            AtomType::Boolean,
            AtomType::Nil,
        ]
        .map(P::Atom)
        .into(),
    );
    let value = P::Reference("value".to_owned());
    let node_ref = |name: &str| P::Reference(name.to_owned());
    let expression = P::Reference("expression".to_owned());
    let sqlcommenter_clause = || P::Optional(Box::new(node_ref("sqlcommenter")));
    let one_or_more = |name: &str, item: P| tagged(name, [item.clone(), P::Repeat(Box::new(item))]);
    let optional_single = |name: &str, item: P| P::Optional(Box::new(tagged(name, [item])));
    let optional_many = |name: &str, item: P| P::Optional(Box::new(one_or_more(name, item)));

    let raw_list_head = P::OneOf(vec![
        P::Atom(AtomType::String),
        P::Atom(AtomType::Integer),
        P::Atom(AtomType::Float),
        P::Atom(AtomType::Boolean),
        P::Atom(AtomType::Nil),
        P::Reference("node".to_owned()),
        P::Reference("raw-list".to_owned()),
    ]);
    let raw_list = P::OneOf(vec![
        P::List(Vec::new()),
        P::List(vec![raw_list_head, P::Repeat(Box::new(value.clone()))]),
    ]);

    let qualified_name = P::OneOf(vec![
        P::List(vec![P::Atom(AtomType::String)]),
        P::List(vec![P::Atom(AtomType::String), P::Atom(AtomType::String)]),
        P::List(vec![
            P::Atom(AtomType::String),
            P::Atom(AtomType::String),
            P::Atom(AtomType::String),
        ]),
    ]);
    let alias = tagged(
        "alias",
        [
            P::Atom(AtomType::String),
            optional_many("columns", P::Atom(AtomType::String)),
        ],
    );
    let table = tagged(
        "table",
        [
            P::Optional(Box::new(tagged("only", [P::Exact(Sexp::symbol("true"))]))),
            qualified_name.clone(),
            P::Optional(Box::new(alias.clone())),
        ],
    );
    let one_expression_clause = |name: &str| optional_single(name, expression.clone());
    let many_value_clause = |name: &str| optional_many(name, value.clone());
    let columns = one_or_more("columns", expression.clone());
    let from = one_or_more("from", value.clone());
    let where_clause = tagged("where", [expression.clone()]);
    let having = tagged("having", [expression.clone()]);
    let grouping_set = P::OneOf(vec![
        tagged("rollup", [P::OneOrMore(Box::new(expression.clone()))]),
        tagged("cube", [P::OneOrMore(Box::new(expression.clone()))]),
        tagged("grouping-sets", [P::Repeat(Box::new(value.clone()))]),
    ]);
    let group_by_item = P::OneOf(vec![expression.clone(), grouping_set]);
    let group_by = P::OneOf(vec![
        one_or_more("group-by", group_by_item),
        tagged(
            "group-by",
            [
                P::Exact(Sexp::symbol("distinct")),
                expression.clone(),
                P::Repeat(Box::new(expression.clone())),
            ],
        ),
    ]);
    let sort_nodes = node_ref("sort");
    let order_by = one_or_more("order-by", sort_nodes.clone());
    let limit = P::OneOf(vec![
        tagged("limit", [expression.clone()]),
        tagged("limit-with-ties", [expression.clone()]),
    ]);
    let distinct = P::OneOf(vec![
        tagged("distinct", []),
        one_or_more("distinct-on", expression.clone()),
    ]);
    let locking = tagged(
        "locking",
        [
            P::OneOf(
                [
                    "for-key-share",
                    "for-share",
                    "for-no-key-update",
                    "for-update",
                ]
                .map(|name| P::Exact(Sexp::symbol(name)))
                .into(),
            ),
            P::Optional(Box::new(one_or_more("of", value.clone()))),
            P::Optional(Box::new(P::OneOf(vec![
                P::Exact(Sexp::symbol("skip-locked")),
                P::Exact(Sexp::symbol("no-wait")),
            ]))),
        ],
    );
    let window = tagged(
        "window",
        [
            P::Optional(Box::new(tagged("name", [P::Atom(AtomType::String)]))),
            optional_single("window", P::Atom(AtomType::String)),
            optional_many("partition-by", expression.clone()),
            optional_many("order-by", sort_nodes.clone()),
            P::Optional(Box::new(value.clone())),
        ],
    );
    let returning = one_or_more(
        "returning",
        P::OneOf(vec![
            expression.clone(),
            tagged("old-alias", [P::Atom(AtomType::String)]),
            tagged("new-alias", [P::Atom(AtomType::String)]),
        ]),
    );
    let set = one_or_more("set", node_ref("assignment"));
    let index = tagged("index", [expression.clone()]);
    let slice = tagged(
        "slice",
        [
            P::Optional(Box::new(expression.clone())),
            P::Optional(Box::new(expression.clone())),
        ],
    );
    let name_component = P::OneOf(vec![
        P::Atom(AtomType::String),
        index.clone(),
        slice.clone(),
    ]);
    let name_column = tagged(
        "name",
        [
            P::Atom(AtomType::String),
            name_component.clone(),
            P::Repeat(Box::new(name_component)),
        ],
    );
    let insert_column = P::OneOf(vec![P::Atom(AtomType::String), name_column.clone()]);
    let assignment_target = P::OneOf(vec![
        P::Atom(AtomType::String),
        name_column,
        one_or_more("columns", insert_column.clone()),
    ]);
    let assignment = tagged("assignment", [assignment_target, expression.clone()]);
    let index_column = tagged(
        "index-column",
        [expression.clone(), P::Repeat(Box::new(value.clone()))],
    );
    let inference = P::OneOf(vec![
        tagged(
            "inference",
            [tagged("constraint", [P::Atom(AtomType::String)])],
        ),
        tagged(
            "inference",
            [
                one_or_more("columns", index_column),
                one_expression_clause("where"),
            ],
        ),
    ]);
    let on_conflict = P::OneOf(vec![
        tagged(
            "on-conflict",
            [
                P::Exact(Sexp::symbol("do-nothing")),
                P::Optional(Box::new(inference.clone())),
            ],
        ),
        tagged(
            "on-conflict",
            [
                P::Exact(Sexp::symbol("do-update")),
                P::Optional(Box::new(inference)),
                set.clone(),
                one_expression_clause("where"),
            ],
        ),
    ]);
    let into = tagged(
        "into",
        [
            P::Optional(Box::new(tagged(
                "persistence",
                [P::OneOf(vec![
                    P::Exact(Sexp::symbol("temporary")),
                    P::Exact(Sexp::symbol("unlogged")),
                ])],
            ))),
            node_ref("table"),
            optional_many("columns", value.clone()),
            optional_many("options", value.clone()),
            optional_single("using", P::Atom(AtomType::String)),
            P::Optional(Box::new(tagged(
                "on-commit",
                [P::OneOf(vec![
                    P::Exact(Sexp::symbol("preserve-rows")),
                    P::Exact(Sexp::symbol("delete-rows")),
                    P::Exact(Sexp::symbol("drop")),
                ])],
            ))),
            optional_single("tablespace", P::Atom(AtomType::String)),
            P::Optional(Box::new(P::Exact(Sexp::symbol("with-no-data")))),
        ],
    );
    let simple_select = tagged(
        "select",
        [
            sqlcommenter_clause(),
            P::Optional(Box::new(distinct)),
            P::Optional(Box::new(into.clone())),
            columns.clone(),
            P::Optional(Box::new(from.clone())),
            P::Optional(Box::new(where_clause.clone())),
            P::Optional(Box::new(group_by.clone())),
            P::Optional(Box::new(having.clone())),
            P::Repeat(Box::new(node_ref("window"))),
            P::Optional(Box::new(order_by.clone())),
            P::Optional(Box::new(limit.clone())),
            one_expression_clause("offset"),
            P::Repeat(Box::new(node_ref("locking"))),
        ],
    );
    let values_query = tagged(
        "values",
        [
            sqlcommenter_clause(),
            node_ref("row"),
            P::Repeat(Box::new(node_ref("row"))),
            P::Optional(Box::new(order_by.clone())),
            P::Optional(Box::new(limit.clone())),
            one_expression_clause("offset"),
        ],
    );
    let set_operation = P::OneOf(
        ["union", "intersect", "except"]
            .into_iter()
            .map(|name| {
                tagged(
                    name,
                    [
                        sqlcommenter_clause(),
                        node_ref("query"),
                        node_ref("query"),
                        P::Optional(Box::new(P::Exact(Sexp::symbol("all")))),
                        P::Optional(Box::new(order_by.clone())),
                        P::Optional(Box::new(limit.clone())),
                        one_expression_clause("offset"),
                        P::Repeat(Box::new(node_ref("locking"))),
                    ],
                )
            })
            .collect(),
    );
    let search = tagged(
        "search",
        [
            P::OneOf(vec![
                P::Exact(Sexp::symbol("breadth-first")),
                P::Exact(Sexp::symbol("depth-first")),
            ]),
            tagged("by", [P::OneOrMore(Box::new(expression.clone()))]),
            tagged("set", [P::Atom(AtomType::String)]),
        ],
    );
    let cycle = tagged(
        "cycle",
        [
            tagged("by", [P::OneOrMore(Box::new(expression.clone()))]),
            tagged("set", [P::Atom(AtomType::String)]),
            tagged("using", [P::Atom(AtomType::String)]),
            tagged("value", [expression.clone()]),
            tagged("default", [expression.clone()]),
        ],
    );
    let cte = tagged(
        "cte",
        [
            P::Atom(AtomType::String),
            node_ref("query-body"),
            optional_many("columns", P::Atom(AtomType::String)),
            P::Optional(Box::new(P::OneOf(vec![
                P::Exact(Sexp::symbol("materialized")),
                P::Exact(Sexp::symbol("not-materialized")),
            ]))),
            P::Optional(Box::new(search)),
            P::Optional(Box::new(cycle)),
        ],
    );
    let with_query = tagged(
        "with",
        [
            sqlcommenter_clause(),
            P::Optional(Box::new(P::Exact(Sexp::symbol("recursive")))),
            node_ref("cte"),
            P::Repeat(Box::new(node_ref("cte"))),
            node_ref("query-body"),
        ],
    );
    let query = P::OneOf(vec![
        node_ref("select-query"),
        node_ref("values-query"),
        node_ref("set-operation"),
        node_ref("with-query"),
    ]);
    let query_body = P::OneOf(vec![
        node_ref("query"),
        node_ref("insert"),
        node_ref("update"),
        node_ref("delete"),
        node_ref("merge"),
    ]);
    let insert = tagged(
        "insert",
        [
            sqlcommenter_clause(),
            tagged("into", [node_ref("table")]),
            tagged("columns", [P::Repeat(Box::new(insert_column.clone()))]),
            P::Optional(Box::new(tagged(
                "overriding",
                [P::OneOf(vec![
                    P::Exact(Sexp::symbol("system-value")),
                    P::Exact(Sexp::symbol("user-value")),
                ])],
            ))),
            tagged(
                "source",
                [P::OneOf(vec![
                    node_ref("query"),
                    P::Exact(Sexp::symbol("default-values")),
                ])],
            ),
            P::Optional(Box::new(on_conflict)),
            P::Optional(Box::new(returning.clone())),
        ],
    );
    let update = tagged(
        "update",
        [
            sqlcommenter_clause(),
            node_ref("table"),
            node_ref("set"),
            many_value_clause("from"),
            one_expression_clause("where"),
            P::Optional(Box::new(returning.clone())),
        ],
    );
    let delete = tagged(
        "delete",
        [
            sqlcommenter_clause(),
            tagged("from", [node_ref("table")]),
            many_value_clause("using"),
            one_expression_clause("where"),
            P::Optional(Box::new(returning.clone())),
        ],
    );
    let merge_action = P::OneOf(vec![
        tagged("do-nothing", []),
        tagged("delete", []),
        tagged("update", [node_ref("set")]),
        tagged(
            "insert",
            [
                tagged("columns", [P::Repeat(Box::new(insert_column))]),
                tagged("values", [P::Repeat(Box::new(expression.clone()))]),
                P::Optional(Box::new(P::OneOf(vec![
                    P::Exact(Sexp::symbol("overriding-user-value")),
                    P::Exact(Sexp::symbol("overriding-system-value")),
                ]))),
            ],
        ),
    ]);
    let when = tagged(
        "when",
        [
            P::OneOf(vec![
                P::Exact(Sexp::symbol("matched")),
                P::Exact(Sexp::symbol("not-matched-by-source")),
                P::Exact(Sexp::symbol("not-matched-by-target")),
            ]),
            one_expression_clause("condition"),
            merge_action,
        ],
    );
    let merge = tagged(
        "merge",
        [
            sqlcommenter_clause(),
            tagged("into", [node_ref("table")]),
            tagged("using", [value.clone()]),
            tagged("on", [expression.clone()]),
            P::Repeat(Box::new(when)),
            P::Optional(Box::new(one_or_more("returning", expression.clone()))),
        ],
    );
    let truncate = tagged(
        "truncate",
        [
            sqlcommenter_clause(),
            node_ref("table"),
            P::Repeat(Box::new(node_ref("table"))),
            P::Optional(Box::new(P::Exact(Sexp::symbol("restart-identity")))),
            P::Optional(Box::new(P::Exact(Sexp::symbol("cascade")))),
        ],
    );
    let fallback_value = node_ref("fallback-value");
    let generic_statement = if statement_tags.is_empty() {
        None
    } else {
        Some(P::List(vec![
            P::OneOf(statement_tags),
            sqlcommenter_clause(),
            P::Repeat(Box::new(fallback_value.clone())),
        ]))
    };
    let fallback_node = P::List(vec![
        P::OneOf(
            output_tags
                .iter()
                .map(|tag| P::Exact(Sexp::symbol(*tag)))
                .collect(),
        ),
        P::Repeat(Box::new(fallback_value.clone())),
    ]);
    let fallback_raw_list = P::OneOf(vec![
        P::List(Vec::new()),
        P::List(vec![
            P::OneOf(vec![
                P::Atom(AtomType::String),
                P::Atom(AtomType::Integer),
                P::Atom(AtomType::Float),
                P::Atom(AtomType::Boolean),
                P::Atom(AtomType::Nil),
                node_ref("fallback-node"),
                node_ref("fallback-raw-list"),
            ]),
            P::Repeat(Box::new(fallback_value.clone())),
        ]),
    ]);
    let fallback_value_pattern = P::OneOf(vec![
        scalar.clone(),
        node_ref("fallback-node"),
        node_ref("fallback-raw-list"),
    ]);
    let generic_node = P::List(vec![P::OneOf(tags), P::Repeat(Box::new(value.clone()))]);
    let column_definition = tagged(
        "column",
        [
            P::Atom(AtomType::String),
            P::Repeat(Box::new(value.clone())),
        ],
    );
    let ddl_collate = tagged("collate", [P::Atom(AtomType::String)]);
    let xmltable_passing = tagged("passing", [P::OneOrMore(Box::new(expression.clone()))]);
    let mut node_alternatives = vec![
        generic_node,
        column_definition,
        ddl_collate,
        xmltable_passing,
        node_ref("select-query"),
        node_ref("values-query"),
        node_ref("set-operation"),
        node_ref("with-query"),
        node_ref("insert"),
        node_ref("update"),
        node_ref("delete"),
        node_ref("merge"),
        node_ref("truncate"),
        node_ref("set"),
        node_ref("as"),
        node_ref("assignment"),
        node_ref("index"),
        node_ref("slice"),
        node_ref("sort"),
    ];
    node_alternatives.extend(
        expression_schemas::STRICT_NODE_TAGS
            .iter()
            .map(|tag| node_ref(&format!("pg-node-{tag}"))),
    );
    node_alternatives.extend(
        ddl_schemas::STRICT_STATEMENT_TAGS
            .iter()
            .chain(utility_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(alter_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(comment_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(create_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(drop_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(misc_schemas::STRICT_STATEMENT_TAGS.iter())
            .map(|tag| node_ref(&format!("pg-statement-{tag}"))),
    );
    let node = P::OneOf(node_alternatives);
    let mut statement_alternatives = vec![
        node_ref("query"),
        node_ref("insert"),
        node_ref("update"),
        node_ref("delete"),
        node_ref("merge"),
        node_ref("truncate"),
    ];
    statement_alternatives.extend(
        ddl_schemas::STRICT_STATEMENT_TAGS
            .iter()
            .chain(utility_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(alter_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(comment_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(create_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(drop_schemas::STRICT_STATEMENT_TAGS.iter())
            .chain(misc_schemas::STRICT_STATEMENT_TAGS.iter())
            .map(|tag| node_ref(&format!("pg-statement-{tag}"))),
    );
    if generic_statement.is_some() {
        statement_alternatives.push(node_ref("generic-statement"));
    }
    let statement = P::OneOf(statement_alternatives);
    let rows = one_or_more("statements", node_ref("statement"));
    let output = P::OneOf(vec![node_ref("statement"), rows, node_ref("sqlcommenter")]);
    let mut rules = vec![
        ("output".to_owned(), output),
        ("statement".to_owned(), statement),
        ("query".to_owned(), query),
        ("query-body".to_owned(), query_body),
        ("select-query".to_owned(), P::OneOf(vec![simple_select])),
        ("values-query".to_owned(), values_query),
        ("set-operation".to_owned(), set_operation),
        ("with-query".to_owned(), with_query),
        ("insert".to_owned(), insert),
        ("update".to_owned(), update),
        ("delete".to_owned(), delete),
        ("merge".to_owned(), merge),
        ("truncate".to_owned(), truncate),
        (
            "value".to_owned(),
            P::OneOf(vec![scalar.clone(), node_ref("node"), node_ref("raw-list")]),
        ),
        ("fallback-value".to_owned(), fallback_value_pattern),
        ("fallback-node".to_owned(), fallback_node),
        ("fallback-raw-list".to_owned(), fallback_raw_list),
        ("node".to_owned(), node),
        ("raw-list".to_owned(), raw_list),
        ("table".to_owned(), table),
        ("alias".to_owned(), alias),
        ("columns".to_owned(), columns),
        ("from".to_owned(), from),
        ("where".to_owned(), where_clause),
        ("group-by".to_owned(), group_by),
        ("having".to_owned(), having),
        ("order-by".to_owned(), order_by),
        ("limit".to_owned(), tagged("limit", [expression.clone()])),
        (
            "limit-with-ties".to_owned(),
            tagged("limit-with-ties", [expression.clone()]),
        ),
        ("offset".to_owned(), tagged("offset", [expression.clone()])),
        ("returning".to_owned(), returning),
        ("set".to_owned(), set),
        ("assignment".to_owned(), assignment),
        ("index".to_owned(), index),
        ("slice".to_owned(), slice),
        (
            "as".to_owned(),
            tagged("as", [expression.clone(), P::Atom(AtomType::String)]),
        ),
        ("into".to_owned(), into),
        ("source".to_owned(), tagged("source", [value.clone()])),
        ("window".to_owned(), window),
        ("locking".to_owned(), locking),
        ("cte".to_owned(), cte),
        ("row".to_owned(), one_or_more("row", expression.clone())),
        (
            "sqlcommenter".to_owned(),
            one_or_more("sqlcommenter", node_ref("sqlcommenter-tag")),
        ),
        (
            "sqlcommenter-tag".to_owned(),
            tagged(
                "tag",
                [P::Atom(AtomType::String), P::Atom(AtomType::String)],
            ),
        ),
    ];
    if let Some(generic_statement) = generic_statement {
        rules.push(("generic-statement".to_owned(), generic_statement));
    }
    rules.push(("qualified-name".to_owned(), qualified_name));
    rules.push((
        "sort".to_owned(),
        tagged(
            "sort",
            [
                expression.clone(),
                P::OneOf(vec![
                    P::Exact(Sexp::symbol("asc")),
                    P::Exact(Sexp::symbol("desc")),
                    tagged("using", [P::Atom(AtomType::String)]),
                ]),
                P::Optional(Box::new(P::OneOf(vec![
                    P::Exact(Sexp::symbol("nulls-first")),
                    P::Exact(Sexp::symbol("nulls-last")),
                ]))),
            ],
        ),
    ));
    rules.push((
        "expression".to_owned(),
        expression_schemas::expression_pattern(),
    ));
    rules.extend(expression_schemas::rules());
    rules.extend(ddl_schemas::rules());
    rules.extend(utility_schemas::rules());
    rules.extend(alter_schemas::rules());
    rules.extend(comment_schemas::rules());
    rules.extend(create_schemas::rules());
    rules.extend(drop_schemas::rules());
    rules.extend(misc_schemas::rules());
    Grammar::new("sxp-pg", "output", rules)
        .expect("the built-in PostgreSQL output grammar is valid")
}

fn tagged(name: &str, fields: impl IntoIterator<Item = Pattern>) -> Pattern {
    Pattern::List(
        std::iter::once(Pattern::Exact(Sexp::symbol(name)))
            .chain(fields)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use crate::{ParserOptions, PostgresParser};
    use sxp::{ParserOptions as SexpParserOptions, SexpParser};

    fn parse_sexp(input: &str) -> sxp::Sexp {
        SexpParser::with_options(SexpParserOptions::default().typed_atoms(true))
            .parse(input)
            .unwrap()
    }

    #[test]
    fn output_grammar_matches_supported_statement_families() {
        let grammar = crate::grammar();
        let mut parser = PostgresParser::new();
        for input in [
            "SELECT 1",
            "WITH items AS (SELECT 1) SELECT * FROM items",
            "INSERT INTO items (id) VALUES (1)",
            "UPDATE items SET id = id + 1",
            "DELETE FROM items WHERE id = 1",
            "CREATE TABLE items (id integer PRIMARY KEY)",
            "CREATE TABLE items (id integer GENERATED ALWAYS AS (id + 1))",
            "CREATE TABLE items (during tstzrange, UNIQUE (during WITHOUT OVERLAPS))",
            "CREATE TABLE items (id integer CHECK (id > 0) NOT ENFORCED)",
            "CREATE TABLE items (id integer CHECK (id > 0) ENFORCED)",
            "CREATE TABLE child (id int, during tstzrange, FOREIGN KEY (id, PERIOD during) REFERENCES parent (id, PERIOD during))",
            "ALTER TABLE items ALTER CONSTRAINT items_check NOT ENFORCED",
            "ALTER TABLE items ADD COLUMN name text",
            "DROP TABLE items",
            "EXPLAIN SELECT 1",
            "BEGIN; SELECT 1; COMMIT",
            "VACUUM (FULL) items",
            "CALL refresh_items()",
        ] {
            let output = parser.parse(input).unwrap();
            assert!(
                grammar.matches_root(&output),
                "output grammar rejected PostgreSQL input {input:?}: {output}"
            );
        }
    }

    #[test]
    fn output_grammar_accepts_typed_round_trips_and_comment_tags() {
        let grammar = crate::grammar();
        let mut parser = PostgresParser::new();
        let output = parser.parse("SELECT true, 1, 'value'").unwrap();
        let round_trip = parse_sexp(&output.to_string());
        assert!(grammar.matches_root(&round_trip));

        let mut parser = PostgresParser::with_options(ParserOptions::default().sqlcommenter(true));
        let output = parser.parse("SELECT 1 /*key='value'*/").unwrap();
        assert!(grammar.matches_root(&output));
    }

    #[test]
    fn output_grammar_rejects_unknown_statement_and_nested_tags() {
        let grammar = crate::grammar();
        for malformed in [
            "(garbage)",
            "(column \"id\")",
            "(create-slot)",
            "(into (table (\"t\")))",
            "(select (garbage))",
            "(statements (select 1) (garbage))",
            "(sqlcommenter (unknown \"key\" \"value\"))",
        ] {
            assert!(
                !grammar.matches_root(&parse_sexp(malformed)),
                "output grammar accepted malformed PostgreSQL output: {malformed}"
            );
        }
    }

    #[test]
    fn output_grammar_enforces_query_and_dml_field_shapes() {
        let grammar = crate::grammar();
        for malformed in [
            "(select)",
            "(select (where 1) (columns 1))",
            "(select (columns 1) (columns 2))",
            "(select (columns 1) (where 1 2))",
            "(select (columns 1) (from))",
            "(update (table (\"t\")) (set) (where 1))",
            "(update (table (\"t\")) (set (assignment \"id\" 1)) (where 1) (from (table (\"u\"))))",
            "(insert (into (table (\"t\"))) (source default-values))",
        ] {
            assert!(
                !grammar.matches_root(&parse_sexp(malformed)),
                "output grammar accepted malformed PostgreSQL fields: {malformed}"
            );
        }
    }

    #[test]
    fn output_grammar_rejects_malformed_expression_and_schema_fields() {
        let grammar = crate::grammar();
        for malformed in [
            "(select (columns (op \"+\" 1 2 3)))",
            "(select (columns (is-null 1 2)))",
            "(create-table (table (\"t\")) (columns (column \"a\" (type \"int\"))) (columns))",
            "(show 1)",
        ] {
            assert!(
                !grammar.matches_root(&parse_sexp(malformed)),
                "output grammar accepted malformed PostgreSQL payload: {malformed}"
            );
        }
    }
}
