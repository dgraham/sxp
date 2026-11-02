use super::*;

impl PostgresParser {
    pub(super) fn lower_insert(&mut self, stmt: &pg::InsertStmt) -> Result<Sexp> {
        let overriding = match pg::OverridingKind::try_from(stmt.r#override) {
            Ok(pg::OverridingKind::OverridingNotSet) => None,
            Ok(pg::OverridingKind::OverridingSystemValue) => Some("system-value"),
            Ok(pg::OverridingKind::OverridingUserValue) => Some("user-value"),
            _ => return self.unsupported("INSERT OVERRIDING clause"),
        };
        let relation = self.required(stmt.relation.as_ref(), "INSERT relation")?;
        let relation = self.lower_relation(relation)?;
        let source = match stmt.select_stmt.as_deref() {
            Some(select) => self.lower(select)?,
            None => self.symbol("default-values"),
        };
        let columns = self.lower_insert_columns(&stmt.cols)?;
        let mut values = vec![
            self.symbol("insert"),
            self.list("into", [relation]),
            self.list("columns", columns),
        ];

        if let Some(overriding) = overriding {
            let overriding = [self.symbol(overriding)];
            values.push(self.list("overriding", overriding));
        }

        values.push(self.list("source", [source]));

        let returning = self.lower_returning_clause(stmt.returning_clause.as_ref())?;
        if let Some(conflict) = stmt.on_conflict_clause.as_ref() {
            values.push(self.lower_on_conflict(conflict)?);
        }
        self.optional_clause(&mut values, "returning", returning);
        let result = Sexp::list(values);
        match stmt.with_clause.as_ref() {
            Some(with_clause) => self.wrap_with(with_clause, result),
            None => Ok(result),
        }
    }

    fn lower_on_conflict(&mut self, clause: &pg::OnConflictClause) -> Result<Sexp> {
        let action = match pg::OnConflictAction::try_from(clause.action) {
            Ok(pg::OnConflictAction::OnconflictNothing) => "do-nothing",
            Ok(pg::OnConflictAction::OnconflictUpdate) => "do-update",
            _ => return self.unsupported("ON CONFLICT action"),
        };
        let mut values = vec![self.symbol("on-conflict"), self.symbol(action)];

        if let Some(inference) = clause.infer.as_ref() {
            values.push(self.lower_inference(inference)?);
        }
        match action {
            "do-nothing" => {
                if !clause.target_list.is_empty() || clause.where_clause.is_some() {
                    return self.unsupported("ON CONFLICT DO NOTHING action clauses");
                }
            }
            "do-update" => {
                let set = self.lower_assignments(&clause.target_list)?;
                if clause.target_list.is_empty() {
                    return self.unsupported("ON CONFLICT DO UPDATE clause");
                }

                values.push(self.list("set", set));
                self.optional_node_clause(&mut values, "where", clause.where_clause.as_deref())?;
            }
            _ => return self.unsupported("ON CONFLICT action"),
        }

        Ok(Sexp::list(values))
    }

    fn lower_inference(&mut self, inference: &pg::InferClause) -> Result<Sexp> {
        if !inference.conname.is_empty()
            && (!inference.index_elems.is_empty() || inference.where_clause.is_some())
        {
            return self.unsupported("ON CONFLICT inference");
        }

        let mut values = vec![self.symbol("inference")];

        if !inference.conname.is_empty() {
            values.push(self.list("constraint", [Sexp::string(inference.conname.clone())]));
        } else if !inference.index_elems.is_empty() {
            let columns = self.lower_inference_elements(&inference.index_elems)?;
            values.push(self.list("columns", columns));
            self.optional_node_clause(&mut values, "where", inference.where_clause.as_deref())?;
        } else {
            return self.unsupported("ON CONFLICT inference without target");
        }

        Ok(Sexp::list(values))
    }

    fn lower_inference_elements(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        nodes
            .iter()
            .map(|node| {
                match self.required_kind(node.node.as_ref(), "ON CONFLICT inference element")? {
                    PgNode::IndexElem(element) => self.lower_index_element(element),
                    _ => self.unsupported("ON CONFLICT inference element"),
                }
            })
            .collect()
    }

