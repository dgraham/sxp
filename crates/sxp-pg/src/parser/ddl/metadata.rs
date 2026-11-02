use super::*;

impl PostgresParser {
    pub(in crate::parser) fn lower_create_stats(
        &mut self,
        stmt: &pg::CreateStatsStmt,
    ) -> Result<Sexp> {
        if !stmt.stxcomment.is_empty() || stmt.transformed {
            return self.unsupported("CREATE STATISTICS options");
        }

        let mut values = vec![
            self.symbol("create-statistics"),
            self.lower_qualified_name(&stmt.defnames)?,
        ];

        let kinds = stmt
            .stat_types
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?;
        let kinds = kinds.into_iter().map(|kind| self.symbol(&kind)).collect();
        self.optional_clause(&mut values, "kinds", kinds);
        let on = self.lower_nodes(&stmt.exprs)?;
        values.push(self.list("on", on));

        let from = self.lower_nodes(&stmt.relations)?;
        values.push(self.list("from", from));

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_stats_element(
        &mut self,
        element: &pg::StatsElem,
    ) -> Result<Sexp> {
        match (element.name.is_empty(), element.expr.as_deref()) {
            (false, None) => Ok(self.list("column", [Sexp::string(element.name.clone())])),
            (true, Some(expr)) => self.lower(expr),
            _ => self.unsupported("statistics element"),
        }
    }

    pub(in crate::parser) fn lower_alter_stats(
        &mut self,
        stmt: &pg::AlterStatsStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("alter-statistics"),
            self.lower_qualified_name(&stmt.defnames)?,
        ];

        let statistics_value_node =
            self.required_node(stmt.stxstattarget.as_deref(), "SET STATISTICS value")?;
        let lowered_set_statistics = [self.lower(statistics_value_node)?];
        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        values.push(self.list("set-statistics", lowered_set_statistics));

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_comment(&mut self, stmt: &pg::CommentStmt) -> Result<Sexp> {
        let object_type = pg::ObjectType::try_from(stmt.objtype)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: COMMENT object type", 0))?;
        let kind = match object_type {
            pg::ObjectType::ObjectTable => "table",
            pg::ObjectType::ObjectView => "view",
            pg::ObjectType::ObjectMatview => "materialized-view",
            pg::ObjectType::ObjectIndex => "index",
            pg::ObjectType::ObjectSequence => "sequence",
            pg::ObjectType::ObjectSchema => "schema",
            pg::ObjectType::ObjectDatabase => "database",
            pg::ObjectType::ObjectRole => "role",
            pg::ObjectType::ObjectColumn => "column",
            pg::ObjectType::ObjectFunction | pg::ObjectType::ObjectRoutine => "function",
            pg::ObjectType::ObjectProcedure => "procedure",
            pg::ObjectType::ObjectAggregate => "aggregate",
            pg::ObjectType::ObjectOperator => "operator",
            pg::ObjectType::ObjectTrigger => "trigger",
            pg::ObjectType::ObjectRule => "rule",
            pg::ObjectType::ObjectPolicy => "policy",
            pg::ObjectType::ObjectTabconstraint => "constraint",
            pg::ObjectType::ObjectCast => "cast",
            pg::ObjectType::ObjectType => "type",
            pg::ObjectType::ObjectDomain => "domain",
            pg::ObjectType::ObjectTablespace => "tablespace",
            pg::ObjectType::ObjectForeignTable => "foreign-table",
            pg::ObjectType::ObjectFdw => "foreign-data-wrapper",
            pg::ObjectType::ObjectForeignServer => "server",
            pg::ObjectType::ObjectLanguage => "language",
            pg::ObjectType::ObjectEventTrigger => "event-trigger",
            pg::ObjectType::ObjectPublication => "publication",
            pg::ObjectType::ObjectSubscription => "subscription",
            pg::ObjectType::ObjectStatisticExt => "statistics",
            pg::ObjectType::ObjectAccessMethod => "access-method",
            pg::ObjectType::ObjectExtension => "extension",
            pg::ObjectType::ObjectCollation => "collation",
            pg::ObjectType::ObjectConversion => "conversion",
            pg::ObjectType::ObjectLargeobject => "large-object",
            pg::ObjectType::ObjectTsconfiguration => "text-search-configuration",
            pg::ObjectType::ObjectTsdictionary => "text-search-dictionary",
            pg::ObjectType::ObjectTsparser => "text-search-parser",
            pg::ObjectType::ObjectTstemplate => "text-search-template",
            pg::ObjectType::ObjectOpclass => "operator-class",
            pg::ObjectType::ObjectOpfamily => "operator-family",
            pg::ObjectType::ObjectTransform => "transform",
            pg::ObjectType::ObjectDomconstraint => "domain-constraint",
            _ => return self.unsupported("COMMENT ON object type"),
        };
        let object = self.required_node(stmt.object.as_deref(), "COMMENT object")?;
        let mut values = vec![self.symbol(&format!("comment-on-{kind}"))];

        if kind == "cast" {
            let names = match self.required_kind(object.node.as_ref(), "COMMENT object")? {
                PgNode::List(names) => names,
                _ => return self.unsupported("COMMENT object"),
            };
            let (source, target) = self.lower_cast_names(names, "COMMENT ON CAST")?;
            values.push(self.list("source", [source]));
            values.push(self.list("target", [target]));
        } else if kind == "operator-class" || kind == "operator-family" {
            values.extend(self.lower_operator_class_name(object, kind)?);
        } else if kind == "transform" {
            values.extend(self.lower_transform_name(object, "COMMENT ON TRANSFORM")?);
        } else if kind == "domain-constraint" {
            values.push(self.lower_domain_constraint_name(object)?);
        } else {
            values.push(self.lower_annotated_object(object, "COMMENT object")?);
        }
        if stmt.comment.is_empty() {
            values.push(self.symbol("drop"));
        } else {
            values.push(Sexp::string(stmt.comment.clone()));
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_annotated_object(
        &mut self,
        object: &pg::Node,
        context: &str,
    ) -> Result<Sexp> {
        match self.required_kind(object.node.as_ref(), context)? {
            PgNode::List(names) => self.lower_qualified_name(&names.items),
            PgNode::String(_) => self.lower_qualified_name(std::slice::from_ref(object)),
            PgNode::TypeName(type_name) => self.lower_qualified_name(&type_name.names),
            PgNode::ObjectWithArgs(function) => self.lower_object_with_args(function),
            PgNode::Integer(oid) => {
                let large_object = [self.symbol(&oid.ival.to_string())];
                Ok(self.list("large-object", large_object))
            }
            _ => self.unsupported(context),
        }
    }

    pub(in crate::parser) fn lower_cast_names(
        &mut self,
        names: &pg::List,
        context: &str,
    ) -> Result<(Sexp, Sexp)> {
        let mut types: Vec<&pg::TypeName> = Vec::with_capacity(names.items.len());
        for node in &names.items {
            match node.node.as_ref() {
                Some(PgNode::TypeName(type_name)) => types.push(type_name),
                _ => return self.unsupported_error(context),
            }
        }

        let [source, target] = types.as_slice() else {
            return self.unsupported_error(context);
        };
        Ok((
            self.lower_qualified_name(&source.names)?,
            self.lower_qualified_name(&target.names)?,
        ))
    }

    fn lower_operator_class_name(&mut self, object: &pg::Node, context: &str) -> Result<Vec<Sexp>> {
        let names = match self.required_kind(object.node.as_ref(), context)? {
            PgNode::List(names) => &names.items,
            _ => return self.unsupported_error(context),
        };
        let (method, name) = match names.split_first() {
            Some((method, name)) if !name.is_empty() => (method, name),
            _ => return self.unsupported_error(context),
        };
        let operator_class_method = [Sexp::string(self.string_node(method)?)];
        Ok(vec![
            self.lower_qualified_name(name)?,
            self.list("using", operator_class_method),
        ])
    }

    pub(in crate::parser) fn lower_transform_name(
        &mut self,
        object: &pg::Node,
        context: &str,
    ) -> Result<Vec<Sexp>> {
        let names = match self.required_kind(object.node.as_ref(), context)? {
            PgNode::List(names) => &names.items,
            _ => return self.unsupported_error(context),
        };
        let [type_node, language] = names.as_slice() else {
            return self.unsupported_error(context);
        };
        let PgNode::TypeName(type_name) = self.required_kind(type_node.node.as_ref(), context)?
        else {
            return self.unsupported_error(context);
        };
        let transform_language = [Sexp::string(self.string_node(language)?)];
        let lowered_type = [self.lower_qualified_name(&type_name.names)?];
        Ok(vec![
            self.list("type", lowered_type),
            self.list("language", transform_language),
        ])
    }

    fn lower_domain_constraint_name(&mut self, object: &pg::Node) -> Result<Sexp> {
        let names =
            match self.required_kind(object.node.as_ref(), "COMMENT ON DOMAIN CONSTRAINT")? {
                PgNode::List(names) => &names.items,
                _ => return self.unsupported("COMMENT ON DOMAIN CONSTRAINT"),
            };
        let [domain, constraint] = names.as_slice() else {
            return self.unsupported("COMMENT ON DOMAIN CONSTRAINT");
        };
        let PgNode::TypeName(domain) =
            self.required_kind(domain.node.as_ref(), "COMMENT ON DOMAIN CONSTRAINT")?
        else {
            return self.unsupported("COMMENT ON DOMAIN CONSTRAINT");
        };
        let mut values = vec![self.symbol("name")];

        values.extend(
            domain
                .names
                .iter()
                .map(|x| self.string_node(x))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(Sexp::string),
        );
        values.push(Sexp::string(self.string_node(constraint)?));

        Ok(Sexp::list(values))
    }
}
