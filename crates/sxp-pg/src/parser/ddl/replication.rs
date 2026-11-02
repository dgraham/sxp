use super::*;

impl PostgresParser {
    pub(in crate::parser) fn lower_create_publication(
        &mut self,
        stmt: &pg::CreatePublicationStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-publication"),
            Sexp::string(stmt.pubname.clone()),
        ];

        self.lower_publication_options(&mut values, &stmt.options, "CREATE PUBLICATION option")?;
        let tables = self.lower_publication_objects(&stmt.pubobjects)?;
        if stmt.for_all_tables {
            values.push(Sexp::list([self.symbol("all-tables")]));
        }
        self.optional_clause(&mut values, "tables", tables);

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_publication(
        &mut self,
        stmt: &pg::AlterPublicationStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("alter-publication"),
            Sexp::string(stmt.pubname.clone()),
        ];

        if !stmt.options.is_empty() {
            self.lower_publication_options(&mut values, &stmt.options, "ALTER PUBLICATION option")?;
            return Ok(Sexp::list(values));
        }

        let action = pg::AlterPublicationAction::try_from(stmt.action).map_err(|_| {
            ParseError::new("unsupported PostgreSQL AST: ALTER PUBLICATION action", 0)
        })?;
        let command = match action {
            pg::AlterPublicationAction::ApAddObjects => "add",
            pg::AlterPublicationAction::ApDropObjects => "drop",
            pg::AlterPublicationAction::ApSetObjects => "set-tables",
            _ => return self.unsupported("ALTER PUBLICATION action"),
        };
        let publication_objects = self.lower_publication_objects(&stmt.pubobjects)?;
        values.push(self.list(command, publication_objects));

