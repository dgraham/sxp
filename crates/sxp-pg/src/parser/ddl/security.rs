use super::*;

impl PostgresParser {
    pub(in crate::parser) fn lower_create_role(
        &mut self,
        stmt: &pg::CreateRoleStmt,
    ) -> Result<Sexp> {
        let kind = pg::RoleStmtType::try_from(stmt.stmt_type)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: role statement", 0))?;
        let command = match kind {
            pg::RoleStmtType::RolestmtRole => "create-role",
            pg::RoleStmtType::RolestmtUser => "create-user",
            pg::RoleStmtType::RolestmtGroup => "create-group",
            _ => return self.unsupported("CREATE ROLE statement type"),
        };
        let mut values = vec![self.symbol(command), Sexp::string(stmt.role.clone())];

        values.extend(self.lower_role_options(&stmt.options, false)?);

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_role(&mut self, stmt: &pg::AlterRoleStmt) -> Result<Sexp> {
        let role = self.required(stmt.role.as_ref(), "ALTER ROLE role")?;

        if stmt.action != 1 && stmt.action != -1 {
            return self.unsupported("ALTER ROLE action");
        }

        let mut values = vec![self.symbol("alter-role"), self.lower_role_spec(role)?];

        values.extend(self.lower_role_options(&stmt.options, stmt.action == -1)?);

        Ok(Sexp::list(values))
    }

    fn lower_role_options(&mut self, options: &[pg::Node], drop: bool) -> Result<Vec<Sexp>> {
        let mut values = Vec::new();
        for node in options {
            let option = match self.required_kind(node.node.as_ref(), "role option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error("role option"),
            };
            match option.defname.as_str() {
                "superuser" => self.lower_role_flag(&mut values, "superuser", option)?,
                "createdb" => self.lower_role_flag(&mut values, "createdb", option)?,
                "createrole" => self.lower_role_flag(&mut values, "createrole", option)?,
                "inherit" => self.lower_role_flag(&mut values, "inherit", option)?,
                "canlogin" => self.lower_role_flag(&mut values, "login", option)?,
                "isreplication" => self.lower_role_flag(&mut values, "replication", option)?,
                "bypassrls" => self.lower_role_flag(&mut values, "bypassrls", option)?,
                "connectionlimit" => {
                    let argument = self.required(option.arg.as_deref(), "connection limit")?;
                    let limit =
                        match self.required_kind(argument.node.as_ref(), "connection limit")? {
                            PgNode::Integer(integer) => integer.ival,
                            _ => return self.unsupported_error("connection limit"),
                        };
                    let connection_limit = [self.symbol(&limit.to_string())];
                    values.push(self.list("connection-limit", connection_limit));
                }
                "password" => {
                    let password = match option.arg.as_deref() {
                        Some(argument) => self.lower(argument)?,
                        None => self.symbol("null"),
                    };
                    values.push(self.list("password", [password]));
                }
                "validUntil" => {
                    let until_value_node =
                        self.required_node(option.arg.as_deref(), "VALID UNTIL value")?;
                    let lowered_valid_until = [self.lower(until_value_node)?];
                    values.push(self.list("valid-until", lowered_valid_until))
                }
                "rolemembers" => {
                    let role_members = self.lower_role_members(option)?;
                    values.push(
                        self.list(if drop { "drop-members" } else { "members" }, role_members),
                    )
                }
                "addroleto" => {
                    let lowered_in_role = self.lower_role_members(option)?;
                    values.push(self.list("in-role", lowered_in_role))
                }
                "adminmembers" => {
                    let lowered_admins = self.lower_role_members(option)?;
                    values.push(self.list("admins", lowered_admins))
                }
                "sysid" => {}
                other => return self.unsupported_error(format!("CREATE ROLE option {other}")),
            }
        }

        Ok(values)
    }

    fn lower_role_flag(
        &mut self,
        values: &mut Vec<Sexp>,
        name: &str,
        option: &pg::DefElem,
    ) -> Result<()> {
        let argument = self.required(option.arg.as_deref(), "role option value")?;
        let enabled = match self.required_kind(argument.node.as_ref(), "role option value")? {
            PgNode::Boolean(value) => value.boolval,
            _ => return self.unsupported_error("role option value"),
        };
        values.push(self.symbol(&if enabled {
            name.to_owned()
        } else {
            format!("no-{name}")
        }));

        Ok(())
    }

    fn lower_role_members(&mut self, option: &pg::DefElem) -> Result<Vec<Sexp>> {
        let argument = self.required(option.arg.as_deref(), "role option value")?;
        let members = match self.required_kind(argument.node.as_ref(), "role option value")? {
            PgNode::List(members) => members,
            _ => return self.unsupported_error("role option value"),
        };
        self.lower_role_spec_nodes(&members.items)
    }

    pub(in crate::parser) fn lower_alter_role_set(
        &mut self,
        stmt: &pg::AlterRoleSetStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("alter-role")];

        let set_statement = self.required(stmt.setstmt.as_ref(), "ALTER ROLE SET statement")?;
        match stmt.role.as_ref() {
            Some(spec) => values.push(self.lower_role_spec(spec)?),
            None => {
                let all_role = [self.symbol("all")];
                values.push(self.list("role", all_role))
            }
        }
        if !stmt.database.is_empty() {
            values.push(self.list("in-database", [Sexp::string(stmt.database.clone())]));
        }

        values.push(self.lower_variable_set(set_statement)?);

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_drop_role(&mut self, stmt: &pg::DropRoleStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("drop-role")];

        if stmt.missing_ok {
            values.push(self.symbol("if-exists"));
        }
        for node in &stmt.roles {
            let spec = match self.required_kind(node.node.as_ref(), "role")? {
                PgNode::RoleSpec(spec) => spec,
                _ => return self.unsupported_error("role"),
            };
            let kind = pg::RoleSpecType::try_from(spec.roletype)
                .map_err(|_| ParseError::new("unsupported PostgreSQL AST: role", 0))?;
            if kind == pg::RoleSpecType::RolespecCstring {
                values.push(Sexp::string(spec.rolename.clone()));
            } else {
                values.push(self.lower_role_spec(spec)?);
            }
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_grant(&mut self, stmt: &pg::GrantStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol(if stmt.is_grant { "grant" } else { "revoke" })];

        let mut privileges = self.lower_access_privileges(&stmt.privileges)?;
        if privileges.is_empty() {
            privileges.push(self.symbol("all"));
        }

        values.push(self.list("privileges", privileges));

        let lowered_on = self.lower_grant_objects(stmt)?;
        values.push(self.list("on", lowered_on));

        let grantees = self.lower_role_spec_nodes(&stmt.grantees)?;
        values.push(self.list(if stmt.is_grant { "to" } else { "from" }, grantees));
        if stmt.grant_option {
            values.push(self.symbol(if stmt.is_grant {
                "with-grant-option"
            } else {
                "grant-option-for"
            }));
        }
        if let Some(grantor) = stmt.grantor.as_ref() {
            let lowered_granted_by = [self.lower_role_spec(grantor)?];
            values.push(self.list("granted-by", lowered_granted_by));
        }
        if matches!(
            pg::DropBehavior::try_from(stmt.behavior),
            Ok(pg::DropBehavior::DropCascade)
        ) {
            values.push(self.symbol("cascade"));
        }

        Ok(Sexp::list(values))
    }

    fn lower_grant_objects(&mut self, stmt: &pg::GrantStmt) -> Result<Vec<Sexp>> {
        let target = pg::GrantTargetType::try_from(stmt.targtype)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: GRANT target", 0))?;
        match target {
            pg::GrantTargetType::AclTargetObject => stmt
                .objects
                .iter()
                .map(|node| self.lower_grant_object(node, stmt.objtype))
                .collect(),
            pg::GrantTargetType::AclTargetAllInSchema => {
                let object_type = pg::ObjectType::try_from(stmt.objtype).map_err(|_| {
                    ParseError::new("unsupported PostgreSQL AST: GRANT object type", 0)
                })?;
                let prefix = match object_type {
                    pg::ObjectType::ObjectTable => "all-tables-in-schema",
                    pg::ObjectType::ObjectSequence => "all-sequences-in-schema",
                    pg::ObjectType::ObjectFunction
                    | pg::ObjectType::ObjectProcedure
                    | pg::ObjectType::ObjectRoutine => "all-functions-in-schema",
                    _ => return self.unsupported_error("GRANT ON ALL object type"),
                };
                let schemas = stmt
                    .objects
                    .iter()
                    .map(|x| self.string_node(x))
                    .collect::<Result<Vec<_>>>()?;
                Ok(schemas
                    .into_iter()
                    .map(|schema| self.list(prefix, [Sexp::string(schema)]))
                    .collect())
            }
            _ => self.unsupported_error("GRANT target"),
        }
    }

    fn lower_grant_object(&mut self, node: &pg::Node, objtype: i32) -> Result<Sexp> {
        let object_type = pg::ObjectType::try_from(objtype)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: GRANT object type", 0))?;
        match object_type {
            pg::ObjectType::ObjectTable
            | pg::ObjectType::ObjectSequence
            | pg::ObjectType::ObjectForeignTable
            | pg::ObjectType::ObjectMatview => {
                let relation = match self.required_kind(node.node.as_ref(), "GRANT object")? {
                    PgNode::RangeVar(relation) => relation,
                    _ => return self.unsupported("GRANT object"),
                };
                self.lower_relation(relation)
            }
            pg::ObjectType::ObjectFunction
            | pg::ObjectType::ObjectProcedure
            | pg::ObjectType::ObjectRoutine
            | pg::ObjectType::ObjectAggregate => {
                let function = match self.required_kind(node.node.as_ref(), "GRANT object")? {
                    PgNode::ObjectWithArgs(function) => function,
                    _ => return self.unsupported("GRANT object"),
                };
                self.lower_object_with_args(function)
            }
            pg::ObjectType::ObjectDatabase
            | pg::ObjectType::ObjectSchema
            | pg::ObjectType::ObjectLanguage
            | pg::ObjectType::ObjectTablespace
            | pg::ObjectType::ObjectFdw
            | pg::ObjectType::ObjectForeignServer
            | pg::ObjectType::ObjectParameterAcl => {
                self.lower_qualified_name(std::slice::from_ref(node))
            }
            pg::ObjectType::ObjectDomain | pg::ObjectType::ObjectType => {
                let names = match self.required_kind(node.node.as_ref(), "GRANT object")? {
                    PgNode::List(names) => names,
                    _ => return self.unsupported("GRANT object"),
                };
                self.lower_qualified_name(&names.items)
            }
            pg::ObjectType::ObjectLargeobject => {
                let oid = match self.required_kind(node.node.as_ref(), "GRANT object")? {
                    PgNode::Integer(integer) => integer.ival,
                    _ => return self.unsupported("GRANT object"),
                };
                let large_object = [self.symbol(&oid.to_string())];
                Ok(self.list("large-object", large_object))
            }
            _ => self.unsupported("GRANT object type"),
        }
    }

    fn lower_access_privileges(&mut self, privileges: &[pg::Node]) -> Result<Vec<Sexp>> {
        privileges
            .iter()
            .map(
                |node| match self.required_kind(node.node.as_ref(), "privilege")? {
                    PgNode::AccessPriv(privilege) => self.lower_access_privilege(privilege),
                    _ => self.unsupported("privilege"),
                },
            )
            .collect()
    }

    pub(in crate::parser) fn lower_access_privilege(
        &mut self,
        privilege: &pg::AccessPriv,
    ) -> Result<Sexp> {
        let columns = self.lower_nodes(&privilege.cols)?;
        let column_privilege = [self.list("columns", columns)];
        if privilege.priv_name.is_empty() {
            let columns = self.lower_nodes(&privilege.cols)?;
            let all = [self.list("columns", columns)];
            if privilege.cols.is_empty() {
                return Ok(self.symbol("all"));
            }
            return Ok(self.list("all", all));
        }
        if privilege.cols.is_empty() {
            return Ok(self.symbol(if privilege.priv_name == "alter system" {
                "alter-system"
            } else {
                privilege.priv_name.as_str()
            }));
        }

        Ok(self.list(privilege.priv_name.as_str(), column_privilege))
    }

    pub(in crate::parser) fn lower_grant_role(&mut self, stmt: &pg::GrantRoleStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol(if stmt.is_grant {
            "grant-role"
        } else {
            "revoke-role"
        })];

        let grantee_roles = self.lower_role_spec_nodes(&stmt.grantee_roles)?;
        for node in &stmt.granted_roles {
            let role = match self.required_kind(node.node.as_ref(), "granted role")? {
                PgNode::AccessPriv(privilege) => privilege,
                _ => return self.unsupported_error("granted role"),
            };
            if !role.cols.is_empty() {
                return self.unsupported("granted role with columns");
            }

            values.push(self.list("role", [Sexp::string(role.priv_name.clone())]));
        }

        values.push(self.list(if stmt.is_grant { "to" } else { "from" }, grantee_roles));
        for node in &stmt.opt {
            let option = match self.required_kind(node.node.as_ref(), "GRANT ROLE option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error("GRANT ROLE option"),
            };
            let name = match option.defname.as_str() {
                "admin" | "inherit" | "set" | "grant" => option.defname.as_str(),
                other => return self.unsupported(format!("GRANT ROLE option {other}")),
            };
            let argument = self.required(option.arg.as_deref(), "GRANT ROLE option value")?;
            let enabled =
                match self.required_kind(argument.node.as_ref(), "GRANT ROLE option value")? {
                    PgNode::Boolean(value) => value.boolval,
                    _ => return self.unsupported_error("GRANT ROLE option value"),
                };
            match (stmt.is_grant, enabled) {
                (true, true) => values.push(self.symbol(&format!("with-{name}-option"))),
                (true, false) => values.push(self.symbol(&format!("without-{name}-option"))),
                (false, false) => values.push(self.symbol(&format!("{name}-option-for"))),
                (false, true) => return self.unsupported(format!("GRANT ROLE {name} option")),
            }
        }
        if let Some(grantor) = stmt.grantor.as_ref() {
            let lowered_granted_by = [self.lower_role_spec(grantor)?];
            values.push(self.list("granted-by", lowered_granted_by));
        }
        if matches!(
            pg::DropBehavior::try_from(stmt.behavior),
            Ok(pg::DropBehavior::DropCascade)
        ) {
            values.push(self.symbol("cascade"));
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_default_privileges(
        &mut self,
        stmt: &pg::AlterDefaultPrivilegesStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("alter-default-privileges")];

        let action = self.required(stmt.action.as_ref(), "ALTER DEFAULT PRIVILEGES action")?;
        for node in &stmt.options {
            let option =
                match self.required_kind(node.node.as_ref(), "ALTER DEFAULT PRIVILEGES option")? {
                    PgNode::DefElem(option) => option,
                    _ => return self.unsupported_error("ALTER DEFAULT PRIVILEGES option"),
                };
            let argument = self.required(
                option.arg.as_deref(),
                "ALTER DEFAULT PRIVILEGES option value",
            )?;
            let items = match self.required_kind(
                argument.node.as_ref(),
                "ALTER DEFAULT PRIVILEGES option value",
            )? {
                PgNode::List(items) => items,
                _ => return self.unsupported_error("ALTER DEFAULT PRIVILEGES option value"),
            };
            match option.defname.as_str() {
                "schemas" => {
                    let schema_names = items
                        .items
                        .iter()
                        .map(|x| self.string_node(x))
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .map(Sexp::string);
                    values.push(self.list("in-schema", schema_names))
                }
                "roles" => {
                    let lowered_for_role = self.lower_role_spec_nodes(&items.items)?;
                    values.push(self.list("for-role", lowered_for_role))
                }
                other => {
                    return self.unsupported(format!("ALTER DEFAULT PRIVILEGES option {other}"));
                }
            }
        }

        values.extend(self.lower_default_privileges_action(action)?);

        Ok(Sexp::list(values))
    }

    fn lower_default_privileges_action(&mut self, action: &pg::GrantStmt) -> Result<Vec<Sexp>> {
        if !matches!(
            pg::GrantTargetType::try_from(action.targtype),
            Ok(pg::GrantTargetType::AclTargetDefaults)
        ) || action.grantor.is_some()
            || !action.objects.is_empty()
        {
            return self.unsupported_error("ALTER DEFAULT PRIVILEGES action");
        }

        let object_type = pg::ObjectType::try_from(action.objtype)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: object type", 0))?;
        let target = match object_type {
            pg::ObjectType::ObjectTable => "on-tables",
            pg::ObjectType::ObjectSequence => "on-sequences",
            pg::ObjectType::ObjectFunction => "on-functions",
            pg::ObjectType::ObjectSchema => "on-schemas",
            pg::ObjectType::ObjectType => "on-types",
            pg::ObjectType::ObjectRoutine => "on-routines",
            pg::ObjectType::ObjectLargeobject => "on-large-objects",
            _ => return self.unsupported_error("ALTER DEFAULT PRIVILEGES object type"),
        };
        let mut values = vec![self.symbol(if action.is_grant { "grant" } else { "revoke" })];

        let mut privileges = self.lower_access_privileges(&action.privileges)?;
        if privileges.is_empty() {
            privileges.push(self.symbol("all"));
        }

        values.push(self.list("privileges", privileges));
        values.push(self.symbol(target));

        let grantees = self.lower_role_spec_nodes(&action.grantees)?;
        values.push(self.list(if action.is_grant { "to" } else { "from" }, grantees));
        if action.grant_option {
            values.push(self.symbol(if action.is_grant {
                "with-grant-option"
            } else {
                "grant-option-for"
            }));
        }

        Ok(values)
    }

    pub(in crate::parser) fn lower_reassign_owned(
        &mut self,
        stmt: &pg::ReassignOwnedStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("reassign-owned")];

        values.extend(self.lower_role_spec_nodes(&stmt.roles)?);

        let role = self.required(stmt.newrole.as_ref(), "REASSIGN OWNED role")?;
        let lowered_to = [self.lower_role_spec(role)?];
        values.push(self.list("to", lowered_to));
        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_drop_owned(&mut self, stmt: &pg::DropOwnedStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("drop-owned")];

        values.extend(self.lower_role_spec_nodes(&stmt.roles)?);
        if matches!(
            pg::DropBehavior::try_from(stmt.behavior),
            Ok(pg::DropBehavior::DropCascade)
        ) {
            values.push(self.symbol("cascade"));
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_security_label(
        &mut self,
        stmt: &pg::SecLabelStmt,
    ) -> Result<Sexp> {
        let object_type = pg::ObjectType::try_from(stmt.objtype).map_err(|_| {
            ParseError::new("unsupported PostgreSQL AST: SECURITY LABEL object type", 0)
        })?;
        match object_type {
            pg::ObjectType::ObjectTable
            | pg::ObjectType::ObjectView
            | pg::ObjectType::ObjectMatview
            | pg::ObjectType::ObjectIndex
            | pg::ObjectType::ObjectSequence
            | pg::ObjectType::ObjectSchema
            | pg::ObjectType::ObjectDatabase
            | pg::ObjectType::ObjectColumn
            | pg::ObjectType::ObjectFunction
            | pg::ObjectType::ObjectRoutine
            | pg::ObjectType::ObjectAggregate
            | pg::ObjectType::ObjectProcedure
            | pg::ObjectType::ObjectRole
            | pg::ObjectType::ObjectType
            | pg::ObjectType::ObjectDomain
            | pg::ObjectType::ObjectForeignTable
            | pg::ObjectType::ObjectLargeobject
            | pg::ObjectType::ObjectFdw
            | pg::ObjectType::ObjectForeignServer
            | pg::ObjectType::ObjectTablespace
            | pg::ObjectType::ObjectEventTrigger
            | pg::ObjectType::ObjectPublication
            | pg::ObjectType::ObjectSubscription
            | pg::ObjectType::ObjectLanguage
            | pg::ObjectType::ObjectStatisticExt
            | pg::ObjectType::ObjectAccessMethod
            | pg::ObjectType::ObjectExtension
            | pg::ObjectType::ObjectCollation
            | pg::ObjectType::ObjectConversion => {}
            _ => return self.unsupported("SECURITY LABEL object type"),
        }

        let object = self.required_node(stmt.object.as_deref(), "SECURITY LABEL object")?;
        let name = self.lower_annotated_object(object, "SECURITY LABEL object")?;
        let mut values = vec![self.symbol("security-label")];

        if !stmt.provider.is_empty() {
            values.push(self.list("provider", [Sexp::string(stmt.provider.clone())]));
        }

        values.push(name);
        if stmt.label.is_empty() {
            values.push(self.symbol("drop"));
        } else {
            values.push(Sexp::string(stmt.label.clone()));
        }

        Ok(Sexp::list(values))
    }
}
