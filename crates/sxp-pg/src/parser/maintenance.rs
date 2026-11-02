use super::*;

impl PostgresParser {
    pub(super) fn lower_checkpoint(&mut self) -> Result<Sexp> {
        Ok(Sexp::list([self.symbol("checkpoint")]))
    }

    pub(super) fn lower_load(&mut self, stmt: &pg::LoadStmt) -> Result<Sexp> {
        Ok(self.list("load", [Sexp::string(stmt.filename.clone())]))
    }

    pub(super) fn lower_alter_system(&mut self, stmt: &pg::AlterSystemStmt) -> Result<Sexp> {
        let setstmt = self.required(stmt.setstmt.as_ref(), "ALTER SYSTEM SET statement")?;
        let kind = pg::VariableSetKind::try_from(setstmt.kind).map_err(|_| {
            ParseError::new("unsupported PostgreSQL AST: ALTER SYSTEM SET statement", 0)
        })?;
        match kind {
            pg::VariableSetKind::VarSetValue => {
                let mut values = vec![
                    self.symbol("alter-system-set"),
                    Sexp::string(setstmt.name.clone()),
                ];

                values.extend(self.lower_nodes(&setstmt.args)?);
                Ok(Sexp::list(values))
            }
            pg::VariableSetKind::VarSetDefault => Ok(Sexp::list([
                self.symbol("alter-system-set"),
                Sexp::string(setstmt.name.clone()),
                self.symbol("default"),
            ])),
            pg::VariableSetKind::VarReset => Ok(Sexp::list([
                self.symbol("alter-system-reset"),
                Sexp::string(setstmt.name.clone()),
            ])),
            pg::VariableSetKind::VarResetAll => Ok(Sexp::list([
                self.symbol("alter-system-reset"),
                self.symbol("all"),
            ])),
            _ => self.unsupported("ALTER SYSTEM SET statement"),
        }
    }

    fn lower_maintenance_option(&mut self, option: &pg::DefElem) -> Result<Sexp> {
        match option.arg.as_deref() {
            None => Ok(self.symbol(&option.defname.replace('_', "-"))),
            Some(argument) => {
                let opt = [
                    Sexp::string(option.defname.replace('_', "-")),
                    self.lower(argument)?,
                ];
                Ok(self.list("option", opt))
            }
        }
    }

    pub(super) fn lower_vacuum(&mut self, stmt: &pg::VacuumStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol(if stmt.is_vacuumcmd {
            "vacuum"
        } else {
            "analyze"
        })];

        for node in &stmt.options {
            let option = match self.required_kind(node.node.as_ref(), "VACUUM option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported("VACUUM option"),
            };
            values.push(self.lower_maintenance_option(option)?);
        }
        if !stmt.rels.is_empty() {
            let tables = stmt
                .rels
                .iter()
                .map(|relation| self.lower_vacuum_relation(relation))
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("tables", tables));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_vacuum_relation(&mut self, node: &pg::Node) -> Result<Sexp> {
        let vacuum_relation = match self.required_kind(node.node.as_ref(), "VACUUM relation")? {
            PgNode::VacuumRelation(relation) => relation,
            _ => return self.unsupported("VACUUM relation"),
        };
        let relation = self.required(vacuum_relation.relation.as_ref(), "VACUUM relation")?;
        let mut values = vec![self.lower_relation(relation)?];

        if !vacuum_relation.va_cols.is_empty() {
            let vacuum_columns = vacuum_relation
                .va_cols
                .iter()
                .map(|column| self.string_node(column))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(Sexp::string);
            values.push(self.list("columns", vacuum_columns));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_cluster(&mut self, stmt: &pg::ClusterStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("cluster")];

        if let Some(relation) = stmt.relation.as_ref() {
            values.push(self.lower_relation(relation)?);
        }
        if !stmt.indexname.is_empty() {
            values.push(self.list("using", [Sexp::string(stmt.indexname.clone())]));
        }
        for node in &stmt.params {
            let option = match self.required_kind(node.node.as_ref(), "CLUSTER option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported("CLUSTER option"),
            };
            values.push(self.lower_maintenance_option(option)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_reindex(&mut self, stmt: &pg::ReindexStmt) -> Result<Sexp> {
        let kind = pg::ReindexObjectType::try_from(stmt.kind)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: REINDEX object type", 0))?;
        let name = match kind {
            pg::ReindexObjectType::ReindexObjectIndex => "reindex-index",
            pg::ReindexObjectType::ReindexObjectTable => "reindex-table",
            pg::ReindexObjectType::ReindexObjectSchema => "reindex-schema",
            pg::ReindexObjectType::ReindexObjectSystem => "reindex-system",
            pg::ReindexObjectType::ReindexObjectDatabase => "reindex-database",
            _ => return self.unsupported("REINDEX object type"),
        };
        let mut values = vec![self.symbol(name)];

        match kind {
            pg::ReindexObjectType::ReindexObjectIndex
            | pg::ReindexObjectType::ReindexObjectTable => {
                let relation = self.required(stmt.relation.as_ref(), "REINDEX relation")?;
                values.push(self.lower_relation(relation)?);
            }
            pg::ReindexObjectType::ReindexObjectSchema => {
                values.push(Sexp::string(stmt.name.clone()));
            }
            _ => {
                if !stmt.name.is_empty() {
                    values.push(Sexp::string(stmt.name.clone()));
                }
            }
        }
        for node in &stmt.params {
            let option = match self.required_kind(node.node.as_ref(), "REINDEX option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported("REINDEX option"),
            };
            match option.defname.as_str() {
                "concurrently" if option.arg.is_none() => values.push(self.symbol("concurrently")),
                "verbose" if option.arg.is_none() => values.push(self.symbol("verbose")),
                "tablespace" => {
                    let tablespace_node =
                        self.required_node(option.arg.as_deref(), "REINDEX tablespace")?;
                    let tablespace_name = [Sexp::string(self.string_node(tablespace_node)?)];
                    values.push(self.list("tablespace", tablespace_name))
                }
                _ => values.push(self.lower_maintenance_option(option)?),
            }
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_refresh_matview(&mut self, stmt: &pg::RefreshMatViewStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("refresh-materialized-view")];

        let relation = self.required(stmt.relation.as_ref(), "REFRESH relation")?;
        if stmt.concurrent {
            values.push(self.symbol("concurrently"));
        }

        values.push(self.lower_relation(relation)?);
        if stmt.skip_data {
            values.push(self.symbol("with-no-data"));
        }

        Ok(Sexp::list(values))
    }
}
