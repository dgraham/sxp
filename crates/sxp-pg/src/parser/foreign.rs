use super::*;

impl PostgresParser {
    pub(crate) fn lower_create_fdw(&mut self, stmt: &pg::CreateFdwStmt) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-foreign-data-wrapper"),
            Sexp::string(stmt.fdwname.clone()),
        ];

        values.extend(self.fdw_function_options(&stmt.func_options)?);

        let options = self.lower_generic_options(&stmt.options, "foreign data wrapper options")?;
        self.optional_clause(&mut values, "options", options);
        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_alter_fdw(&mut self, stmt: &pg::AlterFdwStmt) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("alter-foreign-data-wrapper"),
            Sexp::string(stmt.fdwname.clone()),
        ];

        values.extend(self.fdw_function_options(&stmt.func_options)?);

        let options = self.lower_generic_options(&stmt.options, "foreign data wrapper options")?;
        self.optional_clause(&mut values, "options", options);
        Ok(Sexp::list(values))
    }

    fn fdw_function_options(&mut self, options: &[pg::Node]) -> Result<Vec<Sexp>> {
        options
            .iter()
            .map(|node| {
                let option = match self.required_kind(node.node.as_ref(), "FDW function option")? {
                    PgNode::DefElem(option) => option,
                    _ => return self.unsupported_error("FDW function option"),
                };
                if !option.defnamespace.is_empty()
                    || !matches!(
                        pg::DefElemAction::try_from(option.defaction),
                        Ok(pg::DefElemAction::DefelemUnspec)
                    )
                {
                    return self.unsupported_error("FDW function option");
                }
                match (option.defname.as_str(), option.arg.as_deref()) {
                    ("handler" | "validator", Some(argument)) => {
                        let names = match self
                            .required_kind(argument.node.as_ref(), "FDW function name")?
                        {
                            PgNode::List(names) => names,
                            _ => return self.unsupported_error("FDW function name"),
                        };
                        let qualified = names
                            .items
                            .iter()
                            .map(|node| self.string_node(node))
                            .collect::<Result<Vec<_>>>()?
                            .join(".");
                        Ok(self.list(option.defname.as_str(), [Sexp::string(qualified)]))
                    }
                    ("handler", None) => Ok(Sexp::list([self.symbol("no-handler")])),
                    ("validator", None) => Ok(Sexp::list([self.symbol("no-validator")])),
                    _ => self.unsupported_error("FDW function option"),
                }
            })
            .collect()
    }

    pub(crate) fn lower_create_foreign_server(
        &mut self,
        stmt: &pg::CreateForeignServerStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("create-foreign-server")];

        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }

        values.push(Sexp::string(stmt.servername.clone()));
        if !stmt.servertype.is_empty() {
            values.push(self.list("type", [Sexp::string(stmt.servertype.clone())]));
        }
        if !stmt.version.is_empty() {
            values.push(self.list("version", [Sexp::string(stmt.version.clone())]));
        }

        values.push(self.list("foreign-data-wrapper", [Sexp::string(stmt.fdwname.clone())]));

        let options = self.lower_generic_options(&stmt.options, "foreign server options")?;
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_alter_foreign_server(
        &mut self,
        stmt: &pg::AlterForeignServerStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("alter-server"),
            Sexp::string(stmt.servername.clone()),
        ];

        if stmt.has_version {
            values.push(self.list("version", [Sexp::string(stmt.version.clone())]));
        }

        let options = self.lower_generic_options(&stmt.options, "foreign server options")?;
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_create_user_mapping(
        &mut self,
        stmt: &pg::CreateUserMappingStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("create-user-mapping")];

        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }

        let user = self.required(stmt.user.as_ref(), "user mapping role")?;
        let role = self.lower_role_spec(user)?;
        values.push(self.list("for", [role]));
        values.push(self.list("server", [Sexp::string(stmt.servername.clone())]));

        let options = self.lower_generic_options(&stmt.options, "user mapping options")?;
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_alter_user_mapping(
        &mut self,
        stmt: &pg::AlterUserMappingStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("alter-user-mapping")];

        let user = self.required(stmt.user.as_ref(), "user mapping role")?;
        let role = self.lower_role_spec(user)?;
        values.push(self.list("for", [role]));
        values.push(self.list("server", [Sexp::string(stmt.servername.clone())]));

        let options = self.lower_generic_options(&stmt.options, "user mapping options")?;
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_drop_user_mapping(
        &mut self,
        stmt: &pg::DropUserMappingStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("drop-user-mapping")];

        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        let user = self.required(stmt.user.as_ref(), "user mapping role")?;
        let role = self.lower_role_spec(user)?;
        values.push(self.list("for", [role]));
        values.push(self.list("server", [Sexp::string(stmt.servername.clone())]));

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_create_foreign_table(
        &mut self,
        stmt: &pg::CreateForeignTableStmt,
    ) -> Result<Sexp> {
        let base = self.required(stmt.base_stmt.as_ref(), "CREATE FOREIGN TABLE definition")?;
        let mut values = match self.lower_create_table(base)? {
            Sexp::List(items) if items.first() == Some(&self.symbol("create-table")) => {
                items.into_vec()
            }
            _ => return self.unsupported("CREATE FOREIGN TABLE definition"),
        };
        values[0] = self.symbol("create-foreign-table");
        values.push(self.list("server", [Sexp::string(stmt.servername.clone())]));

        let options = self.lower_generic_options(&stmt.options, "foreign table options")?;
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_import_foreign_schema(
        &mut self,
        stmt: &pg::ImportForeignSchemaStmt,
    ) -> Result<Sexp> {
        let list_type = pg::ImportForeignSchemaType::try_from(stmt.list_type).map_err(|_| {
            ParseError::new("unsupported PostgreSQL AST: import schema list type", 0)
        })?;
        let mut values = vec![
            self.symbol("import-foreign-schema"),
            self.list("remote-schema", [Sexp::string(stmt.remote_schema.clone())]),
        ];

        match list_type {
            pg::ImportForeignSchemaType::FdwImportSchemaAll => {}
            pg::ImportForeignSchemaType::FdwImportSchemaLimitTo => {
                let tables = self.lower_nodes(&stmt.table_list)?;
                values.push(self.list("limit-to", tables));
            }
            pg::ImportForeignSchemaType::FdwImportSchemaExcept => {
                let tables = self.lower_nodes(&stmt.table_list)?;
                values.push(self.list("except", tables));
            }
            _ => return self.unsupported("import foreign schema list type"),
        }

        values.push(self.list("server", [Sexp::string(stmt.server_name.clone())]));
        values.push(self.list("local-schema", [Sexp::string(stmt.local_schema.clone())]));

        let options = self.lower_generic_options(&stmt.options, "import foreign schema options")?;
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }
}

