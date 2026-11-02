use super::*;

impl PostgresParser {
    pub(in crate::parser) fn alter_database_command(
        &mut self,
        object_type: i32,
        context: &str,
    ) -> Result<&'static str> {
        match pg::ObjectType::try_from(object_type)
            .map_err(|_| ParseError::new(format!("unsupported PostgreSQL AST: {context}"), 0))?
        {
            pg::ObjectType::ObjectDatabase => Ok("alter-database"),
            pg::ObjectType::ObjectTablespace => Ok("alter-tablespace"),
            pg::ObjectType::ObjectStatisticExt => Ok("alter-statistics"),
            pg::ObjectType::ObjectPublication => Ok("alter-publication"),
            pg::ObjectType::ObjectSubscription => Ok("alter-subscription"),
            pg::ObjectType::ObjectFunction => Ok("alter-function"),
            pg::ObjectType::ObjectProcedure => Ok("alter-procedure"),
            pg::ObjectType::ObjectAggregate => Ok("alter-aggregate"),
            pg::ObjectType::ObjectOperator => Ok("alter-operator"),
            pg::ObjectType::ObjectCollation => Ok("alter-collation"),
            pg::ObjectType::ObjectConversion => Ok("alter-conversion"),
            pg::ObjectType::ObjectOpclass => Ok("alter-operator-class"),
            pg::ObjectType::ObjectOpfamily => Ok("alter-operator-family"),
            pg::ObjectType::ObjectLanguage => Ok("alter-language"),
            pg::ObjectType::ObjectTransform => Ok("alter-transform"),
            pg::ObjectType::ObjectFdw => Ok("alter-foreign-data-wrapper"),
            pg::ObjectType::ObjectForeignServer => Ok("alter-server"),
            pg::ObjectType::ObjectTsconfiguration => Ok("alter-text-search-configuration"),
            pg::ObjectType::ObjectTsdictionary => Ok("alter-text-search-dictionary"),
            pg::ObjectType::ObjectTsparser => Ok("alter-text-search-parser"),
            pg::ObjectType::ObjectTstemplate => Ok("alter-text-search-template"),
            pg::ObjectType::ObjectSchema => Ok("alter-schema"),
            pg::ObjectType::ObjectType => Ok("alter-type"),
            pg::ObjectType::ObjectEventTrigger => Ok("alter-event-trigger"),
            pg::ObjectType::ObjectLargeobject => Ok("alter-large-object"),
            _ => self.unsupported_error(context),
        }
    }

    pub(in crate::parser) fn lower_create_database(
        &mut self,
        stmt: &pg::CreatedbStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-database"),
            Sexp::string(stmt.dbname.clone()),
        ];

        for node in &stmt.options {
            let option = match self.required_kind(node.node.as_ref(), "CREATE DATABASE option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error("CREATE DATABASE option"),
            };
            match option.defname.as_str() {
                "allow_connections" => {
                    self.push_database_flag(&mut values, option, "allow-connections")?;
                }
                "is_template" => {
                    self.push_database_flag(&mut values, option, "is-template")?;
                }
                "connection_limit" => {
                    let node =
                        self.required_node(option.arg.as_deref(), "CONNECTION LIMIT value")?;
                    let limit =
                        match self.required_kind(node.node.as_ref(), "CONNECTION LIMIT value")? {
                            PgNode::Integer(integer) => integer.ival,
                            _ => return self.unsupported_error("CONNECTION LIMIT value"),
                        };
                    let connection_limit = [self.symbol(&limit.to_string())];
                    values.push(self.list("connection-limit", connection_limit));
                }
                name @ ("owner" | "template" | "encoding" | "locale" | "lc_collate"
                | "lc_ctype" | "tablespace" | "strategy" | "icu_locale"
                | "locale_provider" | "collation_version" | "builtin_locale"
                | "icu_rules" | "oid") => {
                    let name = match name {
                        "lc_collate" => "lc-collate",
                        "lc_ctype" => "lc-type",
                        "icu_locale" => "icu-locale",
                        "locale_provider" => "locale-provider",
                        "collation_version" => "collation-version",
                        "builtin_locale" => "builtin-locale",
                        "icu_rules" => "icu-rules",
                        other => other,
                    };
                    let option_value_node =
                        self.required_node(option.arg.as_deref(), "CREATE DATABASE option value")?;
                    let lowered_option_value = [self.lower(option_value_node)?];
                    values.push(self.list(name, lowered_option_value));
                }
                other => return self.unsupported_error(format!("CREATE DATABASE option {other}")),
            }
        }

        Ok(Sexp::list(values))
    }

    fn push_database_flag(
        &mut self,
        values: &mut Vec<Sexp>,
        option: &pg::DefElem,
        name: &str,
    ) -> Result<()> {
        let enabled = self.lower_flag_argument(option, "CREATE DATABASE option value")?;
        values.push(self.symbol(&if enabled {
            name.to_owned()
        } else {
            format!("no-{name}")
        }));

        Ok(())
    }

    pub(in crate::parser) fn lower_flag_argument(
        &mut self,
        option: &pg::DefElem,
        context: &str,
    ) -> Result<bool> {
        let node = self.required_node(option.arg.as_deref(), "boolean option value")?;
        match self.required_kind(node.node.as_ref(), "boolean option value")? {
            PgNode::Boolean(value) => Ok(value.boolval),
            PgNode::Integer(value) => Ok(value.ival != 0),
            PgNode::AConst(value) => match value.val.as_ref() {
                Some(a_const::Val::Boolval(value)) => Ok(value.boolval),
                Some(a_const::Val::Ival(value)) => Ok(value.ival != 0),
                Some(a_const::Val::Sval(value)) => self.parse_flag_string(&value.sval, context),
                _ => self.unsupported_error(context),
            },
            PgNode::String(value) => self.parse_flag_string(&value.sval, context),
            _ => self.unsupported_error(context),
        }
    }

    pub(in crate::parser) fn parse_flag_string(
        &mut self,
        value: &str,
        context: &str,
    ) -> Result<bool> {
        match value.to_ascii_lowercase().as_str() {
            "true" | "on" | "yes" | "t" | "y" | "1" => Ok(true),
            "false" | "off" | "no" | "f" | "n" | "0" => Ok(false),
            _ => self.unsupported_error(context),
        }
    }

    pub(in crate::parser) fn option_string_value(
        &mut self,
        option: &pg::DefElem,
        context: &str,
    ) -> Result<String> {
        let node = self.required_node(option.arg.as_deref(), context)?;
        match self.required_kind(node.node.as_ref(), context)? {
            PgNode::String(value) => Ok(value.sval.clone()),
            PgNode::TypeName(typename)
                if typename.names.len() == 1
                    && !typename.setof
                    && !typename.pct_type
                    && typename.typmods.is_empty()
                    && typename.array_bounds.is_empty() =>
            {
                self.string_node(typename.names.first().expect("single name"))
            }
            _ => self.unsupported_error(context),
        }
    }

    pub(in crate::parser) fn lower_generic_option(&mut self, option: &pg::DefElem) -> Result<Sexp> {
        let mut values = vec![self.symbol("option"), Sexp::string(option.defname.clone())];

        match option.arg.as_deref() {
            Some(argument) => values.push(self.lower(argument)?),
            None => values.push(self.symbol("true")),
        }
        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_database(
        &mut self,
        stmt: &pg::AlterDatabaseStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("alter-database"),
            Sexp::string(stmt.dbname.clone()),
        ];

        for node in &stmt.options {
            let option = match self.required_kind(node.node.as_ref(), "ALTER DATABASE option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error("ALTER DATABASE option"),
            };
            match option.defname.as_str() {
                "allow_connections" => {
                    self.push_database_flag(&mut values, option, "allow-connections")?;
                }
                "is_template" => {
                    self.push_database_flag(&mut values, option, "is-template")?;
                }
                "tablespace" => {
                    let tablespace_node =
                        self.required_node(option.arg.as_deref(), "ALTER DATABASE tablespace")?;
                    let tablespace_name = [Sexp::string(self.string_node(tablespace_node)?)];
                    values.push(self.list("in-tablespace", tablespace_name))
                }
                "connection_limit" => {
                    let node = self.required_node(
                        option.arg.as_deref(),
                        "ALTER DATABASE CONNECTION LIMIT value",
                    )?;
                    let limit = match self.required_kind(
                        node.node.as_ref(),
                        "ALTER DATABASE CONNECTION LIMIT value",
                    )? {
                        PgNode::Integer(integer) => integer.ival,
                        _ => {
                            return self.unsupported_error("ALTER DATABASE CONNECTION LIMIT value");
                        }
                    };
                    let connection_limit = [self.symbol(&limit.to_string())];
                    values.push(self.list("connection-limit", connection_limit));
                }
                other => return self.unsupported_error(format!("ALTER DATABASE option {other}")),
            }
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_database_set(
        &mut self,
        stmt: &pg::AlterDatabaseSetStmt,
    ) -> Result<Sexp> {
        let set_statement = self.required(stmt.setstmt.as_ref(), "ALTER DATABASE SET statement")?;
        let alter_database = [
            Sexp::string(stmt.dbname.clone()),
            self.lower_variable_set(set_statement)?,
        ];
        Ok(self.list("alter-database", alter_database))
    }

    pub(in crate::parser) fn lower_alter_database_refresh_collation(
        &mut self,
        stmt: &pg::AlterDatabaseRefreshCollStmt,
    ) -> Result<Sexp> {
        let action = [self.symbol("refresh-collation-version")];
        Ok(self.list(
            "alter-database",
            [Sexp::string(stmt.dbname.clone()), Sexp::list(action)],
        ))
    }

    pub(in crate::parser) fn lower_alter_rename(&mut self, stmt: &pg::RenameStmt) -> Result<Sexp> {
        let kind = pg::ObjectType::try_from(stmt.rename_type)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: RENAME object type", 0))?;
        let command = match kind {
            pg::ObjectType::ObjectPublication => "alter-publication",
            pg::ObjectType::ObjectSubscription => "alter-subscription",
            _ => self.alter_database_command(stmt.rename_type, "RENAME object type")?,
        };

        let name = match kind {
            pg::ObjectType::ObjectPublication | pg::ObjectType::ObjectSubscription => {
                let object_node = self.required_node(stmt.object.as_deref(), "RENAME object")?;
                Sexp::string(self.string_node(object_node)?)
            }
            _ => Sexp::string(stmt.subname.clone()),
        };
        let rename_parts = [
            name,
            self.list("rename", [Sexp::string(stmt.newname.clone())]),
        ];

        Ok(self.list(command, rename_parts))
    }

    pub(in crate::parser) fn lower_drop_database(&mut self, stmt: &pg::DropdbStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("drop-database")];

        if stmt.missing_ok {
            values.push(self.symbol("if-exists"));
        }

        values.push(Sexp::string(stmt.dbname.clone()));
        for node in &stmt.options {
            let option = match self.required_kind(node.node.as_ref(), "DROP DATABASE option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error("DROP DATABASE option"),
            };
            match option.defname.as_str() {
                "force" => values.push(self.symbol("force")),
                other => return self.unsupported_error(format!("DROP DATABASE option {other}")),
            }
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_create_tablespace(
        &mut self,
        stmt: &pg::CreateTableSpaceStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-tablespace"),
            Sexp::string(stmt.tablespacename.clone()),
        ];

        if let Some(owner) = stmt.owner.as_ref() {
            let own = [self.lower_role_spec(owner)?];
            values.push(self.list("owner", own));
        }

        values.push(self.list("location", [Sexp::string(stmt.location.clone())]));

        let options = self.lower_nodes(&stmt.options)?;
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_tablespace_options(
        &mut self,
        stmt: &pg::AlterTableSpaceOptionsStmt,
    ) -> Result<Sexp> {
        let action = if stmt.is_reset { "reset" } else { "set" };
        let options = self.lower_nodes(&stmt.options)?;
        let alter_tablespace_string = [
            Sexp::string(stmt.tablespacename.clone()),
            self.list(action, options),
        ];

        Ok(self.list("alter-tablespace", alter_tablespace_string))
    }

    pub(in crate::parser) fn lower_drop_tablespace(
        &mut self,
        stmt: &pg::DropTableSpaceStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("drop-tablespace")];

        if stmt.missing_ok {
            values.push(self.symbol("if-exists"));
        }

        values.push(Sexp::string(stmt.tablespacename.clone()));
        Ok(Sexp::list(values))
    }
}
