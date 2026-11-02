use super::PostgresParser;
use pg_query::protobuf as pg;
use pg_query::protobuf::{a_const, node::Node as PgNode};

fn parse(input: &str) -> sxp::Result<sxp::Sexp> {
    PostgresParser::new().parse(input)
}

fn lowers(sql: &str, expected: &str) {
    assert_eq!(
        parse(sql).map(|sexp| sexp.to_string()),
        Ok(expected.to_owned())
    );
}

#[test]
fn lowers_return_semantically() {
    let stmt = pg::Node {
        node: Some(PgNode::ReturnStmt(Box::new(pg::ReturnStmt {
            returnval: Some(Box::new(pg::Node {
                node: Some(PgNode::AConst(pg::AConst {
                    isnull: false,
                    location: -1,
                    val: Some(a_const::Val::Ival(pg::Integer { ival: 42 })),
                })),
            })),
        }))),
    };
    assert_eq!(
        PostgresParser::default()
            .lower(&stmt)
            .map(|value| value.to_string()),
        Ok("(return 42)".to_owned())
    );
}

#[test]
fn parses_utility_statements() {
    lowers(
        "PREPARE p (int) AS SELECT $1",
        "(prepare \"p\" (parameters (type \"pg_catalog\" \"int4\")) (query (select (columns (parameter 1)))))",
    );
    lowers(
        "DECLARE c BINARY SCROLL CURSOR WITH HOLD FOR SELECT 1",
        "(declare \"c\" binary scroll with-hold (query (select (columns 1))))",
    );
    lowers("FETCH BACKWARD ALL FROM c", "(fetch backward all \"c\")");
    lowers(
        "TRUNCATE t1, t2 RESTART IDENTITY CASCADE",
        "(truncate (table (\"t1\")) (table (\"t2\")) restart-identity cascade)",
    );
    lowers(
        "COPY t (a, b) TO STDOUT (FORMAT csv)",
        "(copy (table (\"t\")) (columns \"a\" \"b\") to stdout (options (option \"format\" \"csv\")))",
    );
    lowers(
        "COMMENT ON COLUMN t.a IS 'x'",
        "(comment-on-column (name \"t\" \"a\") \"x\")",
    );
    lowers(
        "LOCK TABLE t IN ACCESS EXCLUSIVE MODE NOWAIT",
        "(lock (table (\"t\")) access-exclusive nowait)",
    );
    lowers("NOTIFY chan, 'payload'", "(notify \"chan\" \"payload\")");
    lowers(
        "VACUUM (FULL, PARALLEL 2) t (a)",
        "(vacuum full (option \"parallel\" 2) (tables ((table (\"t\")) (columns \"a\"))))",
    );
    lowers(
        "REINDEX TABLE CONCURRENTLY t",
        "(reindex-table (table (\"t\")) concurrently)",
    );
    lowers(
        "REFRESH MATERIALIZED VIEW CONCURRENTLY mv WITH NO DATA",
        "(refresh-materialized-view concurrently (table (\"mv\")) with-no-data)",
    );
}

#[test]
fn parses_do_and_call() {
    lowers("DO $$ BEGIN END $$", "(do (sql \" BEGIN END \"))");
    lowers(
        "DO LANGUAGE plpgsql $$ BEGIN END $$",
        "(do (language \"plpgsql\") (sql \" BEGIN END \"))",
    );
    lowers("CALL myproc(1, 'a')", "(call \"myproc\" 1 \"a\")");
    lowers("CALL myproc(1, NULL)", "(call \"myproc\" 1 null)");
    lowers(
        "CALL myproc(value => NULL)",
        "(call \"myproc\" (named-arg \"value\" null))",
    );
}

#[test]
fn lowers_call_output_arguments() {
    let call = pg::CallStmt {
        funccall: Some(Box::new(pg::FuncCall {
            funcname: vec![pg::Node {
                node: Some(PgNode::String(pg::String { sval: "p".into() })),
            }],
            args: vec![pg::Node {
                node: Some(PgNode::AConst(pg::AConst {
                    isnull: false,
                    location: -1,
                    val: Some(a_const::Val::Ival(pg::Integer { ival: 1 })),
                })),
            }],
            ..Default::default()
        })),
        outargs: vec![pg::Node {
            node: Some(PgNode::AConst(pg::AConst {
                isnull: true,
                location: -1,
                val: None,
            })),
        }],
        ..Default::default()
    };

    assert_eq!(
        PostgresParser::default()
            .lower_call(&call)
            .map(|value| value.to_string()),
        Ok("(call \"p\" 1 (outargs null))".to_owned())
    );
}