#[cfg(test)]
mod tests {
    fn parse(sql: &str) -> sxp::Result<String> {
        super::super::PostgresParser::new()
            .parse(sql)
            .map(|sexp| sexp.to_string())
    }

    fn lowers(sql: &str, expected: &str) {
        assert_eq!(parse(sql), Ok(expected.to_owned()));
    }

    #[test]
    fn lowers_create_foreign_data_wrapper() {
        lowers(
            "CREATE FOREIGN DATA WRAPPER alt_fdw1",
            "(create-foreign-data-wrapper \"alt_fdw1\")",
        );
        lowers(
            "CREATE FOREIGN DATA WRAPPER fdw_heap2 VALIDATOR postgresql_fdw_validator",
            "(create-foreign-data-wrapper \"fdw_heap2\" (validator \"postgresql_fdw_validator\"))",
        );
        lowers(
            "CREATE FOREIGN DATA WRAPPER test_fdw HANDLER test_fdw_handler OPTIONS (testing '1')",
            "(create-foreign-data-wrapper \"test_fdw\" (handler \"test_fdw_handler\") (options (option \"testing\" \"1\")))",
        );
    }

    #[test]
    fn lowers_alter_foreign_data_wrapper() {
        lowers(
            "ALTER FOREIGN DATA WRAPPER test_fdw VALIDATOR postgresql_fdw_validator",
            "(alter-foreign-data-wrapper \"test_fdw\" (validator \"postgresql_fdw_validator\"))",
        );
        lowers(
            "ALTER FOREIGN DATA WRAPPER foo NO VALIDATOR",
            "(alter-foreign-data-wrapper \"foo\" (no-validator))",
        );
        lowers(
            "ALTER FOREIGN DATA WRAPPER foo HANDLER invalid_fdw_handler",
            "(alter-foreign-data-wrapper \"foo\" (handler \"invalid_fdw_handler\"))",
        );
        lowers(
            "ALTER FOREIGN DATA WRAPPER foo OPTIONS (DROP a, SET b '3', ADD c '4')",
            "(alter-foreign-data-wrapper \"foo\" (options (drop-option \"a\") (set-option \"b\" \"3\") (add-option \"c\" \"4\")))",
        );
    }

    #[test]
    fn lowers_create_foreign_server() {
        lowers(
            "CREATE SERVER alt_fserv1 FOREIGN DATA WRAPPER alt_fdw1",
            "(create-foreign-server \"alt_fserv1\" (foreign-data-wrapper \"alt_fdw1\"))",
        );
        lowers(
            "CREATE SERVER s7 TYPE 'oracle' VERSION '17.0' FOREIGN DATA WRAPPER foo OPTIONS (host 'a', dbname 'b')",
            "(create-foreign-server \"s7\" (type \"oracle\") (version \"17.0\") (foreign-data-wrapper \"foo\") (options (option \"host\" \"a\") (option \"dbname\" \"b\")))",
        );
        lowers(
            "CREATE SERVER IF NOT EXISTS s2 FOREIGN DATA WRAPPER foo",
            "(create-foreign-server (if-not-exists) \"s2\" (foreign-data-wrapper \"foo\"))",
        );
    }

