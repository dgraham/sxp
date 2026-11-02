use super::*;

impl PostgresParser {
    pub(super) fn lower_transaction(&mut self, stmt: &pg::TransactionStmt) -> Result<Sexp> {
        let kind = pg::TransactionStmtKind::try_from(stmt.kind)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: transaction statement", 0))?;
        match kind {
            pg::TransactionStmtKind::TransStmtBegin => {
                let mut values = vec![self.symbol("begin")];

                values.extend(self.lower_transaction_modes(&stmt.options)?);
                Ok(Sexp::list(values))
            }
            pg::TransactionStmtKind::TransStmtStart => {
                let mut values = vec![self.symbol("start-transaction")];

                values.extend(self.lower_transaction_modes(&stmt.options)?);
                Ok(Sexp::list(values))
            }
            pg::TransactionStmtKind::TransStmtCommit => {
                let mut values = vec![self.symbol("commit")];

                if stmt.chain {
                    values.push(self.symbol("chain"));
                }
                Ok(Sexp::list(values))
            }
            pg::TransactionStmtKind::TransStmtRollback => {
                if !stmt.savepoint_name.is_empty() {
                    return Ok(self.list(
                        "rollback-to-savepoint",
                        [Sexp::string(stmt.savepoint_name.clone())],
                    ));
                }

                let mut values = vec![self.symbol("rollback")];

                if stmt.chain {
                    values.push(self.symbol("chain"));
                }
                Ok(Sexp::list(values))
            }
            pg::TransactionStmtKind::TransStmtRollbackTo => Ok(self.list(
                "rollback-to-savepoint",
                [Sexp::string(stmt.savepoint_name.clone())],
            )),
            pg::TransactionStmtKind::TransStmtSavepoint => {
                Ok(self.list("savepoint", [Sexp::string(stmt.savepoint_name.clone())]))
            }
            pg::TransactionStmtKind::TransStmtRelease => Ok(self.list(
                "release-savepoint",
                [Sexp::string(stmt.savepoint_name.clone())],
            )),
            pg::TransactionStmtKind::TransStmtPrepare => {
                Ok(self.list("prepare-transaction", [Sexp::string(stmt.gid.clone())]))
            }
            pg::TransactionStmtKind::TransStmtCommitPrepared => {
                Ok(self.list("commit-prepared", [Sexp::string(stmt.gid.clone())]))
            }
            pg::TransactionStmtKind::TransStmtRollbackPrepared => {
                Ok(self.list("rollback-prepared", [Sexp::string(stmt.gid.clone())]))
            }
            _ => self.unsupported("transaction statement"),
        }
    }

    fn lower_transaction_modes(&mut self, options: &[pg::Node]) -> Result<Vec<Sexp>> {
        options
            .iter()
            .map(|node| {
                let option = match self.required_kind(node.node.as_ref(), "transaction option")? {
                    PgNode::DefElem(option) => option,
                    _ => return self.unsupported("transaction option"),
                };
                match option.defname.as_str() {
                    "transaction_isolation" => {
                        let isolation_level_node = self
                            .required_node(option.arg.as_deref(), "transaction isolation level")?;
                        let level = self.string_constant(isolation_level_node)?;
                        let level = match level.as_str() {
                            "read uncommitted" => "read-uncommitted",
                            "read committed" => "read-committed",
                            "repeatable read" => "repeatable-read",
                            "serializable" => "serializable",
                            _ => return self.unsupported(format!("isolation level {level}")),
                        };
                        let isolation = [self.symbol(level)];
                        Ok(self.list("isolation", isolation))
                    }
                    "transaction_read_only" => {
                        let integer_option = match self.integer_option(option)? {
                            0 => "read-write",
                            _ => "read-only",
                        };
                        Ok(self.symbol(integer_option))
                    }
                    "transaction_deferrable" => {
                        let integer_option = match self.integer_option(option)? {
                            0 => "not-deferrable",
                            _ => "deferrable",
                        };
                        Ok(self.symbol(integer_option))
                    }
                    other => self.unsupported(format!("transaction option {other}")),
                }
            })
            .collect()
    }

    fn string_constant(&mut self, node: &pg::Node) -> Result<String> {
        match self.required_kind(node.node.as_ref(), "string constant")? {
            PgNode::AConst(value) => match value.val.as_ref() {
                Some(a_const::Val::Sval(value)) => Ok(value.sval.clone()),
                _ => self.unsupported_error("string constant"),
            },
            PgNode::String(value) => Ok(value.sval.clone()),
            _ => self.unsupported_error("string constant"),
        }
    }

    fn integer_option(&mut self, option: &pg::DefElem) -> Result<i32> {
        let node = self.required_node(option.arg.as_deref(), "integer option")?;
        let constant = match self.required_kind(node.node.as_ref(), "integer option")? {
            PgNode::AConst(value) => value,
            _ => return self.unsupported_error("integer option"),
        };
        match constant.val.as_ref() {
            Some(a_const::Val::Ival(integer)) => Ok(integer.ival),
            _ => self.unsupported_error("integer option"),
        }
    }