#[test]
fn call_function_expression_remains_unsupported() {
    let call = pg::CallStmt {
        funccall: Some(Box::new(pg::FuncCall {
            funcname: vec![pg::Node {
                node: Some(PgNode::String(pg::String { sval: "p".into() })),
            }],
            ..Default::default()
        })),
        funcexpr: Some(Box::new(pg::FuncExpr::default())),
        ..Default::default()
    };

    assert!(PostgresParser::default().lower_call(&call).is_err());
}

#[test]
fn lowers_empty_begin_atomic_body() {
    lowers(
        "CREATE PROCEDURE ptest8(x text) BEGIN ATOMIC END",
        "(create-procedure (name \"ptest8\") (parameters (parameter \"x\" (type \"text\"))) (sql-body))",
    );
}

#[test]
fn lowers_checkpoint_and_discard_semantically() {
    lowers("CHECKPOINT;", "(checkpoint)");
    lowers("DISCARD ALL;", "(discard all)");
    lowers("DISCARD PLANS;", "(discard plans)");
    lowers("DISCARD SEQUENCES;", "(discard sequences)");
    lowers("DISCARD TEMP;", "(discard temp)");
    lowers("DISCARD TEMPORARY;", "(discard temp)");
}

#[test]
fn lowers_load_semantically() {
    lowers("LOAD 'x';", "(load \"x\")");
}

#[test]
fn lowers_alter_system_semantically() {
    lowers(
        "ALTER SYSTEM SET work_mem = '64MB';",
        "(alter-system-set \"work_mem\" \"64MB\")",
    );
    lowers("ALTER SYSTEM SET x = 1;", "(alter-system-set \"x\" 1)");
    lowers(
        "ALTER SYSTEM SET x = 'a', 'b';",
        "(alter-system-set \"x\" \"a\" \"b\")",
    );
    lowers(
        "ALTER SYSTEM SET x TO DEFAULT;",
        "(alter-system-set \"x\" default)",
    );
    lowers(
        "ALTER SYSTEM RESET work_mem;",
        "(alter-system-reset \"work_mem\")",
    );
    lowers("ALTER SYSTEM RESET ALL;", "(alter-system-reset all)");
}

#[test]
fn lowers_cluster_and_reindex_options_semantically() {
    lowers("CLUSTER;", "(cluster)");
    lowers("CLUSTER t;", "(cluster (table (\"t\")))");
    lowers(
        "CLUSTER VERBOSE t USING i;",
        "(cluster (table (\"t\")) (using \"i\") verbose)",
    );
    lowers(
        "CLUSTER (BOGUS) t USING i;",
        "(cluster (table (\"t\")) (using \"i\") bogus)",
    );
    lowers(
        "CLUSTER (BOGUS 'a', verbose) t;",
        "(cluster (table (\"t\")) (option \"bogus\" \"a\") verbose)",
    );
    lowers(
        "CLUSTER (BOGUS -5) t;",
        "(cluster (table (\"t\")) (option \"bogus\" -5))",
    );
    lowers("REINDEX INDEX i;", "(reindex-index (table (\"i\")))");
    lowers("REINDEX SCHEMA s;", "(reindex-schema \"s\")");
    lowers("REINDEX SYSTEM;", "(reindex-system)");
    lowers("REINDEX DATABASE d;", "(reindex-database \"d\")");
    lowers(
        "REINDEX (VERBOSE, CONCURRENTLY) INDEX i;",
        "(reindex-index (table (\"i\")) verbose concurrently)",
    );
    lowers(
        "REINDEX (TABLESPACE fastspace) TABLE t;",
        "(reindex-table (table (\"t\")) (tablespace \"fastspace\"))",
    );
    lowers(
        "REINDEX (BOGUS 3) TABLE t;",
        "(reindex-table (table (\"t\")) (option \"bogus\" 3))",
    );
    lowers(
        "REINDEX (BOGUS 'x', concurrently) INDEX i;",
        "(reindex-index (table (\"i\")) (option \"bogus\" \"x\") concurrently)",
    );
}