        Ok(Sexp::list(values))
    }

    fn lower_publication_options(
        &mut self,
        values: &mut Vec<Sexp>,
        options: &[pg::Node],
        context: &str,
    ) -> Result<()> {
        for node in options {
            let option = match self.required_kind(node.node.as_ref(), context)? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error(context),
            };
            match option.defname.as_str() {
                "publish" => {
                    let publish_option = [Sexp::string(
                        self.option_string_value(option, "publication option value")?,
                    )];
                    values.push(self.list("publish", publish_option))
                }
                "publish_via_partition_root" => {
                    self.push_flag_clause(values, option, "publish-via-partition-root", context)?
                }
                _ => values.push(self.lower_generic_option(option)?),
            }
        }

        Ok(())
    }

    fn lower_publication_objects(&mut self, objects: &[pg::Node]) -> Result<Vec<Sexp>> {
        objects
            .iter()
            .map(|x| self.lower_publication_object(x))
            .collect()
    }

    fn lower_publication_object(&mut self, node: &pg::Node) -> Result<Sexp> {
        let spec = match self.required_kind(node.node.as_ref(), "publication object")? {
            PgNode::PublicationObjSpec(spec) => spec,
            _ => return self.unsupported("publication object"),
        };
        let kind = pg::PublicationObjSpecType::try_from(spec.pubobjtype).map_err(|_| {
            ParseError::new("unsupported PostgreSQL AST: publication object type", 0)
        })?;
        match kind {
            pg::PublicationObjSpecType::PublicationobjTable => {
                let pubtable = self.required(spec.pubtable.as_ref(), "publication table")?;
                let table_relation =
                    self.required(pubtable.relation.as_ref(), "publication table relation")?;
                let mut table = self.lower_relation(table_relation)?;
                if let Sexp::List(items) = &mut table {
                    let mut values = std::mem::take(items).into_vec();
                    self.optional_node_clause(
                        &mut values,
                        "where",
                        pubtable.where_clause.as_deref(),
                    )?;
                    let columns = self.lower_nodes(&pubtable.columns)?;
                    self.optional_clause(&mut values, "columns", columns);
                    *items = values.into_boxed_slice();
                }
                Ok(table)
            }
            pg::PublicationObjSpecType::PublicationobjTablesInSchema => {
                Ok(self.list("schema", [Sexp::string(spec.name.clone())]))
            }
            pg::PublicationObjSpecType::PublicationobjTablesInCurSchema => {
                let current_schema = [self.symbol("current")];
                Ok(self.list("schema", current_schema))
            }
            _ => self.unsupported("publication object type"),
        }
    }

    pub(in crate::parser) fn lower_create_subscription(
        &mut self,
        stmt: &pg::CreateSubscriptionStmt,
    ) -> Result<Sexp> {
        let publications = self.lower_nodes(&stmt.publication)?;
        let mut values = vec![
            self.symbol("create-subscription"),
            Sexp::string(stmt.subname.clone()),
            self.list("connection", [Sexp::string(stmt.conninfo.clone())]),
            self.list("publications", publications),
        ];

        self.lower_subscription_options(&mut values, &stmt.options, "CREATE SUBSCRIPTION option")?;

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_subscription(
        &mut self,
        stmt: &pg::AlterSubscriptionStmt,
    ) -> Result<Sexp> {
        let kind = pg::AlterSubscriptionType::try_from(stmt.kind).map_err(|_| {
            ParseError::new("unsupported PostgreSQL AST: ALTER SUBSCRIPTION kind", 0)
        })?;
        let mut values = vec![
            self.symbol("alter-subscription"),
            Sexp::string(stmt.subname.clone()),
        ];

        match kind {
            pg::AlterSubscriptionType::AlterSubscriptionConnection => {
                values.push(self.list("connection", [Sexp::string(stmt.conninfo.clone())]));
            }
            pg::AlterSubscriptionType::AlterSubscriptionOptions => {
                let mut set_values = Vec::new();
                self.lower_subscription_options(
                    &mut set_values,
                    &stmt.options,
                    "ALTER SUBSCRIPTION option",
                )?;
                values.push(self.list("set", set_values));
            }
            pg::AlterSubscriptionType::AlterSubscriptionSetPublication => {
                let publications = self.lower_nodes(&stmt.publication)?;
                values.push(self.list("publications", publications));
                self.lower_subscription_options(
                    &mut values,
                    &stmt.options,
                    "ALTER SUBSCRIPTION option",
                )?;
            }
            pg::AlterSubscriptionType::AlterSubscriptionAddPublication
            | pg::AlterSubscriptionType::AlterSubscriptionDropPublication => {
                let publication = self.lower_nodes(&stmt.publication)?;
                if !stmt.conninfo.is_empty() {
                    return self.unsupported_error("ALTER SUBSCRIPTION publication connection");
                }

                values.push(self.list(
                    if kind == pg::AlterSubscriptionType::AlterSubscriptionAddPublication {
                        "add-publications"
                    } else {
                        "drop-publications"
                    },
                    publication,
                ));
                self.lower_subscription_options(
                    &mut values,
                    &stmt.options,
                    "ALTER SUBSCRIPTION option",
                )?;
            }
            pg::AlterSubscriptionType::AlterSubscriptionRefresh => {
                values.push(Sexp::list([self.symbol("refresh")]));
                self.lower_subscription_options(
                    &mut values,
                    &stmt.options,
                    "ALTER SUBSCRIPTION option",
                )?;
            }
            pg::AlterSubscriptionType::AlterSubscriptionEnabled => {
                for node in &stmt.options {
                    let option = match self
                        .required_kind(node.node.as_ref(), "ALTER SUBSCRIPTION option")?
                    {
                        PgNode::DefElem(option) => option,
                        _ => return self.unsupported_error("ALTER SUBSCRIPTION option"),
                    };
                    let subscription_state =
                        if self.lower_flag_argument(option, "ALTER SUBSCRIPTION enabled")? {
                            "enable"
                        } else {
                            "disable"
                        };
                    if option.defname != "enabled" {
                        return self.unsupported_error(format!(
                            "ALTER SUBSCRIPTION option {}",
                            option.defname
                        ));
                    }

                    values.push(self.symbol(subscription_state));
                }
            }
            pg::AlterSubscriptionType::AlterSubscriptionSkip => {
                let mut skip_values = Vec::new();
                self.lower_subscription_options(
                    &mut skip_values,
                    &stmt.options,
                    "ALTER SUBSCRIPTION SKIP option",
                )?;
                values.push(self.list("skip", skip_values));
            }
            _ => return self.unsupported("ALTER SUBSCRIPTION kind"),
        }

        Ok(Sexp::list(values))
    }

    fn lower_subscription_options(
        &mut self,
        values: &mut Vec<Sexp>,
        options: &[pg::Node],
        context: &str,
    ) -> Result<()> {
        for node in options {
            let option = match self.required_kind(node.node.as_ref(), context)? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error(context),
            };
            match option.defname.as_str() {
                "copy_data" => self.push_flag_clause(values, option, "copy-data", context)?,
                "two_phase" => self.push_flag_clause(values, option, "two-phase", context)?,
                "binary" => self.push_flag_clause(values, option, "binary", context)?,
                "create_slot" => self.push_flag_clause(values, option, "create-slot", context)?,
                "run_as_owner" => self.push_flag_clause(values, option, "run-as-owner", context)?,
                "failover" => self.push_flag_clause(values, option, "failover", context)?,
                "refresh" => self.push_flag_clause(values, option, "refresh", context)?,
                "enabled" => self.push_flag_clause(values, option, "enabled", context)?,
                "connect" => self.push_flag_clause(values, option, "connect", context)?,
                "password_required" => {
                    self.push_flag_clause(values, option, "password-required", context)?
                }
                "disable_on_error" => {
                    self.push_flag_clause(values, option, "disable-on-error", context)?
                }
                "lsn" | "slot_name" | "streaming" | "synchronous_commit" | "origin" => {
                    let name = match option.defname.as_str() {
                        "slot_name" => "slot-name",
                        "synchronous_commit" => "synchronous-commit",
                        other => other,
                    };
                    let subscription_option = [Sexp::string(
                        self.option_string_value(option, "subscription option value")?,
                    )];
                    values.push(self.list(name, subscription_option));
                }
                _ => values.push(self.lower_generic_option(option)?),
            }
        }

        Ok(())
    }

    fn push_flag_clause(
        &mut self,
        values: &mut Vec<Sexp>,
        option: &pg::DefElem,
        name: &str,
        context: &str,
    ) -> Result<()> {
        let enabled = match option.arg.as_deref() {
            None => true,
            Some(arg) => match self.required_kind(arg.node.as_ref(), context)? {
                PgNode::String(_) | PgNode::TypeName(_) => {
                    let value = self.option_string_value(option, context)?;
                    match self.parse_flag_string(&value, context) {
                        Ok(enabled) => enabled,
                        Err(_) => {
                            values.push(self.list(name, [Sexp::string(value)]));
                            return Ok(());
                        }
                    }
                }
                _ => self.lower_flag_argument(option, context)?,
            },
        };
        values.push(Sexp::list([self.symbol(&if enabled {
            name.to_owned()
        } else {
            format!("no-{name}")
        })]));

        Ok(())
    }

    pub(in crate::parser) fn lower_drop_subscription(
        &mut self,
        stmt: &pg::DropSubscriptionStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("drop-subscription")];

        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        values.push(Sexp::string(stmt.subname.clone()));
        if let Some(behavior) = self.drop_behavior(&stmt.behavior)? {
            values.push(self.symbol(behavior));
        }

        Ok(Sexp::list(values))
    }
}
