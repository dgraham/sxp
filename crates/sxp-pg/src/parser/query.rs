use super::*;

fn limit_clause_name(option: i32) -> Result<&'static str> {
    match pg::LimitOption::try_from(option) {
        Ok(pg::LimitOption::Default | pg::LimitOption::Count) => Ok("limit"),
        Ok(pg::LimitOption::WithTies) => Ok("limit-with-ties"),
        _ => Err(ParseError::new(
            "unsupported PostgreSQL AST: limit option",
            0,
        )),
    }
}

impl PostgresParser {
    pub(super) fn lower_select(&mut self, stmt: &pg::SelectStmt) -> Result<Sexp> {
        let mut result = if stmt.larg.is_some() || stmt.rarg.is_some() {
            self.lower_set_operation(stmt)?
        } else {
            self.lower_simple_select(stmt)?
        };
        if let Some(with_clause) = stmt.with_clause.as_ref() {
            result = self.wrap_with(with_clause, result)?;
        }
        Ok(result)
    }

    fn lower_simple_select(&mut self, stmt: &pg::SelectStmt) -> Result<Sexp> {
        if !stmt.values_lists.is_empty() {
            if !stmt.target_list.is_empty()
                || !stmt.from_clause.is_empty()
                || stmt.where_clause.is_some()
                || !stmt.group_clause.is_empty()
                || stmt.having_clause.is_some()
                || !stmt.distinct_clause.is_empty()
                || stmt.into_clause.is_some()
                || !stmt.locking_clause.is_empty()
                || !stmt.window_clause.is_empty()
            {
                return self.unsupported("VALUES clauses");
            }

            let mut values = vec![self.symbol("values")];

            values.extend(
                stmt.values_lists
                    .iter()
                    .map(|x| self.lower_values_row(x))
                    .collect::<Result<Vec<_>>>()?,
            );
            let order_by = self.lower_nodes(&stmt.sort_clause)?;
            self.optional_clause(&mut values, "order-by", order_by);
            self.optional_node_clause(
                &mut values,
                limit_clause_name(stmt.limit_option)?,
                stmt.limit_count.as_deref(),
            )?;
            self.optional_node_clause(&mut values, "offset", stmt.limit_offset.as_deref())?;
            return Ok(Sexp::list(values));
        }

        let mut values = vec![self.symbol("select")];

        let columns = self.lower_nodes(&stmt.target_list)?;
        match stmt.distinct_clause.as_slice() {
            [] => {}
            [marker] if marker.node.is_none() => values.push(Sexp::list([self.symbol("distinct")])),
            expressions => {
                let distinct_on = self.lower_nodes(expressions)?;
                values.push(self.list("distinct-on", distinct_on))
            }
        }
        if let Some(into) = stmt.into_clause.as_ref() {
            values.push(self.lower_into(into)?);
        }

        values.push(self.list("columns", columns));

        let from = self.lower_nodes(&stmt.from_clause)?;
        self.optional_clause(&mut values, "from", from);
        self.optional_node_clause(&mut values, "where", stmt.where_clause.as_deref())?;
        if stmt.group_distinct {
            let group_by = [self.symbol("distinct")]
                .into_iter()
                .chain(self.lower_nodes(&stmt.group_clause)?);
            values.push(self.list("group-by", group_by));
        } else {
            let group_by = self.lower_nodes(&stmt.group_clause)?;
            self.optional_clause(&mut values, "group-by", group_by);
        }
        self.optional_node_clause(&mut values, "having", stmt.having_clause.as_deref())?;

        values.extend(self.lower_nodes(&stmt.window_clause)?);

        let order_by = self.lower_nodes(&stmt.sort_clause)?;
        self.optional_clause(&mut values, "order-by", order_by);
        self.optional_node_clause(
            &mut values,
            limit_clause_name(stmt.limit_option)?,
            stmt.limit_count.as_deref(),
        )?;
        self.optional_node_clause(&mut values, "offset", stmt.limit_offset.as_deref())?;
        values.extend(self.lower_nodes(&stmt.locking_clause)?);

        Ok(Sexp::list(values))
    }

