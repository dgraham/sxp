use super::super::PostgresParser;
use pg_query::protobuf as pg;
use pg_query::protobuf::node::Node as PgNode;

fn parse(input: &str) -> sxp::Result<sxp::Sexp> {
    PostgresParser::new().parse(input)
}

fn lowers(sql: &str, expected: &str) {
    assert_eq!(
        parse(sql).map(|sexp| sexp.to_string()),
        Ok(expected.to_owned())
    );
}

fn create_constraint_flags(sql: &str, kind: pg::ConstrType) -> (bool, bool) {
    let parsed = pg_query::parse(sql, 0).unwrap();
    let statement = parsed.protobuf.stmts[0]
        .stmt
        .as_ref()
        .and_then(|node| node.node.as_ref())
        .expect("CREATE TABLE statement");
    let PgNode::CreateStmt(create) = statement else {
        panic!("expected CREATE TABLE AST");
    };
    let mut constraints = Vec::new();
    for element in &create.table_elts {
        match element.node.as_ref() {
            Some(PgNode::Constraint(constraint))
                if pg::ConstrType::try_from(constraint.contype) == Ok(kind) =>
            {
                constraints.push(constraint)
            }
            Some(PgNode::ColumnDef(column)) => {
                for node in &column.constraints {
                    if let Some(PgNode::Constraint(constraint)) = node.node.as_ref()
                        && pg::ConstrType::try_from(constraint.contype) == Ok(kind)
                    {
                        constraints.push(constraint);
                    }
                }
            }
            _ => {}
        }
    }
    let [constraint] = constraints.as_slice() else {
        panic!("expected one {kind:?} constraint");
    };
    (constraint.is_enforced, constraint.skip_validation)
}

#[test]
fn lowers_alter_database_refresh_collation_version() {
    lowers(
        "ALTER DATABASE mydb REFRESH COLLATION VERSION",
        "(alter-database \"mydb\" (refresh-collation-version))",
    );
}

#[test]
fn lowers_partition_columns_without_types() {
    lowers(
        "CREATE TABLE part_b PARTITION OF parted \
                     (b NOT NULL DEFAULT 1, b CHECK (b >= 0)) FOR VALUES IN ('b')",
        "(create-table (table (\"part_b\")) (columns (column \"b\" (not-null) (default 1)) (column \"b\" (check (op \">=\" (column \"b\") 0)))) (partition-of (table (\"parted\"))) (bounds (in (value \"b\"))))",
    );
}

#[test]
fn lowers_reloption_keyword_values() {
    lowers(
        "create table pagg_test (x int, y int) with (autovacuum_enabled = off)",
        "(create-table (table (\"pagg_test\")) (columns (column \"x\" (type \"pg_catalog\" \"int4\")) (column \"y\" (type \"pg_catalog\" \"int4\"))) (options (option \"autovacuum_enabled\" \"off\")))",
    );
    lowers(
        "create index gin_test_idx on gin_test_tbl using gin (i) \
                     with (fastupdate = on, gin_pending_list_limit = 4096)",
        "(create-index (name \"gin_test_idx\") (on (table (\"gin_test_tbl\"))) (using \"gin\") (columns (index-column \"i\")) (options (option \"fastupdate\" \"on\") (option \"gin_pending_list_limit\" 4096)))",
    );
    lowers(
        "alter table alterlock set (autovacuum_enabled = off)",
        "(alter-table (table (\"alterlock\")) (set (option \"autovacuum_enabled\" \"off\")))",
    );
}

#[test]
fn lowers_fixed_size_array_signatures() {
    lowers(
        "CREATE FUNCTION fixed_array(x int[3]) RETURNS int[1][2] \
                     LANGUAGE sql AS $$ SELECT $$",
        "(create-function (name \"fixed_array\") (parameters (parameter \"x\" (type \"pg_catalog\" \"int4\" (array 3)))) (returns (type \"pg_catalog\" \"int4\" (array 1) (array 2))) (language \"sql\") (as \" SELECT \"))",
    );
}

#[test]
fn lowers_setof_operator_signature() {
    lowers(
        "CREATE OPERATOR #*# (leftarg = SETOF int8, procedure = factorial)",
        "(create-operator (name \"#*#\") (leftarg (setof (type \"int8\"))) (procedure (type \"factorial\")))",
    );
}

#[test]
fn lowers_pct_type_function_signature() {
    lowers(
        "CREATE FUNCTION hobbies_by_name(hobbies_r.name%TYPE) \
                     RETURNS hobbies_r.person%TYPE \
                     AS 'select person from hobbies_r where name = $1' \
                     LANGUAGE SQL",
        "(create-function (name \"hobbies_by_name\") (parameters (parameter (type \"hobbies_r\" \"name\" %-type))) (returns (type \"hobbies_r\" \"person\" %-type)) (as \"select person from hobbies_r where name = $1\") (language \"sql\"))",
    );
}

#[test]
fn lowers_ddl_semantically() {
    lowers(
        "CREATE UNLOGGED TABLE accounts \
                     (id bigint PRIMARY KEY, email text NOT NULL, active boolean DEFAULT true)",
        "(create-table (persistence unlogged) (table (\"accounts\")) (columns (column \"id\" (type \"pg_catalog\" \"int8\") (primary-key)) (column \"email\" (type \"text\") (not-null)) (column \"active\" (type \"pg_catalog\" \"bool\") (default true))))",
    );
    lowers(
        "ALTER TABLE accounts ADD COLUMN name text, ALTER COLUMN email SET NOT NULL",
        "(alter-table (table (\"accounts\")) (add-column (column \"name\" (type \"text\"))) (set-not-null \"email\"))",
    );
    lowers(
        "ALTER TABLE accounts ADD CONSTRAINT email_nn NOT NULL email NOT VALID",
        "(alter-table (table (\"accounts\")) (add-constraint (not-null not-valid (name \"email_nn\"))))",
    );
    lowers(
        "CREATE TEMP TABLE audit (id integer)",
        "(create-table (persistence temporary) (table (\"audit\")) (columns (column \"id\" (type \"pg_catalog\" \"int4\"))))",
    );
    lowers(
        "ALTER TABLE audit RENAME COLUMN id TO audit_id",
        "(alter-table (table (\"audit\")) (rename-column \"id\" \"audit_id\"))",
    );
}

#[test]
fn lowers_extended_ddl_semantically() {
    lowers(
        "CREATE SCHEMA IF NOT EXISTS app",
        "(create-schema (if-not-exists) \"app\")",
    );
    lowers(
        "CREATE UNIQUE INDEX IF NOT EXISTS users_email_idx ON app.users (email DESC) \
                     WHERE active",
        "(create-index unique (if-not-exists) (name \"users_email_idx\") (on (table (\"app\" \"users\"))) (columns (index-column \"email\" desc)) (where (column \"active\")))",
    );
    lowers(
        "CREATE OR REPLACE VIEW app.active_users (id) AS \
                     SELECT id FROM app.users WHERE active",
        "(create-view or-replace (table (\"app\" \"active_users\")) (columns \"id\") (query (select (columns (column \"id\")) (from (table (\"app\" \"users\"))) (where (column \"active\")))))",
    );
    lowers(
        "CREATE MATERIALIZED VIEW app.user_counts AS \
                     SELECT count(*) FROM app.users WITH NO DATA",
        "(create-materialized-view (table (\"app\" \"user_counts\")) (query (select (columns (call \"count\" *)) (from (table (\"app\" \"users\"))))) with-no-data)",
    );
    lowers(
        "CREATE SEQUENCE app.order_ids INCREMENT BY 2 START WITH 10",
        "(create-sequence (table (\"app\" \"order_ids\")) (options (option \"increment\" 2) (option \"start\" 10)))",
    );
    lowers(
        "ALTER SEQUENCE app.order_ids RESTART WITH 100",
        "(alter-sequence (table (\"app\" \"order_ids\")) (options (option \"restart\" 100)))",
    );
    lowers(
        "ALTER SCHEMA app RENAME TO archive",
        "(alter-schema \"app\" (rename \"archive\"))",
    );
    lowers(
        "CREATE TYPE status AS ENUM ('new', 'done')",
        "(create-enum (name \"status\") (values \"new\" \"done\"))",
    );
    lowers(
        "CREATE TABLE events (id integer) PARTITION BY RANGE (id)",
        "(create-table (table (\"events\")) (columns (column \"id\" (type \"pg_catalog\" \"int4\"))) (partition-by range (columns \"id\")))",
    );
    lowers(
        "DROP TABLE IF EXISTS app.users CASCADE",
        "(drop-table (if-exists) (name \"app\" \"users\") cascade)",
    );
}

#[test]
fn lowers_type_and_constraint_ddl_semantically() {
    lowers(
        "CREATE DOMAIN email AS text NOT NULL CHECK (VALUE <> '')",
        "(create-domain (name \"email\") (type \"text\") (constraints (not-null) (check (op \"<>\" (column \"value\") \"\"))))",
    );
    lowers(
        "CREATE TYPE address AS (street text, zip integer)",
        "(create-type (name \"address\") (attributes (column \"street\" (type \"text\")) (column \"zip\" (type \"pg_catalog\" \"int4\"))))",
    );
    lowers(
        "ALTER TYPE status ADD VALUE 'pending' AFTER 'new'",
        "(alter-enum (name \"status\") (add-value \"pending\" (after \"new\")))",
    );
    lowers(
        "CREATE TABLE orders (user_id integer REFERENCES users (id) ON DELETE CASCADE)",
        "(create-table (table (\"orders\")) (columns (column \"user_id\" (type \"pg_catalog\" \"int4\") (foreign-key (columns) (references (table (\"users\")) (columns \"id\")) (on-delete cascade)))))",
    );
}

#[test]
fn lowers_set_operation_locking() {
    lowers(
        "SELECT 1 UNION SELECT 2 FOR UPDATE",
        "(union (select (columns 1)) (select (columns 2)) (locking for-update))",
    );
    lowers(
        "SELECT 1 UNION SELECT 2 FOR SHARE OF t SKIP LOCKED",
        "(union (select (columns 1)) (select (columns 2)) (locking for-share (of (table (\"t\"))) skip-locked))",
    );
    lowers(
        "SELECT 1 EXCEPT SELECT 2 FOR NO KEY UPDATE NOWAIT",
        "(except (select (columns 1)) (select (columns 2)) (locking for-no-key-update no-wait))",
    );
}

#[test]
fn rejects_invalid_and_unsupported_sql() {
    assert!(parse("SELECT FROM").is_err());
}

#[test]
fn lowers_pg18_temporal_constraints() {
    let unique = "CREATE TABLE t (during tstzrange, \
                  UNIQUE (during WITHOUT OVERLAPS))";
    let foreign_key = "CREATE TABLE child (id int, during tstzrange, \
                       FOREIGN KEY (id, PERIOD during) \
                       REFERENCES parent (id, PERIOD during))";

    lowers(
        unique,
        "(create-table (table (\"t\")) (columns (column \"during\" (type \"tstzrange\")) (unique \"during\" without-overlaps)))",
    );
    lowers(
        "CREATE TABLE t (during tstzrange, PRIMARY KEY (during WITHOUT OVERLAPS))",
        "(create-table (table (\"t\")) (columns (column \"during\" (type \"tstzrange\")) (primary-key \"during\" without-overlaps)))",
    );
    lowers(
        foreign_key,
        "(create-table (table (\"child\")) (columns (column \"id\" (type \"pg_catalog\" \"int4\")) (column \"during\" (type \"tstzrange\")) (foreign-key (columns \"id\" (period \"during\")) (references (table (\"parent\")) (columns \"id\" (period \"during\"))))))",
    );
    lowers(
        "CREATE TABLE child (id int, during tstzrange, \
         FOREIGN KEY (id, PERIOD during) REFERENCES parent)",
        "(create-table (table (\"child\")) (columns (column \"id\" (type \"pg_catalog\" \"int4\")) (column \"during\" (type \"tstzrange\")) (foreign-key (columns \"id\" (period \"during\")) (references (table (\"parent\")) (columns)))))",
    );
    lowers(
        "CREATE TABLE child (id int, during tstzrange, \
         FOREIGN KEY (id, PERIOD during) REFERENCES parent (id, during))",
        "(create-table (table (\"child\")) (columns (column \"id\" (type \"pg_catalog\" \"int4\")) (column \"during\" (type \"tstzrange\")) (foreign-key (columns \"id\" (period \"during\")) (references (table (\"parent\")) (columns \"id\" \"during\")))))",
    );
}