    pub(super) fn lower_merge(&mut self, stmt: &pg::MergeStmt) -> Result<Sexp> {
        let condition_node =
            self.required_node(stmt.join_condition.as_deref(), "MERGE join condition")?;
        let on = [self.lower(condition_node)?];
        let source_relation_node =
            self.required_node(stmt.source_relation.as_deref(), "MERGE source relation")?;
        let using = [self.lower(source_relation_node)?];
        let target_relation = self.required(stmt.relation.as_ref(), "MERGE target relation")?;
        let into = [self.lower_relation(target_relation)?];
        let mut values = vec![
            self.symbol("merge"),
            self.list("into", into),
            self.list("using", using),
            self.list("on", on),
        ];

        values.extend(
            stmt.merge_when_clauses
                .iter()
                .map(|x| self.lower_merge_when(x))
                .collect::<Result<Vec<_>>>()?,
        );
        let returning = self.lower_returning_clause(stmt.returning_clause.as_ref())?;
        self.optional_clause(&mut values, "returning", returning);
        let result = Sexp::list(values);
        match stmt.with_clause.as_ref() {
            Some(with_clause) => self.wrap_with(with_clause, result),
            None => Ok(result),
        }
    }

    fn lower_merge_when(&mut self, node: &pg::Node) -> Result<Sexp> {
        let clause = match self.required_kind(node.node.as_ref(), "MERGE WHEN clause")? {
            PgNode::MergeWhenClause(clause) => clause,
            _ => return self.unsupported("MERGE WHEN clause"),
        };
        let match_kind = match pg::MergeMatchKind::try_from(clause.match_kind) {
            Ok(pg::MergeMatchKind::MergeWhenMatched) => "matched",
            Ok(pg::MergeMatchKind::MergeWhenNotMatchedBySource) => "not-matched-by-source",
            Ok(pg::MergeMatchKind::MergeWhenNotMatchedByTarget) => "not-matched-by-target",
            _ => return self.unsupported("MERGE match kind"),
        };
        let mut values = vec![self.symbol("when"), self.symbol(match_kind)];

        if let Some(condition) = clause.condition.as_deref() {
            let cond = [self.lower(condition)?];
            values.push(self.list("condition", cond));
        }

        values.push(self.lower_merge_action(clause)?);

        Ok(Sexp::list(values))
    }

    fn lower_merge_action(&mut self, clause: &pg::MergeWhenClause) -> Result<Sexp> {
        let overriding = match pg::OverridingKind::try_from(clause.r#override) {
            Ok(pg::OverridingKind::OverridingNotSet) => None,
            Ok(pg::OverridingKind::OverridingUserValue) => Some("overriding-user-value"),
            Ok(pg::OverridingKind::OverridingSystemValue) => Some("overriding-system-value"),
            _ => return self.unsupported("MERGE OVERRIDING clause"),
        };
        match pg::CmdType::try_from(clause.command_type) {
            Ok(pg::CmdType::CmdNothing) => {
                if overriding.is_some()
                    || !clause.target_list.is_empty()
                    || !clause.values.is_empty()
                {
                    return self.unsupported("MERGE DO NOTHING action clauses");
                }
                Ok(Sexp::list([self.symbol("do-nothing")]))
            }
            Ok(pg::CmdType::CmdDelete) => {
                if overriding.is_some()
                    || !clause.target_list.is_empty()
                    || !clause.values.is_empty()
                {
                    return self.unsupported("MERGE DELETE action clauses");
                }
                Ok(Sexp::list([self.symbol("delete")]))
            }
            Ok(pg::CmdType::CmdUpdate) => {
                let set = self.lower_assignments(&clause.target_list)?;
                let update = [self.list("set", set)];
                if overriding.is_some() || !clause.values.is_empty() {
                    return self.unsupported("MERGE UPDATE action clauses");
                }
                Ok(self.list("update", update))
            }
            Ok(pg::CmdType::CmdInsert) => {
                let values = self.lower_nodes(&clause.values)?;
                let columns = self.lower_insert_columns(&clause.target_list)?;
                let mut values = vec![
                    self.symbol("insert"),
                    self.list("columns", columns),
                    self.list("values", values),
                ];

                if let Some(overriding) = overriding {
                    values.push(self.symbol(overriding));
                }
                Ok(Sexp::list(values))
            }
            _ => self.unsupported("MERGE action"),
        }
    }