    fn lower_set_operation(&mut self, stmt: &pg::SelectStmt) -> Result<Sexp> {
        if !stmt.target_list.is_empty()
            || !stmt.from_clause.is_empty()
            || stmt.where_clause.is_some()
            || !stmt.group_clause.is_empty()
            || stmt.having_clause.is_some()
            || !stmt.window_clause.is_empty()
            || !stmt.values_lists.is_empty()
            || !stmt.distinct_clause.is_empty()
        {
            return self.unsupported("set operation SELECT clauses");
        }

        let name = match pg::SetOperation::try_from(stmt.op) {
            Ok(pg::SetOperation::SetopUnion) => "union",
            Ok(pg::SetOperation::SetopIntersect) => "intersect",
            Ok(pg::SetOperation::SetopExcept) => "except",
            _ => return self.unsupported("set operation"),
        };
        let left = self.required(stmt.larg.as_deref(), "set operation left query")?;
        let right = self.required(stmt.rarg.as_deref(), "set operation right query")?;
        let mut values = vec![
            self.symbol(name),
            self.lower_select(left)?,
            self.lower_select(right)?,
        ];

        let order_by = self.lower_nodes(&stmt.sort_clause)?;
        if stmt.all {
            values.push(self.symbol("all"));
        }
        self.optional_clause(&mut values, "order-by", order_by);
        self.optional_node_clause(
            &mut values,
            limit_clause_name(stmt.limit_option)?,
            stmt.limit_count.as_deref(),
        )?;
        self.optional_node_clause(&mut values, "offset", stmt.limit_offset.as_deref())?;
        values.extend(self.lower_nodes(&stmt.locking_clause)?);

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_into(&mut self, into: &pg::IntoClause) -> Result<Sexp> {
        let relation = self.required(into.rel.as_ref(), "SELECT INTO relation")?;
        let mut values = vec![self.symbol("into")];

        match relation.relpersistence.as_str() {
            "p" | "" => {}
            "t" => {
                let temporary_persistence = [self.symbol("temporary")];
                values.push(self.list("persistence", temporary_persistence))
            }
            "u" => {
                let unlogged_persistence = [self.symbol("unlogged")];
                values.push(self.list("persistence", unlogged_persistence))
            }
            persistence => return self.unsupported(format!("table persistence {persistence}")),
        }

        values.push(self.lower_relation(relation)?);
        if !into.col_names.is_empty() {
            let columns = self.lower_nodes(&into.col_names)?;
            values.push(self.list("columns", columns));
        }

        let options = self.lower_nodes(&into.options)?;
        self.optional_clause(&mut values, "options", options);
        if !into.access_method.is_empty() {
            values.push(self.list("using", [Sexp::string(into.access_method.clone())]));
        }
        match pg::OnCommitAction::try_from(into.on_commit) {
            Ok(pg::OnCommitAction::OncommitNoop) => {}
            Ok(pg::OnCommitAction::OncommitPreserveRows) => {
                let action = [self.symbol("preserve-rows")];
                values.push(self.list("on-commit", action));
            }
            Ok(pg::OnCommitAction::OncommitDeleteRows) => {
                let action = [self.symbol("delete-rows")];
                values.push(self.list("on-commit", action));
            }
            Ok(pg::OnCommitAction::OncommitDrop) => {
                let action = [self.symbol("drop")];
                values.push(self.list("on-commit", action));
            }
            _ => return self.unsupported("SELECT INTO on commit action"),
        }
        if !into.table_space_name.is_empty() {
            values.push(self.list("tablespace", [Sexp::string(into.table_space_name.clone())]));
        }

        if into.view_query.is_some() {
            return self.unsupported("SELECT INTO view query");
        }

        if into.skip_data {
            values.push(self.symbol("with-no-data"));
        }

        Ok(Sexp::list(values))
    }

    fn lower_values_row(&mut self, node: &pg::Node) -> Result<Sexp> {
        match self.required_kind(node.node.as_ref(), "VALUES row")? {
            PgNode::List(values) => {
                let row = self.lower_nodes(&values.items)?;
                Ok(self.list("row", row))
            }
            _ => self.unsupported("VALUES row"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::PostgresParser;
    use pg_query::protobuf as pg;
    use pg_query::protobuf::node::Node as PgNode;
    use sxp::{Atom, Sexp};

    fn parse(input: &str) -> sxp::Result<Sexp> {
        PostgresParser::new().parse(input)
    }

    fn lowers(sql: &str, expected: &str) {
        assert_eq!(
            parse(sql).map(|sexp| sexp.to_string()),
            Ok(expected.to_owned())
        );
    }

    #[test]
    fn lowers_postgres_18_row_comparison_types() {
        let integer = |value| pg::Node {
            node: Some(PgNode::AConst(pg::AConst {
                val: Some(pg::a_const::Val::Ival(pg::Integer { ival: value })),
                ..Default::default()
            })),
        };
        for (comparison, name) in [
            (pg::CompareType::CompareOverlap, "overlaps"),
            (pg::CompareType::CompareContainedBy, "contained-by"),
        ] {
            let expression = pg::RowCompareExpr {
                cmptype: comparison as i32,
                largs: vec![integer(1), integer(2)],
                rargs: vec![integer(3), integer(4)],
                ..Default::default()
            };
            let mut parser = PostgresParser::new();
            let lowered = parser
                .lower(&pg::Node {
                    node: Some(PgNode::RowCompareExpr(Box::new(expression))),
                })
                .map(|value| value.to_string());
            assert_eq!(
                lowered,
                Ok(format!("(row-compare {name} (left 1 2) (right 3 4))"))
            );
        }
    }

    #[test]
    fn preserves_boolean_literal_atoms() {
        let sexp = parse("SELECT true, false").unwrap();
        let atoms: Vec<_> = sexp
            .iter()
            .filter_map(|node| match node {
                Sexp::Atom(atom) => Some(atom),
                Sexp::List(_) => None,
            })
            .collect();

        assert!(atoms.contains(&&Atom::Boolean(true)));
        assert!(atoms.contains(&&Atom::Boolean(false)));
    }

    #[test]
    fn lowers_min_max_expressions() {
        lowers(
            "SELECT a, b, greatest(b, 'CCC') FROM collate_test1 ORDER BY 3",
            "(select (columns (column \"a\") (column \"b\") (call \"greatest\" (column \"b\") \"CCC\")) (from (table (\"collate_test1\"))) (order-by (sort 3 asc)))",
        );
        lowers(
            "SELECT least('a', 'b')",
            "(select (columns (call \"least\" \"a\" \"b\")))",
        );
    }

    #[test]
    fn lowers_star_indirection() {
        lowers(
            "select description, (test_conv(inbytes, 'utf8', 'utf8')).* \
                     from utf8_verification_inputs",
            "(select (columns (column \"description\") (indirection (call \"test_conv\" (column \"inbytes\") \"utf8\" \"utf8\") *)) (from (table (\"utf8_verification_inputs\"))))",
        );
    }

    #[test]
    fn lowers_float8_array_cast() {
        lowers(
            "SELECT float8_accum('{4,140,2900}'::float8[], 100)",
            "(select (columns (call \"float8_accum\" (cast \"{4,140,2900}\" (type \"float8\" (array))) 100)))",
        );
    }

    #[test]
    fn lowers_fixed_size_array_casts() {
        lowers(
            "SELECT '{}'::int[3]",
            "(select (columns (cast \"{}\" (type \"pg_catalog\" \"int4\" (array 3)))))",
        );
        lowers(
            "SELECT '{}'::int[1][2]",
            "(select (columns (cast \"{}\" (type \"pg_catalog\" \"int4\" (array 1) (array 2)))))",
        );
    }

    #[test]
    fn lowers_select_semantically() {
        lowers(
            "SELECT u.id, count(*) AS total FROM users AS u \
                     LEFT JOIN posts p ON p.user_id = u.id WHERE u.active AND p.id > 10 \
                     GROUP BY u.id HAVING count(*) > 1 ORDER BY total DESC NULLS LAST \
                     LIMIT 5 OFFSET 2",
            "(select (columns (column \"u\" \"id\") (as (call \"count\" *) \"total\")) (from (join left (table (\"users\") (alias \"u\")) (table (\"posts\") (alias \"p\")) (on (op \"=\" (column \"p\" \"user_id\") (column \"u\" \"id\"))))) (where (and (column \"u\" \"active\") (op \">\" (column \"p\" \"id\") 10))) (group-by (column \"u\" \"id\")) (having (op \">\" (call \"count\" *) 1)) (order-by (sort (column \"total\") desc nulls-last)) (limit 5) (offset 2))",
        );
    }

    #[test]
    fn lowers_ctes_and_set_operations_semantically() {
        lowers(
            "WITH active AS (SELECT id FROM users WHERE active) SELECT id FROM active",
            "(with (cte \"active\" (select (columns (column \"id\")) (from (table (\"users\"))) (where (column \"active\")))) (select (columns (column \"id\")) (from (table (\"active\")))))",
        );
        lowers(
            "SELECT id FROM users UNION ALL SELECT id FROM admins ORDER BY id",
            "(union (select (columns (column \"id\")) (from (table (\"users\")))) (select (columns (column \"id\")) (from (table (\"admins\")))) all (order-by (sort (column \"id\") asc)))",
        );
    }

    #[test]
    fn lowers_group_by_distinct() {
        lowers(
            "SELECT 1 GROUP BY DISTINCT 1",
            "(select (columns 1) (group-by distinct 1))",
        );
        lowers(
            "SELECT a, count(*) FROM t GROUP BY DISTINCT a, b HAVING count(*) > 1",
            "(select (columns (column \"a\") (call \"count\" *)) (from (table (\"t\"))) (group-by distinct (column \"a\") (column \"b\")) (having (op \">\" (call \"count\" *) 1)))",
        );
        lowers(
            "SELECT 1 GROUP BY DISTINCT 1 ORDER BY 1",
            "(select (columns 1) (group-by distinct 1) (order-by (sort 1 asc)))",
        );
        lowers(
            "SELECT 1 UNION SELECT 2 GROUP BY DISTINCT 2",
            "(union (select (columns 1)) (select (columns 2) (group-by distinct 2)))",
        );
    }

    #[test]
    fn parses_cte_search_and_cycle() {
        lowers(
            "WITH RECURSIVE t(n) AS (SELECT 1) \
                     SEARCH BREADTH FIRST BY n SET ord SELECT * FROM t",
            "(with recursive (cte \"t\" (select (columns 1)) (columns \"n\") (search breadth-first (by (column \"n\")) (set \"ord\"))) (select (columns (column *)) (from (table (\"t\")))))",
        );
        lowers(
            "WITH RECURSIVE t(n) AS (SELECT 1) \
                 SEARCH BREADTH FIRST BY n SET ord \
                 CYCLE n SET mark TO 'yes' DEFAULT 'no' USING path \
                 SELECT * FROM t",
            "(with recursive (cte \"t\" (select (columns 1)) (columns \"n\") (search breadth-first (by (column \"n\")) (set \"ord\")) (cycle (by (column \"n\")) (set \"mark\") (using \"path\") (value \"yes\") (default \"no\"))) (select (columns (column *)) (from (table (\"t\")))))",
        );
        lowers(
            "WITH RECURSIVE t(n, m) AS (SELECT 1, 2) \
                     SEARCH DEPTH FIRST BY n, m SET ord SELECT * FROM t",
            "(with recursive (cte \"t\" (select (columns 1 2)) (columns \"n\" \"m\") (search depth-first (by (column \"n\") (column \"m\")) (set \"ord\"))) (select (columns (column *)) (from (table (\"t\")))))",
        );
        lowers(
            "WITH RECURSIVE t(n) AS (SELECT 1) \
                     CYCLE n SET is_cycle USING path SELECT * FROM t",
            "(with recursive (cte \"t\" (select (columns 1)) (columns \"n\") (cycle (by (column \"n\")) (set \"is_cycle\") (using \"path\") (value true) (default false))) (select (columns (column *)) (from (table (\"t\")))))",
        );
    }

    #[test]
    fn parses_cte_materialization() {
        lowers(
            "WITH c AS MATERIALIZED (SELECT 1) SELECT * FROM c",
            "(with (cte \"c\" (select (columns 1)) materialized) (select (columns (column *)) (from (table (\"c\")))))",
        );
        lowers(
            "WITH c AS NOT MATERIALIZED (SELECT 1) SELECT * FROM c",
            "(with (cte \"c\" (select (columns 1)) not-materialized) (select (columns (column *)) (from (table (\"c\")))))",
        );
    }

    #[test]
    fn parses_select_into() {
        lowers(
            "SELECT * INTO new_t FROM t WHERE a = 1",
            "(select (into (table (\"new_t\"))) (columns (column *)) (from (table (\"t\"))) (where (op \"=\" (column \"a\") 1)))",
        );
        lowers(
            "SELECT 1 UNION SELECT 2 INTO new_t",
            "(union (select (columns 1)) (select (into (table (\"new_t\"))) (columns 2)))",
        );
    }

    #[test]
    fn lowers_subqueries_and_common_expressions_semantically() {
        lowers(
            "SELECT CASE WHEN u.active THEN COALESCE(u.name, $1) ELSE NULL END AS name, \
                     ARRAY[1, 2, $2] AS ids FROM (SELECT id, active, name FROM users) AS u \
                     WHERE u.id IN (SELECT id FROM admins)",
            "(select (columns (as (case (when (column \"u\" \"active\") (coalesce (column \"u\" \"name\") (parameter 1))) (else null)) \"name\") (as (array 1 2 (parameter 2)) \"ids\")) (from (subquery (select (columns (column \"id\") (column \"active\") (column \"name\")) (from (table (\"users\")))) (alias \"u\"))) (where (in-subquery (column \"u\" \"id\") (select (columns (column \"id\")) (from (table (\"admins\")))))))",
        );
    }

    #[test]
    fn lowers_operator_with_subquery_operand() {
        lowers(
            "SELECT 1 = (SELECT 2)",
            "(select (columns (op \"=\" 1 (scalar-subquery (select (columns 2))))))",
        );
        lowers(
            "SELECT (SELECT 1) = (SELECT 2)",
            "(select (columns (op \"=\" (scalar-subquery (select (columns 1))) (scalar-subquery (select (columns 2))))))",
        );
        lowers(
            "SELECT (1,2) = (SELECT 1,2)",
            "(select (columns (op \"=\" (row 1 2) (scalar-subquery (select (columns 1 2))))))",
        );
        lowers(
            "SELECT 1 = ((SELECT 2))",
            "(select (columns (op \"=\" 1 (scalar-subquery (select (columns 2))))))",
        );
        lowers(
            "UPDATE t SET a = 1 WHERE a = (SELECT 2)",
            "(update (table (\"t\")) (set (assignment \"a\" 1)) (where (op \"=\" (column \"a\") (scalar-subquery (select (columns 2))))))",
        );
        lowers(
            "SELECT 1 + (SELECT 2)",
            "(select (columns (op \"+\" 1 (scalar-subquery (select (columns 2))))))",
        );
        lowers(
            "SELECT 'a' LIKE (SELECT 'b')",
            "(select (columns (like \"a\" (scalar-subquery (select (columns \"b\"))))))",
        );
    }

    #[test]
    fn lowers_order_by_using_operator() {
        lowers(
            "SELECT 1 ORDER BY 1 USING <",
            "(select (columns 1) (order-by (sort 1 (using \"<\"))))",
        );
        lowers(
            "SELECT 1 ORDER BY 1 USING < NULLS FIRST",
            "(select (columns 1) (order-by (sort 1 (using \"<\") nulls-first)))",
        );
        lowers(
            "SELECT * FROM t ORDER BY a USING OPERATOR(pg_catalog.>)",
            "(select (columns (column *)) (from (table (\"t\"))) (order-by (sort (column \"a\") (using \"pg_catalog.>\"))))",
        );
        lowers(
            "SELECT rank() OVER (ORDER BY 1 USING <) FROM t",
            "(select (columns (call \"rank\" (over (order-by (sort 1 (using \"<\")))))) (from (table (\"t\"))))",
        );
    }

    #[test]
    fn parses_catalog_qualified_relations() {
        lowers(
            "SELECT id FROM db2.public.users",
            "(select (columns (column \"id\")) (from (table (\"db2\" \"public\" \"users\"))))",
        );
    }

    #[test]
    fn parses_only_relations() {
        lowers(
            "SELECT id FROM ONLY users",
            "(select (columns (column \"id\")) (from (table (only true) (\"users\"))))",
        );
    }

    #[test]
    fn lowers_natural_joins() {
        lowers(
            "SELECT * FROM t1 NATURAL JOIN t2",
            "(select (columns (column *)) (from (join inner (table (\"t1\")) (table (\"t2\")) natural)))",
        );
        lowers(
            "SELECT * FROM a NATURAL LEFT JOIN b",
            "(select (columns (column *)) (from (join left (table (\"a\")) (table (\"b\")) natural)))",
        );
        lowers(
            "SELECT * FROM a NATURAL RIGHT JOIN b",
            "(select (columns (column *)) (from (join right (table (\"a\")) (table (\"b\")) natural)))",
        );
        lowers(
            "SELECT * FROM a NATURAL FULL JOIN b",
            "(select (columns (column *)) (from (join full (table (\"a\")) (table (\"b\")) natural)))",
        );
    }

    #[test]
    fn lowers_cross_joins() {
        lowers(
            "SELECT * FROM t1 CROSS JOIN t2",
            "(select (columns (column *)) (from (join cross (table (\"t1\")) (table (\"t2\")))))",
        );
        lowers(
            "SELECT * FROM t CROSS JOIN LATERAL f() AS g",
            "(select (columns (column *)) (from (join cross (table (\"t\")) (function lateral (call \"f\") (alias \"g\")))))",
        );
    }

    #[test]
    fn lowers_join_aliases() {
        lowers(
            "SELECT * FROM (a JOIN b ON true) AS j",
            "(select (columns (column *)) (from (join inner (table (\"a\")) (table (\"b\")) (on true) (alias \"j\"))))",
        );
        lowers(
            "SELECT * FROM a JOIN b USING (x) AS u",
            "(select (columns (column *)) (from (join inner (table (\"a\")) (table (\"b\")) (using \"x\") (using-alias \"u\"))))",
        );
        lowers(
            "SELECT * FROM (t1 AS x JOIN t2 AS y ON true) AS j",
            "(select (columns (column *)) (from (join inner (table (\"t1\") (alias \"x\")) (table (\"t2\") (alias \"y\")) (on true) (alias \"j\"))))",
        );
    }

    #[test]
    fn parses_sql_value_functions() {
        lowers(
            "SELECT CURRENT_TIMESTAMP(3)",
            "(select (columns (current-timestamp (precision 3))))",
        );
        lowers(
            "SELECT CURRENT_DATE, session_user",
            "(select (columns (current-date) (session-user)))",
        );
    }

    #[test]
    fn parses_between_and_similar() {
        lowers(
            "SELECT x NOT BETWEEN SYMMETRIC 10 AND 1",
            "(select (columns (not-between-symmetric (column \"x\") 10 1)))",
        );
        lowers(
            "SELECT name SIMILAR TO 'a+'",
            "(select (columns (similar-to (column \"name\") \"a+\")))",
        );
        lowers(
            "SELECT name COLLATE \"C\" FROM t",
            "(select (columns (collate (column \"name\") \"C\")) (from (table (\"t\"))))",
        );
    }

    #[test]
    fn parses_grouping_sets() {
        lowers(
            "SELECT grouping(a) FROM t GROUP BY ROLLUP(a, b)",
            "(select (columns (grouping (column \"a\"))) (from (table (\"t\"))) (group-by (rollup (column \"a\") (column \"b\"))))",
        );
        lowers(
            "SELECT count(*) FROM t GROUP BY GROUPING SETS ((a), ())",
            "(select (columns (call \"count\" *)) (from (table (\"t\"))) (group-by (grouping-sets (grouping-set (column \"a\")) (grouping-set))))",
        );
    }

    #[test]
    fn lowers_select_into_persistence() {
        lowers(
            "SELECT * INTO x FROM t",
            "(select (into (table (\"x\"))) (columns (column *)) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT * INTO TEMP x FROM t",
            "(select (into (persistence temporary) (table (\"x\"))) (columns (column *)) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT * INTO UNLOGGED x FROM t",
            "(select (into (persistence unlogged) (table (\"x\"))) (columns (column *)) (from (table (\"t\"))))",
        );
    }

    #[test]
    fn lowers_select_into_clause_options_from_ast() {
        use pg_query::protobuf as pg;
        use pg_query::protobuf::node::Node as PgNode;

        let mut parser = PostgresParser::new();
        let into = pg::IntoClause {
            rel: Some(pg::RangeVar {
                relname: "new_t".to_owned(),
                relpersistence: "t".to_owned(),
                inh: true,
                ..Default::default()
            }),
            options: vec![pg::Node {
                node: Some(PgNode::DefElem(Box::new(pg::DefElem {
                    defname: "fillfactor".to_owned(),
                    defaction: pg::DefElemAction::DefelemUnspec as i32,
                    arg: Some(Box::new(pg::Node {
                        node: Some(PgNode::Integer(pg::Integer { ival: 80 })),
                    })),
                    ..Default::default()
                }))),
            }],
            on_commit: pg::OnCommitAction::OncommitDrop as i32,
            access_method: "heap".to_owned(),
            table_space_name: "fast".to_owned(),
            ..Default::default()
        };

        assert_eq!(
                parser.lower_into(&into).map(|value| value.to_string()),
                Ok("(into (persistence temporary) (table (\"new_t\")) (options (option \"fillfactor\" 80)) (using \"heap\") (on-commit drop) (tablespace \"fast\"))".to_owned())
            );
    }

    #[test]
    fn lowers_create_table_as_select_into_ast_marker() {
        use pg_query::protobuf as pg;
        use pg_query::protobuf::node::Node as PgNode;

        let mut parsed = pg_query::parse("CREATE TABLE new_t AS SELECT 1", 0).unwrap();
        let statement = parsed.protobuf.stmts[0]
            .stmt
            .as_mut()
            .and_then(|node| node.node.as_mut())
            .unwrap();
        let PgNode::CreateTableAsStmt(create_as) = statement else {
            panic!("expected CREATE TABLE AS AST");
        };
        create_as.is_select_into = true;

        let mut parser = PostgresParser::new();
        let lowered = parser
            .lower(&pg::Node {
                node: Some(PgNode::CreateTableAsStmt(create_as.clone())),
            })
            .map(|value| value.to_string());
        assert_eq!(
            lowered,
            Ok("(create-table-as (table (\"new_t\")) (query (select (columns 1))))".to_owned())
        );
    }

    #[test]
    fn parses_function_relations() {
        lowers(
            "SELECT * FROM generate_series(1, 10) AS g(n)",
            "(select (columns (column *)) (from (function (call \"generate_series\" 1 10) (alias \"g\" (columns \"n\")))))",
        );
        lowers(
            "SELECT * FROM ROWS FROM (f(), g() AS (y int)) WITH ORDINALITY",
            "(select (columns (column *)) (from (rows-from (function (call \"f\")) (function (call \"g\") (columns (column \"y\" (type \"pg_catalog\" \"int4\")))) with-ordinality)))",
        );
        lowers(
            "SELECT * FROM t TABLESAMPLE SYSTEM (10) REPEATABLE (42)",
            "(select (columns (column *)) (from (table-sample (table (\"t\")) \"system\" (args 10) (repeatable 42))))",
        );
        lowers(
            "SELECT * FROM t FOR SHARE OF t SKIP LOCKED",
            "(select (columns (column *)) (from (table (\"t\"))) (locking for-share (of (table (\"t\"))) skip-locked))",
        );
        lowers(
            "CREATE TABLE t2 (LIKE t INCLUDING ALL)",
            "(create-table (table (\"t2\")) (columns (like (table (\"t\")) all)))",
        );
    }

    #[test]
    fn parses_json_constructors() {
        lowers(
            "SELECT JSON_ARRAY(1, 2)",
            "(select (columns (json-array (items 1 2) absent-on-null)))",
        );
        lowers(
            "SELECT JSON_OBJECT('a': 1)",
            "(select (columns (json-object (entry (key \"a\") (value 1)))))",
        );
        lowers(
            "SELECT JSON_OBJECT('a': 1 ABSENT ON NULL WITH UNIQUE)",
            "(select (columns (json-object (entry (key \"a\") (value 1)) absent-on-null unique-keys)))",
        );
        lowers(
            "SELECT JSON('[]' WITH UNIQUE KEYS)",
            "(select (columns (json-parse \"[]\" unique-keys)))",
        );
        lowers(
            "SELECT JSON_SCALAR(123)",
            "(select (columns (json-scalar 123)))",
        );
        lowers(
            "SELECT JSON_SERIALIZE('[]' RETURNING text)",
            "(select (columns (json-serialize \"[]\" (returning (type \"text\")))))",
        );
        lowers(
            "SELECT JSON_ARRAY(SELECT x FROM t)",
            "(select (columns (json-array (query (select (columns (column \"x\")) (from (table (\"t\"))))) absent-on-null)))",
        );
    }

    #[test]
    fn parses_json_query_functions() {
        lowers(
            "SELECT JSON_QUERY(j, '$.a') FROM t",
            "(select (columns (json-query (column \"j\") \"$.a\")) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_VALUE(j, '$.a' RETURNING int) FROM t",
            "(select (columns (json-value (column \"j\") \"$.a\" (returning (type \"pg_catalog\" \"int4\")))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_EXISTS(j, '$.a' TRUE ON ERROR) FROM t",
            "(select (columns (json-exists (column \"j\") \"$.a\" (on-error (true)))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_QUERY(j, '$.a' WITH CONDITIONAL WRAPPER) FROM t",
            "(select (columns (json-query (column \"j\") \"$.a\" with-conditional-wrapper)) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_QUERY(j, '$.a' KEEP QUOTES) FROM t",
            "(select (columns (json-query (column \"j\") \"$.a\" keep-quotes)) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_EXISTS(j, '$.a' PASSING x AS p) FROM t",
            "(select (columns (json-exists (column \"j\") \"$.a\" (passing (arg \"p\" (column \"x\"))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_VALUE(j, '$.a' RETURNING text DEFAULT 'x' ON EMPTY NULL ON ERROR) FROM t",
            "(select (columns (json-value (column \"j\") \"$.a\" (returning (type \"text\")) (on-empty (default \"x\")) (on-error (null)))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_QUERY(j FORMAT JSON, '$.a') FROM t",
            "(select (columns (json-query ((column \"j\") (format json)) \"$.a\")) (from (table (\"t\"))))",
        );
    }

    #[test]
    fn parses_json_aggregates() {
        lowers(
            "SELECT JSON_ARRAYAGG(a ORDER BY b NULLS FIRST RETURNING jsonb) FROM t",
            "(select (columns (json-arrayagg (agg (column \"a\")) (order-by (sort (column \"b\") asc nulls-first)) (returning (type \"jsonb\")) absent-on-null)) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_OBJECTAGG(k: v) FROM t",
            "(select (columns (json-objectagg (agg (entry (key (column \"k\")) (value (column \"v\")))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_ARRAYAGG(a) FILTER (WHERE a > 1) FROM t",
            "(select (columns (json-arrayagg (agg (column \"a\")) (filter (op \">\" (column \"a\") 1)) absent-on-null)) (from (table (\"t\"))))",
        );
    }

    #[test]
    fn parses_is_json() {
        lowers(
            "SELECT a IS JSON",
            "(select (columns (is-json (column \"a\"))))",
        );
        lowers(
            "SELECT a IS NOT JSON VALUE WITH UNIQUE KEYS",
            "(select (columns (not (is-json (column \"a\") with-unique-keys))))",
        );
        lowers(
            "SELECT a IS JSON SCALAR",
            "(select (columns (is-json (column \"a\") scalar)))",
        );
    }

    #[test]
    fn parses_xml_expressions() {
        lowers(
            "SELECT XMLPARSE(DOCUMENT '<a/>')",
            "(select (columns (xml-parse document \"<a/>\")))",
        );
        lowers(
            "SELECT XMLPARSE(CONTENT 'x' PRESERVE WHITESPACE)",
            "(select (columns (xml-parse content \"x\" preserve-whitespace)))",
        );
        lowers(
            "SELECT XMLPI(NAME pi, 'content')",
            "(select (columns (xml-pi (name \"pi\") \"content\")))",
        );
        lowers(
            "SELECT XMLPI(NAME pi)",
            "(select (columns (xml-pi (name \"pi\"))))",
        );
        lowers(
            "SELECT XMLFOREST(a, b AS \"b2\") FROM t",
            "(select (columns (xml-forest (elem (name \"a\") (column \"a\")) (elem (name \"b2\") (column \"b\")))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT XMLELEMENT(NAME e, XMLATTRIBUTES(a AS x), 'text')",
            "(select (columns (xml-element (name \"e\") (attributes (attr (name \"x\") (column \"a\"))) \"text\")))",
        );
        lowers(
            "SELECT XMLELEMENT(NAME e, XMLATTRIBUTES(a))",
            "(select (columns (xml-element (name \"e\") (attributes (attr (name \"a\") (column \"a\"))))))",
        );
        lowers(
            "SELECT XMLSERIALIZE(DOCUMENT x AS text)",
            "(select (columns (xml-serialize document (column \"x\") (as (type \"text\")))))",
        );
        lowers(
            "SELECT XMLSERIALIZE(CONTENT x AS text INDENT)",
            "(select (columns (xml-serialize content (column \"x\") (as (type \"text\")) indent)))",
        );
        lowers(
            "SELECT x IS DOCUMENT",
            "(select (columns (is-document (column \"x\"))))",
        );
        lowers(
            "SELECT XMLCONCAT(x, y)",
            "(select (columns (xml-concat (column \"x\") (column \"y\"))))",
        );
        lowers(
            "SELECT XMLROOT(x, VERSION '1.0')",
            "(select (columns (xml-root (column \"x\") (version \"1.0\"))))",
        );
        lowers(
            "SELECT XMLROOT(x, VERSION '1.0', STANDALONE YES)",
            "(select (columns (xml-root (column \"x\") (version \"1.0\") (standalone yes))))",
        );
    }

    #[test]
    fn lowers_xml_attributes_without_derived_name() {
        lowers(
            "SELECT xmlelement(name element, xmlattributes('unnamed and wrong'))",
            "(select (columns (xml-element (name \"element\") (attributes (attr \"unnamed and wrong\")))))",
        );
    }

    #[test]
    fn parses_json_table() {
        lowers(
            "SELECT * FROM JSON_TABLE(j, '$' COLUMNS (a int PATH '$.a')) AS jt",
            "(select (columns (column *)) (from (json-table (column \"j\") (path \"$\") (columns (json-column \"a\" (type \"pg_catalog\" \"int4\") (path \"$.a\"))) (alias \"jt\"))))",
        );
    }

    #[test]
    fn parses_window_function_over_clauses() {
        lowers(
            "SELECT rank() OVER (ORDER BY a DESC NULLS LAST) FROM t",
            "(select (columns (call \"rank\" (over (order-by (sort (column \"a\") desc nulls-last))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (PARTITION BY b ORDER BY c) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (partition-by (column \"b\")) (order-by (sort (column \"c\") asc))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT count(*) OVER () FROM t",
            "(select (columns (call \"count\" * (over))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT first_value(a) OVER (ORDER BY x ROWS 1 PRECEDING) FROM t",
            "(select (columns (call \"first_value\" (column \"a\") (over (order-by (sort (column \"x\") asc)) (frame rows (preceding 1))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a ORDER BY b) OVER () FROM t",
            "(select (columns (call \"sum\" (column \"a\") (order-by (sort (column \"b\") asc)) (over))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) FILTER (WHERE b > 1) OVER (ORDER BY c) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (filter (op \">\" (column \"b\") 1)) (over (order-by (sort (column \"c\") asc))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT JSON_ARRAYAGG(a) OVER () FROM t",
            "(select (columns (json-arrayagg (agg (column \"a\")) (over) absent-on-null)) (from (table (\"t\"))))",
        );
    }

    #[test]
    fn parses_window_references() {
        lowers(
            "SELECT rank() OVER w FROM t WINDOW w AS (PARTITION BY b ORDER BY c)",
            "(select (columns (call \"rank\" (over (window \"w\")))) (from (table (\"t\"))) (window (name \"w\") (partition-by (column \"b\")) (order-by (sort (column \"c\") asc))))",
        );
        lowers(
            "SELECT rank() OVER (w ORDER BY a) FROM t WINDOW w AS (PARTITION BY b)",
            "(select (columns (call \"rank\" (over (window \"w\") (order-by (sort (column \"a\") asc))))) (from (table (\"t\"))) (window (name \"w\") (partition-by (column \"b\"))))",
        );
        lowers(
            "SELECT rank() OVER (w ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t WINDOW w AS (PARTITION BY b)",
            "(select (columns (call \"rank\" (over (window \"w\") (frame rows (between (preceding 1) (current-row)))))) (from (table (\"t\"))) (window (name \"w\") (partition-by (column \"b\"))))",
        );
        lowers(
            "SELECT rank() OVER v FROM t WINDOW w AS (PARTITION BY b), v AS (w)",
            "(select (columns (call \"rank\" (over (window \"v\")))) (from (table (\"t\"))) (window (name \"w\") (partition-by (column \"b\"))) (window (name \"v\") (window \"w\")))",
        );
        lowers(
            "SELECT dense_rank() OVER w FROM t",
            "(select (columns (call \"dense_rank\" (over (window \"w\")))) (from (table (\"t\"))))",
        );
    }

    #[test]
    fn parses_xmltable_namespaces() {
        lowers(
            "SELECT * FROM XMLTABLE(XMLNAMESPACES(DEFAULT 'u1'), '//r' PASSING x \
                     COLUMNS a int) AS xt",
            "(select (columns (column *)) (from (xmltable (namespaces (default \"u1\")) \"//r\" (passing (column \"x\")) (columns (xml-column \"a\" (type \"pg_catalog\" \"int4\"))) (alias \"xt\"))))",
        );
        lowers(
            "SELECT * FROM XMLTABLE(XMLNAMESPACES('u2' AS n, DEFAULT 'u1'), '//r' \
                     PASSING x COLUMNS a int) AS xt",
            "(select (columns (column *)) (from (xmltable (namespaces (namespace \"n\" \"u2\") (default \"u1\")) \"//r\" (passing (column \"x\")) (columns (xml-column \"a\" (type \"pg_catalog\" \"int4\"))) (alias \"xt\"))))",
        );
    }

    #[test]
    fn lowers_xmlroot_standalone_no_value() {
        lowers(
            "SELECT XMLROOT(x, VERSION '1.0', STANDALONE NO VALUE)",
            "(select (columns (xml-root (column \"x\") (version \"1.0\") (standalone no-value))))",
        );
        lowers(
            "SELECT XMLROOT(x, VERSION NO VALUE, STANDALONE NO VALUE)",
            "(select (columns (xml-root (column \"x\") (version null) (standalone no-value))))",
        );
    }

    #[test]
    fn parses_window_frame_clauses() {
        lowers(
            "SELECT sum(a) OVER (ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (between (preceding 1) (current-row)))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame range (between (unbounded-preceding) (unbounded-following)))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ROWS 2 PRECEDING) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (preceding 2))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ROWS UNBOUNDED PRECEDING) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (unbounded-preceding))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (GROUPS CURRENT ROW) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame groups (current-row))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (RANGE CURRENT ROW) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame range (current-row))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (between (current-row) (unbounded-following)))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ROWS BETWEEN 1 PRECEDING AND 2 FOLLOWING) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (between (preceding 1) (following 2)))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (GROUPS BETWEEN 1 PRECEDING AND 1 FOLLOWING EXCLUDE CURRENT ROW) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame groups (between (preceding 1) (following 1)) (exclude current-row))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING EXCLUDE GROUP) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (between (preceding 1) (following 1)) (exclude group))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING EXCLUDE TIES) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (between (preceding 1) (following 1)) (exclude ties))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING EXCLUDE NO OTHERS) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (between (preceding 1) (following 1)))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ORDER BY x ROWS BETWEEN 1.5 PRECEDING AND 1.5 FOLLOWING) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (order-by (sort (column \"x\") asc)) (frame rows (between (preceding 1.5) (following 1.5)))))) (from (table (\"t\"))))",
        );
        lowers(
            "SELECT sum(id) OVER (ROWS 1 PRECEDING) FROM users",
            "(select (columns (call \"sum\" (column \"id\") (over (frame rows (preceding 1))))) (from (table (\"users\"))))",
        );
        lowers(
            "SELECT sum(a) OVER (ROWS 1+1 PRECEDING) FROM t",
            "(select (columns (call \"sum\" (column \"a\") (over (frame rows (preceding (op \"+\" 1 1)))))) (from (table (\"t\"))))",
        );
    }
}