    pub(super) fn lower_variable_set(&mut self, stmt: &pg::VariableSetStmt) -> Result<Sexp> {
        let kind = pg::VariableSetKind::try_from(stmt.kind)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: SET statement", 0))?;
        match kind {
            pg::VariableSetKind::VarSetMulti => {
                let name = match stmt.name.as_str() {
                    "TRANSACTION" => "set-transaction",
                    "SESSION CHARACTERISTICS" => "set-session-characteristics",
                    "TRANSACTION SNAPSHOT" => "set-transaction-snapshot",
                    other => return self.unsupported(format!("SET {other}")),
                };
                let mut values = vec![self.symbol(name)];

                if stmt.is_local {
                    values.push(self.symbol("local"));
                }
                if name == "set-transaction-snapshot" {
                    values.extend(self.lower_nodes(&stmt.args)?);
                } else {
                    values.extend(self.lower_transaction_modes(&stmt.args)?);
                }
                Ok(Sexp::list(values))
            }
            pg::VariableSetKind::VarReset => {
                Ok(self.list("reset", [Sexp::string(stmt.name.clone())]))
            }
            pg::VariableSetKind::VarResetAll => {
                let all_reset = [self.symbol("all")];
                Ok(self.list("reset", all_reset))
            }
            pg::VariableSetKind::VarSetValue
            | pg::VariableSetKind::VarSetDefault
            | pg::VariableSetKind::VarSetCurrent => {
                let mut values = vec![self.symbol("set")];

                if stmt.is_local {
                    values.push(self.symbol("local"));
                }

                values.push(Sexp::string(stmt.name.clone()));
                match kind {
                    pg::VariableSetKind::VarSetDefault => values.push(self.symbol("default")),
                    pg::VariableSetKind::VarSetCurrent => values.push(self.symbol("from-current")),
                    _ => values.extend(self.lower_nodes(&stmt.args)?),
                }
                Ok(Sexp::list(values))
            }
            _ => self.unsupported("SET statement"),
        }
    }

    pub(super) fn lower_constraints_set(&mut self, stmt: &pg::ConstraintsSetStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("set-constraints")];

        if stmt.constraints.is_empty() {
            values.push(self.symbol("all"));
        } else {
            let constraints = stmt
                .constraints
                .iter()
                .map(|node| {
                    match self.required_kind(node.node.as_ref(), "SET CONSTRAINTS name")? {
                        PgNode::RangeVar(relation) => self.lower_relation(relation),
                        _ => self.unsupported("SET CONSTRAINTS name"),
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("constraints", constraints));
        }

        values.push(self.symbol(if stmt.deferred {
            "deferred"
        } else {
            "immediate"
        }));

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
    fn lowers_set_constraints_semantically() {
        lowers(
            "SET CONSTRAINTS ALL DEFERRED;",
            "(set-constraints all deferred)",
        );
        lowers(
            "SET CONSTRAINTS ALL IMMEDIATE;",
            "(set-constraints all immediate)",
        );
        lowers(
            "SET CONSTRAINTS fk_trig DEFERRED;",
            "(set-constraints (constraints (table (\"fk_trig\"))) deferred)",
        );
        lowers(
            "SET CONSTRAINTS a, b IMMEDIATE;",
            "(set-constraints (constraints (table (\"a\")) (table (\"b\"))) immediate)",
        );
        lowers(
            "SET CONSTRAINTS schema.a DEFERRED;",
            "(set-constraints (constraints (table (\"schema\" \"a\"))) deferred)",
        );
    }

    #[test]
    fn lowers_transaction_and_set_semantically() {
        lowers("BEGIN;", "(begin)");
        lowers(
            "START TRANSACTION READ ONLY, DEFERRABLE;",
            "(start-transaction read-only deferrable)",
        );
        lowers(
            "START TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY, DEFERRABLE",
            "(start-transaction (isolation repeatable-read) read-only deferrable)",
        );
        lowers(
            "BEGIN ISOLATION LEVEL SERIALIZABLE;",
            "(begin (isolation serializable))",
        );
        lowers(
            "BEGIN TRANSACTION READ WRITE NOT DEFERRABLE;",
            "(begin read-write not-deferrable)",
        );
        lowers("COMMIT AND CHAIN;", "(commit chain)");
        lowers("ROLLBACK AND CHAIN;", "(rollback chain)");
        lowers("SAVEPOINT s;", "(savepoint \"s\")");
        lowers("RELEASE SAVEPOINT s;", "(release-savepoint \"s\")");
        lowers("ROLLBACK TO SAVEPOINT s;", "(rollback-to-savepoint \"s\")");
        lowers("ROLLBACK TO SAVEPOINT s", "(rollback-to-savepoint \"s\")");
        lowers(
            "PREPARE TRANSACTION 'gid';",
            "(prepare-transaction \"gid\")",
        );
        lowers("COMMIT PREPARED 'gid';", "(commit-prepared \"gid\")");
        lowers("ROLLBACK PREPARED 'gid';", "(rollback-prepared \"gid\")");
        lowers("SET TRANSACTION READ ONLY;", "(set-transaction read-only)");
        lowers(
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE;",
            "(set-transaction (isolation serializable))",
        );
        lowers(
            "SET SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL SERIALIZABLE;",
            "(set-session-characteristics (isolation serializable))",
        );
        lowers("SET LOCAL x = 'y';", "(set local \"x\" \"y\")");
        lowers(
            "SET LOCAL datestyle TO postgres, iso",
            "(set local \"datestyle\" \"postgres\" \"iso\")",
        );
        lowers("SET x TO DEFAULT;", "(set \"x\" default)");
        lowers("SET x FROM CURRENT;", "(set \"x\" from-current)");
        lowers("SET x = 'a', 'b';", "(set \"x\" \"a\" \"b\")");
        lowers("RESET x;", "(reset \"x\")");
        lowers("RESET ALL;", "(reset all)");
        lowers("RESET ALL", "(reset all)");
    }

    #[test]
    fn lowers_set_transaction_snapshot() {
        lowers(
            "SET TRANSACTION SNAPSHOT 'FFF-FFF-F'",
            "(set-transaction-snapshot \"FFF-FFF-F\")",
        );
    }
}