    pub(super) fn lower_update(&mut self, stmt: &pg::UpdateStmt) -> Result<Sexp> {
        let set = self.lower_assignments(&stmt.target_list)?;
        let relation = self.required(stmt.relation.as_ref(), "UPDATE relation")?;
        let mut values = vec![
            self.symbol("update"),
            self.lower_relation(relation)?,
            self.list("set", set),
        ];

        let from = self.lower_nodes(&stmt.from_clause)?;
        self.optional_clause(&mut values, "from", from);
        self.optional_node_clause(&mut values, "where", stmt.where_clause.as_deref())?;
        let returning = self.lower_returning_clause(stmt.returning_clause.as_ref())?;
        self.optional_clause(&mut values, "returning", returning);
        let result = Sexp::list(values);
        match stmt.with_clause.as_ref() {
            Some(with_clause) => self.wrap_with(with_clause, result),
            None => Ok(result),
        }
    }

    pub(super) fn lower_delete(&mut self, stmt: &pg::DeleteStmt) -> Result<Sexp> {
        let relation = self.required(stmt.relation.as_ref(), "DELETE relation")?;
        let from = [self.lower_relation(relation)?];
        let mut values = vec![self.symbol("delete"), self.list("from", from)];

        let using = self.lower_nodes(&stmt.using_clause)?;
        self.optional_clause(&mut values, "using", using);
        self.optional_node_clause(&mut values, "where", stmt.where_clause.as_deref())?;
        let returning = self.lower_returning_clause(stmt.returning_clause.as_ref())?;
        self.optional_clause(&mut values, "returning", returning);
        let result = Sexp::list(values);
        match stmt.with_clause.as_ref() {
            Some(with_clause) => self.wrap_with(with_clause, result),
            None => Ok(result),
        }
    }

    fn lower_returning_clause(
        &mut self,
        clause: Option<&pg::ReturningClause>,
    ) -> Result<Vec<Sexp>> {
        let Some(clause) = clause else {
            return Ok(Vec::new());
        };

        let mut old_alias = None;
        let mut new_alias = None;
        for node in &clause.options {
            let option = match self.required_kind(node.node.as_ref(), "RETURNING option")? {
                PgNode::ReturningOption(option) => option,
                _ => return self.unsupported_error("RETURNING option"),
            };
            if option.value.is_empty() {
                return self.unsupported_error("RETURNING alias name");
            }
            let alias = match pg::ReturningOptionKind::try_from(option.option) {
                Ok(pg::ReturningOptionKind::ReturningOptionOld) => &mut old_alias,
                Ok(pg::ReturningOptionKind::ReturningOptionNew) => &mut new_alias,
                _ => return self.unsupported_error("RETURNING option kind"),
            };
            if alias.replace(option.value.as_str()).is_some() {
                return self.unsupported_error("duplicate RETURNING alias");
            }
        }

        let mut values = Vec::new();
        if let Some(alias) = old_alias {
            values.push(self.list("old-alias", [Sexp::string(alias)]));
        }
        if let Some(alias) = new_alias {
            values.push(self.list("new-alias", [Sexp::string(alias)]));
        }
        values.extend(self.lower_nodes(&clause.exprs)?);
        Ok(values)
    }

    fn lower_insert_columns(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        nodes
            .iter()
            .map(
                |node| match self.required_kind(node.node.as_ref(), "INSERT column")? {
                    PgNode::ResTarget(target)
                        if target.val.is_none() && !target.name.is_empty() =>
                    {
                        self.lower_res_target_name(target)
                    }
                    _ => self.unsupported("INSERT column"),
                },
            )
            .collect()
    }

    fn lower_res_target_name(&mut self, target: &pg::ResTarget) -> Result<Sexp> {
        if target.name.is_empty() {
            return self.unsupported("name");
        }

        if target.indirection.is_empty() {
            return Ok(Sexp::string(target.name.clone()));
        }

        let mut values = vec![Sexp::string(target.name.clone())];

        values.extend(
            target
                .indirection
                .iter()
                .map(|x| self.lower_name_indirection_item(x))
                .collect::<Result<Vec<_>>>()?,
        );

        Ok(self.list("name", values))
    }

    fn lower_name_indirection_item(&mut self, node: &pg::Node) -> Result<Sexp> {
        match self.required_kind(node.node.as_ref(), "indirection item")? {
            PgNode::String(field) => Ok(Sexp::string(field.sval.clone())),
            PgNode::AIndices(indices) => self.lower_indices(indices),
            _ => self.unsupported("indirection item"),
        }
    }