#[test]
fn lowers_vacuum_and_analyze_semantically() {
    lowers("VACUUM;", "(vacuum)");
    lowers(
        "VACUUM t, u;",
        "(vacuum (tables ((table (\"t\"))) ((table (\"u\")))))",
    );
    lowers(
        "VACUUM (FULL, ANALYZE) t;",
        "(vacuum full analyze (tables ((table (\"t\")))))",
    );
    lowers(
        "VACUUM (FULL) t, u;",
        "(vacuum full (tables ((table (\"t\"))) ((table (\"u\")))))",
    );
    lowers(
        "VACUUM (PARALLEL 4) t;",
        "(vacuum (option \"parallel\" 4) (tables ((table (\"t\")))))",
    );
    lowers(
        "VACUUM (SKIP_LOCKED, TRUNCATE off) t;",
        "(vacuum skip-locked (option \"truncate\" \"off\") (tables ((table (\"t\")))))",
    );
    lowers(
        "VACUUM (INDEX_CLEANUP auto, BUFFER_USAGE_LIMIT 128) t;",
        "(vacuum (option \"index-cleanup\" \"auto\") (option \"buffer-usage-limit\" 128) (tables ((table (\"t\")))))",
    );
    lowers(
        "VACUUM (FULL true) t;",
        "(vacuum (option \"full\" \"true\") (tables ((table (\"t\")))))",
    );
    lowers("ANALYZE;", "(analyze)");
    lowers("ANALYZE VERBOSE;", "(analyze verbose)");
    lowers(
        "ANALYZE t (col1, col2);",
        "(analyze (tables ((table (\"t\")) (columns \"col1\" \"col2\"))))",
    );
    lowers(
        "ANALYZE VERBOSE t;",
        "(analyze verbose (tables ((table (\"t\")))))",
    );
}

#[test]
fn lowers_truncate_lock_cursor_semantically() {
    lowers(
        "TRUNCATE t, u;",
        "(truncate (table (\"t\")) (table (\"u\")))",
    );
    lowers(
        "TRUNCATE ONLY t RESTART IDENTITY CASCADE;",
        "(truncate (table (only true) (\"t\")) restart-identity cascade)",
    );
    lowers(
        "TRUNCATE t CONTINUE IDENTITY RESTRICT;",
        "(truncate (table (\"t\")))",
    );
    lowers(
        "LOCK TABLE t, u;",
        "(lock (table (\"t\")) (table (\"u\")) access-exclusive)",
    );
    lowers(
        "LOCK TABLE t IN SHARE ROW EXCLUSIVE MODE NOWAIT;",
        "(lock (table (\"t\")) share-row-exclusive nowait)",
    );
    lowers(
        "LOCK TABLE ONLY t;",
        "(lock (table (only true) (\"t\")) access-exclusive)",
    );
    lowers(
        "DECLARE c BINARY INSENSITIVE SCROLL CURSOR WITH HOLD FOR SELECT 1;",
        "(declare \"c\" binary scroll insensitive with-hold (query (select (columns 1))))",
    );
    lowers(
        "DECLARE c NO SCROLL CURSOR FOR SELECT 1;",
        "(declare \"c\" no-scroll (query (select (columns 1))))",
    );
    lowers("FETCH ALL FROM c;", "(fetch all \"c\")");
    lowers("MOVE BACKWARD 2 FROM c;", "(move backward 2 \"c\")");
    lowers("CLOSE c;", "(close \"c\")");
}

#[test]
fn lowers_notify_family_semantically() {
    lowers("LISTEN c;", "(listen \"c\")");
    lowers("NOTIFY c;", "(notify \"c\")");
    lowers("NOTIFY c, 'payload';", "(notify \"c\" \"payload\")");
    lowers("UNLISTEN c;", "(unlisten \"c\")");
    lowers("UNLISTEN *;", "(unlisten *)");
}
