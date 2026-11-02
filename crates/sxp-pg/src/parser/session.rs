use super::*;

impl PostgresParser {
    pub(super) fn lower_discard(&mut self, stmt: &pg::DiscardStmt) -> Result<Sexp> {
        let target = pg::DiscardMode::try_from(stmt.target)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: DISCARD target", 0))?;
        let kind = match target {
            pg::DiscardMode::DiscardAll => "all",
            pg::DiscardMode::DiscardPlans => "plans",
            pg::DiscardMode::DiscardSequences => "sequences",
            pg::DiscardMode::DiscardTemp => "temp",
            _ => return self.unsupported("DISCARD target"),
        };
        let discard_target = [self.symbol(kind)];

        Ok(self.list("discard", discard_target))
    }

    pub(super) fn lower_prepare(&mut self, stmt: &pg::PrepareStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("prepare"), Sexp::string(stmt.name.clone())];

        let query_node = self.required_node(stmt.query.as_deref(), "PREPARE query")?;
        let query = [self.lower(query_node)?];
        if !stmt.argtypes.is_empty() {
            let params = stmt
                .argtypes
                .iter()
                .map(|node| {
                    match self.required_kind(node.node.as_ref(), "PREPARE parameter type")? {
                        PgNode::TypeName(type_name) => self.lower_type(type_name),
                        _ => self.unsupported("PREPARE parameter type"),
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("parameters", params));
        }

        values.push(self.list("query", query));

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_execute(&mut self, stmt: &pg::ExecuteStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("execute"), Sexp::string(stmt.name.clone())];

        if !stmt.params.is_empty() {
            let parameters = self.lower_nodes(&stmt.params)?;
            values.push(self.list("parameters", parameters));
        }
        Ok(Sexp::list(values))
    }

    pub(super) fn lower_deallocate(&mut self, stmt: &pg::DeallocateStmt) -> Result<Sexp> {
        if stmt.isall {
            let all_deallocate = [self.symbol("all")];
            Ok(self.list("deallocate", all_deallocate))
        } else {
            Ok(self.list("deallocate", [Sexp::string(stmt.name.clone())]))
        }
    }

    pub(super) fn lower_declare_cursor(&mut self, stmt: &pg::DeclareCursorStmt) -> Result<Sexp> {
        const CURSOR_OPTIONS: i32 = 0x001 | 0x002 | 0x004 | 0x008 | 0x020 | 0x100;

        if stmt.options & !CURSOR_OPTIONS != 0 {
            return self.unsupported("DECLARE CURSOR option");
        }

        let mut values = vec![
            self.symbol("declare"),
            Sexp::string(stmt.portalname.clone()),
        ];

        let cursor_query_node = self.required_node(stmt.query.as_deref(), "cursor query")?;
        let query = [self.lower(cursor_query_node)?];
        if stmt.options & 0x001 != 0 {
            values.push(self.symbol("binary"));
        }
        if stmt.options & 0x002 != 0 {
            values.push(self.symbol("scroll"));
        }
        if stmt.options & 0x004 != 0 {
            values.push(self.symbol("no-scroll"));
        }
        if stmt.options & 0x008 != 0 {
            values.push(self.symbol("insensitive"));
        }
        if stmt.options & 0x020 != 0 {
            values.push(self.symbol("with-hold"));
        }

        values.push(self.list("query", query));

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_close_cursor(&mut self, stmt: &pg::ClosePortalStmt) -> Result<Sexp> {
        Ok(self.list("close", [Sexp::string(stmt.portalname.clone())]))
    }

    pub(super) fn lower_fetch(&mut self, stmt: &pg::FetchStmt) -> Result<Sexp> {
        const FETCH_ALL: i64 = i64::MAX;
        let direction = pg::FetchDirection::try_from(stmt.direction)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: FETCH direction", 0))?;
        let mut values = vec![self.symbol(if stmt.ismove { "move" } else { "fetch" })];

        match direction {
            pg::FetchDirection::FetchForward => {
                let count = if stmt.how_many == FETCH_ALL {
                    "all".to_owned()
                } else {
                    stmt.how_many.to_string()
                };
                values.push(self.symbol(&count));
            }
            pg::FetchDirection::FetchBackward => {
                values.push(self.symbol("backward"));

                let count = if stmt.how_many == FETCH_ALL {
                    "all".to_owned()
                } else {
                    stmt.how_many.to_string()
                };
                values.push(self.symbol(&count));
            }
            pg::FetchDirection::FetchAbsolute => {
                values.push(self.symbol("absolute"));
                values.push(self.symbol(&stmt.how_many.to_string()));
            }
            pg::FetchDirection::FetchRelative => {
                values.push(self.symbol("relative"));
                values.push(self.symbol(&stmt.how_many.to_string()));
            }
            _ => return self.unsupported("FETCH direction"),
        }

        values.push(Sexp::string(stmt.portalname.clone()));

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_lock(&mut self, stmt: &pg::LockStmt) -> Result<Sexp> {
        let mode = match stmt.mode {
            1 => "access-share",
            2 => "row-share",
            3 => "row-exclusive",
            4 => "share-update-exclusive",
            5 => "share",
            6 => "share-row-exclusive",
            7 => "exclusive",
            8 => "access-exclusive",
            _ => return self.unsupported("LOCK mode"),
        };
        let mut values = vec![self.symbol("lock")];

        values.extend(self.lower_nodes(&stmt.relations)?);
        values.push(self.symbol(mode));
        if stmt.nowait {
            values.push(self.symbol("nowait"));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_notify(&mut self, stmt: &pg::NotifyStmt) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("notify"),
            Sexp::string(stmt.conditionname.clone()),
        ];

        if !stmt.payload.is_empty() {
            values.push(Sexp::string(stmt.payload.clone()));
        }
        Ok(Sexp::list(values))
    }

    pub(super) fn lower_listen(&mut self, stmt: &pg::ListenStmt) -> Result<Sexp> {
        Ok(self.list("listen", [Sexp::string(stmt.conditionname.clone())]))
    }

    pub(super) fn lower_unlisten(&mut self, stmt: &pg::UnlistenStmt) -> Result<Sexp> {
        if stmt.conditionname.is_empty() {
            let wildcard_symbol = [self.symbol("*")];
            Ok(self.list("unlisten", wildcard_symbol))
        } else {
            Ok(self.list("unlisten", [Sexp::string(stmt.conditionname.clone())]))
        }
    }
}