    fn lower_assignments(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        let mut assignments = Vec::with_capacity(nodes.len());
        let mut index = 0;
        while index < nodes.len() {
            let target =
                match self.required_kind(nodes[index].node.as_ref(), "UPDATE assignment")? {
                    PgNode::ResTarget(target) => target,
                    _ => return self.unsupported_error("UPDATE assignment"),
                };
            let reference = match target.val.as_ref().and_then(|value| value.node.as_ref()) {
                Some(PgNode::MultiAssignRef(reference)) => reference,
                _ => {
                    let value = self.required_node(target.val.as_deref(), "assignment value")?;
                    let assignment = [self.lower_res_target_name(target)?, self.lower(value)?];
                    if target.name.is_empty() {
                        return self.unsupported_error("UPDATE assignment");
                    }
                    assignments.push(self.list("assignment", assignment));
                    index += 1;
                    continue;
                }
            };
            if reference.colno != 1 || nodes.len() - index < reference.ncolumns.max(0) as usize {
                return self.unsupported_error("multi-column UPDATE assignment");
            }

            let mut names = vec![self.lower_res_target_name(target)?];

            for step in 1..reference.ncolumns.max(0) as usize {
                let target = match self
                    .required_kind(nodes[index + step].node.as_ref(), "UPDATE assignment")?
                {
                    PgNode::ResTarget(target) => target,
                    _ => return self.unsupported_error("UPDATE assignment"),
                };
                match target.val.as_ref().and_then(|value| value.node.as_ref()) {
                    Some(PgNode::MultiAssignRef(part))
                        if part.colno == step as i32 + 1 && part.ncolumns == reference.ncolumns =>
                    {
                        names.push(self.lower_res_target_name(target)?);
                    }
                    _ => return self.unsupported_error("multi-column UPDATE assignment"),
                }
            }

            let value = self.required_node(reference.source.as_deref(), "assignment value")?;
            let assignment = [self.list("columns", names), self.lower(value)?];
            assignments.push(self.list("assignment", assignment));
            index += reference.ncolumns.max(0) as usize;
        }

        Ok(assignments)
    }

    pub(super) fn lower_truncate(&mut self, stmt: &pg::TruncateStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("truncate")];

        values.extend(self.lower_nodes(&stmt.relations)?);
        if stmt.restart_seqs {
            values.push(self.symbol("restart-identity"));
        }
        match pg::DropBehavior::try_from(stmt.behavior) {
            Ok(pg::DropBehavior::DropRestrict) => {}
            Ok(pg::DropBehavior::DropCascade) => values.push(self.symbol("cascade")),
            _ => return self.unsupported("TRUNCATE behavior"),
        }

        Ok(Sexp::list(values))
    }
}

#[cfg(test)]
mod tests {
    use super::super::PostgresParser;

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
    fn lowers_current_of_expressions() {
        lowers(
            "DELETE FROM uctest WHERE CURRENT OF c1",
            "(delete (from (table (\"uctest\"))) (where (current-of \"c1\")))",
        );
        lowers(
            "UPDATE uctest SET f1 = 8 WHERE CURRENT OF c1",
            "(update (table (\"uctest\")) (set (assignment \"f1\" 8)) (where (current-of \"c1\")))",
        );
    }

    #[test]
    fn lowers_insert_default_values() {
        lowers(
            "insert into def_test default values",
            "(insert (into (table (\"def_test\"))) (columns) (source default-values))",
        );
    }

    #[test]
    fn lowers_dml_semantically() {
        lowers(
            "INSERT INTO users (name, active) VALUES ('Ada', true) RETURNING id",
            "(insert (into (table (\"users\"))) (columns \"name\" \"active\") (source (values (row \"Ada\" true))) (returning (column \"id\")))",
        );
        lowers(
            "UPDATE users SET name = 'Grace' WHERE id = 1 RETURNING id",
            "(update (table (\"users\")) (set (assignment \"name\" \"Grace\")) (where (op \"=\" (column \"id\") 1)) (returning (column \"id\")))",
        );
        lowers(
            "DELETE FROM users WHERE id = 1 RETURNING id",
            "(delete (from (table (\"users\"))) (where (op \"=\" (column \"id\") 1)) (returning (column \"id\")))",
        );
    }