#[test]
fn lowers_pg18_enforcement_attributes() {
    let create = "CREATE TABLE t (a int CHECK (a > 0) NOT ENFORCED)";
    let alter = "ALTER TABLE t ALTER CONSTRAINT c NOT ENFORCED";

    lowers(
        "CREATE TABLE t (a int CHECK (a > 0))",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (check (op \">\" (column \"a\") 0)))))",
    );
    lowers(
        create,
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (check (op \">\" (column \"a\") 0) not-enforced))))",
    );
    lowers(
        "CREATE TABLE t (a int CHECK (a > 0) ENFORCED)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (check (op \">\" (column \"a\") 0) enforced))))",
    );
    lowers(
        "CREATE TABLE t (a int, CHECK (a > 0) NOT ENFORCED)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\")) (check (op \">\" (column \"a\") 0) not-valid not-enforced)))",
    );
    lowers(
        "CREATE TABLE child (id int, CONSTRAINT fk FOREIGN KEY (id) REFERENCES parent (id) NOT ENFORCED)",
        "(create-table (table (\"child\")) (columns (column \"id\" (type \"pg_catalog\" \"int4\")) (foreign-key (columns \"id\") (references (table (\"parent\")) (columns \"id\")) not-valid not-enforced (name \"fk\"))))",
    );
    lowers(
        alter,
        "(alter-table (table (\"t\")) (alter-constraint \"c\" not-enforced))",
    );
    lowers(
        "ALTER TABLE t ALTER CONSTRAINT c ENFORCED",
        "(alter-table (table (\"t\")) (alter-constraint \"c\" enforced))",
    );
    lowers(
        "ALTER TABLE t ALTER CONSTRAINT c INHERIT",
        "(alter-table (table (\"t\")) (alter-constraint \"c\" inherit))",
    );
    lowers(
        "ALTER TABLE t ALTER CONSTRAINT c NO INHERIT",
        "(alter-table (table (\"t\")) (alter-constraint \"c\" no-inherit))",
    );
}

#[test]
fn distinguishes_pg18_enforcement_ast_flags() {
    assert_eq!(
        create_constraint_flags(
            "CREATE TABLE t (a int CHECK (a > 0))",
            pg::ConstrType::ConstrCheck,
        ),
        (true, false),
    );
    assert_eq!(
        create_constraint_flags(
            "CREATE TABLE child (id int, FOREIGN KEY (id) REFERENCES parent (id))",
            pg::ConstrType::ConstrForeign,
        ),
        (true, false),
    );
    assert_eq!(
        create_constraint_flags(
            "CREATE TABLE t (a int PRIMARY KEY)",
            pg::ConstrType::ConstrPrimary,
        ),
        (false, false),
    );
    assert_eq!(
        create_constraint_flags(
            "CREATE TABLE child (id int, FOREIGN KEY (id) REFERENCES parent (id) NOT ENFORCED)",
            pg::ConstrType::ConstrForeign,
        ),
        (false, true),
    );
}

#[test]
fn rejects_malformed_temporal_foreign_key_ast() {
    let mut parser = PostgresParser::new();
    let mut constraint = pg::Constraint {
        fk_with_period: true,
        ..Default::default()
    };
    assert!(parser.lower_foreign_key_constraint(&constraint).is_err());

    constraint.pk_with_period = true;
    assert!(parser.lower_foreign_key_constraint(&constraint).is_err());
}

#[test]
fn parses_role_statements() {
    lowers("CREATE ROLE alice", "(create-role \"alice\")");
    lowers(
        "CREATE USER bob WITH LOGIN PASSWORD 'pw' VALID UNTIL '2027-01-01' CONNECTION LIMIT 5",
        "(create-user \"bob\" login (password \"pw\") (valid-until \"2027-01-01\") (connection-limit 5))",
    );
    lowers("CREATE GROUP g", "(create-group \"g\")");
    lowers(
        "CREATE ROLE admin NOLOGIN IN ROLE alice",
        "(create-role \"admin\" no-login (in-role (role \"alice\")))",
    );
    lowers(
        "CREATE ROLE r1 ROLE alice, bob",
        "(create-role \"r1\" (members (role \"alice\") (role \"bob\")))",
    );
    lowers(
        "ALTER ROLE alice WITH LOGIN",
        "(alter-role (role \"alice\") login)",
    );
    lowers(
        "ALTER ROLE alice RENAME TO alice2",
        "(rename-role \"alice\" \"alice2\")",
    );
    lowers(
        "ALTER ROLE alice SET search_path TO public",
        "(alter-role (role \"alice\") (set \"search_path\" \"public\"))",
    );
    lowers(
        "ALTER ROLE alice IN DATABASE d SET search_path = public",
        "(alter-role (role \"alice\") (in-database \"d\") (set \"search_path\" \"public\"))",
    );
    lowers(
        "ALTER ROLE ALL SET search_path TO public",
        "(alter-role (role all) (set \"search_path\" \"public\"))",
    );
    lowers("DROP ROLE alice, bob", "(drop-role \"alice\" \"bob\")");
    lowers(
        "DROP ROLE IF EXISTS alice",
        "(drop-role if-exists \"alice\")",
    );
    lowers("DROP ROLE CURRENT_ROLE", "(drop-role (role current-role))");
}

#[test]
fn parses_grants() {
    lowers(
        "GRANT SELECT, INSERT (a, b) ON t TO bob, PUBLIC",
        "(grant (privileges select (insert (columns \"a\" \"b\"))) (on (table (\"t\"))) (to (role \"bob\") (role public)))",
    );
    lowers(
        "GRANT ALL ON SCHEMA s TO bob",
        "(grant (privileges all) (on (name \"s\")) (to (role \"bob\")))",
    );
    lowers(
        "GRANT EXECUTE ON FUNCTION f(int) TO bob",
        "(grant (privileges execute) (on (name \"f\" (parameters (type \"pg_catalog\" \"int4\")))) (to (role \"bob\")))",
    );
    lowers(
        "GRANT USAGE ON SEQUENCE seq TO PUBLIC",
        "(grant (privileges usage) (on (table (\"seq\"))) (to (role public)))",
    );
    lowers(
        "GRANT SELECT ON ALL TABLES IN SCHEMA s TO bob",
        "(grant (privileges select) (on (all-tables-in-schema \"s\")) (to (role \"bob\")))",
    );
    lowers(
        "GRANT SELECT ON t TO bob WITH GRANT OPTION GRANTED BY admin",
        "(grant (privileges select) (on (table (\"t\"))) (to (role \"bob\")) with-grant-option (granted-by (role \"admin\")))",
    );
    lowers(
        "REVOKE SELECT ON t FROM bob CASCADE",
        "(revoke (privileges select) (on (table (\"t\"))) (from (role \"bob\")) cascade)",
    );
    lowers(
        "REVOKE GRANT OPTION FOR SELECT ON t FROM bob",
        "(revoke (privileges select) (on (table (\"t\"))) (from (role \"bob\")) grant-option-for)",
    );
    lowers(
        "GRANT admin, reader TO bob WITH ADMIN OPTION",
        "(grant-role (role \"admin\") (role \"reader\") (to (role \"bob\")) with-admin-option)",
    );
    lowers(
        "REVOKE admin FROM bob RESTRICT",
        "(revoke-role (role \"admin\") (from (role \"bob\")))",
    );
    lowers(
        "GRANT SELECT ON LARGE OBJECT 42 TO PUBLIC",
        "(grant (privileges select) (on (large-object 42)) (to (role public)))",
    );
    lowers(
        "GRANT USAGE ON DOMAIN d TO bob",
        "(grant (privileges usage) (on (name \"d\")) (to (role \"bob\")))",
    );
    lowers(
        "GRANT USAGE ON DOMAIN sch.d TO bob, PUBLIC",
        "(grant (privileges usage) (on (name \"sch\" \"d\")) (to (role \"bob\") (role public)))",
    );
    lowers(
        "GRANT ALTER SYSTEM ON PARAMETER work_mem TO bob",
        "(grant (privileges alter-system) (on (name \"work_mem\")) (to (role \"bob\")))",
    );
    lowers(
        "GRANT ALTER SYSTEM ON PARAMETER planner.debug TO bob WITH GRANT OPTION",
        "(grant (privileges alter-system) (on (name \"planner.debug\")) (to (role \"bob\")) with-grant-option)",
    );
    lowers(
        "REVOKE ALTER SYSTEM ON PARAMETER work_mem FROM bob CASCADE",
        "(revoke (privileges alter-system) (on (name \"work_mem\")) (from (role \"bob\")) cascade)",
    );
    lowers(
        "REVOKE USAGE ON DOMAIN d FROM bob",
        "(revoke (privileges usage) (on (name \"d\")) (from (role \"bob\")))",
    );
}

#[test]
fn parses_role_grant_options() {
    lowers(
        "GRANT r1 TO r2 WITH ADMIN FALSE",
        "(grant-role (role \"r1\") (to (role \"r2\")) without-admin-option)",
    );
    lowers(
        "GRANT r1 TO r2 WITH GRANT OPTION",
        "(grant-role (role \"r1\") (to (role \"r2\")) with-grant-option)",
    );
    lowers(
        "GRANT r1 TO r2 WITH GRANT FALSE",
        "(grant-role (role \"r1\") (to (role \"r2\")) without-grant-option)",
    );
    lowers(
        "GRANT r1 TO r2 WITH INHERIT TRUE",
        "(grant-role (role \"r1\") (to (role \"r2\")) with-inherit-option)",
    );
    lowers(
        "GRANT r1 TO r2 WITH SET FALSE",
        "(grant-role (role \"r1\") (to (role \"r2\")) without-set-option)",
    );
    lowers(
        "GRANT r1 TO r2 WITH ADMIN OPTION, INHERIT FALSE, SET TRUE",
        "(grant-role (role \"r1\") (to (role \"r2\")) with-admin-option without-inherit-option with-set-option)",
    );
    lowers(
        "GRANT r1 TO r2 WITH ADMIN TRUE, GRANT TRUE",
        "(grant-role (role \"r1\") (to (role \"r2\")) with-admin-option with-grant-option)",
    );
    lowers(
        "REVOKE INHERIT OPTION FOR r1 FROM r2",
        "(revoke-role (role \"r1\") (from (role \"r2\")) inherit-option-for)",
    );
    lowers(
        "REVOKE SET OPTION FOR r1 FROM r2 CASCADE",
        "(revoke-role (role \"r1\") (from (role \"r2\")) set-option-for cascade)",
    );
    lowers(
        "REVOKE ADMIN OPTION FOR r1 FROM r2",
        "(revoke-role (role \"r1\") (from (role \"r2\")) admin-option-for)",
    );
    lowers(
        "GRANT r1 TO r2 WITH ADMIN OPTION GRANTED BY boss",
        "(grant-role (role \"r1\") (to (role \"r2\")) with-admin-option (granted-by (role \"boss\")))",
    );
}

#[test]
fn parses_privilege_administration() {
    lowers(
        "ALTER DEFAULT PRIVILEGES IN SCHEMA s GRANT SELECT ON TABLES TO bob",
        "(alter-default-privileges (in-schema \"s\") grant (privileges select) on-tables (to (role \"bob\")))",
    );
    lowers(
        "ALTER DEFAULT PRIVILEGES FOR ROLE alice REVOKE ALL ON FUNCTIONS FROM PUBLIC",
        "(alter-default-privileges (for-role (role \"alice\")) revoke (privileges all) on-functions (from (role public)))",
    );
    lowers(
        "ALTER DEFAULT PRIVILEGES GRANT SELECT ON LARGE OBJECTS TO bob",
        "(alter-default-privileges grant (privileges select) on-large-objects (to (role \"bob\")))",
    );
    lowers(
        "REASSIGN OWNED BY alice, bob TO admin",
        "(reassign-owned (role \"alice\") (role \"bob\") (to (role \"admin\")))",
    );
    lowers(
        "DROP OWNED BY alice CASCADE",
        "(drop-owned (role \"alice\") cascade)",
    );
    lowers(
        "SECURITY LABEL ON TABLE t IS 'label'",
        "(security-label (name \"t\") \"label\")",
    );
    lowers(
        "SECURITY LABEL FOR provider ON COLUMN t.a IS 'x'",
        "(security-label (provider \"provider\") (name \"t\" \"a\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON TABLE t IS NULL",
        "(security-label (name \"t\") drop)",
    );
}

#[test]
fn parses_security_labels() {
    lowers(
        "SECURITY LABEL ON ROLE bob IS 'classified'",
        "(security-label (name \"bob\") \"classified\")",
    );
    lowers(
        "SECURITY LABEL ON TYPE status IS 'x'",
        "(security-label (name \"status\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON DOMAIN sch.d IS 'x'",
        "(security-label (name \"sch\" \"d\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON PROCEDURE p(int) IS 'x'",
        "(security-label (name \"p\" (parameters (type \"pg_catalog\" \"int4\"))) \"x\")",
    );
    lowers(
        "SECURITY LABEL ON LARGE OBJECT 42 IS 'x'",
        "(security-label (large-object 42) \"x\")",
    );
    lowers(
        "SECURITY LABEL ON FOREIGN TABLE ft IS 'x'",
        "(security-label (name \"ft\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON FOREIGN DATA WRAPPER w IS 'x'",
        "(security-label (name \"w\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON SERVER sv IS 'x'",
        "(security-label (name \"sv\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON TABLESPACE ts IS 'x'",
        "(security-label (name \"ts\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON EVENT TRIGGER et IS 'x'",
        "(security-label (name \"et\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON PUBLICATION p IS 'x'",
        "(security-label (name \"p\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON SUBSCRIPTION sub IS 'x'",
        "(security-label (name \"sub\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON LANGUAGE l IS 'x'",
        "(security-label (name \"l\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON STATISTICS st IS 'x'",
        "(security-label (name \"st\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON ACCESS METHOD am IS 'x'",
        "(security-label (name \"am\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON EXTENSION e IS 'x'",
        "(security-label (name \"e\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON COLLATION c IS 'x'",
        "(security-label (name \"c\") \"x\")",
    );
    lowers(
        "SECURITY LABEL ON CONVERSION cv IS 'x'",
        "(security-label (name \"cv\") \"x\")",
    );
}

#[test]
fn parses_comments() {
    lowers(
        "COMMENT ON PROCEDURE p(int) IS 'x'",
        "(comment-on-procedure (name \"p\" (parameters (type \"pg_catalog\" \"int4\"))) \"x\")",
    );
    lowers(
        "COMMENT ON TYPE ty IS 'x'",
        "(comment-on-type (name \"ty\") \"x\")",
    );
    lowers(
        "COMMENT ON DOMAIN sch.d IS 'x'",
        "(comment-on-domain (name \"sch\" \"d\") \"x\")",
    );
    lowers(
        "COMMENT ON TABLESPACE ts IS 'x'",
        "(comment-on-tablespace (name \"ts\") \"x\")",
    );
    lowers(
        "COMMENT ON FOREIGN TABLE ft IS 'x'",
        "(comment-on-foreign-table (name \"ft\") \"x\")",
    );
    lowers(
        "COMMENT ON FOREIGN DATA WRAPPER w IS 'x'",
        "(comment-on-foreign-data-wrapper (name \"w\") \"x\")",
    );
    lowers(
        "COMMENT ON SERVER sv IS 'x'",
        "(comment-on-server (name \"sv\") \"x\")",
    );
    lowers(
        "COMMENT ON LANGUAGE l IS 'x'",
        "(comment-on-language (name \"l\") \"x\")",
    );
    lowers(
        "COMMENT ON EVENT TRIGGER et IS 'x'",
        "(comment-on-event-trigger (name \"et\") \"x\")",
    );
    lowers(
        "COMMENT ON PUBLICATION p IS 'x'",
        "(comment-on-publication (name \"p\") \"x\")",
    );
    lowers(
        "COMMENT ON SUBSCRIPTION sub IS 'x'",
        "(comment-on-subscription (name \"sub\") \"x\")",
    );
    lowers(
        "COMMENT ON STATISTICS st IS 'x'",
        "(comment-on-statistics (name \"st\") \"x\")",
    );
    lowers(
        "COMMENT ON LARGE OBJECT 42 IS 'x'",
        "(comment-on-large-object (large-object 42) \"x\")",
    );
    lowers(
        "COMMENT ON ACCESS METHOD am IS 'x'",
        "(comment-on-access-method (name \"am\") \"x\")",
    );
    lowers(
        "COMMENT ON EXTENSION e IS 'x'",
        "(comment-on-extension (name \"e\") \"x\")",
    );
    lowers(
        "COMMENT ON COLLATION c IS 'x'",
        "(comment-on-collation (name \"c\") \"x\")",
    );
    lowers(
        "COMMENT ON CONVERSION cv IS 'x'",
        "(comment-on-conversion (name \"cv\") \"x\")",
    );
    lowers(
        "COMMENT ON OPERATOR +(int, int) IS 'x'",
        "(comment-on-operator (name \"+\" (parameters (type \"pg_catalog\" \"int4\") (type \"pg_catalog\" \"int4\"))) \"x\")",
    );
    lowers(
        "COMMENT ON TRIGGER tr ON t IS 'x'",
        "(comment-on-trigger (name \"t\" \"tr\") \"x\")",
    );
    lowers(
        "COMMENT ON RULE rl ON t IS 'x'",
        "(comment-on-rule (name \"t\" \"rl\") \"x\")",
    );
    lowers(
        "COMMENT ON POLICY pol ON t IS 'x'",
        "(comment-on-policy (name \"t\" \"pol\") \"x\")",
    );
    lowers(
        "COMMENT ON CONSTRAINT cons ON t IS 'x'",
        "(comment-on-constraint (name \"t\" \"cons\") \"x\")",
    );
    lowers(
        "COMMENT ON TEXT SEARCH CONFIGURATION tsc IS 'x'",
        "(comment-on-text-search-configuration (name \"tsc\") \"x\")",
    );
    lowers(
        "COMMENT ON TEXT SEARCH DICTIONARY tsd IS 'x'",
        "(comment-on-text-search-dictionary (name \"tsd\") \"x\")",
    );
    lowers(
        "COMMENT ON TEXT SEARCH PARSER tsp IS 'x'",
        "(comment-on-text-search-parser (name \"tsp\") \"x\")",
    );
    lowers(
        "COMMENT ON TEXT SEARCH TEMPLATE tst IS 'x'",
        "(comment-on-text-search-template (name \"tst\") \"x\")",
    );
    lowers(
        "COMMENT ON CAST (int AS text) IS 'x'",
        "(comment-on-cast (source (name \"pg_catalog\" \"int4\")) (target (name \"text\")) \"x\")",
    );
    lowers(
        "COMMENT ON AGGREGATE agg(int) IS 'x'",
        "(comment-on-aggregate (name \"agg\" (parameters (type \"pg_catalog\" \"int4\"))) \"x\")",
    );
    lowers(
        "COMMENT ON TABLE t IS NULL",
        "(comment-on-table (name \"t\") drop)",
    );
}

#[test]
fn parses_dead_arm_lockins() {
    lowers(
        "ALTER SCHEMA s RENAME TO s2",
        "(alter-schema \"s\" (rename \"s2\"))",
    );
    lowers("ALTER ROLE r RENAME TO r2", "(rename-role \"r\" \"r2\")");
    lowers(
        "GRANT SELECT ON SEQUENCE q TO bob",
        "(grant (privileges select) (on (table (\"q\"))) (to (role \"bob\")))",
    );
}

#[test]
fn parses_database_statements() {
    lowers(
        "CREATE DATABASE mydb OWNER alice",
        "(create-database \"mydb\" (owner \"alice\"))",
    );
    lowers(
        "CREATE DATABASE mydb WITH OWNER = alice ENCODING = 'UTF8' LC_COLLATE = 'C' \
                     TEMPLATE = template0 ALLOW_CONNECTIONS true CONNECTION LIMIT 50",
        "(create-database \"mydb\" (owner \"alice\") (encoding \"UTF8\") (lc-collate \"C\") (template \"template0\") allow-connections (connection-limit 50))",
    );
    lowers(
        "CREATE DATABASE mydb ICU_LOCALE = 'en' LOCALE_PROVIDER = icu \
                 COLLATION_VERSION = '153.80' BUILTIN_LOCALE = 'C.UTF-8' \
                 ICU_RULES = '&a < b' OID = 42",
        "(create-database \"mydb\" (icu-locale \"en\") (locale-provider \"icu\") (collation-version \"153.80\") (builtin-locale \"C.UTF-8\") (icu-rules \"&a < b\") (oid 42))",
    );
    lowers(
        "ALTER DATABASE mydb RENAME TO newdb",
        "(alter-database \"mydb\" (rename \"newdb\"))",
    );
    lowers(
        "ALTER DATABASE mydb OWNER TO alice",
        "(alter-database \"mydb\" (owner (role \"alice\")))",
    );
    lowers(
        "ALTER DATABASE mydb SET search_path TO public",
        "(alter-database \"mydb\" (set \"search_path\" \"public\"))",
    );
    lowers(
        "ALTER DATABASE mydb SET TABLESPACE ts1",
        "(alter-database \"mydb\" (in-tablespace \"ts1\"))",
    );
    lowers(
        "ALTER DATABASE mydb ALLOW_CONNECTIONS false IS_TEMPLATE true",
        "(alter-database \"mydb\" no-allow-connections is-template)",
    );
    lowers(
        "DROP DATABASE IF EXISTS mydb (FORCE)",
        "(drop-database if-exists \"mydb\" force)",
    );
}

#[test]
fn parses_tablespace_statements() {
    lowers(
        "CREATE TABLESPACE ts OWNER alice LOCATION '/data/ts' \
                     WITH (random_page_cost = 1.5)",
        "(create-tablespace \"ts\" (owner (role \"alice\")) (location \"/data/ts\") (options (option \"random_page_cost\" 1.5)))",
    );
    lowers(
        "ALTER TABLESPACE ts SET (random_page_cost = 2.0)",
        "(alter-tablespace \"ts\" (set (option \"random_page_cost\" 2.0)))",
    );
    lowers(
        "ALTER TABLESPACE ts RESET (random_page_cost)",
        "(alter-tablespace \"ts\" (reset (option \"random_page_cost\")))",
    );
    lowers(
        "ALTER TABLESPACE ts RENAME TO ts2",
        "(alter-tablespace \"ts\" (rename \"ts2\"))",
    );
    lowers(
        "ALTER TABLESPACE ts OWNER TO bob",
        "(alter-tablespace \"ts\" (owner (role \"bob\")))",
    );
    lowers(
        "DROP TABLESPACE IF EXISTS ts",
        "(drop-tablespace if-exists \"ts\")",
    );
}

#[test]
fn parses_extension_statements() {
    lowers("CREATE EXTENSION hstore", "(create-extension \"hstore\")");
    lowers(
        "CREATE EXTENSION IF NOT EXISTS hstore VERSION '1.4' SCHEMA public CASCADE",
        "(create-extension \"hstore\" (if-not-exists) (version \"1.4\") (schema \"public\") cascade)",
    );
    lowers(
        "ALTER EXTENSION hstore UPDATE TO '2.0'",
        "(alter-extension \"hstore\" (update-to \"2.0\"))",
    );
    lowers(
        "ALTER EXTENSION hstore UPDATE",
        "(alter-extension \"hstore\")",
    );
    lowers(
        "ALTER EXTENSION hstore ADD FUNCTION f(int)",
        "(alter-extension \"hstore\" (add (name \"f\" (parameters (type \"pg_catalog\" \"int4\")))))",
    );
    lowers(
        "ALTER EXTENSION hstore DROP FUNCTION f(int)",
        "(alter-extension \"hstore\" (drop (name \"f\" (parameters (type \"pg_catalog\" \"int4\")))))",
    );
    lowers(
        "ALTER EXTENSION hstore ADD TABLE t",
        "(alter-extension \"hstore\" (add (name \"t\")))",
    );
    lowers(
        "DROP EXTENSION IF EXISTS hstore CASCADE",
        "(drop-extension (if-exists) (name \"hstore\") cascade)",
    );
}

#[test]
fn parses_conversion_and_cast_statements() {
    lowers(
        "CREATE CONVERSION myconv FOR 'LATIN1' TO 'UTF8' FROM myfunc",
        "(create-conversion (name \"myconv\") (for \"LATIN1\") (to \"UTF8\") (from (name \"myfunc\")))",
    );
    lowers(
        "CREATE DEFAULT CONVERSION myconv FOR 'LATIN1' TO 'UTF8' FROM myfunc",
        "(create-conversion (name \"myconv\") (for \"LATIN1\") (to \"UTF8\") (from (name \"myfunc\")) default)",
    );
    lowers(
        "CREATE CAST (integer AS bigint) WITH INOUT AS IMPLICIT",
        "(create-cast (type \"pg_catalog\" \"int4\") (type \"pg_catalog\" \"int8\") with-inout (as implicit))",
    );
    lowers(
        "CREATE CAST (integer AS bigint) WITH FUNCTION f(int) AS ASSIGNMENT",
        "(create-cast (type \"pg_catalog\" \"int4\") (type \"pg_catalog\" \"int8\") (function (name \"f\" (parameters (type \"pg_catalog\" \"int4\")))) (as assignment))",
    );
    lowers(
        "CREATE CAST (integer AS bigint) WITH INOUT",
        "(create-cast (type \"pg_catalog\" \"int4\") (type \"pg_catalog\" \"int8\") with-inout (as explicit))",
    );
}

#[test]
fn lowers_create_function_statements() {
    lowers(
        "CREATE FUNCTION f(a int, b text DEFAULT 'x') RETURNS int LANGUAGE plpgsql AS $$ BEGIN RETURN 1; END $$",
        "(create-function (name \"f\") (parameters (parameter \"a\" (type \"pg_catalog\" \"int4\")) (parameter \"b\" (type \"text\") (default \"x\"))) (returns (type \"pg_catalog\" \"int4\")) (language \"plpgsql\") (as \" BEGIN RETURN 1; END \"))",
    );

    lowers(
        "CREATE OR REPLACE PROCEDURE p(IN x int, OUT y int) LANGUAGE sql AS $$ INSERT INTO t VALUES (x) $$",
        "(create-procedure or-replace (name \"p\") (parameters (parameter in \"x\" (type \"pg_catalog\" \"int4\")) (parameter out \"y\" (type \"pg_catalog\" \"int4\"))) (language \"sql\") (as \" INSERT INTO t VALUES (x) \"))",
    );

    lowers(
        "CREATE FUNCTION sq() RETURNS SETOF int LANGUAGE sql AS $$ SELECT 1 $$",
        "(create-function (name \"sq\") (returns (setof (type \"pg_catalog\" \"int4\"))) (language \"sql\") (as \" SELECT 1 \"))",
    );

    lowers(
        "CREATE FUNCTION arr(x int[]) RETURNS text[] LANGUAGE sql AS $$ SELECT ARRAY['a'] $$",
        "(create-function (name \"arr\") (parameters (parameter \"x\" (type \"pg_catalog\" \"int4\" (array)))) (returns (type \"text\" (array))) (language \"sql\") (as \" SELECT ARRAY['a'] \"))",
    );

    lowers(
        "CREATE FUNCTION tbl(a int, b text) RETURNS TABLE(c int, d text) LANGUAGE sql AS $$ SELECT 1, 'x' $$",
        "(create-function (name \"tbl\") (parameters (parameter \"a\" (type \"pg_catalog\" \"int4\")) (parameter \"b\" (type \"text\")) (parameter table \"c\" (type \"pg_catalog\" \"int4\")) (parameter table \"d\" (type \"text\"))) (returns (setof (type \"pg_catalog\" \"record\"))) (language \"sql\") (as \" SELECT 1, 'x' \"))",
    );

    lowers(
        "CREATE FUNCTION opt() RETURNS int LANGUAGE sql VOLATILE STRICT SECURITY DEFINER LEAKPROOF PARALLEL SAFE COST 100 ROWS 10 SET search_path = public AS $$ SELECT 1 $$",
        "(create-function (name \"opt\") (returns (type \"pg_catalog\" \"int4\")) (language \"sql\") volatile strict security-definer leakproof (parallel safe) (cost 100) (rows 10) (set \"search_path\" \"public\") (as \" SELECT 1 \"))",
    );

    lowers(
        "CREATE FUNCTION stab() RETURNS int LANGUAGE sql STABLE CALLED ON NULL INPUT PARALLEL UNSAFE AS $$ SELECT 1 $$",
        "(create-function (name \"stab\") (returns (type \"pg_catalog\" \"int4\")) (language \"sql\") stable called-on-null-input (parallel unsafe) (as \" SELECT 1 \"))",
    );

    lowers(
        "CREATE FUNCTION sb() RETURNS int LANGUAGE SQL RETURN 1",
        "(create-function (name \"sb\") (returns (type \"pg_catalog\" \"int4\")) (language \"sql\") (sql-body (return 1)))",
    );

    lowers(
        "CREATE FUNCTION atomic(x int) RETURNS int LANGUAGE SQL BEGIN ATOMIC SELECT x; SELECT x + 1; END",
        "(create-function (name \"atomic\") (parameters (parameter \"x\" (type \"pg_catalog\" \"int4\"))) (returns (type \"pg_catalog\" \"int4\")) (language \"sql\") (sql-body (select (columns (column \"x\"))) (select (columns (op \"+\" (column \"x\") 1)))))",
    );
}

#[test]
fn lowers_create_trigger_statements() {
    lowers(
        "CREATE TRIGGER trg BEFORE INSERT OR UPDATE OF a, b OR DELETE OR TRUNCATE ON t FOR EACH ROW EXECUTE FUNCTION fn(1, 'x')",
        "(create-trigger \"trg\" before (on (table (\"t\"))) (events insert delete (update \"a\" \"b\") truncate) (for-each row) (function (name \"fn\") (args \"1\" \"x\")))",
    );

    lowers(
        "CREATE TRIGGER trg INSTEAD OF UPDATE ON v FOR EACH STATEMENT WHEN (NEW.a > 1) EXECUTE PROCEDURE fn()",
        "(create-trigger \"trg\" instead (on (table (\"v\"))) (events update) (for-each statement) (when (op \">\" (column \"new\" \"a\") 1)) (function (name \"fn\")))",
    );

    lowers(
        "CREATE TRIGGER trg AFTER INSERT ON t REFERENCING NEW TABLE AS new_t OLD TABLE AS old_t FOR EACH STATEMENT EXECUTE FUNCTION fn('x')",
        "(create-trigger \"trg\" after (on (table (\"t\"))) (events insert) (for-each statement) (transitions (new-table \"new_t\") (old-table \"old_t\")) (function (name \"fn\") (args \"x\")))",
    );

    lowers(
        "CREATE TRIGGER trg AFTER UPDATE ON t REFERENCING NEW ROW AS n OLD ROW AS o FOR EACH ROW EXECUTE FUNCTION fn()",
        "(create-trigger \"trg\" after (on (table (\"t\"))) (events update) (for-each row) (transitions (new-row \"n\") (old-row \"o\")) (function (name \"fn\")))",
    );

    lowers(
        "CREATE CONSTRAINT TRIGGER trg AFTER INSERT ON t FOR EACH ROW EXECUTE FUNCTION fn()",
        "(create-trigger \"trg\" constraint after (on (table (\"t\"))) (events insert) (for-each row) (function (name \"fn\")))",
    );
}

#[test]
fn lowers_function_drops() {
    lowers(
        "DROP FUNCTION f(int, text)",
        "(drop-function (name \"f\" (parameters (type \"pg_catalog\" \"int4\") (type \"text\"))))",
    );

    lowers(
        "DROP FUNCTION IF EXISTS f CASCADE",
        "(drop-function (if-exists) (name \"f\" *) cascade)",
    );

    lowers("DROP FUNCTION f", "(drop-function (name \"f\" *))");

    lowers("DROP PROCEDURE p", "(drop-procedure (name \"p\" *))");

    lowers(
        "DROP AGGREGATE agg(int)",
        "(drop-aggregate (name \"agg\" (parameters (type \"pg_catalog\" \"int4\"))))",
    );

    lowers(
        "DROP OPERATOR + (int, int)",
        "(drop-operator (name \"+\" (parameters (type \"pg_catalog\" \"int4\") (type \"pg_catalog\" \"int4\"))))",
    );

    lowers(
        "DROP OPERATOR - (NONE, int)",
        "(drop-operator (name \"-\" (parameters any (type \"pg_catalog\" \"int4\"))))",
    );
}

#[test]
fn lowers_function_alterations() {
    lowers(
        "ALTER FUNCTION f(int, text) RENAME TO g",
        "(alter-function (name \"f\" (parameters (type \"pg_catalog\" \"int4\") (type \"text\"))) (rename \"g\"))",
    );

    lowers(
        "ALTER PROCEDURE p RENAME TO q",
        "(alter-procedure (name \"p\" *) (rename \"q\"))",
    );

    lowers(
        "ALTER AGGREGATE agg(int) RENAME TO agg2",
        "(alter-aggregate (name \"agg\" (parameters (type \"pg_catalog\" \"int4\"))) (rename \"agg2\"))",
    );

    lowers(
        "ALTER FUNCTION f(int) OWNER TO alice",
        "(alter-function (name \"f\" (parameters (type \"pg_catalog\" \"int4\"))) (owner (role \"alice\")))",
    );

    lowers(
        "ALTER PROCEDURE p(int) OWNER TO \"bob\"",
        "(alter-procedure (name \"p\" (parameters (type \"pg_catalog\" \"int4\"))) (owner (role \"bob\")))",
    );

    lowers(
        "ALTER AGGREGATE agg(int) OWNER TO carol",
        "(alter-aggregate (name \"agg\" (parameters (type \"pg_catalog\" \"int4\"))) (owner (role \"carol\")))",
    );

    lowers(
        "ALTER OPERATOR + (int, int) OWNER TO dave",
        "(alter-operator (name \"+\" (parameters (type \"pg_catalog\" \"int4\") (type \"pg_catalog\" \"int4\"))) (owner (role \"dave\")))",
    );

    lowers(
        "ALTER FUNCTION f(int) SET SCHEMA private",
        "(alter-function (name \"f\" (parameters (type \"pg_catalog\" \"int4\"))) (set-schema \"private\"))",
    );

    lowers(
        "ALTER OPERATOR + (int, int) SET SCHEMA ops",
        "(alter-operator (name \"+\" (parameters (type \"pg_catalog\" \"int4\") (type \"pg_catalog\" \"int4\"))) (set-schema \"ops\"))",
    );

    lowers(
        "ALTER FUNCTION f(int) SET search_path = public, pg_catalog",
        "(alter-function (name \"f\" (parameters (type \"pg_catalog\" \"int4\"))) (set \"search_path\" \"public\" \"pg_catalog\"))",
    );

    lowers(
        "ALTER FUNCTION f(int) RESET ALL",
        "(alter-function (name \"f\" (parameters (type \"pg_catalog\" \"int4\"))) (reset all))",
    );

    lowers(
        "ALTER PROCEDURE p COST 500 ROWS 100",
        "(alter-procedure (name \"p\" *) (cost 500) (rows 100))",
    );

    lowers(
        "ALTER ROUTINE f(integer) IMMUTABLE",
        "(alter-routine (name \"f\" (parameters (type \"pg_catalog\" \"int4\"))) immutable)",
    );

    lowers(
        "ALTER FUNCTION f(int) DEPENDS ON EXTENSION cube",
        "(alter-function-depends (name \"f\" (parameters (type \"pg_catalog\" \"int4\"))) (depends-on-extension \"cube\"))",
    );

    lowers(
        "ALTER PROCEDURE p(int) NO DEPENDS ON EXTENSION cube",
        "(alter-procedure-depends (name \"p\" (parameters (type \"pg_catalog\" \"int4\"))) (depends-on-extension \"cube\") remove)",
    );
}

#[test]
fn lowers_function_calls_and_operator_comment() {
    lowers(
        "SELECT percentile_disc(0.5) WITHIN GROUP (ORDER BY x) FROM t",
        "(select (columns (call \"percentile_disc\" 0.5 within-group (order-by (sort (column \"x\") asc)))) (from (table (\"t\"))))",
    );

    lowers(
        "SELECT concat_v(x, VARIADIC ARRAY['a']) FROM t",
        "(select (columns (call \"concat_v\" (column \"x\") (variadic (array \"a\")))) (from (table (\"t\"))))",
    );

    lowers(
        "SELECT count(DISTINCT x) FROM t",
        "(select (columns (call \"count\" (distinct (column \"x\")))) (from (table (\"t\"))))",
    );

    lowers(
        "COMMENT ON OPERATOR - (NONE, int) IS 'unary minus'",
        "(comment-on-operator (name \"-\" (parameters any (type \"pg_catalog\" \"int4\"))) \"unary minus\")",
    );
}

#[test]
fn parses_partition_creation_and_attach() {
    lowers(
        "CREATE TABLE p1 PARTITION OF p FOR VALUES FROM (1) TO (10)",
        "(create-table (table (\"p1\")) (columns) (partition-of (table (\"p\"))) (bounds (from (value 1)) (to (value 10))))",
    );
    lowers(
        "CREATE TABLE p2 PARTITION OF p FOR VALUES IN (1, DEFAULT)",
        "(create-table (table (\"p2\")) (columns) (partition-of (table (\"p\"))) (bounds (in (value 1) (value default))))",
    );
    lowers(
        "CREATE TABLE p3 PARTITION OF p FOR VALUES WITH (MODULUS 4, REMAINDER 1)",
        "(create-table (table (\"p3\")) (columns) (partition-of (table (\"p\"))) (bounds (modulus 4) (remainder 1)))",
    );
    lowers(
        "CREATE TABLE p4 PARTITION OF p DEFAULT",
        "(create-table (table (\"p4\")) (columns) (partition-of (table (\"p\"))) (bounds default))",
    );
    lowers(
        "CREATE TABLE p5 PARTITION OF p FOR VALUES FROM (MINVALUE) TO (MAXVALUE)",
        "(create-table (table (\"p5\")) (columns) (partition-of (table (\"p\"))) (bounds (from (minvalue)) (to (maxvalue))))",
    );
    lowers(
        "ALTER TABLE p ATTACH PARTITION p1 FOR VALUES FROM (1) TO (10)",
        "(alter-table (table (\"p\")) (attach-partition (table (\"p1\")) (bounds (from (value 1)) (to (value 10)))))",
    );
    lowers(
        "ALTER TABLE p DETACH PARTITION p1",
        "(alter-table (table (\"p\")) (detach-partition (table (\"p1\"))))",
    );
    lowers(
        "ALTER TABLE p DETACH PARTITION p1 CONCURRENTLY",
        "(alter-table (table (\"p\")) (detach-partition (table (\"p1\")) concurrently))",
    );
    lowers(
        "ALTER TABLE p DETACH PARTITION p1 FINALIZE",
        "(alter-table (table (\"p\")) (detach-partition (table (\"p1\")) finalize))",
    );
}

#[test]
fn parses_statistics_statements() {
    lowers(
        "CREATE STATISTICS st (dependencies, mcv) ON a, b FROM t",
        "(create-statistics (name \"st\") (kinds dependencies mcv) (on (column \"a\") (column \"b\")) (from (table (\"t\"))))",
    );
    lowers(
        "CREATE STATISTICS myschema.st ON a FROM t",
        "(create-statistics (name \"myschema\" \"st\") (on (column \"a\")) (from (table (\"t\"))))",
    );
    lowers(
        "ALTER STATISTICS st SET STATISTICS 100",
        "(alter-statistics (name \"st\") (set-statistics 100))",
    );
    lowers(
        "ALTER STATISTICS IF EXISTS st SET STATISTICS 100",
        "(alter-statistics (name \"st\") (if-exists) (set-statistics 100))",
    );
    lowers(
        "ALTER STATISTICS st SET SCHEMA s2",
        "(alter-statistics (name \"st\") (set-schema \"s2\"))",
    );
    lowers(
        "ALTER STATISTICS st OWNER TO me",
        "(alter-statistics (name \"st\") (owner (role \"me\")))",
    );
}

#[test]
fn parses_table_and_index_options() {
    lowers(
        "CREATE TABLE t (a int) WITH (fillfactor = 70)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\"))) (options (option \"fillfactor\" 70)))",
    );
    lowers(
        "ALTER TABLE t SET (fillfactor = 70)",
        "(alter-table (table (\"t\")) (set (option \"fillfactor\" 70)))",
    );
    lowers(
        "ALTER TABLE t RESET (fillfactor)",
        "(alter-table (table (\"t\")) (reset (option \"fillfactor\")))",
    );
    lowers(
        "CREATE INDEX i ON t (a) WITH (fillfactor = 50)",
        "(create-index (name \"i\") (on (table (\"t\"))) (columns (index-column \"a\")) (options (option \"fillfactor\" 50)))",
    );
    lowers(
        "CREATE INDEX i ON t ((a + 1), b COLLATE \"C\" DESC NULLS LAST) \
                     INCLUDE (c) WITH (deduplicate_items = true) TABLESPACE ts WHERE a > 1",
        "(create-index (name \"i\") (on (table (\"t\"))) (columns (index-column (op \"+\" (column \"a\") 1)) (index-column \"b\" (collate \"C\") desc nulls-last)) (include (index-column \"c\")) (options (option \"deduplicate_items\" \"true\")) (tablespace \"ts\") (where (op \">\" (column \"a\") 1)))",
    );
}

#[test]
fn parses_publication_statements() {
    lowers("CREATE PUBLICATION p", "(create-publication \"p\")");
    lowers(
        "CREATE PUBLICATION p FOR ALL TABLES",
        "(create-publication \"p\" (all-tables))",
    );
    lowers(
        "CREATE PUBLICATION p FOR TABLES IN SCHEMA s1, s2",
        "(create-publication \"p\" (tables (schema \"s1\") (schema \"s2\")))",
    );
    lowers(
        "CREATE PUBLICATION p FOR TABLES IN SCHEMA CURRENT_SCHEMA",
        "(create-publication \"p\" (tables (schema current)))",
    );
    lowers(
        "CREATE PUBLICATION p FOR TABLE t1 WHERE (a > 0), t2",
        "(create-publication \"p\" (tables (table (\"t1\") (where (op \">\" (column \"a\") 0))) (table (\"t2\"))))",
    );
    lowers(
        "CREATE PUBLICATION p FOR TABLE sch.t1 (a, b)",
        "(create-publication \"p\" (tables (table (\"sch\" \"t1\") (columns \"a\" \"b\"))))",
    );
    lowers(
        "CREATE PUBLICATION p FOR TABLE ONLY t1",
        "(create-publication \"p\" (tables (table (only true) (\"t1\"))))",
    );
    lowers(
        "CREATE PUBLICATION p WITH (publish = 'insert, update', publish_via_partition_root = true)",
        "(create-publication \"p\" (publish \"insert, update\") (publish-via-partition-root))",
    );
    lowers(
        "CREATE PUBLICATION p WITH (publish_via_partition_root = false)",
        "(create-publication \"p\" (no-publish-via-partition-root))",
    );
    lowers(
        "ALTER PUBLICATION p ADD TABLE t3 WHERE (a > 0)",
        "(alter-publication \"p\" (add (table (\"t3\") (where (op \">\" (column \"a\") 0)))))",
    );
    lowers(
        "ALTER PUBLICATION p ADD TABLES IN SCHEMA s2",
        "(alter-publication \"p\" (add (schema \"s2\")))",
    );
    lowers(
        "ALTER PUBLICATION p SET TABLE t1",
        "(alter-publication \"p\" (set-tables (table (\"t1\"))))",
    );
    lowers(
        "ALTER PUBLICATION p SET TABLES IN SCHEMA s2",
        "(alter-publication \"p\" (set-tables (schema \"s2\")))",
    );
    lowers(
        "ALTER PUBLICATION p DROP TABLE t2",
        "(alter-publication \"p\" (drop (table (\"t2\"))))",
    );
    lowers(
        "ALTER PUBLICATION p DROP TABLES IN SCHEMA s2",
        "(alter-publication \"p\" (drop (schema \"s2\")))",
    );
    lowers(
        "ALTER PUBLICATION p SET (publish = 'none')",
        "(alter-publication \"p\" (publish \"none\"))",
    );
    lowers(
        "ALTER PUBLICATION p SET (publish_via_partition_root = false)",
        "(alter-publication \"p\" (no-publish-via-partition-root))",
    );
    lowers(
        "ALTER PUBLICATION p RENAME TO q",
        "(alter-publication \"p\" (rename \"q\"))",
    );
    lowers(
        "ALTER PUBLICATION p OWNER TO alice",
        "(alter-publication \"p\" (owner (role \"alice\")))",
    );
    lowers("DROP PUBLICATION p", "(drop-publication \"p\")");
    lowers(
        "DROP PUBLICATION IF EXISTS p1, p2 CASCADE",
        "(drop-publication (if-exists) \"p1\" \"p2\" cascade)",
    );
    lowers(
        "CREATE PUBLICATION p FOR TABLE t1 WITH (publish = 'none')",
        "(create-publication \"p\" (publish \"none\") (tables (table (\"t1\"))))",
    );
}

#[test]
fn parses_subscription_statements() {
    lowers(
        "CREATE SUBSCRIPTION s CONNECTION 'host=localhost dbname=d' PUBLICATION p1, p2",
        "(create-subscription \"s\" (connection \"host=localhost dbname=d\") (publications \"p1\" \"p2\"))",
    );
    lowers(
        "CREATE SUBSCRIPTION s CONNECTION 'host=localhost dbname=d' PUBLICATION p WITH (copy_data = false, two_phase = true, slot_name = 'myslot', streaming = 'on', binary = true)",
        "(create-subscription \"s\" (connection \"host=localhost dbname=d\") (publications \"p\") (no-copy-data) (two-phase) (slot-name \"myslot\") (streaming \"on\") (binary))",
    );
    lowers(
        "CREATE SUBSCRIPTION s CONNECTION 'host=localhost' PUBLICATION p WITH (create_slot = false, run_as_owner = true, failover = true)",
        "(create-subscription \"s\" (connection \"host=localhost\") (publications \"p\") (no-create-slot) (run-as-owner) (failover))",
    );
    lowers(
        "CREATE SUBSCRIPTION s CONNECTION 'host=localhost' PUBLICATION p WITH (synchronous_commit = 'local', origin = 'any')",
        "(create-subscription \"s\" (connection \"host=localhost\") (publications \"p\") (synchronous-commit \"local\") (origin \"any\"))",
    );
    lowers(
        "ALTER SUBSCRIPTION s CONNECTION 'host=other dbname=x'",
        "(alter-subscription \"s\" (connection \"host=other dbname=x\"))",
    );
    lowers(
        "ALTER SUBSCRIPTION s SET PUBLICATION p1, p2 WITH (refresh = false)",
        "(alter-subscription \"s\" (publications \"p1\" \"p2\") (no-refresh))",
    );
    lowers(
        "ALTER SUBSCRIPTION s REFRESH PUBLICATION",
        "(alter-subscription \"s\" (refresh))",
    );
    lowers(
        "ALTER SUBSCRIPTION s REFRESH PUBLICATION WITH (copy_data = false)",
        "(alter-subscription \"s\" (refresh) (no-copy-data))",
    );
    lowers(
        "ALTER SUBSCRIPTION s ENABLE",
        "(alter-subscription \"s\" enable)",
    );
    lowers(
        "ALTER SUBSCRIPTION s DISABLE",
        "(alter-subscription \"s\" disable)",
    );
    lowers(
        "ALTER SUBSCRIPTION s SET (slot_name = 'x')",
        "(alter-subscription \"s\" (set (slot-name \"x\")))",
    );
    lowers(
        "ALTER SUBSCRIPTION s SKIP (lsn = '0/1234567')",
        "(alter-subscription \"s\" (skip (lsn \"0/1234567\")))",
    );
    lowers(
        "ALTER SUBSCRIPTION s RENAME TO t",
        "(alter-subscription \"s\" (rename \"t\"))",
    );
    lowers(
        "ALTER SUBSCRIPTION s OWNER TO alice",
        "(alter-subscription \"s\" (owner (role \"alice\")))",
    );
    lowers("DROP SUBSCRIPTION s", "(drop-subscription \"s\")");
    lowers(
        "DROP SUBSCRIPTION IF EXISTS s CASCADE",
        "(drop-subscription (if-exists) \"s\" cascade)",
    );
}

#[test]
fn parses_policy_statements() {
    lowers(
        "CREATE POLICY p ON t USING (a > 0)",
        "(create-policy \"p\" (on (table (\"t\"))) (using (op \">\" (column \"a\") 0)))",
    );
    lowers(
        "CREATE POLICY p ON t AS PERMISSIVE FOR SELECT TO alice, public \
                     USING (a > 0) WITH CHECK (b < 5)",
        "(create-policy \"p\" (on (table (\"t\"))) (for select) (to (role \"alice\") (role public)) (using (op \">\" (column \"a\") 0)) (with-check (op \"<\" (column \"b\") 5)))",
    );
    lowers(
        "CREATE POLICY p ON t FOR INSERT TO app WITH CHECK (true)",
        "(create-policy \"p\" (on (table (\"t\"))) (for insert) (to (role \"app\")) (with-check true))",
    );
    lowers(
        "CREATE POLICY p ON t AS RESTRICTIVE FOR UPDATE TO current_user",
        "(create-policy \"p\" (on (table (\"t\"))) (for update) (as restrictive) (to (role current-user)))",
    );
    lowers(
        "CREATE POLICY p ON t FOR DELETE",
        "(create-policy \"p\" (on (table (\"t\"))) (for delete))",
    );
    lowers(
        "CREATE POLICY p ON public.t USING (t.a > 0)",
        "(create-policy \"p\" (on (table (\"public\" \"t\"))) (using (op \">\" (column \"t\" \"a\") 0)))",
    );
    lowers(
        "ALTER POLICY p ON t TO bob USING (a = 1) WITH CHECK (a = 2)",
        "(alter-policy \"p\" (on (table (\"t\"))) (to (role \"bob\")) (using (op \"=\" (column \"a\") 1)) (with-check (op \"=\" (column \"a\") 2)))",
    );
    lowers(
        "ALTER POLICY p ON t",
        "(alter-policy \"p\" (on (table (\"t\"))))",
    );
    lowers(
        "ALTER POLICY p ON t RENAME TO p2",
        "(rename-policy \"p\" (on (table (\"t\"))) \"p2\")",
    );
    lowers(
        "ALTER TABLE t ENABLE ROW LEVEL SECURITY",
        "(alter-table (table (\"t\")) (enable-row-level-security))",
    );
    lowers(
        "ALTER TABLE t DISABLE ROW LEVEL SECURITY",
        "(alter-table (table (\"t\")) (disable-row-level-security))",
    );
    lowers(
        "ALTER TABLE t FORCE ROW LEVEL SECURITY",
        "(alter-table (table (\"t\")) (force-row-level-security))",
    );
    lowers(
        "ALTER TABLE t NO FORCE ROW LEVEL SECURITY",
        "(alter-table (table (\"t\")) (no-force-row-level-security))",
    );
    lowers(
        "DROP POLICY p ON t",
        "(drop-policy \"p\" (on (table (\"t\"))))",
    );
    lowers(
        "DROP POLICY IF EXISTS p ON t CASCADE",
        "(drop-policy (if-exists) \"p\" (on (table (\"t\"))) cascade)",
    );
    lowers(
        "DROP POLICY p ON public.t",
        "(drop-policy \"p\" (on (table (\"public\" \"t\"))))",
    );
}

#[test]
fn parses_rule_statements() {
    lowers(
        "CREATE RULE r AS ON INSERT TO t DO NOTHING",
        "(create-rule \"r\" on-insert (to (table (\"t\"))) (nothing))",
    );
    lowers(
        "CREATE RULE r AS ON INSERT TO t DO INSTEAD INSERT INTO t2 VALUES (1)",
        "(create-rule \"r\" on-insert (to (table (\"t\"))) instead (actions (insert (into (table (\"t2\"))) (columns) (source (values (row 1))))))",
    );
    lowers(
        "CREATE RULE r AS ON INSERT TO t DO ALSO NOTIFY x",
        "(create-rule \"r\" on-insert (to (table (\"t\"))) (actions (notify \"x\")))",
    );
    lowers(
        "CREATE RULE r AS ON INSERT TO t DO INSTEAD \
                     (INSERT INTO t2 VALUES (1); DELETE FROM t2)",
        "(create-rule \"r\" on-insert (to (table (\"t\"))) instead (actions (insert (into (table (\"t2\"))) (columns) (source (values (row 1)))) (delete (from (table (\"t2\"))))))",
    );
    lowers(
        "CREATE RULE r AS ON DELETE TO t WHERE a > 5 \
                     DO INSTEAD DELETE FROM t2",
        "(create-rule \"r\" on-delete (to (table (\"t\"))) instead (when (op \">\" (column \"a\") 5)) (actions (delete (from (table (\"t2\"))))))",
    );
    lowers(
        "CREATE OR REPLACE RULE r AS ON SELECT TO t DO INSTEAD SELECT * FROM t2",
        "(create-rule \"r\" or-replace on-select (to (table (\"t\"))) instead (actions (select (columns (column *)) (from (table (\"t2\"))))))",
    );
    lowers(
        "DROP RULE IF EXISTS r ON t",
        "(drop-rule (if-exists) \"r\" (on (table (\"t\"))))",
    );
}

#[test]
fn parses_create_table_inherits() {
    lowers(
        "CREATE TABLE t2 (i int) INHERITS (t1);",
        "(create-table (table (\"t2\")) (columns (column \"i\" (type \"pg_catalog\" \"int4\"))) (inherits (table (\"t1\"))))",
    );
}

#[test]
fn parses_create_table_of_type() {
    lowers(
        "CREATE TABLE t OF person_type;",
        "(create-table (table (\"t\")) (of (type \"person_type\")) (columns))",
    );
    lowers(
        "CREATE TABLE t OF schema.person_type;",
        "(create-table (table (\"t\")) (of (type \"schema\" \"person_type\")) (columns))",
    );
}

#[test]
fn parses_create_table_on_commit() {
    lowers(
        "CREATE TEMP TABLE t (i int) ON COMMIT DROP;",
        "(create-table (persistence temporary) (table (\"t\")) (columns (column \"i\" (type \"pg_catalog\" \"int4\"))) (on-commit drop))",
    );
    lowers(
        "CREATE TABLE t (i int) ON COMMIT DELETE ROWS;",
        "(create-table (table (\"t\")) (columns (column \"i\" (type \"pg_catalog\" \"int4\"))) (on-commit delete-rows))",
    );
    lowers(
        "CREATE TABLE t (i int) ON COMMIT PRESERVE ROWS;",
        "(create-table (table (\"t\")) (columns (column \"i\" (type \"pg_catalog\" \"int4\"))) (on-commit preserve-rows))",
    );
}

#[test]
fn parses_create_table_tablespace() {
    lowers(
        "CREATE TABLE t (i int) TABLESPACE ts;",
        "(create-table (table (\"t\")) (columns (column \"i\" (type \"pg_catalog\" \"int4\"))) (tablespace \"ts\"))",
    );
}

#[test]
fn parses_create_table_using() {
    lowers(
        "CREATE TABLE t (i int) USING heap;",
        "(create-table (table (\"t\")) (columns (column \"i\" (type \"pg_catalog\" \"int4\"))) (using \"heap\"))",
    );
}

#[test]
fn parses_create_view_options() {
    lowers(
        "CREATE VIEW v WITH (security_barrier) AS SELECT 1;",
        "(create-view (table (\"v\")) (query (select (columns 1))) (options (option \"security_barrier\")))",
    );
    lowers(
        "CREATE VIEW v WITH (check_option = local) AS SELECT 1;",
        "(create-view (table (\"v\")) (query (select (columns 1))) (options (option \"check_option\" \"local\")))",
    );
    lowers(
        "CREATE VIEW v WITH (security_invoker = true) AS SELECT 1;",
        "(create-view (table (\"v\")) (query (select (columns 1))) (options (option \"security_invoker\" \"true\")))",
    );
}

#[test]
fn parses_create_view_persistence() {
    lowers(
        "CREATE TEMP VIEW v AS SELECT 1;",
        "(create-view (persistence temporary) (table (\"v\")) (query (select (columns 1))))",
    );
    lowers(
        "CREATE OR REPLACE UNLOGGED VIEW app.v AS SELECT 1;",
        "(create-view or-replace (persistence unlogged) (table (\"app\" \"v\")) (query (select (columns 1))))",
    );
}

#[test]
fn parses_recursive_view() {
    lowers(
        "CREATE RECURSIVE VIEW v (n) AS SELECT 1;",
        "(create-view recursive (table (\"v\")) (columns \"n\") (query (select (columns 1))))",
    );
    lowers(
        "CREATE OR REPLACE RECURSIVE VIEW app.v (a, b) AS SELECT 1, 2;",
        "(create-view or-replace recursive (table (\"app\" \"v\")) (columns \"a\" \"b\") (query (select (columns 1 2))))",
    );
    lowers(
        "CREATE RECURSIVE VIEW v (n) WITH (security_barrier) AS SELECT 1;",
        "(create-view recursive (table (\"v\")) (columns \"n\") (query (select (columns 1))) (options (option \"security_barrier\")))",
    );
}

#[test]
fn parses_recursive_view_query_as_written() {
    lowers(
        "CREATE RECURSIVE VIEW v (n) AS WITH RECURSIVE c AS (SELECT 1) SELECT * FROM c;",
        "(create-view recursive (table (\"v\")) (columns \"n\") (query (with recursive (cte \"c\" (select (columns 1))) (select (columns (column *)) (from (table (\"c\")))))))",
    );
}

#[test]
fn parses_self_named_recursive_cte_view_without_recursive_flag() {
    lowers(
        "CREATE VIEW v (n) AS WITH RECURSIVE v (n) AS (SELECT 1) SELECT n FROM v;",
        "(create-view (table (\"v\")) (columns \"n\") (query (with recursive (cte \"v\" (select (columns 1)) (columns \"n\")) (select (columns (column \"n\")) (from (table (\"v\")))))))",
    );
}

#[test]
fn parses_create_materialized_view_options() {
    lowers(
        "CREATE MATERIALIZED VIEW m WITH (fillfactor = 70) AS SELECT 1;",
        "(create-materialized-view (table (\"m\")) (options (option \"fillfactor\" 70)) (query (select (columns 1))))",
    );
    lowers(
        "CREATE MATERIALIZED VIEW m TABLESPACE ts AS SELECT 1;",
        "(create-materialized-view (table (\"m\")) (tablespace \"ts\") (query (select (columns 1))))",
    );
    lowers(
        "CREATE MATERIALIZED VIEW m USING heap AS SELECT 1;",
        "(create-materialized-view (table (\"m\")) (using \"heap\") (query (select (columns 1))))",
    );
}

#[test]
fn parses_create_table_as() {
    lowers(
        "CREATE TABLE t AS SELECT 1;",
        "(create-table-as (table (\"t\")) (query (select (columns 1))))",
    );
    lowers(
        "CREATE TABLE IF NOT EXISTS t (a, b) AS SELECT 1, 2 WITH NO DATA;",
        "(create-table-as (if-not-exists) (table (\"t\")) (columns \"a\" \"b\") (query (select (columns 1 2))) with-no-data)",
    );
    lowers(
        "CREATE TEMP TABLE t AS SELECT 1;",
        "(create-table-as (persistence temporary) (table (\"t\")) (query (select (columns 1))))",
    );
}

#[test]
fn parses_create_schema_authorization() {
    lowers(
        "CREATE SCHEMA AUTHORIZATION alice;",
        "(create-schema (authorization (role \"alice\")))",
    );
    lowers(
        "CREATE SCHEMA s AUTHORIZATION alice;",
        "(create-schema \"s\" (authorization (role \"alice\")))",
    );
}

#[test]
fn parses_create_schema_elements() {
    lowers(
        "CREATE SCHEMA s CREATE TABLE t2 (i int);",
        "(create-schema \"s\" (elements (create-table (table (\"t2\")) (columns (column \"i\" (type \"pg_catalog\" \"int4\"))))))",
    );
    lowers(
        "CREATE SCHEMA s CREATE VIEW v AS SELECT 1;",
        "(create-schema \"s\" (elements (create-view (table (\"v\")) (query (select (columns 1))))))",
    );
    lowers(
        "CREATE SCHEMA AUTHORIZATION alice CREATE TABLE t2 (i int);",
        "(create-schema (authorization (role \"alice\")) (elements (create-table (table (\"t2\")) (columns (column \"i\" (type \"pg_catalog\" \"int4\"))))))",
    );
}

#[test]
fn parses_sequence_type() {
    lowers(
        "CREATE SEQUENCE s AS int;",
        "(create-sequence (table (\"s\")) (options (option \"as\" (type \"pg_catalog\" \"int4\"))))",
    );
}

#[test]
fn parses_domain_collation() {
    lowers(
        "CREATE DOMAIN d AS text COLLATE \"C\";",
        "(create-domain (name \"d\") (type \"text\") (collate \"C\") (constraints))",
    );
    lowers(
        "CREATE DOMAIN d AS text COLLATE pg_catalog.\"C\";",
        "(create-domain (name \"d\") (type \"text\") (collate \"pg_catalog.C\") (constraints))",
    );
}

#[test]
fn parses_alter_enum_empty_values() {
    lowers(
        "ALTER TYPE e ADD VALUE '';",
        "(alter-enum (name \"e\") (add-value \"\"))",
    );
    lowers(
        "ALTER TYPE e RENAME VALUE 'a' TO '';",
        "(alter-enum (name \"e\") (rename-value \"a\" \"\"))",
    );
}

#[test]
fn parses_partition_by_collate_opclass() {
    lowers(
        "CREATE TABLE t (x int) PARTITION BY RANGE (x COLLATE \"C\");",
        "(create-table (table (\"t\")) (columns (column \"x\" (type \"pg_catalog\" \"int4\"))) (partition-by range (columns (partition-column \"x\" (collate \"C\")))))",
    );
    lowers(
        "CREATE TABLE t (x int) PARTITION BY LIST (x int_ops);",
        "(create-table (table (\"t\")) (columns (column \"x\" (type \"pg_catalog\" \"int4\"))) (partition-by list (columns (partition-column \"x\" (opclass \"int_ops\")))))",
    );
    lowers(
        "CREATE TABLE t (x int) PARTITION BY LIST (x public.int_ops);",
        "(create-table (table (\"t\")) (columns (column \"x\" (type \"pg_catalog\" \"int4\"))) (partition-by list (columns (partition-column \"x\" (opclass \"public\" \"int_ops\")))))",
    );
}

#[test]
fn parses_alter_non_table_options() {
    lowers(
        "ALTER VIEW v SET (fillfactor = 70);",
        "(alter-view (table (\"v\")) (set (option \"fillfactor\" 70)))",
    );
    lowers(
        "ALTER VIEW v RESET (fillfactor);",
        "(alter-view (table (\"v\")) (reset (option \"fillfactor\")))",
    );
    lowers(
        "ALTER INDEX i SET (fillfactor = 90);",
        "(alter-index (table (\"i\")) (set (option \"fillfactor\" 90)))",
    );
    lowers(
        "ALTER MATERIALIZED VIEW m SET (fillfactor = 90);",
        "(alter-materialized-view (table (\"m\")) (set (option \"fillfactor\" 90)))",
    );
}

#[test]
fn parses_alter_non_table_owner_tablespace() {
    lowers(
        "ALTER VIEW v OWNER TO alice;",
        "(alter-view (table (\"v\")) (owner (role \"alice\")))",
    );
    lowers(
        "ALTER VIEW v OWNER TO CURRENT_USER;",
        "(alter-view (table (\"v\")) (owner (role current-user)))",
    );
    lowers(
        "ALTER INDEX i SET TABLESPACE ts;",
        "(alter-index (table (\"i\")) (set-tablespace \"ts\"))",
    );
}

#[test]
fn parses_drop_index_concurrently() {
    lowers(
        "DROP INDEX CONCURRENTLY i;",
        "(drop-index (name \"i\") concurrently)",
    );
    lowers(
        "DROP INDEX CONCURRENTLY IF EXISTS i;",
        "(drop-index (if-exists) (name \"i\") concurrently)",
    );
    lowers(
        "DROP INDEX CONCURRENTLY i CASCADE;",
        "(drop-index (name \"i\") concurrently cascade)",
    );
}

#[test]
fn lowers_alter_table_column_actions() {
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET STATISTICS 100",
        "(alter-table (table (\"t\")) (set-statistics \"c\" 100))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET STATISTICS DEFAULT",
        "(alter-table (table (\"t\")) (set-statistics \"c\" default))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET STORAGE EXTERNAL",
        "(alter-table (table (\"t\")) (set-storage \"c\" external))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET COMPRESSION lz4",
        "(alter-table (table (\"t\")) (set-compression \"c\" lz4))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET (n_distinct = 100)",
        "(alter-table (table (\"t\")) (set-options \"c\" (option \"n_distinct\" 100)))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c RESET (n_distinct)",
        "(alter-table (table (\"t\")) (reset-options \"c\" (option \"n_distinct\")))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c OPTIONS (x 'y', SET z 'w', ADD q 'r', DROP p)",
        "(alter-table (table (\"t\")) (alter-column-options \"c\" (option \"x\" \"y\") (set-option \"z\" \"w\") (add-option \"q\" \"r\") (drop-option \"p\")))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c DROP IDENTITY",
        "(alter-table (table (\"t\")) (drop-identity \"c\"))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c DROP IDENTITY IF EXISTS",
        "(alter-table (table (\"t\")) (drop-identity \"c\" if-exists))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET EXPRESSION AS (a + 1)",
        "(alter-table (table (\"t\")) (set-expression \"c\" (op \"+\" (column \"a\") 1)))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c DROP EXPRESSION",
        "(alter-table (table (\"t\")) (drop-expression \"c\"))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c DROP EXPRESSION IF EXISTS",
        "(alter-table (table (\"t\")) (drop-expression \"c\" if-exists))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c ADD GENERATED ALWAYS AS IDENTITY",
        "(alter-table (table (\"t\")) (add-identity \"c\" always))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c ADD GENERATED BY DEFAULT AS IDENTITY (START WITH 5 INCREMENT BY 2)",
        "(alter-table (table (\"t\")) (add-identity \"c\" by-default (options (option \"start\" 5) (option \"increment\" 2))))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET GENERATED ALWAYS",
        "(alter-table (table (\"t\")) (set-identity \"c\" always))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET GENERATED BY DEFAULT",
        "(alter-table (table (\"t\")) (set-identity \"c\" by-default))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET START WITH 10",
        "(alter-table (table (\"t\")) (set-identity \"c\" (option \"start\" 10)))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c RESTART",
        "(alter-table (table (\"t\")) (set-identity \"c\" (restart)))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c RESTART WITH 42",
        "(alter-table (table (\"t\")) (set-identity \"c\" (restart 42)))",
    );
}

#[test]
fn lowers_alter_table_relation_actions() {
    lowers(
        "ALTER TABLE t VALIDATE CONSTRAINT c",
        "(alter-table (table (\"t\")) (validate-constraint \"c\"))",
    );
    lowers(
        "ALTER TABLE t REPLICA IDENTITY FULL",
        "(alter-table (table (\"t\")) (replica-identity full))",
    );
    lowers(
        "ALTER TABLE t REPLICA IDENTITY NOTHING",
        "(alter-table (table (\"t\")) (replica-identity nothing))",
    );
    lowers(
        "ALTER TABLE t REPLICA IDENTITY DEFAULT",
        "(alter-table (table (\"t\")) (replica-identity default))",
    );
    lowers(
        "ALTER TABLE t REPLICA IDENTITY USING INDEX i",
        "(alter-table (table (\"t\")) (replica-identity (using-index \"i\")))",
    );
    lowers(
        "ALTER TABLE t INHERIT p",
        "(alter-table (table (\"t\")) (inherit (table (\"p\"))))",
    );
    lowers(
        "ALTER TABLE t NO INHERIT p",
        "(alter-table (table (\"t\")) (no-inherit (table (\"p\"))))",
    );
    lowers(
        "ALTER TABLE t OF ty",
        "(alter-table (table (\"t\")) (of (type \"ty\")))",
    );
    lowers(
        "ALTER TABLE t NOT OF",
        "(alter-table (table (\"t\")) (not-of))",
    );
    lowers(
        "ALTER TABLE t SET ACCESS METHOD heap",
        "(alter-table (table (\"t\")) (set-access-method \"heap\"))",
    );
    lowers(
        "ALTER TABLE t SET ACCESS METHOD DEFAULT",
        "(alter-table (table (\"t\")) (set-access-method default))",
    );
    lowers(
        "ALTER TABLE t CLUSTER ON i",
        "(alter-table (table (\"t\")) (cluster-on \"i\"))",
    );
    lowers(
        "ALTER TABLE t SET WITHOUT CLUSTER",
        "(alter-table (table (\"t\")) (set-without-cluster))",
    );
    lowers(
        "ALTER TABLE t SET WITHOUT OIDS",
        "(alter-table (table (\"t\")) (set-without-oids))",
    );
    lowers(
        "ALTER TABLE t OPTIONS (fillfactor '70')",
        "(alter-table (table (\"t\")) (options (option \"fillfactor\" \"70\")))",
    );
    lowers(
        "ALTER TABLE t ALTER CONSTRAINT c DEFERRABLE INITIALLY DEFERRED",
        "(alter-table (table (\"t\")) (alter-constraint \"c\" deferrable initially-deferred))",
    );
    lowers(
        "ALTER TABLE t ALTER CONSTRAINT c NOT DEFERRABLE",
        "(alter-table (table (\"t\")) (alter-constraint \"c\"))",
    );
}

#[test]
fn lowers_alter_table_trigger_and_rule_actions() {
    lowers(
        "ALTER TABLE t ENABLE TRIGGER tr",
        "(alter-table (table (\"t\")) (enable-trigger \"tr\"))",
    );
    lowers(
        "ALTER TABLE t ENABLE ALWAYS TRIGGER tr",
        "(alter-table (table (\"t\")) (enable-always-trigger \"tr\"))",
    );
    lowers(
        "ALTER TABLE t ENABLE REPLICA TRIGGER tr",
        "(alter-table (table (\"t\")) (enable-replica-trigger \"tr\"))",
    );
    lowers(
        "ALTER TABLE t DISABLE TRIGGER tr",
        "(alter-table (table (\"t\")) (disable-trigger \"tr\"))",
    );
    lowers(
        "ALTER TABLE t ENABLE TRIGGER ALL",
        "(alter-table (table (\"t\")) (enable-trigger-all))",
    );
    lowers(
        "ALTER TABLE t DISABLE TRIGGER ALL",
        "(alter-table (table (\"t\")) (disable-trigger-all))",
    );
    lowers(
        "ALTER TABLE t ENABLE TRIGGER USER",
        "(alter-table (table (\"t\")) (enable-trigger-user))",
    );
    lowers(
        "ALTER TABLE t DISABLE TRIGGER USER",
        "(alter-table (table (\"t\")) (disable-trigger-user))",
    );
    lowers(
        "ALTER TABLE t ENABLE RULE r",
        "(alter-table (table (\"t\")) (enable-rule \"r\"))",
    );
    lowers(
        "ALTER TABLE t ENABLE ALWAYS RULE r",
        "(alter-table (table (\"t\")) (enable-always-rule \"r\"))",
    );
    lowers(
        "ALTER TABLE t ENABLE REPLICA RULE r",
        "(alter-table (table (\"t\")) (enable-replica-rule \"r\"))",
    );
    lowers(
        "ALTER TABLE t DISABLE RULE r",
        "(alter-table (table (\"t\")) (disable-rule \"r\"))",
    );
}

#[test]
fn lowers_generated_and_identity_constraints() {
    lowers(
        "CREATE TABLE t (a int NULL)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (null))))",
    );
    lowers(
        "CREATE TABLE t (a int GENERATED ALWAYS AS IDENTITY)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (identity always))))",
    );
    lowers(
        "CREATE TABLE t (a int GENERATED BY DEFAULT AS IDENTITY (START WITH 5 INCREMENT BY 2))",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (identity by-default (options (option \"start\" 5) (option \"increment\" 2))))))",
    );
    lowers(
        "CREATE TABLE t (a int GENERATED ALWAYS AS (a * 2) STORED)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (generated always (op \"*\" (column \"a\") 2) stored))))",
    );
    lowers(
        "CREATE TABLE t (a int GENERATED ALWAYS AS (a * 2))",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (generated always (op \"*\" (column \"a\") 2) virtual))))",
    );
    lowers(
        "CREATE TABLE t (a int GENERATED ALWAYS AS (a * 2) VIRTUAL)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (generated always (op \"*\" (column \"a\") 2) virtual))))",
    );
    lowers(
        "ALTER TABLE t ADD COLUMN c int GENERATED ALWAYS AS (a + 1) STORED",
        "(alter-table (table (\"t\")) (add-column (column \"c\" (type \"pg_catalog\" \"int4\") (generated always (op \"+\" (column \"a\") 1) stored))))",
    );
    lowers(
        "ALTER TABLE t ADD COLUMN c int GENERATED ALWAYS AS IDENTITY",
        "(alter-table (table (\"t\")) (add-column (column \"c\" (type \"pg_catalog\" \"int4\") (identity always))))",
    );
}

#[test]
fn lowers_exclusion_constraints() {
    lowers(
        "CREATE TABLE t (a int, EXCLUDE USING gist (a WITH =))",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\")) (exclude (using \"gist\") (element (index-column \"a\") (operators \"=\")))))",
    );
    lowers(
        "CREATE TABLE t (a int, b int, EXCLUDE USING gist (a WITH =, b WITH <>) WHERE (a > 0))",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\")) (column \"b\" (type \"pg_catalog\" \"int4\")) (exclude (using \"gist\") (element (index-column \"a\") (operators \"=\")) (element (index-column \"b\") (operators \"<>\")) (where (op \">\" (column \"a\") 0)))))",
    );
    lowers(
        "CREATE TABLE t (a int, CONSTRAINT e EXCLUDE USING gist (a WITH =))",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\")) (exclude (using \"gist\") (element (index-column \"a\") (operators \"=\")) (name \"e\"))))",
    );
    lowers(
        "CREATE TABLE t (a int, EXCLUDE USING gist (a WITH =) DEFERRABLE INITIALLY DEFERRED)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\")) (exclude (using \"gist\") (element (index-column \"a\") (operators \"=\")) deferrable initially-deferred)))",
    );
    lowers(
        "ALTER TABLE t ADD CONSTRAINT c EXCLUDE USING gist (a WITH =)",
        "(alter-table (table (\"t\")) (add-constraint (exclude (using \"gist\") (element (index-column \"a\") (operators \"=\")) (name \"c\"))))",
    );
}

#[test]
fn folds_column_deferrability_into_their_constraint() {
    lowers(
        "CREATE TABLE t (a int UNIQUE DEFERRABLE)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (unique deferrable))))",
    );
    lowers(
        "CREATE TABLE t (a int PRIMARY KEY DEFERRABLE INITIALLY DEFERRED)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (primary-key deferrable initially-deferred))))",
    );
    lowers(
        "CREATE TABLE t (a int CONSTRAINT c UNIQUE NOT DEFERRABLE INITIALLY IMMEDIATE)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (unique not-deferrable initially-immediate (name \"c\")))))",
    );
    lowers(
        "CREATE TABLE t (a int UNIQUE INITIALLY DEFERRED)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (unique initially-deferred))))",
    );
    lowers(
        "CREATE TABLE t (a int NOT NULL DEFERRABLE)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (not-null deferrable))))",
    );
    lowers(
        "CREATE TABLE t (a int REFERENCES u (id) DEFERRABLE INITIALLY DEFERRED)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (foreign-key (columns) (references (table (\"u\")) (columns \"id\")) deferrable initially-deferred))))",
    );
    lowers(
        "CREATE TABLE t (a int, CONSTRAINT c UNIQUE (a) DEFERRABLE INITIALLY DEFERRED)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\")) (unique \"a\" deferrable initially-deferred (name \"c\"))))",
    );
}

#[test]
fn lowers_view_check_option() {
    lowers(
        "CREATE VIEW v AS SELECT 1 WITH LOCAL CHECK OPTION",
        "(create-view (table (\"v\")) (query (select (columns 1))) (check-option local))",
    );
    lowers(
        "CREATE VIEW v AS SELECT 1 WITH CASCADED CHECK OPTION",
        "(create-view (table (\"v\")) (query (select (columns 1))) (check-option cascaded))",
    );
    lowers(
        "CREATE VIEW v AS SELECT 1 WITH CHECK OPTION",
        "(create-view (table (\"v\")) (query (select (columns 1))) (check-option cascaded))",
    );
}

#[test]
fn lowers_alter_subscription_publications() {
    lowers(
        "ALTER SUBSCRIPTION s ADD PUBLICATION p1, p2",
        "(alter-subscription \"s\" (add-publications \"p1\" \"p2\"))",
    );
    lowers(
        "ALTER SUBSCRIPTION s DROP PUBLICATION p1",
        "(alter-subscription \"s\" (drop-publications \"p1\"))",
    );
}

#[test]
fn lowers_drop_object_types() {
    lowers("DROP COLLATION c", "(drop-collation (name \"c\"))");
    lowers(
        "DROP STATISTICS st1, st2",
        "(drop-statistics (name \"st1\") (name \"st2\"))",
    );
    lowers(
        "DROP TEXT SEARCH CONFIGURATION tsc",
        "(drop-text-search-configuration (name \"tsc\"))",
    );
    lowers(
        "DROP FOREIGN TABLE ft",
        "(drop-foreign-table (name \"ft\"))",
    );
    lowers(
        "DROP ACCESS METHOD am",
        "(drop-access-method (name \"am\"))",
    );
    lowers(
        "DROP EVENT TRIGGER et",
        "(drop-event-trigger (name \"et\"))",
    );
    lowers(
        "DROP FOREIGN DATA WRAPPER fdw",
        "(drop-foreign-data-wrapper (name \"fdw\"))",
    );
    lowers("DROP SERVER s", "(drop-server (name \"s\"))");
    lowers("DROP LANGUAGE l", "(drop-language (name \"l\"))");
    lowers(
        "DROP TRIGGER tr ON t",
        "(drop-trigger \"tr\" (on (table (\"t\"))))",
    );
    lowers(
        "DROP OPERATOR CLASS public.oc USING btree CASCADE",
        "(drop-operator-class (name \"public\" \"oc\") (using \"btree\") cascade)",
    );
    lowers(
        "DROP CAST (int AS text)",
        "(drop-cast (source (name \"pg_catalog\" \"int4\")) (target (name \"text\")))",
    );
    lowers(
        "DROP TRANSFORM FOR int LANGUAGE plpgsql",
        "(drop-transform (type (name \"pg_catalog\" \"int4\")) (language \"plpgsql\"))",
    );
    lowers("DROP TYPE t", "(drop-type (name \"t\"))");
    lowers("DROP DOMAIN d", "(drop-domain (name \"d\"))");
}

#[test]
fn lowers_comment_object_types() {
    lowers(
        "COMMENT ON OPERATOR CLASS sch.x USING y IS 'z'",
        "(comment-on-operator-class (name \"sch\" \"x\") (using \"y\") \"z\")",
    );
    lowers(
        "COMMENT ON TRANSFORM FOR int LANGUAGE sql IS 'z'",
        "(comment-on-transform (type (name \"pg_catalog\" \"int4\")) (language \"sql\") \"z\")",
    );
    lowers(
        "COMMENT ON CONSTRAINT c ON DOMAIN sch.d IS NULL",
        "(comment-on-domain-constraint (name \"sch\" \"d\" \"c\") drop)",
    );
}

#[test]
fn lowers_like_and_create_table_as_options() {
    lowers(
        "CREATE TABLE t (LIKE u INCLUDING COMPRESSION)",
        "(create-table (table (\"t\")) (columns (like (table (\"u\")) compression)))",
    );
    lowers(
        "CREATE TEMPORARY TABLE t ON COMMIT DROP AS SELECT 1",
        "(create-table-as (persistence temporary) (table (\"t\")) (on-commit drop) (query (select (columns 1))))",
    );
    lowers(
        "ALTER TABLE t RENAME CONSTRAINT c TO d",
        "(alter-table (table (\"t\")) (rename-constraint \"c\" \"d\"))",
    );
}

#[test]
fn rejects_unsupported_exclusion_constraint_features() {
    let error = parse("CREATE TABLE t (a int, EXCLUDE USING gist (a WITH =) WITH (fillfactor=70))")
        .expect_err("expected EXCLUDE constraint to be rejected");
    assert_eq!(
        error.to_string(),
        "unsupported PostgreSQL AST: EXCLUDE constraint at byte 0",
        "unexpected error for WITH (fillfactor=70)"
    );
}

#[test]
fn parses_add_column_if_not_exists() {
    lowers(
        "ALTER TABLE t ADD COLUMN IF NOT EXISTS c int",
        "(alter-table (table (\"t\")) (add-column (column \"c\" (type \"pg_catalog\" \"int4\")) (if-not-exists)))",
    );
    lowers(
        "ALTER TABLE t ADD COLUMN c int",
        "(alter-table (table (\"t\")) (add-column (column \"c\" (type \"pg_catalog\" \"int4\"))))",
    );
}

#[test]
fn parses_alter_column_type() {
    lowers(
        "ALTER TABLE t ALTER COLUMN c TYPE int",
        "(alter-table (table (\"t\")) (alter-column-type \"c\" (type \"pg_catalog\" \"int4\")))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c TYPE text USING c::text",
        "(alter-table (table (\"t\")) (alter-column-type \"c\" (type \"text\") (using (cast (column \"c\") (type \"text\")))))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c TYPE text COLLATE \"C\"",
        "(alter-table (table (\"t\")) (alter-column-type \"c\" (type \"text\") (collate \"C\")))",
    );
}

#[test]
fn parses_column_collation_compression_and_storage() {
    lowers(
        "CREATE TABLE t (c text COLLATE \"C\")",
        "(create-table (table (\"t\")) (columns (column \"c\" (type \"text\") (collate \"C\"))))",
    );
    lowers(
        "CREATE TABLE t (c text COMPRESSION lz4)",
        "(create-table (table (\"t\")) (columns (column \"c\" (type \"text\") (compression \"lz4\"))))",
    );
    lowers(
        "CREATE TABLE t (c int STORAGE PLAIN)",
        "(create-table (table (\"t\")) (columns (column \"c\" (type \"pg_catalog\" \"int4\") (storage \"plain\"))))",
    );
}

#[test]
fn lowers_exclusion_constraint_include() {
    lowers(
        "CREATE TABLE t (a int, b int, EXCLUDE USING gist (a WITH =) INCLUDE (b))",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\")) (column \"b\" (type \"pg_catalog\" \"int4\")) (exclude (using \"gist\") (element (index-column \"a\") (operators \"=\")) (include \"b\"))))",
    );
}

#[test]
fn folds_leading_constraint_attributes_standalone() {
    lowers(
        "CREATE TABLE t (a int DEFERRABLE)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (deferrable))))",
    );
    lowers(
        "CREATE TABLE t (a int NOT NULL DEFERRABLE)",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (not-null deferrable))))",
    );
    lowers(
        "CREATE TABLE t (c text COLLATE \"C\" DEFERRABLE)",
        "(create-table (table (\"t\")) (columns (column \"c\" (type \"text\") (collate \"C\") (deferrable))))",
    );
    lowers(
        "CREATE TABLE t (c text COLLATE \"C\" NOT NULL DEFERRABLE)",
        "(create-table (table (\"t\")) (columns (column \"c\" (type \"text\") (collate \"C\") (not-null deferrable))))",
    );
}

#[test]
fn parses_comment_on_role() {
    lowers(
        "COMMENT ON ROLE r IS 'x'",
        "(comment-on-role (name \"r\") \"x\")",
    );
    lowers(
        "COMMENT ON ROLE r IS NULL",
        "(comment-on-role (name \"r\") drop)",
    );
}

#[test]
fn parses_alter_column_set_statistics_by_number() {
    lowers(
        "ALTER TABLE t ALTER COLUMN 1 SET STATISTICS 100",
        "(alter-table (table (\"t\")) (set-statistics 1 100))",
    );
    lowers(
        "ALTER TABLE t ALTER COLUMN c SET STATISTICS 100",
        "(alter-table (table (\"t\")) (set-statistics \"c\" 100))",
    );
}

#[test]
fn lowers_alter_trigger_rename() {
    lowers(
        "ALTER TRIGGER \"ParentTrig\" ON parent RENAME TO anothertrig",
        "(rename-trigger \"ParentTrig\" (on (table (\"parent\"))) (rename \"anothertrig\"))",
    );
}

#[test]
fn lowers_alter_rule_rename() {
    lowers(
        "ALTER RULE \"InsertRule\" ON rule_v1 RENAME TO \"NewInsertRule\"",
        "(rename-rule \"InsertRule\" (on (table (\"rule_v1\"))) (rename \"NewInsertRule\"))",
    );
}

#[test]
fn lowers_alter_event_trigger_rename() {
    lowers(
        "ALTER EVENT TRIGGER regress_event_trigger RENAME TO regress_event_trigger3",
        "(alter-event-trigger (name \"regress_event_trigger\") (rename \"regress_event_trigger3\"))",
    );
}

#[test]
fn lowers_alter_domain_rename_constraint() {
    lowers(
        "ALTER DOMAIN testdomain1 RENAME CONSTRAINT unsigned TO unsigned_foo",
        "(alter-domain (name \"testdomain1\") (rename-constraint \"unsigned\" \"unsigned_foo\"))",
    );
}

#[test]
fn lowers_type_attribute_renames() {
    lowers(
        "ALTER TYPE test_type RENAME ATTRIBUTE a TO aa",
        "(alter-type (name \"test_type\") (rename-attribute \"a\" \"aa\"))",
    );
    lowers(
        "ALTER TYPE test_type2 RENAME ATTRIBUTE a TO aa CASCADE",
        "(alter-type (name \"test_type2\") (rename-attribute \"a\" \"aa\") cascade)",
    );
}

#[test]
fn lowers_alter_collation_rename() {
    lowers(
        "ALTER COLLATION c1 RENAME TO c2",
        "(alter-collation (name \"c1\") (rename \"c2\"))",
    );
}

#[test]
fn lowers_alter_owner_object_kinds() {
    lowers(
        "ALTER SCHEMA testns OWNER TO regress_owner",
        "(alter-schema \"testns\" (owner (role \"regress_owner\")))",
    );
    lowers(
        "ALTER TYPE multitextrange1 OWNER TO regress_owner",
        "(alter-type (name \"multitextrange1\") (owner (role \"regress_owner\")))",
    );
    lowers(
        "ALTER EVENT TRIGGER regress_event_trigger OWNER TO regress_owner",
        "(alter-event-trigger \"regress_event_trigger\" (owner (role \"regress_owner\")))",
    );
    lowers(
        "ALTER LARGE OBJECT 42 OWNER TO regress_owner",
        "(alter-large-object (oid 42) (owner (role \"regress_owner\")))",
    );
}

#[test]
fn lowers_subscription_bare_flags() {
    lowers(
        "CREATE SUBSCRIPTION sub CONNECTION 'dbname=foo' PUBLICATION pub WITH (create_slot)",
        "(create-subscription \"sub\" (connection \"dbname=foo\") (publications \"pub\") (create-slot))",
    );
    lowers(
        "ALTER SUBSCRIPTION sub SET (failover)",
        "(alter-subscription \"sub\" (set (failover)))",
    );
}

#[test]
fn lowers_subscription_string_flag_values() {
    lowers(
        "CREATE SUBSCRIPTION sub CONNECTION 'dbname=foo' PUBLICATION pub \
                     WITH (connect = false, binary = foo)",
        "(create-subscription \"sub\" (connection \"dbname=foo\") (publications \"pub\") (no-connect) (binary \"foo\"))",
    );
    lowers(
        "CREATE SUBSCRIPTION sub2 CONNECTION 'dbname=foo' PUBLICATION pub \
                     WITH (connect = false, two_phase = foo)",
        "(create-subscription \"sub2\" (connection \"dbname=foo\") (publications \"pub\") (no-connect) (two-phase \"foo\"))",
    );
    lowers(
        "CREATE SUBSCRIPTION sub3 CONNECTION 'dbname=foo' PUBLICATION pub \
                     WITH (connect = false, disable_on_error = foo)",
        "(create-subscription \"sub3\" (connection \"dbname=foo\") (publications \"pub\") (no-connect) (disable-on-error \"foo\"))",
    );
}

#[test]
fn lowers_publication_flag_values() {
    lowers(
        "CREATE PUBLICATION pub FOR TABLE t WITH (publish_via_partition_root = true)",
        "(create-publication \"pub\" (publish-via-partition-root) (tables (table (\"t\"))))",
    );
    lowers(
        "CREATE PUBLICATION pub FOR TABLE t WITH (publish_via_partition_root = 0)",
        "(create-publication \"pub\" (no-publish-via-partition-root) (tables (table (\"t\"))))",
    );
}

#[test]
fn lowers_unique_constraint_index_options() {
    lowers(
        "ALTER TABLE t ADD UNIQUE (a, b) INCLUDE (c) WITH (fillfactor = 70)",
        "(alter-table (table (\"t\")) (add-constraint (unique \"a\" \"b\" (include \"c\") (options (option \"fillfactor\" 70)))))",
    );
    lowers(
        "ALTER TABLE t ADD CONSTRAINT pk PRIMARY KEY USING INDEX pk_index",
        "(alter-table (table (\"t\")) (add-constraint (primary-key (using-index \"pk_index\") (name \"pk\"))))",
    );
    lowers(
        "ALTER TABLE t ADD UNIQUE (a) USING INDEX TABLESPACE ts",
        "(alter-table (table (\"t\")) (add-constraint (unique \"a\" (tablespace \"ts\"))))",
    );
}

#[test]
fn lowers_foreign_key_set_null_columns() {
    lowers(
        "CREATE TABLE t (a int REFERENCES u (id) ON DELETE SET NULL (b))",
        "(create-table (table (\"t\")) (columns (column \"a\" (type \"pg_catalog\" \"int4\") (foreign-key (columns) (references (table (\"u\")) (columns \"id\")) (on-delete set-null (columns \"b\"))))))",
    );
}

#[test]
fn lowers_alter_database_connection_limit() {
    lowers(
        "ALTER DATABASE test1 WITH CONNECTION LIMIT = 10",
        "(alter-database \"test1\" (connection-limit 10))",
    );
}
