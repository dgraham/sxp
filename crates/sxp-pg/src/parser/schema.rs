use super::*;

impl PostgresParser {
    pub(super) fn lower_column_definition(&mut self, column: &pg::ColumnDef) -> Result<Sexp> {
        let mut values = vec![self.symbol("column"), Sexp::string(column.colname.clone())];

        if let Some(type_name) = column.type_name.as_ref() {
            values.push(self.lower_type(type_name)?);
        }
        self.push_column_storage_clauses(&mut values, column)?;
        if !column.fdwoptions.is_empty() {
            let options = self.lower_nodes(&column.fdwoptions)?;
            values.push(self.list("options", options));
        }
        let constraint_start = values.len();
        if column.is_not_null {
            values.push(Sexp::list([self.symbol("not-null")]));
        }
        if let Some(default) = column.raw_default.as_ref() {
            let lowered_default = [self.lower(default)?];
            values.push(self.list("default", lowered_default));
        }
        for node in &column.constraints {
            let PgNode::Constraint(constraint) =
                self.required_kind(node.node.as_ref(), "column constraint")?
            else {
                return self.unsupported("column constraint");
            };
            let kind = pg::ConstrType::try_from(constraint.contype)
                .map_err(|_| ParseError::new("unsupported constraint", 0))?;
            let Some(attribute) = constraint_attribute(kind) else {
                values.push(self.lower(node)?);
                continue;
            };
            let foldable = values.len() > constraint_start;
            match values.last_mut() {
                Some(Sexp::List(items)) if foldable => {
                    let position = match items.last() {
                        Some(Sexp::List(last)) if matches!(last.first(), Some(Sexp::Atom(Atom::Symbol(name))) if name.as_ref() == "name") => {
                            items.len() - 1
                        }
                        _ => items.len(),
                    };
                    let mut item_values = std::mem::take(items).into_vec();
                    item_values.insert(position, self.symbol(attribute));
                    *items = item_values.into_boxed_slice();
                }
                _ => values.push(Sexp::list([self.symbol(attribute)])),
            }
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn push_column_storage_clauses(
        &mut self,
        values: &mut Vec<Sexp>,
        column: &pg::ColumnDef,
    ) -> Result<()> {
        if let Some(clause) = column.coll_clause.as_ref() {
            values.push(self.lower_collation_clause(clause, "column collation")?);
        }
        if !column.compression.is_empty() {
            values.push(self.list("compression", [Sexp::string(column.compression.clone())]));
        }
        if !column.storage_name.is_empty() {
            values.push(self.list("storage", [Sexp::string(column.storage_name.clone())]));
        }

        Ok(())
    }

    pub(super) fn lower_type(&mut self, typename: &pg::TypeName) -> Result<Sexp> {
        let mut inner = typename.clone();
        let setof = inner.setof;
        inner.setof = false;
        let core = self.lower_plain_type(&inner)?;
        Ok(if setof {
            self.list("setof", [core])
        } else {
            core
        })
    }

    pub(super) fn lower_plain_type(&mut self, typename: &pg::TypeName) -> Result<Sexp> {
        let names = typename
            .names
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?;
        let mut values = vec![self.symbol("type")];

        values.extend(names.into_iter().map(Sexp::string));
        if typename.pct_type {
            values.push(self.symbol("%-type"));
        }
        if !typename.typmods.is_empty() {
            let modifiers = self.lower_nodes(&typename.typmods)?;
            values.push(self.list("modifiers", modifiers));
        }
        for bound in &typename.array_bounds {
            let node = match self.required_kind(bound.node.as_ref(), "array bound")? {
                PgNode::Integer(integer) => integer,
                _ => return self.unsupported("array bound"),
            };
            if node.ival < 0 {
                values.push(Sexp::list([self.symbol("array")]));
            } else if node.ival == 0 {
                return self.unsupported("array bound");
            } else {
                let array = [self.symbol(&node.ival.to_string())];
                values.push(self.list("array", array));
            }
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_qualified_name(&mut self, nodes: &[pg::Node]) -> Result<Sexp> {
        let qualified_name_parts = nodes
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(Sexp::string);
        Ok(self.list("name", qualified_name_parts))
    }

    pub(super) fn lower_index_element(&mut self, element: &pg::IndexElem) -> Result<Sexp> {
        if !element.indexcolname.is_empty() {
            return self.unsupported("CREATE INDEX column options");
        }

        let expr = match (element.name.is_empty(), element.expr.as_deref()) {
            (false, None) => Sexp::string(element.name.clone()),
            (true, Some(expr)) => self.lower(expr)?,
            _ => return self.unsupported("CREATE INDEX column"),
        };
        let direction = match pg::SortByDir::try_from(element.ordering) {
            Ok(pg::SortByDir::SortbyDefault | pg::SortByDir::SortbyAsc) => None,
            Ok(pg::SortByDir::SortbyDesc) => Some("desc"),
            _ => return self.unsupported("CREATE INDEX column ordering"),
        };
        let nulls = match pg::SortByNulls::try_from(element.nulls_ordering) {
            Ok(pg::SortByNulls::SortbyNullsDefault) => None,
            Ok(pg::SortByNulls::SortbyNullsFirst) => Some("nulls-first"),
            Ok(pg::SortByNulls::SortbyNullsLast) => Some("nulls-last"),
            _ => return self.unsupported("CREATE INDEX column null ordering"),
        };
        let mut values = vec![self.symbol("index-column"), expr];

        if !element.collation.is_empty() {
            let collate = self.lower_nodes(&element.collation)?;
            values.push(self.list("collate", collate));
        }
        if !element.opclass.is_empty() {
            let mut opclass = element
                .opclass
                .iter()
                .map(|x| self.string_node(x))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(Sexp::string)
                .collect::<Vec<_>>();
            if !element.opclassopts.is_empty() {
                let options = self.lower_nodes(&element.opclassopts)?;
                opclass.push(self.list("options", options));
            }

            values.push(self.list("opclass", opclass));
        }
        if let Some(direction) = direction {
            values.push(self.symbol(direction));
        }
        if let Some(nulls) = nulls {
            values.push(self.symbol(nulls));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_definition_option(&mut self, option: &pg::DefElem) -> Result<Sexp> {
        if !matches!(
            pg::DefElemAction::try_from(option.defaction),
            Ok(pg::DefElemAction::DefelemUnspec)
        ) {
            return self.unsupported("DDL option action");
        }
        let name = if option.defnamespace.is_empty() {
            option.defname.clone()
        } else {
            format!("{}.{}", option.defnamespace, option.defname)
        };
        let mut values = vec![self.symbol("option"), Sexp::string(name)];

        if let Some(argument) = option.arg.as_deref() {
            values.push(
                match self.required_kind(argument.node.as_ref(), "option value")? {
                    PgNode::TypeName(typename)
                        if typename.names.len() == 1
                            && !typename.setof
                            && !typename.pct_type
                            && typename.typmods.is_empty()
                            && typename.array_bounds.is_empty() =>
                    {
                        Sexp::string(self.string_node(&typename.names[0])?)
                    }
                    _ => self.lower(argument)?,
                },
            );
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_partition_spec(
        &mut self,
        specification: &pg::PartitionSpec,
    ) -> Result<Sexp> {
        let strategy = match pg::PartitionStrategy::try_from(specification.strategy) {
            Ok(pg::PartitionStrategy::List) => "list",
            Ok(pg::PartitionStrategy::Range) => "range",
            Ok(pg::PartitionStrategy::Hash) => "hash",
            _ => return self.unsupported("partition strategy"),
        };
        let columns = self.lower_nodes(&specification.part_params)?;
        let partition_by = [self.symbol(strategy), self.list("columns", columns)];

        Ok(self.list("partition-by", partition_by))
    }

    pub(super) fn lower_partition_element(&mut self, element: &pg::PartitionElem) -> Result<Sexp> {
        let expr = match (element.name.is_empty(), element.expr.as_deref()) {
            (false, None) => Sexp::string(element.name.clone()),
            (true, Some(expr)) => self.lower(expr)?,
            _ => return self.unsupported("partition column"),
        };
        if element.collation.is_empty() && element.opclass.is_empty() {
            return Ok(expr);
        }

        let mut values = vec![self.symbol("partition-column"), expr];

        if !element.collation.is_empty() {
            let collate = self.lower_nodes(&element.collation)?;
            values.push(self.list("collate", collate));
        }
        if !element.opclass.is_empty() {
            let operator_classes = element
                .opclass
                .iter()
                .map(|x| self.string_node(x))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(Sexp::string);
            values.push(self.list("opclass", operator_classes));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_partition_bound_spec(
        &mut self,
        bound: &pg::PartitionBoundSpec,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("bounds")];

        if bound.is_default {
            values.push(self.symbol("default"));
            return Ok(Sexp::list(values));
        }
        match bound.strategy.as_str() {
            "r" => {
                let from = self.lower_partition_datums(&bound.lowerdatums)?;
                values.push(self.list("from", from));

                let to = self.lower_partition_datums(&bound.upperdatums)?;
                values.push(self.list("to", to));
            }
            "l" => {
                let partition_list_datums = self.lower_partition_datums(&bound.listdatums)?;
                values.push(self.list("in", partition_list_datums))
            }
            "h" => {
                let modulus = [self.symbol(&bound.modulus.to_string())];
                values.push(self.list("modulus", modulus));

                let remainder = [self.symbol(&bound.remainder.to_string())];
                values.push(self.list("remainder", remainder));
            }
            _ => return self.unsupported("partition bound strategy"),
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_partition_datums(&mut self, datums: &[pg::Node]) -> Result<Vec<Sexp>> {
        datums
            .iter()
            .map(|datum| {
                match self.required_kind(datum.node.as_ref(), "partition bound datum")? {
                    PgNode::PartitionRangeDatum(range_datum) => {
                        self.lower_partition_range_datum(range_datum)
                    }
                    PgNode::ColumnRef(reference) => {
                        if let Some(keyword) = minmax_keyword(reference) {
                            return Ok(Sexp::list([self.symbol(keyword)]));
                        }
                        self.list_value(datum)
                    }
                    _ => self.list_value(datum),
                }
            })
            .collect()
    }

    pub(super) fn list_value(&mut self, node: &pg::Node) -> Result<Sexp> {
        let list_item = [self.lower(node)?];
        Ok(self.list("value", list_item))
    }

    pub(super) fn lower_partition_range_datum(
        &mut self,
        datum: &pg::PartitionRangeDatum,
    ) -> Result<Sexp> {
        match pg::PartitionRangeDatumKind::try_from(datum.kind) {
            Ok(pg::PartitionRangeDatumKind::PartitionRangeDatumMinvalue) => {
                Ok(Sexp::list([self.symbol("minvalue")]))
            }
            Ok(pg::PartitionRangeDatumKind::PartitionRangeDatumMaxvalue) => {
                Ok(Sexp::list([self.symbol("maxvalue")]))
            }
            Ok(pg::PartitionRangeDatumKind::PartitionRangeDatumValue) => {
                let bound_value_node =
                    self.required_node(datum.value.as_deref(), "partition bound value")?;
                self.list_value(bound_value_node)
            }
            _ => self.unsupported("partition bound datum kind"),
        }
    }

    pub(super) fn lower_partition_command(
        &mut self,
        command: &pg::PartitionCmd,
        finalize: bool,
    ) -> Result<Sexp> {
        let partition_name = self.required(command.name.as_ref(), "partition name")?;
        let name = self.lower_relation(partition_name)?;
        let mut values = if let Some(bound) = command.bound.as_ref() {
            vec![
                self.symbol("attach-partition"),
                name,
                self.lower_partition_bound_spec(bound)?,
            ]
        } else {
            let mut values = vec![self.symbol("detach-partition"), name];

            if finalize {
                values.push(self.symbol("finalize"));
            }
            values
        };
        if command.concurrent {
            values.push(self.symbol("concurrently"));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_constraint(&mut self, constraint: &pg::Constraint) -> Result<Sexp> {
        let kind = pg::ConstrType::try_from(constraint.contype)
            .map_err(|_| ParseError::new("unsupported constraint", 0))?;
        match kind {
            pg::ConstrType::ConstrPrimary | pg::ConstrType::ConstrUnique => {
                validate_index_constraint(constraint)?
            }
            pg::ConstrType::ConstrForeign
            | pg::ConstrType::ConstrIdentity
            | pg::ConstrType::ConstrGenerated
            | pg::ConstrType::ConstrExclusion => {}
            _ => validate_simple_constraint(constraint)?,
        }

        let mut values = match kind {
            pg::ConstrType::ConstrNotnull => vec![self.symbol("not-null")],
            pg::ConstrType::ConstrNull => vec![self.symbol("null")],
            pg::ConstrType::ConstrDefault => {
                let default_expression_node =
                    self.required_node(constraint.raw_expr.as_deref(), "default expression")?;
                vec![self.symbol("default"), self.lower(default_expression_node)?]
            }
            pg::ConstrType::ConstrCheck => {
                let check_expression_node =
                    self.required_node(constraint.raw_expr.as_deref(), "check expression")?;
                vec![self.symbol("check"), self.lower(check_expression_node)?]
            }
            pg::ConstrType::ConstrPrimary => {
                if constraint.nulls_not_distinct {
                    return self.unsupported("PRIMARY KEY NULLS NOT DISTINCT");
                }

                let mut values = vec![self.symbol("primary-key")];

                values.extend(self.lower_nodes(&constraint.keys)?);
                if constraint.without_overlaps {
                    values.push(self.symbol("without-overlaps"));
                }
                self.push_index_constraint_options(&mut values, constraint)?;
                values
            }
            pg::ConstrType::ConstrUnique => {
                let mut values = vec![self.symbol("unique")];

                values.extend(self.lower_nodes(&constraint.keys)?);
                if constraint.nulls_not_distinct {
                    values.push(self.symbol("nulls-not-distinct"));
                }
                if constraint.without_overlaps {
                    values.push(self.symbol("without-overlaps"));
                }
                self.push_index_constraint_options(&mut values, constraint)?;
                values
            }
            pg::ConstrType::ConstrForeign => self.lower_foreign_key_constraint(constraint)?,
            pg::ConstrType::ConstrIdentity => self.lower_identity_constraint(constraint)?,
            pg::ConstrType::ConstrGenerated => self.lower_generated_constraint(constraint)?,
            pg::ConstrType::ConstrExclusion => self.lower_exclusion_constraint(constraint)?,
            pg::ConstrType::ConstrAttrDeferrable => vec![self.symbol("deferrable")],
            pg::ConstrType::ConstrAttrNotDeferrable => {
                vec![self.symbol("not-deferrable")]
            }
            pg::ConstrType::ConstrAttrDeferred => {
                vec![self.symbol("initially-deferred")]
            }
            pg::ConstrType::ConstrAttrImmediate => {
                vec![self.symbol("initially-immediate")]
            }
            pg::ConstrType::ConstrAttrEnforced => vec![self.symbol("enforced")],
            pg::ConstrType::ConstrAttrNotEnforced => vec![self.symbol("not-enforced")],
            _ => return self.unsupported(format!("constraint {}", kind.as_str_name())),
        };
        self.add_constraint_modifiers(&mut values, constraint)?;
        if !constraint.conname.is_empty() {
            values.push(self.list("name", [Sexp::string(constraint.conname.clone())]));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_generated_constraint(
        &mut self,
        constraint: &pg::Constraint,
    ) -> Result<Vec<Sexp>> {
        let column_expression_node = self.required_node(
            constraint.raw_expr.as_deref(),
            "generated column expression",
        )?;
        let generation_kind =
            self.generation_kind(&constraint.generated_when, "generated column")?;
        let storage_kind = match constraint.generated_kind.as_str() {
            "s" => "stored",
            "v" => "virtual",
            _ => return self.unsupported_error("generated column kind"),
        };
        if !constraint.conname.is_empty()
            || constraint.deferrable
            || constraint.initdeferred
            || constraint.skip_validation
            || constraint.is_no_inherit
            || !constraint.cooked_expr.is_empty()
            || constraint.nulls_not_distinct
            || !constraint.keys.is_empty()
            || !constraint.including.is_empty()
            || !constraint.exclusions.is_empty()
            || !constraint.options.is_empty()
            || !constraint.indexname.is_empty()
            || !constraint.indexspace.is_empty()
            || constraint.reset_default_tblspc
            || !constraint.access_method.is_empty()
            || constraint.where_clause.is_some()
            || constraint.pktable.is_some()
            || !constraint.fk_attrs.is_empty()
            || !constraint.pk_attrs.is_empty()
            || !constraint.fk_matchtype.is_empty()
            || !constraint.fk_upd_action.is_empty()
            || !constraint.fk_del_action.is_empty()
            || !constraint.fk_del_set_cols.is_empty()
            || !constraint.old_conpfeqop.is_empty()
            || constraint.old_pktable_oid != 0
        {
            return self.unsupported_error("generated column");
        }
        Ok(vec![
            self.symbol("generated"),
            self.symbol(generation_kind),
            self.lower(column_expression_node)?,
            self.symbol(storage_kind),
        ])
    }

    pub(super) fn lower_exclusion_constraint(
        &mut self,
        constraint: &pg::Constraint,
    ) -> Result<Vec<Sexp>> {
        if constraint.access_method.is_empty()
            || constraint.nulls_not_distinct
            || constraint.skip_validation
            || !constraint.keys.is_empty()
            || !constraint.options.is_empty()
            || !constraint.indexname.is_empty()
            || !constraint.indexspace.is_empty()
            || constraint.reset_default_tblspc
            || constraint.pktable.is_some()
            || !constraint.fk_attrs.is_empty()
            || !constraint.pk_attrs.is_empty()
            || !constraint.fk_matchtype.is_empty()
            || !constraint.fk_upd_action.is_empty()
            || !constraint.fk_del_action.is_empty()
            || !constraint.fk_del_set_cols.is_empty()
            || !constraint.cooked_expr.is_empty()
            || !constraint.generated_when.is_empty()
        {
            return self.unsupported_error("EXCLUDE constraint");
        }

        let mut values = vec![
            self.symbol("exclude"),
            self.list("using", [Sexp::string(constraint.access_method.clone())]),
        ];

        for exclusion in &constraint.exclusions {
            let PgNode::List(element) =
                self.required_kind(exclusion.node.as_ref(), "EXCLUDE element")?
            else {
                return self.unsupported_error("EXCLUDE element");
            };
            if element.items.len() != 2 {
                return self.unsupported_error("EXCLUDE element");
            }

            let PgNode::IndexElem(index) =
                self.required_kind(element.items[0].node.as_ref(), "EXCLUDE element column")?
            else {
                return self.unsupported_error("EXCLUDE element column");
            };
            let PgNode::List(operators) =
                self.required_kind(element.items[1].node.as_ref(), "EXCLUDE element operator")?
            else {
                return self.unsupported_error("EXCLUDE element operator");
            };
            let exclusion_operators = operators
                .items
                .iter()
                .map(|x| self.string_node(x))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(Sexp::string);
            let lowered_element = [
                self.lower_index_element(index)?,
                self.list("operators", exclusion_operators),
            ];
            if operators.items.is_empty() {
                return self.unsupported_error("EXCLUDE element operator");
            }

            values.push(self.list("element", lowered_element));
        }
        self.push_index_constraint_options(&mut values, constraint)?;
        if let Some(where_clause) = constraint.where_clause.as_deref() {
            let lowered_where = [self.lower(where_clause)?];
            values.push(self.list("where", lowered_where));
        }

        Ok(values)
    }

    pub(super) fn push_index_constraint_options(
        &mut self,
        values: &mut Vec<Sexp>,
        constraint: &pg::Constraint,
    ) -> Result<()> {
        if !constraint.including.is_empty() {
            let include = self.lower_nodes(&constraint.including)?;
            values.push(self.list("include", include));
        }
        if !constraint.indexname.is_empty() {
            values.push(self.list("using-index", [Sexp::string(constraint.indexname.clone())]));
        }
        if !constraint.indexspace.is_empty() {
            values.push(self.list("tablespace", [Sexp::string(constraint.indexspace.clone())]));
        }
        if !constraint.options.is_empty() {
            let options = self.lower_nodes(&constraint.options)?;
            values.push(self.list("options", options));
        }

        Ok(())
    }

    pub(super) fn lower_foreign_key_constraint(
        &mut self,
        constraint: &pg::Constraint,
    ) -> Result<Vec<Sexp>> {
        let period_columns_are_present = (!constraint.fk_with_period
            || constraint.fk_attrs.last().is_some_and(is_column_name))
            && (!constraint.pk_with_period
                || constraint.pk_attrs.last().is_some_and(is_column_name));
        if !period_columns_are_present {
            return self.unsupported_error("FOREIGN KEY period columns");
        }

        let mut referenced_columns = self.lower_nodes(&constraint.pk_attrs)?;
        let referenced_relation = self.required(
            constraint.pktable.as_ref(),
            "FOREIGN KEY referenced relation",
        )?;
        if constraint.pk_with_period {
            let period_column = referenced_columns.pop().ok_or_else(|| {
                ParseError::new("unsupported PostgreSQL AST: FOREIGN KEY period column", 0)
            })?;
            referenced_columns.push(self.list("period", [period_column]));
        }
        let references = [
            self.lower_relation(referenced_relation)?,
            self.list("columns", referenced_columns),
        ];
        let mut foreign_key_columns = self.lower_nodes(&constraint.fk_attrs)?;
        if constraint.fk_with_period {
            let period_column = foreign_key_columns.pop().ok_or_else(|| {
                ParseError::new("unsupported PostgreSQL AST: FOREIGN KEY period column", 0)
            })?;
            foreign_key_columns.push(self.list("period", [period_column]));
        }
        if !constraint.keys.is_empty()
            || !constraint.including.is_empty()
            || !constraint.exclusions.is_empty()
            || !constraint.options.is_empty()
            || !constraint.indexname.is_empty()
            || !constraint.indexspace.is_empty()
            || constraint.reset_default_tblspc
            || !constraint.access_method.is_empty()
            || constraint.where_clause.is_some()
            || constraint.nulls_not_distinct
            || !constraint.cooked_expr.is_empty()
            || !constraint.generated_when.is_empty()
        {
            return Err(ParseError::new(
                "unsupported PostgreSQL AST: FOREIGN KEY options",
                0,
            ));
        }

        let mut values = vec![
            self.symbol("foreign-key"),
            self.list("columns", foreign_key_columns),
            self.list("references", references),
        ];

        if let Some(match_type) = foreign_key_match_type(&constraint.fk_matchtype)? {
            values.push(self.symbol(match_type));
        }
        if let Some(action) = foreign_key_action(&constraint.fk_upd_action, "on-update")? {
            let on_update = [self.symbol(action)];
            values.push(self.list("on-update", on_update));
        }
        if !constraint.fk_del_set_cols.is_empty() {
            if constraint.fk_del_action != "n" && constraint.fk_del_action != "d" {
                return self.unsupported_error("FOREIGN KEY ON DELETE action");
            }

            let action = match foreign_key_action(&constraint.fk_del_action, "on-delete")? {
                Some(action) => action,
                None => return self.unsupported_error("FOREIGN KEY ON DELETE action"),
            };
            let columns = self.lower_nodes(&constraint.fk_del_set_cols)?;
            let on_delete = [self.symbol(action), self.list("columns", columns)];
            values.push(self.list("on-delete", on_delete));
        } else if let Some(action) = foreign_key_action(&constraint.fk_del_action, "on-delete")? {
            let on_delete = [self.symbol(action)];
            values.push(self.list("on-delete", on_delete));
        }

        Ok(values)
    }

    pub(super) fn add_constraint_modifiers(
        &mut self,
        values: &mut Vec<Sexp>,
        constraint: &pg::Constraint,
    ) -> Result<()> {
        if constraint.skip_validation {
            values.push(self.symbol("not-valid"));
        }
        if constraint.is_no_inherit {
            values.push(self.symbol("no-inherit"));
        }
        if constraint.deferrable {
            values.push(self.symbol("deferrable"));
        } else if constraint.initdeferred {
            return Err(ParseError::new(
                "unsupported PostgreSQL AST: INITIALLY DEFERRED non-deferrable constraint",
                0,
            ));
        }
        if constraint.initdeferred {
            values.push(self.symbol("initially-deferred"));
        }
        let kind = pg::ConstrType::try_from(constraint.contype)
            .map_err(|_| ParseError::new("unsupported constraint", 0))?;
        if matches!(
            kind,
            pg::ConstrType::ConstrCheck | pg::ConstrType::ConstrForeign
        ) && !constraint.is_enforced
        {
            values.push(self.symbol("not-enforced"));
        }

        Ok(())
    }
}

fn constraint_attribute(kind: pg::ConstrType) -> Option<&'static str> {
    match kind {
        pg::ConstrType::ConstrAttrDeferrable => Some("deferrable"),
        pg::ConstrType::ConstrAttrNotDeferrable => Some("not-deferrable"),
        pg::ConstrType::ConstrAttrDeferred => Some("initially-deferred"),
        pg::ConstrType::ConstrAttrImmediate => Some("initially-immediate"),
        pg::ConstrType::ConstrAttrEnforced => Some("enforced"),
        pg::ConstrType::ConstrAttrNotEnforced => Some("not-enforced"),
        _ => None,
    }
}

fn minmax_keyword(reference: &pg::ColumnRef) -> Option<&'static str> {
    if let [field] = reference.fields.as_slice()
        && let Some(PgNode::String(keyword)) = field.node.as_ref()
    {
        match keyword.sval.as_str() {
            "minvalue" => return Some("minvalue"),
            "maxvalue" => return Some("maxvalue"),
            _ => {}
        }
    }
    None
}

fn validate_simple_constraint(constraint: &pg::Constraint) -> Result<()> {
    if !constraint.including.is_empty()
        || !constraint.exclusions.is_empty()
        || !constraint.options.is_empty()
        || !constraint.indexname.is_empty()
        || !constraint.indexspace.is_empty()
        || constraint.reset_default_tblspc
        || !constraint.access_method.is_empty()
        || constraint.where_clause.is_some()
        || constraint.pktable.is_some()
        || !constraint.fk_attrs.is_empty()
        || !constraint.pk_attrs.is_empty()
        || !constraint.fk_matchtype.is_empty()
        || !constraint.fk_upd_action.is_empty()
        || !constraint.fk_del_action.is_empty()
        || !constraint.fk_del_set_cols.is_empty()
        || !constraint.cooked_expr.is_empty()
        || !constraint.generated_when.is_empty()
    {
        return Err(ParseError::new(
            "unsupported PostgreSQL AST: constraint options",
            0,
        ));
    }

    Ok(())
}

fn validate_index_constraint(constraint: &pg::Constraint) -> Result<()> {
    if !constraint.exclusions.is_empty()
        || constraint.reset_default_tblspc
        || !constraint.access_method.is_empty()
        || constraint.where_clause.is_some()
        || constraint.pktable.is_some()
        || !constraint.fk_attrs.is_empty()
        || !constraint.pk_attrs.is_empty()
        || !constraint.fk_matchtype.is_empty()
        || !constraint.fk_upd_action.is_empty()
        || !constraint.fk_del_action.is_empty()
        || !constraint.fk_del_set_cols.is_empty()
        || !constraint.cooked_expr.is_empty()
        || !constraint.generated_when.is_empty()
    {
        return Err(ParseError::new(
            "unsupported PostgreSQL AST: constraint options",
            0,
        ));
    }

    Ok(())
}

fn foreign_key_match_type(match_type: &str) -> Result<Option<&'static str>> {
    match match_type {
        "" | "s" => Ok(None),
        "f" => Ok(Some("match-full")),
        _ => Err(ParseError::new(
            "unsupported PostgreSQL AST: FOREIGN KEY match type",
            0,
        )),
    }
}

fn is_column_name(node: &pg::Node) -> bool {
    matches!(
        node.node.as_ref(),
        Some(PgNode::String(string)) if !string.sval.is_empty()
    )
}

fn foreign_key_action(action: &str, name: &str) -> Result<Option<&'static str>> {
    match action {
        "" | "a" => Ok(None),
        "r" => Ok(Some("restrict")),
        "c" => Ok(Some("cascade")),
        "n" => Ok(Some("set-null")),
        "d" => Ok(Some("set-default")),
        _ => Err(ParseError::new(
            format!("unsupported PostgreSQL AST: {name} action"),
            0,
        )),
    }
}