    #[test]
    fn lowers_returning_old_new_aliases() {
        lowers(
            "INSERT INTO users (name) VALUES ('Ada') \
             RETURNING WITH (OLD AS old_row, NEW AS new_row) old_row.name, new_row.name",
            "(insert (into (table (\"users\"))) (columns \"name\") (source (values (row \"Ada\"))) (returning (old-alias \"old_row\") (new-alias \"new_row\") (column \"old_row\" \"name\") (column \"new_row\" \"name\")))",
        );
        lowers_returning(
            "INSERT INTO users (name) VALUES ('Ada') \
             RETURNING WITH (OLD AS old_row, NEW AS new_row) old_row.name, new_row.name",
            "(returning (old-alias \"old_row\") (new-alias \"new_row\") (column \"old_row\" \"name\") (column \"new_row\" \"name\"))",
        );
        lowers_returning(
            "UPDATE users SET name = 'Grace' \
             RETURNING WITH (NEW AS new_row) new_row.name",
            "(returning (new-alias \"new_row\") (column \"new_row\" \"name\"))",
        );
        lowers_returning(
            "DELETE FROM users \
             RETURNING WITH (OLD AS old_row) old_row.name",
            "(returning (old-alias \"old_row\") (column \"old_row\" \"name\"))",
        );
    }

    fn lowers_returning(sql: &str, expected: &str) {
        use pg_query::protobuf::node::Node as PgNode;

        let parsed = pg_query::parse(sql, 0).unwrap();
        let statement = parsed.protobuf.stmts[0].stmt.as_ref().unwrap();
        let clause = match statement.node.as_ref().unwrap() {
            PgNode::InsertStmt(stmt) => stmt.returning_clause.as_ref(),
            PgNode::UpdateStmt(stmt) => stmt.returning_clause.as_ref(),
            PgNode::DeleteStmt(stmt) => stmt.returning_clause.as_ref(),
            _ => panic!("expected DML statement"),
        };
        let mut parser = PostgresParser::new();
        let values = parser.lower_returning_clause(clause).unwrap();
        assert_eq!(parser.list("returning", values).to_string(), expected);
    }

    #[test]
    fn rejects_malformed_returning_options() {
        use pg_query::protobuf::{Node, ReturningClause, ReturningOption, node::Node as PgNode};

        let malformed_node = Node {
            node: Some(PgNode::Alias(Default::default())),
        };
        let invalid_enum = Node {
            node: Some(PgNode::ReturningOption(ReturningOption {
                option: 99,
                value: "row_alias".to_owned(),
                location: 0,
            })),
        };
        let old_alias = Node {
            node: Some(PgNode::ReturningOption(ReturningOption {
                option: pg_query::protobuf::ReturningOptionKind::ReturningOptionOld as i32,
                value: "row_alias".to_owned(),
                location: 0,
            })),
        };
        for options in [
            vec![malformed_node],
            vec![invalid_enum],
            vec![old_alias.clone(), old_alias],
        ] {
            let clause = ReturningClause {
                options,
                exprs: Vec::new(),
            };
            assert!(
                PostgresParser::new()
                    .lower_returning_clause(Some(&clause))
                    .is_err()
            );
        }
    }

    #[test]
    fn lowers_default_values_in_upserts() {
        assert_eq!(
            parse(
                "INSERT INTO users (id, active) VALUES ($1, DEFAULT) \
                     ON CONFLICT (id) DO UPDATE SET active = $2",
            )
            .map(|value| value.to_string().contains("default")),
            Ok(true)
        );
    }

    #[test]
    fn lowers_insert_on_conflict_semantically() {
        lowers(
            "INSERT INTO users (email) VALUES ('ada@example.com') ON CONFLICT (email) DO NOTHING",
            "(insert (into (table (\"users\"))) (columns \"email\") (source (values (row \"ada@example.com\"))) (on-conflict do-nothing (inference (columns (index-column \"email\")))))",
        );
        lowers(
            "INSERT INTO users (email, name) VALUES ('ada@example.com', 'Ada') \
                     ON CONFLICT (email) WHERE active DO UPDATE \
                     SET name = EXCLUDED.name WHERE users.active",
            "(insert (into (table (\"users\"))) (columns \"email\" \"name\") (source (values (row \"ada@example.com\" \"Ada\"))) (on-conflict do-update (inference (columns (index-column \"email\")) (where (column \"active\"))) (set (assignment \"name\" (column \"excluded\" \"name\"))) (where (column \"users\" \"active\"))))",
        );
    }