    #[test]
    fn lowers_alter_foreign_server() {
        lowers(
            "ALTER SERVER s0 OPTIONS (a '1')",
            "(alter-server \"s0\" (options (option \"a\" \"1\")))",
        );
        lowers(
            "ALTER SERVER s1 VERSION '1.0' OPTIONS (servername 's1')",
            "(alter-server \"s1\" (version \"1.0\") (options (option \"servername\" \"s1\")))",
        );
        lowers(
            "ALTER SERVER s8 OPTIONS (connect_timeout '30', SET dbname 'db1', DROP host)",
            "(alter-server \"s8\" (options (option \"connect_timeout\" \"30\") (set-option \"dbname\" \"db1\") (drop-option \"host\")))",
        );
    }

    #[test]
    fn lowers_create_user_mapping() {
        lowers(
            "create user mapping for regress_evt_user server useless_server",
            "(create-user-mapping (for (role \"regress_evt_user\")) (server \"useless_server\"))",
        );
        lowers(
            "CREATE USER MAPPING IF NOT EXISTS FOR current_user SERVER s1",
            "(create-user-mapping (if-not-exists) (for (role current-user)) (server \"s1\"))",
        );
        lowers(
            "CREATE USER MAPPING FOR public SERVER s4 OPTIONS (\"this mapping\" 'is public')",
            "(create-user-mapping (for (role public)) (server \"s4\") (options (option \"this mapping\" \"is public\")))",
        );
    }

    #[test]
    fn lowers_alter_user_mapping() {
        lowers(
            "ALTER USER MAPPING FOR regress_test_role SERVER s4 OPTIONS (user 'secret')",
            "(alter-user-mapping (for (role \"regress_test_role\")) (server \"s4\") (options (option \"user\" \"secret\")))",
        );
    }

    #[test]
    fn lowers_drop_user_mapping() {
        lowers(
            "DROP USER MAPPING FOR regress_test_role SERVER ss4",
            "(drop-user-mapping (for (role \"regress_test_role\")) (server \"ss4\"))",
        );
        lowers(
            "DROP USER MAPPING IF EXISTS FOR regress_test_role SERVER ss4",
            "(drop-user-mapping (if-exists) (for (role \"regress_test_role\")) (server \"ss4\"))",
        );
    }

    #[test]
    fn lowers_create_foreign_table() {
        lowers(
            "CREATE FOREIGN TABLE ft1 (c1 integer NOT NULL) SERVER s0",
            "(create-foreign-table (table (\"ft1\")) (columns (column \"c1\" (type \"pg_catalog\" \"int4\") (not-null))) (server \"s0\"))",
        );
        lowers(
            "CREATE FOREIGN TABLE ft2 (LIKE t INCLUDING ALL) SERVER s0",
            "(create-foreign-table (table (\"ft2\")) (columns (like (table (\"t\")) all)) (server \"s0\"))",
        );
        lowers(
            "CREATE FOREIGN table tableam_fdw_heapx () SERVER fs_heap2",
            "(create-foreign-table (table (\"tableam_fdw_heapx\")) (columns) (server \"fs_heap2\"))",
        );
        lowers(
            "CREATE FOREIGN TABLE fp PARTITION OF tableam_parted_a_heap2 DEFAULT SERVER x",
            "(create-foreign-table (table (\"fp\")) (columns) (partition-of (table (\"tableam_parted_a_heap2\"))) (bounds default) (server \"x\"))",
        );
    }

    #[test]
    fn lowers_import_foreign_schema() {
        lowers(
            "IMPORT FOREIGN SCHEMA s1 FROM SERVER s9 INTO public",
            "(import-foreign-schema (remote-schema \"s1\") (server \"s9\") (local-schema \"public\"))",
        );
        lowers(
            "IMPORT FOREIGN SCHEMA s1 LIMIT TO (t1) FROM SERVER s9 INTO public",
            "(import-foreign-schema (remote-schema \"s1\") (limit-to (table (\"t1\"))) (server \"s9\") (local-schema \"public\"))",
        );
        lowers(
            "IMPORT FOREIGN SCHEMA s1 EXCEPT (t1, t2) FROM SERVER s9 INTO public OPTIONS (option1 'value1')",
            "(import-foreign-schema (remote-schema \"s1\") (except (table (\"t1\")) (table (\"t2\"))) (server \"s9\") (local-schema \"public\") (options (option \"option1\" \"value1\")))",
        );
    }

    #[test]
    fn lowers_column_fdw_options() {
        lowers(
            "CREATE FOREIGN TABLE t (c int OPTIONS (x 'y')) SERVER s",
            "(create-foreign-table (table (\"t\")) (columns (column \"c\" (type \"pg_catalog\" \"int4\") (options (option \"x\" \"y\")))) (server \"s\"))",
        );
    }

    #[test]
    fn parses_alter_foreign_table() {
        lowers(
            "ALTER FOREIGN TABLE ft ADD COLUMN c int",
            "(alter-foreign-table (table (\"ft\")) (add-column (column \"c\" (type \"pg_catalog\" \"int4\"))))",
        );
    }
}
