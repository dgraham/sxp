use super::*;

impl PostgresParser {
    pub(super) fn lower_current_of(&mut self, expr: &pg::CurrentOfExpr) -> Result<Sexp> {
        if expr.cursor_name.is_empty() {
            return self.unsupported("CURRENT OF cursor");
        }

        Ok(self.list("current-of", [Sexp::string(expr.cursor_name.clone())]))
    }

    pub(super) fn lower_relation(&mut self, relation: &pg::RangeVar) -> Result<Sexp> {
        let mut names = Vec::new();
        if !relation.catalogname.is_empty() {
            names.push(Sexp::string(relation.catalogname.clone()));
        }
        if !relation.schemaname.is_empty() {
            names.push(Sexp::string(relation.schemaname.clone()));
        }
        names.push(Sexp::string(relation.relname.clone()));
        let mut values = vec![self.symbol("table")];

        if !relation.inh {
            let true_only = [self.symbol("true")];
            values.push(self.list("only", true_only));
        }

        values.push(Sexp::list(names));
        if let Some(alias) = relation.alias.as_ref() {
            values.push(self.lower_alias(alias)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_range_subselect(&mut self, subselect: &pg::RangeSubselect) -> Result<Sexp> {
        let subquery = self.required_node(subselect.subquery.as_deref(), "subquery relation")?;
        let mut values = vec![self.symbol("subquery"), self.lower(subquery)?];

        if subselect.lateral {
            values.push(self.symbol("lateral"));
        }
        if let Some(alias) = subselect.alias.as_ref() {
            values.push(self.lower_alias(alias)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alias(&mut self, alias: &pg::Alias) -> Result<Sexp> {
        let mut values = vec![self.symbol("alias"), Sexp::string(alias.aliasname.clone())];

        if !alias.colnames.is_empty() {
            let columns = self.lower_nodes(&alias.colnames)?;
            values.push(self.list("columns", columns));
        }
        Ok(Sexp::list(values))
    }

    pub(super) fn wrap_with(&mut self, with_clause: &pg::WithClause, body: Sexp) -> Result<Sexp> {
        let mut values = vec![self.symbol("with")];

        if with_clause.recursive {
            values.push(self.symbol("recursive"));
        }

        values.extend(self.lower_nodes(&with_clause.ctes)?);
        values.push(body);

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_cte(&mut self, cte: &pg::CommonTableExpr) -> Result<Sexp> {
        let materialization = match pg::CteMaterialize::try_from(cte.ctematerialized) {
            Ok(pg::CteMaterialize::Default) => None,
            Ok(pg::CteMaterialize::Always) => Some("materialized"),
            Ok(pg::CteMaterialize::Never) => Some("not-materialized"),
            _ => return self.unsupported("CTE materialization"),
        };
        let cte_query_node = self.required_node(cte.ctequery.as_deref(), "CTE query")?;
        let mut values = vec![
            self.symbol("cte"),
            Sexp::string(cte.ctename.clone()),
            self.lower(cte_query_node)?,
        ];

        if !cte.aliascolnames.is_empty() {
            let columns = self.lower_nodes(&cte.aliascolnames)?;
            values.push(self.list("columns", columns));
        }
        if let Some(materialization) = materialization {
            values.push(self.symbol(materialization));
        }
        if let Some(search) = cte.search_clause.as_ref() {
            values.push(self.lower_cte_search(search)?);
        }
        if let Some(cycle) = cte.cycle_clause.as_ref() {
            values.push(self.lower_cte_cycle(cycle)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_cte_search(&mut self, clause: &pg::CteSearchClause) -> Result<Sexp> {
        let order = if clause.search_breadth_first {
            "breadth-first"
        } else {
            "depth-first"
        };
        let by = self.lower_cte_columns(&clause.search_col_list)?;
        let search_string = [
            self.symbol(order),
            self.list("by", by),
            self.list("set", [Sexp::string(clause.search_seq_column.clone())]),
        ];

        Ok(self.list("search", search_string))
    }

    pub(super) fn lower_cte_cycle(&mut self, clause: &pg::CteCycleClause) -> Result<Sexp> {
        let by = self.lower_cte_columns(&clause.cycle_col_list)?;
        let mut values = vec![
            self.symbol("cycle"),
            self.list("by", by),
            self.list("set", [Sexp::string(clause.cycle_mark_column.clone())]),
            self.list("using", [Sexp::string(clause.cycle_path_column.clone())]),
        ];

        self.optional_node_clause(&mut values, "value", clause.cycle_mark_value.as_deref())?;
        self.optional_node_clause(&mut values, "default", clause.cycle_mark_default.as_deref())?;

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_cte_columns(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        nodes
            .iter()
            .map(|node| {
                let cte_column_name = [Sexp::string(self.string_node(node)?)];
                Ok(self.list("column", cte_column_name))
            })
            .collect()
    }

    pub(super) fn lower_join(&mut self, join: &pg::JoinExpr) -> Result<Sexp> {
        let kind = match pg::JoinType::try_from(join.jointype) {
            Ok(pg::JoinType::JoinInner) => "inner",
            Ok(pg::JoinType::JoinLeft) => "left",
            Ok(pg::JoinType::JoinRight) => "right",
            Ok(pg::JoinType::JoinFull) => "full",
            _ => return self.unsupported("JOIN type"),
        };
        let cross = join.quals.is_none() && join.using_clause.is_empty() && !join.is_natural;
        let right_relation_node =
            self.required_node(join.rarg.as_deref(), "right JOIN relation")?;
        let left_relation_node = self.required_node(join.larg.as_deref(), "left JOIN relation")?;
        let mut values = vec![
            self.symbol("join"),
            self.symbol(if cross { "cross" } else { kind }),
            self.lower(left_relation_node)?,
            self.lower(right_relation_node)?,
        ];

        if let Some(qualifier) = join.quals.as_ref() {
            let on = [self.lower(qualifier)?];
            values.push(self.list("on", on));
        } else if !join.using_clause.is_empty() {
            let using = self.lower_nodes(&join.using_clause)?;
            values.push(self.list("using", using));
        }
        if let Some(alias) = join.join_using_alias.as_ref() {
            let mut using_alias = vec![Sexp::string(alias.aliasname.clone())];

            if !alias.colnames.is_empty() {
                let columns = self.lower_nodes(&alias.colnames)?;
                using_alias.push(self.list("columns", columns));
            }

            values.push(self.list("using-alias", using_alias));
        }
        if let Some(alias) = join.alias.as_ref() {
            values.push(self.lower_alias(alias)?);
        }
        if join.is_natural {
            values.push(self.symbol("natural"));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_range_function(&mut self, function: &pg::RangeFunction) -> Result<Sexp> {
        if !function.is_rowsfrom && function.functions.len() != 1 {
            return self.unsupported("function relation");
        }

        let mut values = vec![self.symbol(if function.is_rowsfrom {
            "rows-from"
        } else {
            "function"
        })];

        if function.lateral {
            values.push(self.symbol("lateral"));
        }
        for node in &function.functions {
            let pair = match self.required_kind(node.node.as_ref(), "function relation")? {
                PgNode::List(pair) => pair,
                _ => return self.unsupported("function relation"),
            };
            let [expr, definition] = pair.items.as_slice() else {
                return self.unsupported("function relation");
            };
            let columns = match definition.node.as_ref() {
                Some(PgNode::List(columns)) if !columns.items.is_empty() => {
                    Some(self.lower_nodes(&columns.items)?)
                }
                None | Some(PgNode::List(_)) => None,
                _ => return self.unsupported("function relation"),
            };
            if function.is_rowsfrom {
                let mut item = vec![self.symbol("function"), self.lower(expr)?];

                if let Some(columns) = columns {
                    item.push(self.list("columns", columns));
                }

                values.push(Sexp::list(item));
            } else {
                values.push(self.lower(expr)?);

                let top_level = match function.coldeflist.is_empty() {
                    true => columns,
                    false => Some(self.lower_nodes(&function.coldeflist)?),
                };
                if let Some(columns) = top_level {
                    values.push(self.list("columns", columns));
                }
            }
        }
        if function.ordinality {
            values.push(self.symbol("with-ordinality"));
        }
        if let Some(alias) = function.alias.as_ref() {
            values.push(self.lower_alias(alias)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_table_sample(&mut self, sample: &pg::RangeTableSample) -> Result<Sexp> {
        let tablesample_relation_node =
            self.required_node(sample.relation.as_deref(), "TABLESAMPLE relation")?;
        let mut values = vec![
            self.symbol("table-sample"),
            self.lower(tablesample_relation_node)?,
        ];

        values.extend(
            sample
                .method
                .iter()
                .map(|x| self.string_node(x))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(Sexp::string),
        );
        if !sample.args.is_empty() {
            let args = self.lower_nodes(&sample.args)?;
            values.push(self.list("args", args));
        }
        if let Some(repeatable) = sample.repeatable.as_deref() {
            let repeat = [self.lower(repeatable)?];
            values.push(self.list("repeatable", repeat));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_locking_clause(&mut self, clause: &pg::LockingClause) -> Result<Sexp> {
        let strength = match pg::LockClauseStrength::try_from(clause.strength) {
            Ok(pg::LockClauseStrength::LcsForkeyshare) => "for-key-share",
            Ok(pg::LockClauseStrength::LcsForshare) => "for-share",
            Ok(pg::LockClauseStrength::LcsFornokeyupdate) => "for-no-key-update",
            Ok(pg::LockClauseStrength::LcsForupdate) => "for-update",
            _ => return self.unsupported("row locking strength"),
        };
        let mut values = vec![self.symbol("locking"), self.symbol(strength)];

        if !clause.locked_rels.is_empty() {
            let of = self.lower_nodes(&clause.locked_rels)?;
            values.push(self.list("of", of));
        }
        match pg::LockWaitPolicy::try_from(clause.wait_policy) {
            Ok(pg::LockWaitPolicy::LockWaitBlock) => {}
            Ok(pg::LockWaitPolicy::LockWaitSkip) => values.push(self.symbol("skip-locked")),
            Ok(pg::LockWaitPolicy::LockWaitError) => values.push(self.symbol("no-wait")),
            _ => return self.unsupported("row locking wait policy"),
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_table_like_clause(&mut self, clause: &pg::TableLikeClause) -> Result<Sexp> {
        let like_relation = self.required(clause.relation.as_ref(), "LIKE relation")?;
        let relation = self.lower_relation(like_relation)?;
        let mut values = vec![self.symbol("like"), relation];

        if clause.options == 0x7FFF_FFFF {
            values.push(self.symbol("all"));
        } else {
            const LIKE_OPTIONS: &[(u32, &str)] = &[
                (0x001, "comments"),
                (0x002, "compression"),
                (0x004, "constraints"),
                (0x008, "defaults"),
                (0x010, "generated"),
                (0x020, "identity"),
                (0x040, "indexes"),
                (0x080, "statistics"),
                (0x100, "storage"),
            ];
            for (bit, name) in LIKE_OPTIONS {
                if clause.options & bit != 0 {
                    values.push(self.symbol(name));
                }
            }
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_range_table_func(&mut self, function: &pg::RangeTableFunc) -> Result<Sexp> {
        let mut values = vec![self.symbol("xmltable")];

        let row_expression_node =
            self.required_node(function.rowexpr.as_deref(), "XMLTABLE row expression")?;
        if !function.namespaces.is_empty() {
            let namespace_value_node = function
                .namespaces
                .iter()
                .map(|node| {
                    let namespace =
                        match self.required_kind(node.node.as_ref(), "XMLTABLE namespace")? {
                            PgNode::ResTarget(target) => target,
                            _ => return self.unsupported("XMLTABLE namespace"),
                        };
                    let namespace_value_node =
                        self.required_node(namespace.val.as_deref(), "XMLTABLE namespace value")?;
                    let value = self.lower(namespace_value_node)?;
                    if namespace.name.is_empty() {
                        Ok(self.list("default", [value]))
                    } else {
                        Ok(self.list("namespace", [Sexp::string(namespace.name.clone()), value]))
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("namespaces", namespace_value_node));
        }

        values.push(self.lower(row_expression_node)?);

        let columns = function
            .columns
            .iter()
            .map(
                |node| match self.required_kind(node.node.as_ref(), "XMLTABLE column")? {
                    PgNode::RangeTableFuncCol(column) => self.lower_range_table_func_col(column),
                    _ => self.unsupported("XMLTABLE column"),
                },
            )
            .collect::<Result<Vec<_>>>()?;
        if let Some(document) = function.docexpr.as_deref() {
            let passing = [self.lower(document)?];
            values.push(self.list("passing", passing));
        }

        values.push(self.list("columns", columns));
        if let Some(alias) = function.alias.as_ref() {
            values.push(self.lower_alias(alias)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_range_table_func_col(
        &mut self,
        column: &pg::RangeTableFuncCol,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("xml-column"),
            Sexp::string(column.colname.clone()),
        ];

        if let Some(type_name) = column.type_name.as_ref() {
            values.push(self.lower_type(type_name)?);
        }
        if let Some(path) = column.colexpr.as_deref() {
            let lowered_path = [self.lower(path)?];
            values.push(self.list("path", lowered_path));
        }
        if let Some(default) = column.coldefexpr.as_deref() {
            let lowered_default = [self.lower(default)?];
            values.push(self.list("default", lowered_default));
        }
        if column.for_ordinality {
            values.push(self.symbol("for-ordinality"));
        }
        if column.is_not_null {
            values.push(self.symbol("not-null"));
        }

        Ok(Sexp::list(values))
    }
}