    #[test]
    fn lowers_merge_semantically() {
        lowers(
            "MERGE INTO target AS t USING source AS s ON t.id = s.id \
                     WHEN MATCHED AND s.active THEN UPDATE SET value = s.value \
                     WHEN NOT MATCHED THEN INSERT (id, value) VALUES (s.id, s.value)",
            "(merge (into (table (\"target\") (alias \"t\"))) (using (table (\"source\") (alias \"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when matched (condition (column \"s\" \"active\")) (update (set (assignment \"value\" (column \"s\" \"value\"))))) (when not-matched-by-target (insert (columns \"id\" \"value\") (values (column \"s\" \"id\") (column \"s\" \"value\")))))",
        );
    }

    #[test]
    fn lowers_merge_when_match_kinds() {
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED AND s.x > \
                     1 THEN DO NOTHING",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when matched (condition (op \">\" (column \"s\" \"x\") 1)) (do-nothing)))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED THEN \
                     DO NOTHING",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when not-matched-by-target (do-nothing)))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED BY \
                     SOURCE THEN DELETE",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when not-matched-by-source (delete)))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED BY \
                     SOURCE AND t.a > 0 THEN UPDATE SET a = 1",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when not-matched-by-source (condition (op \">\" (column \"t\" \"a\") 0)) (update (set (assignment \"a\" 1)))))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN \
                     DELETE",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when matched (delete)))",
        );
    }

    #[test]
    fn lowers_merge_insert_variants() {
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED THEN \
                     INSERT (a) OVERRIDING SYSTEM VALUE VALUES (2)",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when not-matched-by-target (insert (columns \"a\") (values 2) overriding-system-value)))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED THEN \
                     INSERT (a) OVERRIDING USER VALUE VALUES (2)",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when not-matched-by-target (insert (columns \"a\") (values 2) overriding-user-value)))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED THEN \
                     INSERT VALUES (1)",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when not-matched-by-target (insert (columns) (values 1))))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED THEN \
                     INSERT DEFAULT VALUES",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when not-matched-by-target (insert (columns) (values))))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN NOT MATCHED THEN \
                     INSERT (a) VALUES (DEFAULT)",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when not-matched-by-target (insert (columns \"a\") (values default))))",
        );
    }

    #[test]
    fn lowers_merge_sources_and_returning() {
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN \
                     UPDATE SET a = 1 RETURNING *",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when matched (update (set (assignment \"a\" 1)))) (returning (column *)))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN \
                     UPDATE SET a = 1 RETURNING t.id, s.id",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when matched (update (set (assignment \"a\" 1)))) (returning (column \"t\" \"id\") (column \"s\" \"id\")))",
        );
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN \
                     UPDATE SET (a, b) = (1, 2)",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when matched (update (set (assignment (columns \"a\" \"b\") (row 1 2))))))",
        );
        lowers(
            "WITH c AS (SELECT 1 AS id) MERGE INTO t USING c ON t.id = \
                     c.id WHEN MATCHED THEN DELETE",
            "(with (cte \"c\" (select (columns (as 1 \"id\")))) (merge (into (table (\"t\"))) (using (table (\"c\"))) (on (op \"=\" (column \"t\" \"id\") (column \"c\" \"id\"))) (when matched (delete))))",
        );
        lowers(
            "MERGE INTO t USING (SELECT id FROM x) sub ON t.id = sub.id \
                     WHEN MATCHED THEN DELETE",
            "(merge (into (table (\"t\"))) (using (subquery (select (columns (column \"id\")) (from (table (\"x\")))) (alias \"sub\"))) (on (op \"=\" (column \"t\" \"id\") (column \"sub\" \"id\"))) (when matched (delete)))",
        );
    }

    #[test]
    fn lowers_insert_on_conflict_variants() {
        lowers(
            "INSERT INTO t VALUES (1) ON CONFLICT ON CONSTRAINT t_a_key \
                     DO NOTHING",
            "(insert (into (table (\"t\"))) (columns) (source (values (row 1))) (on-conflict do-nothing (inference (constraint \"t_a_key\"))))",
        );
        lowers(
            "INSERT INTO t VALUES (1) ON CONFLICT DO NOTHING",
            "(insert (into (table (\"t\"))) (columns) (source (values (row 1))) (on-conflict do-nothing))",
        );
        lowers(
            "INSERT INTO t VALUES (1) ON CONFLICT (lower(b)) WHERE a > \
                     0 DO UPDATE SET a = 2",
            "(insert (into (table (\"t\"))) (columns) (source (values (row 1))) (on-conflict do-update (inference (columns (index-column (call \"lower\" (column \"b\")))) (where (op \">\" (column \"a\") 0))) (set (assignment \"a\" 2))))",
        );
        lowers(
            "INSERT INTO t VALUES (1) ON CONFLICT (a) DO UPDATE SET a = \
                     2 WHERE t.b > 0",
            "(insert (into (table (\"t\"))) (columns) (source (values (row 1))) (on-conflict do-update (inference (columns (index-column \"a\"))) (set (assignment \"a\" 2)) (where (op \">\" (column \"t\" \"b\") 0))))",
        );
        lowers(
            "INSERT INTO t VALUES (1, 2) ON CONFLICT (a) DO UPDATE SET \
                     (a, b) = (3, 4)",
            "(insert (into (table (\"t\"))) (columns) (source (values (row 1 2))) (on-conflict do-update (inference (columns (index-column \"a\"))) (set (assignment (columns \"a\" \"b\") (row 3 4)))))",
        );
        lowers(
            "INSERT INTO t VALUES (1) ON CONFLICT (a DESC) DO NOTHING",
            "(insert (into (table (\"t\"))) (columns) (source (values (row 1))) (on-conflict do-nothing (inference (columns (index-column \"a\" desc)))))",
        );
        lowers(
            "INSERT INTO t VALUES (1) ON CONFLICT (a) WHERE a > 0 DO \
                     NOTHING",
            "(insert (into (table (\"t\"))) (columns) (source (values (row 1))) (on-conflict do-nothing (inference (columns (index-column \"a\")) (where (op \">\" (column \"a\") 0)))))",
        );
        lowers(
            "INSERT INTO t (a) VALUES (1) ON CONFLICT (a) DO UPDATE SET \
                     a = 1 RETURNING *",
            "(insert (into (table (\"t\"))) (columns \"a\") (source (values (row 1))) (on-conflict do-update (inference (columns (index-column \"a\"))) (set (assignment \"a\" 1))) (returning (column *)))",
        );
    }

    #[test]
    fn lowers_merge_action_returning() {
        lowers(
            "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN \
                     UPDATE SET a = 1 RETURNING merge_action()",
            "(merge (into (table (\"t\"))) (using (table (\"s\"))) (on (op \"=\" (column \"t\" \"id\") (column \"s\" \"id\"))) (when matched (update (set (assignment \"a\" 1)))) (returning (merge-action)))",
        );
    }

    #[test]
    fn parses_insert_overriding() {
        lowers(
            "INSERT INTO t (a) OVERRIDING SYSTEM VALUE VALUES (1)",
            "(insert (into (table (\"t\"))) (columns \"a\") (overriding system-value) (source (values (row 1))))",
        );
        lowers(
            "INSERT INTO t (a) OVERRIDING USER VALUE VALUES (1)",
            "(insert (into (table (\"t\"))) (columns \"a\") (overriding user-value) (source (values (row 1))))",
        );
    }

    #[test]
    fn parses_multi_column_update() {
        lowers(
            "UPDATE t SET a = 1, (b, c) = (2, 3)",
            "(update (table (\"t\")) (set (assignment \"a\" 1) (assignment (columns \"b\" \"c\") (row 2 3))))",
        );
    }

    #[test]
    fn lowers_insert_columns_with_indirection() {
        lowers(
            "INSERT INTO t (a[1], b.c) VALUES (1, 2)",
            "(insert (into (table (\"t\"))) (columns (name \"a\" (index 1)) (name \"b\" \"c\")) (source (values (row 1 2))))",
        );
    }

    #[test]
    fn lowers_update_assignment_targets_with_indirection() {
        lowers(
            "UPDATE t SET b.c = 1, a[2] = 2 WHERE true",
            "(update (table (\"t\")) (set (assignment (name \"b\" \"c\") 1) (assignment (name \"a\" (index 2)) 2)) (where true))",
        );
        lowers(
            "UPDATE t SET a[1:2] = 3",
            "(update (table (\"t\")) (set (assignment (name \"a\" (slice 1 2)) 3)))",
        );
        lowers(
            "UPDATE t SET (b.c, d) = (1, 2)",
            "(update (table (\"t\")) (set (assignment (columns (name \"b\" \"c\") \"d\") (row 1 2))))",
        );
    }
}
