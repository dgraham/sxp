use super::*;

mod database;
mod metadata;
mod replication;
mod routines;
mod security;
#[cfg(test)]
mod tests;

fn recursive_view_query(stmt: &pg::ViewStmt) -> Option<&pg::Node> {
    let relation = stmt.view.as_ref()?;
    let PgNode::SelectStmt(query) = stmt.query.as_deref().and_then(|node| node.node.as_ref())?
    else {
        return None;
    };
    let with = query.with_clause.as_ref()?;
    if !with.recursive || with.ctes.len() != 1 {
        return None;
    }

    let Some(PgNode::CommonTableExpr(cte)) = with.ctes[0].node.as_ref() else {
        return None;
    };
    let columns = string_nodes(&stmt.aliases)?;
    if cte.ctename != relation.relname || string_nodes(&cte.aliascolnames)? != columns {
        return None;
    }
    if !is_recursive_view_body(query, relation, &columns) {
        return None;
    }
    cte.ctequery.as_deref()
}

fn is_recursive_view_body(
    query: &pg::SelectStmt,
    relation: &pg::RangeVar,
    columns: &[&str],
) -> bool {
    if query.from_clause.len() != 1 || query.target_list.len() != columns.len() {
        return false;
    }

    let Some(PgNode::RangeVar(from)) = query.from_clause[0].node.as_ref() else {
        return false;
    };
    if !from.catalogname.is_empty()
        || !from.schemaname.is_empty()
        || from.relname != relation.relname
        || from.alias.is_some()
        || from.location != -1
    {
        return false;
    }
    for (target, column) in query.target_list.iter().zip(columns) {
        let Some(PgNode::ResTarget(target)) = target.node.as_ref() else {
            return false;
        };
        let Some(PgNode::ColumnRef(reference)) =
            target.val.as_deref().and_then(|node| node.node.as_ref())
        else {
            return false;
        };
        if target.location != -1
            || !target.name.is_empty()
            || !target.indirection.is_empty()
            || reference.location != -1
        {
            return false;
        }

        let [field] = reference.fields.as_slice() else {
            return false;
        };
        let Some(PgNode::String(string)) = field.node.as_ref() else {
            return false;
        };
        if string.sval != *column {
            return false;
        }
    }
    true
}

fn is_single_public_role(roles: &[pg::Node]) -> bool {
    match roles {
        [role] => match role.node.as_ref() {
            Some(PgNode::RoleSpec(spec)) => {
                pg::RoleSpecType::try_from(spec.roletype) == Ok(pg::RoleSpecType::RolespecPublic)
            }
            _ => false,
        },
        _ => false,
    }
}

const TRIGGER_TYPE_BEFORE: i32 = 1 << 1;
const TRIGGER_TYPE_INSERT: i32 = 1 << 2;
const TRIGGER_TYPE_DELETE: i32 = 1 << 3;
const TRIGGER_TYPE_UPDATE: i32 = 1 << 4;
const TRIGGER_TYPE_TRUNCATE: i32 = 1 << 5;
const TRIGGER_TYPE_INSTEAD: i32 = 1 << 6;
const TRIGGER_TYPE_EVENTS: i32 =
    TRIGGER_TYPE_INSERT | TRIGGER_TYPE_DELETE | TRIGGER_TYPE_UPDATE | TRIGGER_TYPE_TRUNCATE;

fn string_nodes(nodes: &[pg::Node]) -> Option<Vec<&str>> {
    if nodes.is_empty() {
        return None;
    }
    nodes
        .iter()
        .map(|node| match node.node.as_ref()? {
            PgNode::String(string) => Some(string.sval.as_str()),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Copy)]
enum AlterCommandName {
    Required,
    Optional,
    Forbidden,
}

#[derive(Clone, Copy)]
enum AlterCommandBehavior {
    RestrictOnly,
    CascadeAllowed,
}

#[derive(Clone, Copy)]
enum AlterCommandDef {
    Required,
    Optional,
    Forbidden,
}

#[derive(Clone, Copy)]
enum AlterCommandMissingOk {
    Optional,
    Forbidden,
}

#[derive(Clone, Copy)]
enum AlterCommandNumber {
    Optional,
    Forbidden,
}

impl PostgresParser {
    pub(super) fn lower_create_table(&mut self, stmt: &pg::CreateStmt) -> Result<Sexp> {
        let partbound = stmt.partbound.as_ref();
        let relation = self.required(stmt.relation.as_ref(), "CREATE TABLE relation")?;
        let mut values = vec![self.symbol("create-table")];

        match relation.relpersistence.as_str() {
            "p" | "" => {}
            "t" => {
                let temporary_persistence = [self.symbol("temporary")];
                values.push(self.list("persistence", temporary_persistence))
            }
            "u" => {
                let unlogged_persistence = [self.symbol("unlogged")];
                values.push(self.list("persistence", unlogged_persistence))
            }
            persistence => return self.unsupported(format!("table persistence {persistence}")),
        }
        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }

        values.push(self.lower_relation(relation)?);

        let columns = self.lower_nodes(&stmt.table_elts)?;
        if let Some(of_typename) = stmt.of_typename.as_ref() {
            let of = [self.lower_type(of_typename)?];
            values.push(self.list("of", of));
        }

        values.push(self.list("columns", columns));

        let constraints = self.lower_nodes(&stmt.constraints)?;
        if partbound.is_none() {
            let inherits = self.lower_nodes(&stmt.inh_relations)?;
            self.optional_clause(&mut values, "inherits", inherits);
        }
        self.optional_clause(&mut values, "constraints", constraints);
        let options = self.lower_nodes(&stmt.options)?;
        self.optional_clause(&mut values, "options", options);
        if !stmt.access_method.is_empty() {
            values.push(self.list("using", [Sexp::string(stmt.access_method.clone())]));
        }
        match pg::OnCommitAction::try_from(stmt.oncommit) {
            Ok(pg::OnCommitAction::OncommitNoop) => {}
            Ok(pg::OnCommitAction::OncommitPreserveRows) => {
                let preserve_rows_on_commit = [self.symbol("preserve-rows")];
                values.push(self.list("on-commit", preserve_rows_on_commit));
            }
            Ok(pg::OnCommitAction::OncommitDeleteRows) => {
                let delete_rows_on_commit = [self.symbol("delete-rows")];
                values.push(self.list("on-commit", delete_rows_on_commit));
            }
            Ok(pg::OnCommitAction::OncommitDrop) => {
                let drop_on_commit = [self.symbol("drop")];
                values.push(self.list("on-commit", drop_on_commit));
            }
            _ => return self.unsupported("CREATE TABLE on commit action"),
        }
        if !stmt.tablespacename.is_empty() {
            values.push(self.list("tablespace", [Sexp::string(stmt.tablespacename.clone())]));
        }
        if let Some(bound) = partbound {
            let parent = match stmt.inh_relations.as_slice() {
                [parent] => parent,
                _ => return self.unsupported("CREATE TABLE partition parents"),
            };
            let partition_of = [self.lower(parent)?];
            values.push(self.list("partition-of", partition_of));
            values.push(self.lower_partition_bound_spec(bound)?);
        }
        if let Some(partition) = stmt.partspec.as_ref() {
            values.push(self.lower_partition_spec(partition)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_create_schema(&mut self, stmt: &pg::CreateSchemaStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("create-schema")];

        let elements = self.lower_nodes(&stmt.schema_elts)?;
        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }
        if !stmt.schemaname.is_empty() {
            values.push(Sexp::string(stmt.schemaname.clone()));
        }
        if let Some(authrole) = stmt.authrole.as_ref() {
            let auth = [self.lower_role_spec(authrole)?];
            values.push(self.list("authorization", auth));
        }
        self.optional_clause(&mut values, "elements", elements);

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_drop(&mut self, stmt: &pg::DropStmt) -> Result<Sexp> {
        let behavior = self.drop_behavior(&stmt.behavior)?;

        if stmt.objects.is_empty() {
            return self.unsupported("DROP object");
        }

        if stmt.concurrent
            && pg::ObjectType::try_from(stmt.remove_type) != Ok(pg::ObjectType::ObjectIndex)
        {
            return self.unsupported("DROP concurrency");
        }
        if pg::ObjectType::try_from(stmt.remove_type) == Ok(pg::ObjectType::ObjectPublication) {
            let mut values = vec![self.symbol("drop-publication")];

            if stmt.missing_ok {
                values.push(Sexp::list([self.symbol("if-exists")]));
            }
            for node in &stmt.objects {
                match self.required_kind(node.node.as_ref(), "publication name")? {
                    PgNode::String(name) => values.push(Sexp::string(name.sval.clone())),
                    _ => return self.unsupported("publication name"),
                }
            }
            if let Some(behavior) = behavior {
                values.push(self.symbol(behavior));
            }
            return Ok(Sexp::list(values));
        }
        let dropped_kind = match pg::ObjectType::try_from(stmt.remove_type) {
            Ok(pg::ObjectType::ObjectPolicy) => Some("policy"),
            Ok(pg::ObjectType::ObjectRule) => Some("rule"),
            Ok(pg::ObjectType::ObjectTrigger) => Some("trigger"),
            _ => None,
        };
        if let Some(dropped_kind) = dropped_kind {
            let mut values = vec![self.symbol(&format!("drop-{dropped_kind}"))];

            if stmt.missing_ok {
                values.push(Sexp::list([self.symbol("if-exists")]));
            }
            for node in &stmt.objects {
                let names = match self
                    .required_kind(node.node.as_ref(), "dropped policy/rule/trigger name")?
                {
                    PgNode::List(names) => &names.items,
                    _ => return self.unsupported("dropped policy/rule/trigger name"),
                };
                let (name, table_path) = match names.as_slice() {
                    [table_path @ .., last] if !table_path.is_empty() => (last, table_path),
                    _ => return self.unsupported("dropped policy/rule/trigger name"),
                };
                values.push(Sexp::string(self.string_node(name)?));

                let partition_table_path = [Sexp::list(
                    table_path
                        .iter()
                        .map(|x| self.string_node(x))
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .map(Sexp::string),
                )];
                let on_clause = [self.list("table", partition_table_path)];
                values.push(self.list("on", on_clause));
            }
            if let Some(behavior) = behavior {
                values.push(self.symbol(behavior));
            }
            return Ok(Sexp::list(values));
        }
        let dropped_kind = match pg::ObjectType::try_from(stmt.remove_type) {
            Ok(pg::ObjectType::ObjectOpclass) => Some("operator-class"),
            Ok(pg::ObjectType::ObjectOpfamily) => Some("operator-family"),
            _ => None,
        };
        if let Some(dropped_kind) = dropped_kind {
            let mut values = vec![self.symbol(&format!("drop-{dropped_kind}"))];

            if stmt.missing_ok {
                values.push(Sexp::list([self.symbol("if-exists")]));
            }
            for node in &stmt.objects {
                let names =
                    match self.required_kind(node.node.as_ref(), "dropped operator class name")? {
                        PgNode::List(names) => &names.items,
                        _ => return self.unsupported("dropped operator class name"),
                    };
                let (method, name) = match names.split_first() {
                    Some((method, name)) if !name.is_empty() => (method, name),
                    _ => return self.unsupported("dropped operator class name"),
                };
                values.push(self.lower_qualified_name(name)?);

                let access_method = [Sexp::string(self.string_node(method)?)];
                values.push(self.list("using", access_method));
            }
            if let Some(behavior) = behavior {
                values.push(self.symbol(behavior));
            }
            return Ok(Sexp::list(values));
        }
        if pg::ObjectType::try_from(stmt.remove_type) == Ok(pg::ObjectType::ObjectCast) {
            let [node] = stmt.objects.as_slice() else {
                return self.unsupported("dropped cast name");
            };
            let names = match self.required_kind(node.node.as_ref(), "dropped cast name")? {
                PgNode::List(names) => names,
                _ => return self.unsupported("dropped cast name"),
            };
            let (source, target) = self.lower_cast_names(names, "DROP CAST")?;
            let mut values = vec![self.symbol("drop-cast")];

            if stmt.missing_ok {
                values.push(Sexp::list([self.symbol("if-exists")]));
            }

            values.push(self.list("source", [source]));
            values.push(self.list("target", [target]));
            if let Some(behavior) = behavior {
                values.push(self.symbol(behavior));
            }
            return Ok(Sexp::list(values));
        }
        if pg::ObjectType::try_from(stmt.remove_type) == Ok(pg::ObjectType::ObjectTransform) {
            let [node] = stmt.objects.as_slice() else {
                return self.unsupported("dropped transform name");
            };
            let mut values = vec![self.symbol("drop-transform")];

            if stmt.missing_ok {
                values.push(Sexp::list([self.symbol("if-exists")]));
            }

            values.extend(self.lower_transform_name(node, "DROP TRANSFORM")?);
            if let Some(behavior) = behavior {
                values.push(self.symbol(behavior));
            }
            return Ok(Sexp::list(values));
        }

        let object_kind = match pg::ObjectType::try_from(stmt.remove_type) {
            Ok(pg::ObjectType::ObjectSchema) => "schema",
            Ok(pg::ObjectType::ObjectTable) => "table",
            Ok(pg::ObjectType::ObjectIndex) => "index",
            Ok(pg::ObjectType::ObjectView) => "view",
            Ok(pg::ObjectType::ObjectMatview) => "materialized-view",
            Ok(pg::ObjectType::ObjectSequence) => "sequence",
            Ok(pg::ObjectType::ObjectDomain) => "domain",
            Ok(pg::ObjectType::ObjectType) => "type",
            Ok(pg::ObjectType::ObjectExtension) => "extension",
            Ok(pg::ObjectType::ObjectFunction) => "function",
            Ok(pg::ObjectType::ObjectProcedure) => "procedure",
            Ok(pg::ObjectType::ObjectRoutine) => "routine",
            Ok(pg::ObjectType::ObjectAggregate) => "aggregate",
            Ok(pg::ObjectType::ObjectOperator) => "operator",
            Ok(pg::ObjectType::ObjectCollation) => "collation",
            Ok(pg::ObjectType::ObjectConversion) => "conversion",
            Ok(pg::ObjectType::ObjectStatisticExt) => "statistics",
            Ok(pg::ObjectType::ObjectTsconfiguration) => "text-search-configuration",
            Ok(pg::ObjectType::ObjectTsdictionary) => "text-search-dictionary",
            Ok(pg::ObjectType::ObjectTsparser) => "text-search-parser",
            Ok(pg::ObjectType::ObjectTstemplate) => "text-search-template",
            Ok(pg::ObjectType::ObjectForeignTable) => "foreign-table",
            Ok(pg::ObjectType::ObjectAccessMethod) => "access-method",
            Ok(pg::ObjectType::ObjectEventTrigger) => "event-trigger",
            Ok(pg::ObjectType::ObjectFdw) => "foreign-data-wrapper",
            Ok(pg::ObjectType::ObjectForeignServer) => "server",
            Ok(pg::ObjectType::ObjectLanguage) => "language",
            _ => return self.unsupported("DROP object type"),
        };
        let mut values = vec![self.symbol(&format!("drop-{object_kind}"))];

        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        values.extend(
            stmt.objects
                .iter()
                .map(|x| self.lower_object_name(x))
                .collect::<Result<Vec<_>>>()?,
        );
        if stmt.concurrent {
            values.push(self.symbol("concurrently"));
        }
        if let Some(behavior) = behavior {
            values.push(self.symbol(behavior));
        }

        Ok(Sexp::list(values))
    }

    fn drop_behavior(&mut self, behavior: &i32) -> Result<Option<&'static str>> {
        match pg::DropBehavior::try_from(*behavior) {
            Ok(pg::DropBehavior::DropRestrict) => Ok(None),
            Ok(pg::DropBehavior::DropCascade) => Ok(Some("cascade")),
            _ => self.unsupported_error("DROP behavior"),
        }
    }

    fn lower_object_name(&mut self, node: &pg::Node) -> Result<Sexp> {
        match self.required_kind(node.node.as_ref(), "DDL object name")? {
            PgNode::List(names) => self.lower_qualified_name(&names.items),
            PgNode::RangeVar(relation) => self.lower_relation(relation),
            PgNode::String(name) => Ok(self.list("name", [Sexp::string(name.sval.clone())])),
            PgNode::ObjectWithArgs(object) => self.lower_object_with_args(object),
            PgNode::TypeName(type_name) => self.lower_qualified_name(&type_name.names),
            _ => self.unsupported("DDL object name"),
        }
    }

    pub(super) fn lower_create_index(&mut self, stmt: &pg::IndexStmt) -> Result<Sexp> {
        if !stmt.exclude_op_names.is_empty()
            || !stmt.idxcomment.is_empty()
            || stmt.primary
            || stmt.isconstraint
            || stmt.deferrable
            || stmt.initdeferred
            || stmt.reset_default_tblspc
        {
            return self.unsupported("CREATE INDEX options");
        }

        let mut values = vec![self.symbol("create-index")];

        let index_relation = self.required(stmt.relation.as_ref(), "CREATE INDEX relation")?;
        let on = [self.lower_relation(index_relation)?];
        if stmt.unique {
            values.push(self.symbol("unique"));
        }
        if stmt.nulls_not_distinct {
            values.push(self.symbol("nulls-not-distinct"));
        }
        if stmt.concurrent {
            values.push(self.symbol("concurrently"));
        }
        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }
        if !stmt.idxname.is_empty() {
            values.push(self.list("name", [Sexp::string(stmt.idxname.clone())]));
        }

        values.push(self.list("on", on));

        let columns = self.lower_nodes(&stmt.index_params)?;
        if !stmt.access_method.is_empty() && stmt.access_method != "btree" {
            values.push(self.list("using", [Sexp::string(stmt.access_method.clone())]));
        }

        values.push(self.list("columns", columns));

        let include = self.lower_nodes(&stmt.index_including_params)?;
        self.optional_clause(&mut values, "include", include);
        let options = self.lower_nodes(&stmt.options)?;
        self.optional_clause(&mut values, "options", options);
        if !stmt.table_space.is_empty() {
            values.push(self.list("tablespace", [Sexp::string(stmt.table_space.clone())]));
        }
        self.optional_node_clause(&mut values, "where", stmt.where_clause.as_deref())?;

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_create_view(&mut self, stmt: &pg::ViewStmt) -> Result<Sexp> {
        let check_option = match pg::ViewCheckOption::try_from(stmt.with_check_option) {
            Ok(pg::ViewCheckOption::NoCheckOption) => None,
            Ok(pg::ViewCheckOption::LocalCheckOption) => Some("local"),
            Ok(pg::ViewCheckOption::CascadedCheckOption) => Some("cascaded"),
            Ok(pg::ViewCheckOption::Undefined) | Err(_) => {
                return self.unsupported("CREATE VIEW options");
            }
        };
        let recursive = recursive_view_query(stmt);
        let mut values = vec![self.symbol("create-view")];

        if stmt.replace {
            values.push(self.symbol("or-replace"));
        }
        if recursive.is_some() {
            values.push(self.symbol("recursive"));
        }

        let relation = self.required(stmt.view.as_ref(), "CREATE VIEW relation")?;
        match relation.relpersistence.as_str() {
            "p" | "" => {}
            "t" => {
                let temporary_persistence = [self.symbol("temporary")];
                values.push(self.list("persistence", temporary_persistence))
            }
            "u" => {
                let unlogged_persistence = [self.symbol("unlogged")];
                values.push(self.list("persistence", unlogged_persistence))
            }
            persistence => return self.unsupported(format!("view persistence {persistence}")),
        }

        values.push(self.lower_relation(relation)?);

        let columns = self.lower_nodes(&stmt.aliases)?;
        self.optional_clause(&mut values, "columns", columns);
        let query = match recursive {
            Some(query) => self.lower(query)?,
            None => {
                let view_query_node =
                    self.required_node(stmt.query.as_deref(), "CREATE VIEW query")?;
                self.lower(view_query_node)?
            }
        };
        values.push(self.list("query", [query]));

        let options = self.lower_view_options(&stmt.options)?;
        self.optional_clause(&mut values, "options", options);
        if let Some(option) = check_option {
            let check_option = [self.symbol(option)];
            values.push(self.list("check-option", check_option));
        }

        Ok(Sexp::list(values))
    }

    fn lower_view_options(&mut self, options: &[pg::Node]) -> Result<Vec<Sexp>> {
        options
            .iter()
            .map(
                |node| match self.required_kind(node.node.as_ref(), "view option")? {
                    PgNode::DefElem(option) => match option.defname.as_str() {
                        "check_option" => {
                            let arg = self
                                .required_node(option.arg.as_deref(), "view check option value")?;
                            let PgNode::TypeName(typename) =
                                self.required_kind(arg.node.as_ref(), "view check option value")?
                            else {
                                return self.unsupported("view check option value");
                            };
                            let value = typename
                                .names
                                .iter()
                                .map(|x| self.string_node(x))
                                .collect::<Result<Vec<_>>>()?
                                .join(".");
                            Ok(self.list(
                                "option",
                                [Sexp::string("check_option"), Sexp::string(value)],
                            ))
                        }
                        _ => self.lower_definition_option(option),
                    },
                    _ => self.unsupported("view option"),
                },
            )
            .collect()
    }

    pub(super) fn lower_create_materialized_view(
        &mut self,
        stmt: &pg::CreateTableAsStmt,
    ) -> Result<Sexp> {
        let into = self.required(stmt.into.as_ref(), "CREATE MATERIALIZED VIEW target")?;
        if into.view_query.is_some()
            || !matches!(
                pg::OnCommitAction::try_from(into.on_commit),
                Ok(pg::OnCommitAction::OncommitNoop)
            )
        {
            return self.unsupported("CREATE MATERIALIZED VIEW options");
        }

        let mut values = vec![self.symbol("create-materialized-view")];

        let view_relation = self.required(into.rel.as_ref(), "materialized view relation")?;
        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }

        values.push(self.lower_relation(view_relation)?);

        let columns = self.lower_nodes(&into.col_names)?;
        self.optional_clause(&mut values, "columns", columns);
        let options = self.lower_nodes(&into.options)?;
        if !into.access_method.is_empty() {
            values.push(self.list("using", [Sexp::string(into.access_method.clone())]));
        }
        self.optional_clause(&mut values, "options", options);
        let view_query =
            self.required_node(stmt.query.as_deref(), "CREATE MATERIALIZED VIEW query")?;
        let query = [self.lower(view_query)?];
        if !into.table_space_name.is_empty() {
            values.push(self.list("tablespace", [Sexp::string(into.table_space_name.clone())]));
        }

        values.push(self.list("query", query));
        if into.skip_data {
            values.push(self.symbol("with-no-data"));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_create_table_as(&mut self, stmt: &pg::CreateTableAsStmt) -> Result<Sexp> {
        let into = self.required(stmt.into.as_ref(), "CREATE TABLE AS target")?;
        let relation = self.required(into.rel.as_ref(), "CREATE TABLE AS relation")?;
        let mut values = vec![self.symbol("create-table-as")];

        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }
        match relation.relpersistence.as_str() {
            "p" | "" => {}
            "t" => {
                let temporary_persistence = [self.symbol("temporary")];
                values.push(self.list("persistence", temporary_persistence))
            }
            "u" => {
                let unlogged_persistence = [self.symbol("unlogged")];
                values.push(self.list("persistence", unlogged_persistence))
            }
            persistence => return self.unsupported(format!("table persistence {persistence}")),
        }

        values.push(self.lower_relation(relation)?);

        let columns = self.lower_nodes(&into.col_names)?;
        self.optional_clause(&mut values, "columns", columns);
        let options = self.lower_nodes(&into.options)?;
        self.optional_clause(&mut values, "options", options);
        let as_query_node = self.required_node(stmt.query.as_deref(), "CREATE TABLE AS query")?;
        let query = [self.lower(as_query_node)?];
        if !into.access_method.is_empty() {
            values.push(self.list("using", [Sexp::string(into.access_method.clone())]));
        }
        match pg::OnCommitAction::try_from(into.on_commit) {
            Ok(pg::OnCommitAction::OncommitNoop) => {}
            Ok(pg::OnCommitAction::OncommitPreserveRows) => {
                let preserve_rows_on_commit = [self.symbol("preserve-rows")];
                values.push(self.list("on-commit", preserve_rows_on_commit));
            }
            Ok(pg::OnCommitAction::OncommitDeleteRows) => {
                let delete_rows_on_commit = [self.symbol("delete-rows")];
                values.push(self.list("on-commit", delete_rows_on_commit));
            }
            Ok(pg::OnCommitAction::OncommitDrop) => {
                let drop_on_commit = [self.symbol("drop")];
                values.push(self.list("on-commit", drop_on_commit));
            }
            _ => return self.unsupported("CREATE TABLE AS on commit action"),
        }
        if !into.table_space_name.is_empty() {
            values.push(self.list("tablespace", [Sexp::string(into.table_space_name.clone())]));
        }

        if into.view_query.is_some() {
            return self.unsupported("CREATE TABLE AS options");
        }

        values.push(self.list("query", query));
        if into.skip_data {
            values.push(self.symbol("with-no-data"));
        }

        Ok(Sexp::list(values))
    }

    fn lower_sequence_options(&mut self, options: &[pg::Node]) -> Result<Vec<Sexp>> {
        options
            .iter()
            .map(
                |node| match self.required_kind(node.node.as_ref(), "sequence option")? {
                    PgNode::DefElem(option) if option.defname == "as" => {
                        let arg = self.required_node(option.arg.as_deref(), "sequence type")?;
                        let PgNode::TypeName(typename) =
                            self.required_kind(arg.node.as_ref(), "sequence type")?
                        else {
                            return self.unsupported("sequence type");
                        };
                        let opt = [Sexp::string("as"), self.lower_type(typename)?];
                        Ok(self.list("option", opt))
                    }
                    PgNode::DefElem(option) => self.lower_definition_option(option),
                    _ => self.unsupported("sequence option"),
                },
            )
            .collect()
    }

    pub(super) fn lower_create_sequence(&mut self, stmt: &pg::CreateSeqStmt) -> Result<Sexp> {
        let sequence_relation =
            self.required(stmt.sequence.as_ref(), "CREATE SEQUENCE relation")?;
        if stmt.owner_id != 0 || stmt.for_identity {
            return self.unsupported("CREATE SEQUENCE identity options");
        }

        let mut values = vec![
            self.symbol("create-sequence"),
            self.lower_relation(sequence_relation)?,
        ];

        let options = self.lower_sequence_options(&stmt.options)?;
        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alter_sequence(&mut self, stmt: &pg::AlterSeqStmt) -> Result<Sexp> {
        let sequence_relation = self.required(stmt.sequence.as_ref(), "ALTER SEQUENCE relation")?;

        if stmt.for_identity {
            return self.unsupported("ALTER SEQUENCE identity options");
        }

        let mut values = vec![
            self.symbol("alter-sequence"),
            self.lower_relation(sequence_relation)?,
        ];

        let options = self.lower_sequence_options(&stmt.options)?;
        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }
        self.optional_clause(&mut values, "options", options);

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_create_domain(&mut self, stmt: &pg::CreateDomainStmt) -> Result<Sexp> {
        let domain_type = self.required(stmt.type_name.as_ref(), "domain type")?;
        let mut values = vec![
            self.symbol("create-domain"),
            self.lower_qualified_name(&stmt.domainname)?,
            self.lower_type(domain_type)?,
        ];

        let constraints = self.lower_nodes(&stmt.constraints)?;
        if let Some(clause) = stmt.coll_clause.as_ref() {
            values.push(self.lower_collation_clause(clause, "CREATE DOMAIN collation")?);
        }

        values.push(self.list("constraints", constraints));

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_collation_clause(
        &mut self,
        clause: &pg::CollateClause,
        context: &str,
    ) -> Result<Sexp> {
        if clause.arg.is_some() {
            return self.unsupported_error(context);
        }

        let name = clause
            .collname
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?
            .join(".");

        Ok(self.list("collate", [Sexp::string(name)]))
    }

    pub(super) fn lower_create_composite_type(
        &mut self,
        stmt: &pg::CompositeTypeStmt,
    ) -> Result<Sexp> {
        let attributes = self.lower_nodes(&stmt.coldeflist)?;
        let type_name = self.required(stmt.typevar.as_ref(), "composite type name")?;
        let create_type = [
            self.lower_type_relation_name(type_name)?,
            self.list("attributes", attributes),
        ];

        Ok(self.list("create-type", create_type))
    }

    pub(super) fn lower_create_enum(&mut self, stmt: &pg::CreateEnumStmt) -> Result<Sexp> {
        let values = self.lower_nodes(&stmt.vals)?;
        let create_enum = [
            self.lower_qualified_name(&stmt.type_name)?,
            self.list("values", values),
        ];
        Ok(self.list("create-enum", create_enum))
    }

    pub(super) fn lower_alter_enum(&mut self, stmt: &pg::AlterEnumStmt) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("alter-enum"),
            self.lower_qualified_name(&stmt.type_name)?,
        ];

        if !stmt.old_val.is_empty() {
            if !stmt.new_val_neighbor.is_empty() {
                return self.unsupported("ALTER TYPE enum rename");
            }

            values.push(self.list(
                "rename-value",
                [
                    Sexp::string(stmt.old_val.clone()),
                    Sexp::string(stmt.new_val.clone()),
                ],
            ));
        } else {
            let mut add_value = vec![self.symbol("add-value"), Sexp::string(stmt.new_val.clone())];

            if !stmt.new_val_neighbor.is_empty() {
                add_value.push(self.list(
                    if stmt.new_val_is_after {
                        "after"
                    } else {
                        "before"
                    },
                    [Sexp::string(stmt.new_val_neighbor.clone())],
                ));
            }
            if stmt.skip_if_new_val_exists {
                add_value.push(Sexp::list([self.symbol("if-not-exists")]));
            }

            values.push(Sexp::list(add_value));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alter_domain(&mut self, stmt: &pg::AlterDomainStmt) -> Result<Sexp> {
        let type_name = self.lower_qualified_name(&stmt.type_name)?;
        let values = match stmt.subtype.as_str() {
            "T" => match stmt.def.as_deref() {
                Some(default) => {
                    let set_default = [self.lower(default)?];
                    vec![
                        self.symbol("alter-domain"),
                        type_name,
                        self.list("set-default", set_default),
                    ]
                }
                None => vec![
                    self.symbol("alter-domain"),
                    type_name,
                    Sexp::list([self.symbol("drop-default")]),
                ],
            },
            "N" if stmt.def.is_none() => vec![
                self.symbol("alter-domain"),
                type_name,
                Sexp::list([self.symbol("drop-not-null")]),
            ],
            "O" if stmt.def.is_none() => vec![
                self.symbol("alter-domain"),
                type_name,
                Sexp::list([self.symbol("set-not-null")]),
            ],
            "C" => {
                let constraint_node =
                    self.required_node(stmt.def.as_deref(), "domain constraint")?;
                let add_constraint = [self.lower(constraint_node)?];
                vec![
                    self.symbol("alter-domain"),
                    type_name,
                    self.list("add-constraint", add_constraint),
                ]
            }
            "X" if stmt.def.is_none() => {
                let behavior = match pg::DropBehavior::try_from(stmt.behavior) {
                    Ok(pg::DropBehavior::DropRestrict) => None,
                    Ok(pg::DropBehavior::DropCascade) => Some("cascade"),
                    _ => return self.unsupported("ALTER DOMAIN drop behavior"),
                };
                let mut action = vec![
                    self.symbol("drop-constraint"),
                    Sexp::string(stmt.name.clone()),
                ];

                if stmt.missing_ok {
                    action.push(self.symbol("if-exists"));
                }
                if let Some(behavior) = behavior {
                    action.push(self.symbol(behavior));
                }
                vec![self.symbol("alter-domain"), type_name, Sexp::list(action)]
            }
            "V" if stmt.def.is_none() && !stmt.name.is_empty() => vec![
                self.symbol("alter-domain"),
                type_name,
                self.list("validate-constraint", [Sexp::string(stmt.name.clone())]),
            ],
            _ => return self.unsupported("ALTER DOMAIN action"),
        };

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alter_table(&mut self, stmt: &pg::AlterTableStmt) -> Result<Sexp> {
        let object_type = pg::ObjectType::try_from(stmt.objtype)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: ALTER object type", 0))?;
        let table_relation = self.required(stmt.relation.as_ref(), "ALTER TABLE relation")?;
        let alter_statement_name = format!("alter-{}", self.object_type_name(object_type)?);
        let mut values = vec![
            self.symbol(&alter_statement_name),
            self.lower_relation(table_relation)?,
        ];

        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        values.extend(
            stmt.cmds
                .iter()
                .map(|x| self.lower_alter_command(x))
                .collect::<Result<Vec<_>>>()?,
        );

        Ok(Sexp::list(values))
    }

    fn lower_alter_command(&mut self, node: &pg::Node) -> Result<Sexp> {
        let PgNode::AlterTableCmd(command) =
            self.required_kind(node.node.as_ref(), "ALTER TABLE command")?
        else {
            return self.unsupported("ALTER TABLE command");
        };
        let action = pg::AlterTableType::try_from(command.subtype)
            .map_err(|_| ParseError::new("unsupported ALTER TABLE action", 0))?;
        match action {
            pg::AlterTableType::AtAddColumn => {
                self.validate_alter_command_with(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Required,
                    AlterCommandMissingOk::Optional,
                    AlterCommandNumber::Forbidden,
                    AlterCommandBehavior::CascadeAllowed,
                )?;
                let column_definition_node =
                    self.required_node(command.def.as_deref(), "column definition")?;
                let mut values = vec![self.lower(column_definition_node)?];

                if command.missing_ok {
                    values.push(Sexp::list([self.symbol("if-not-exists")]));
                }
                if matches!(
                    pg::DropBehavior::try_from(command.behavior),
                    Ok(pg::DropBehavior::DropCascade)
                ) {
                    values.push(self.symbol("cascade"));
                }
                Ok(self.list("add-column", values))
            }
            pg::AlterTableType::AtDropColumn => {
                self.lower_drop_alter_action("drop-column", command)
            }
            pg::AlterTableType::AtAlterColumnType => {
                self.validate_alter_command_with(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Required,
                    AlterCommandMissingOk::Forbidden,
                    AlterCommandNumber::Forbidden,
                    AlterCommandBehavior::CascadeAllowed,
                )?;
                let column_type_node = self
                    .required_node(command.def.as_deref(), "new column type")?
                    .node
                    .as_ref();
                let PgNode::ColumnDef(column) =
                    self.required_kind(column_type_node, "new column type")?
                else {
                    return self.unsupported("new column type");
                };
                let column_type = self.required(column.type_name.as_ref(), "new column type")?;
                let mut values = vec![
                    Sexp::string(command.name.clone()),
                    self.lower_type(column_type)?,
                ];

                self.push_column_storage_clauses(&mut values, column)?;
                if let Some(using) = column.raw_default.as_deref() {
                    let lowered_using = [self.lower(using)?];
                    values.push(self.list("using", lowered_using));
                }
                if matches!(
                    pg::DropBehavior::try_from(command.behavior),
                    Ok(pg::DropBehavior::DropCascade)
                ) {
                    values.push(self.symbol("cascade"));
                }
                Ok(self.list("alter-column-type", values))
            }
            pg::AlterTableType::AtColumnDefault => {
                let default_values = match command.def.as_deref() {
                    Some(default) => vec![Sexp::string(command.name.clone()), self.lower(default)?],
                    None => vec![Sexp::string(command.name.clone())],
                };
                Ok(self.list(
                    if command.def.is_some() {
                        "set-default"
                    } else {
                        "drop-default"
                    },
                    default_values,
                ))
            }
            pg::AlterTableType::AtDropNotNull => {
                Ok(self.list("drop-not-null", [Sexp::string(command.name.clone())]))
            }
            pg::AlterTableType::AtSetNotNull => {
                Ok(self.list("set-not-null", [Sexp::string(command.name.clone())]))
            }
            pg::AlterTableType::AtAddConstraint => {
                let constraint_node = self.required_node(command.def.as_deref(), "constraint")?;
                let add_constraint = [self.lower(constraint_node)?];
                Ok(self.list("add-constraint", add_constraint))
            }
            pg::AlterTableType::AtDropConstraint => {
                self.lower_drop_alter_action("drop-constraint", command)
            }
            pg::AlterTableType::AtSetLogged => Ok(Sexp::list([self.symbol("set-logged")])),
            pg::AlterTableType::AtSetUnLogged => Ok(Sexp::list([self.symbol("set-unlogged")])),
            pg::AlterTableType::AtEnableRowSecurity => {
                Ok(Sexp::list([self.symbol("enable-row-level-security")]))
            }
            pg::AlterTableType::AtDisableRowSecurity => {
                Ok(Sexp::list([self.symbol("disable-row-level-security")]))
            }
            pg::AlterTableType::AtForceRowSecurity => {
                Ok(Sexp::list([self.symbol("force-row-level-security")]))
            }
            pg::AlterTableType::AtNoForceRowSecurity => {
                Ok(Sexp::list([self.symbol("no-force-row-level-security")]))
            }
            pg::AlterTableType::AtChangeOwner => {
                let role = self.required(command.newowner.as_ref(), "ALTER OWNER role")?;
                let owner = self.lower_role_spec(role)?;
                Ok(self.list("owner", [owner]))
            }
            pg::AlterTableType::AtSetTableSpace => {
                Ok(self.list("set-tablespace", [Sexp::string(command.name.clone())]))
            }
            pg::AlterTableType::AtSetRelOptions => {
                let set = self.lower_alter_options(command)?;
                Ok(self.list("set", set))
            }
            pg::AlterTableType::AtResetRelOptions => {
                let reset = self.lower_alter_options(command)?;
                Ok(self.list("reset", reset))
            }
            pg::AlterTableType::AtAttachPartition
            | pg::AlterTableType::AtDetachPartition
            | pg::AlterTableType::AtDetachPartitionFinalize => {
                let command_node = self
                    .required_node(command.def.as_deref(), "partition command")?
                    .node
                    .as_ref();
                let partition = match self.required_kind(command_node, "partition command")? {
                    PgNode::PartitionCmd(partition) => partition,
                    _ => return self.unsupported("ALTER TABLE partition command"),
                };
                self.lower_partition_command(
                    partition,
                    action == pg::AlterTableType::AtDetachPartitionFinalize,
                )
            }
            pg::AlterTableType::AtSetStatistics => {
                let numbered = command.num != 0;
                self.validate_alter_command_with(
                    command,
                    if numbered {
                        AlterCommandName::Forbidden
                    } else {
                        AlterCommandName::Required
                    },
                    AlterCommandDef::Optional,
                    AlterCommandMissingOk::Forbidden,
                    AlterCommandNumber::Optional,
                    AlterCommandBehavior::RestrictOnly,
                )?;
                let column = if numbered {
                    self.symbol(&command.num.to_string())
                } else {
                    Sexp::string(command.name.clone())
                };
                let default_set_statistics = [
                    column,
                    match command.def.as_deref() {
                        Some(value) => self.lower(value)?,
                        None => self.symbol("default"),
                    },
                ];
                Ok(self.list("set-statistics", default_set_statistics))
            }
            pg::AlterTableType::AtSetStorage => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Required,
                )?;
                let storage_node = self.required_node(command.def.as_deref(), "column storage")?;
                let storage_mode = self.string_node(storage_node)?;
                let set_storage_string = [
                    Sexp::string(command.name.clone()),
                    self.symbol(&storage_mode),
                ];
                Ok(self.list("set-storage", set_storage_string))
            }
            pg::AlterTableType::AtSetCompression => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Required,
                )?;
                let compression_node =
                    self.required_node(command.def.as_deref(), "column compression")?;
                let compression_method = self.string_node(compression_node)?;
                let set_compression_string = [
                    Sexp::string(command.name.clone()),
                    self.symbol(&compression_method),
                ];
                Ok(self.list("set-compression", set_compression_string))
            }
            pg::AlterTableType::AtSetOptions | pg::AlterTableType::AtResetOptions => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Required,
                )?;
                let mut options = vec![Sexp::string(command.name.clone())];

                options.extend(self.lower_alter_options(command)?);
                Ok(self.list(
                    if action == pg::AlterTableType::AtSetOptions {
                        "set-options"
                    } else {
                        "reset-options"
                    },
                    options,
                ))
            }
            pg::AlterTableType::AtAlterColumnGenericOptions => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Required,
                )?;
                let options = self.lower_command_options(command, "column options")?;
                let mut values = vec![Sexp::string(command.name.clone())];

                values.extend(self.lower_generic_options(options, "column options")?);
                Ok(self.list("alter-column-options", values))
            }
            pg::AlterTableType::AtDropIdentity => {
                self.lower_drop_alter_action("drop-identity", command)
            }
            pg::AlterTableType::AtSetExpression => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Required,
                )?;
                let column_expression_node =
                    self.required_node(command.def.as_deref(), "column expression")?;
                let lowered_set_expression = [
                    Sexp::string(command.name.clone()),
                    self.lower(column_expression_node)?,
                ];
                Ok(self.list("set-expression", lowered_set_expression))
            }
            pg::AlterTableType::AtDropExpression => {
                self.lower_drop_alter_action("drop-expression", command)
            }
            pg::AlterTableType::AtAddIdentity => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Required,
                )?;
                let constraint = self.lower_constraint_node(command, "identity constraint")?;
                let mut values = vec![Sexp::string(command.name.clone())];

                values.extend(self.lower_identity_clause(constraint)?);
                Ok(self.list("add-identity", values))
            }
            pg::AlterTableType::AtSetIdentity => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Required,
                )?;
                let options = self.lower_command_options(command, "identity options")?;
                let mut values = vec![Sexp::string(command.name.clone())];

                values.extend(self.lower_identity_options(options)?);
                Ok(self.list("set-identity", values))
            }
            pg::AlterTableType::AtValidateConstraint => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Forbidden,
                )?;
                Ok(self.list("validate-constraint", [Sexp::string(command.name.clone())]))
            }
            pg::AlterTableType::AtReplicaIdentity => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Required,
                )?;
                let identity_node = self
                    .required_node(command.def.as_deref(), "replica identity")?
                    .node
                    .as_ref();
                let identity = match self.required_kind(identity_node, "replica identity")? {
                    PgNode::ReplicaIdentityStmt(identity) => identity,
                    _ => return self.unsupported("replica identity"),
                };
                self.lower_replica_identity(identity)
            }
            pg::AlterTableType::AtAddInherit | pg::AlterTableType::AtDropInherit => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Required,
                )?;
                let table_node = self
                    .required_node(command.def.as_deref(), "parent table")?
                    .node
                    .as_ref();
                let relation = match self.required_kind(table_node, "parent table")? {
                    PgNode::RangeVar(relation) => relation,
                    _ => return self.unsupported("parent table"),
                };
                let lowered_relation = [self.lower_relation(relation)?];
                Ok(self.list(
                    if action == pg::AlterTableType::AtAddInherit {
                        "inherit"
                    } else {
                        "no-inherit"
                    },
                    lowered_relation,
                ))
            }
            pg::AlterTableType::AtAddOf => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Required,
                )?;
                let composite_type_node =
                    self.required_node(command.def.as_deref(), "composite type")?;
                let lowered_of = [self.lower_type_node(composite_type_node)?];
                Ok(self.list("of", lowered_of))
            }
            pg::AlterTableType::AtDropOf => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Forbidden,
                )?;
                Ok(Sexp::list([self.symbol("not-of")]))
            }
            pg::AlterTableType::AtSetAccessMethod => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Optional,
                    AlterCommandDef::Forbidden,
                )?;
                let default_set_access_method = [if command.name.is_empty() {
                    self.symbol("default")
                } else {
                    Sexp::string(command.name.clone())
                }];
                Ok(self.list("set-access-method", default_set_access_method))
            }
            pg::AlterTableType::AtClusterOn => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Required,
                    AlterCommandDef::Forbidden,
                )?;
                Ok(self.list("cluster-on", [Sexp::string(command.name.clone())]))
            }
            pg::AlterTableType::AtDropCluster => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Forbidden,
                )?;
                Ok(Sexp::list([self.symbol("set-without-cluster")]))
            }
            pg::AlterTableType::AtDropOids => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Forbidden,
                )?;
                Ok(Sexp::list([self.symbol("set-without-oids")]))
            }
            pg::AlterTableType::AtGenericOptions => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Required,
                )?;
                let options = self.lower_command_options(command, "table options")?;
                let lowered_options = self.lower_generic_options(options, "table options")?;
                Ok(self.list("options", lowered_options))
            }
            pg::AlterTableType::AtAlterConstraint => {
                self.validate_alter_command(
                    command,
                    AlterCommandName::Forbidden,
                    AlterCommandDef::Required,
                )?;
                let constraint_node = self.required_node(command.def.as_deref(), "constraint")?;
                let PgNode::AtalterConstraint(constraint) =
                    self.required_kind(constraint_node.node.as_ref(), "constraint")?
                else {
                    return self.unsupported_error("constraint");
                };
                self.lower_alter_constraint(constraint)
            }
            pg::AlterTableType::AtEnableTrig => {
                self.lower_alter_toggle(command, "enable-trigger", true)
            }
            pg::AlterTableType::AtEnableAlwaysTrig => {
                self.lower_alter_toggle(command, "enable-always-trigger", true)
            }
            pg::AlterTableType::AtEnableReplicaTrig => {
                self.lower_alter_toggle(command, "enable-replica-trigger", true)
            }
            pg::AlterTableType::AtDisableTrig => {
                self.lower_alter_toggle(command, "disable-trigger", true)
            }
            pg::AlterTableType::AtEnableTrigAll => {
                self.lower_alter_toggle(command, "enable-trigger-all", false)
            }
            pg::AlterTableType::AtDisableTrigAll => {
                self.lower_alter_toggle(command, "disable-trigger-all", false)
            }
            pg::AlterTableType::AtEnableTrigUser => {
                self.lower_alter_toggle(command, "enable-trigger-user", false)
            }
            pg::AlterTableType::AtDisableTrigUser => {
                self.lower_alter_toggle(command, "disable-trigger-user", false)
            }
            pg::AlterTableType::AtEnableRule => {
                self.lower_alter_toggle(command, "enable-rule", true)
            }
            pg::AlterTableType::AtEnableAlwaysRule => {
                self.lower_alter_toggle(command, "enable-always-rule", true)
            }
            pg::AlterTableType::AtEnableReplicaRule => {
                self.lower_alter_toggle(command, "enable-replica-rule", true)
            }
            pg::AlterTableType::AtDisableRule => {
                self.lower_alter_toggle(command, "disable-rule", true)
            }
            _ => self.unsupported(format!("ALTER TABLE action {}", action.as_str_name())),
        }
    }

    fn validate_alter_command(
        &mut self,
        command: &pg::AlterTableCmd,
        name: AlterCommandName,
        def: AlterCommandDef,
    ) -> Result<()> {
        self.validate_alter_command_with(
            command,
            name,
            def,
            AlterCommandMissingOk::Forbidden,
            AlterCommandNumber::Forbidden,
            AlterCommandBehavior::RestrictOnly,
        )
    }

    fn validate_alter_command_with(
        &mut self,
        command: &pg::AlterTableCmd,
        name: AlterCommandName,
        def: AlterCommandDef,
        missing_ok: AlterCommandMissingOk,
        number: AlterCommandNumber,
        behavior_allowed: AlterCommandBehavior,
    ) -> Result<()> {
        if command.newowner.is_some() || command.recurse {
            return self.unsupported_error("ALTER TABLE command");
        }

        if command.num != 0 && matches!(number, AlterCommandNumber::Forbidden) {
            return self.unsupported_error("ALTER TABLE command column number");
        }

        match name {
            AlterCommandName::Required if command.name.is_empty() => {
                return self.unsupported_error("ALTER TABLE command name");
            }
            AlterCommandName::Forbidden if !command.name.is_empty() => {
                return self.unsupported_error("ALTER TABLE command name");
            }
            _ => {}
        }
        match def {
            AlterCommandDef::Required if command.def.is_none() => {
                return self.unsupported_error("ALTER TABLE command definition");
            }
            AlterCommandDef::Forbidden if command.def.is_some() => {
                return self.unsupported_error("ALTER TABLE command definition");
            }
            _ => {}
        }

        if command.missing_ok && matches!(missing_ok, AlterCommandMissingOk::Forbidden) {
            return self.unsupported_error("ALTER TABLE command IF EXISTS");
        }

        match (
            pg::DropBehavior::try_from(command.behavior),
            behavior_allowed,
        ) {
            (Ok(pg::DropBehavior::DropRestrict), _) => {}
            (Ok(pg::DropBehavior::DropCascade), AlterCommandBehavior::CascadeAllowed) => {}
            _ => return self.unsupported_error("ALTER TABLE drop behavior"),
        }

        Ok(())
    }

    fn lower_command_options<'a>(
        &mut self,
        command: &'a pg::AlterTableCmd,
        context: &str,
    ) -> Result<&'a [pg::Node]> {
        let node = self.required_node(command.def.as_deref(), context)?;
        let PgNode::List(options) = self.required_kind(node.node.as_ref(), context)? else {
            return self.unsupported_error(context);
        };
        Ok(&options.items)
    }

    fn lower_constraint_node<'a>(
        &mut self,
        command: &'a pg::AlterTableCmd,
        context: &str,
    ) -> Result<&'a pg::Constraint> {
        let constraint_node = self
            .required_node(command.def.as_deref(), context)?
            .node
            .as_ref();
        match self.required_kind(constraint_node, context)? {
            PgNode::Constraint(constraint) => Ok(constraint),
            _ => self.unsupported_error(context),
        }
    }

    fn lower_alter_toggle(
        &mut self,
        command: &pg::AlterTableCmd,
        name: &str,
        named: bool,
    ) -> Result<Sexp> {
        self.validate_alter_command(
            command,
            if named {
                AlterCommandName::Required
            } else {
                AlterCommandName::Forbidden
            },
            AlterCommandDef::Forbidden,
        )?;
        Ok(if named {
            self.list(name, [Sexp::string(command.name.clone())])
        } else {
            Sexp::list([self.symbol(name)])
        })
    }

    pub(super) fn lower_identity_constraint(
        &mut self,
        constraint: &pg::Constraint,
    ) -> Result<Vec<Sexp>> {
        let mut values = vec![self.symbol("identity")];

        values.extend(self.lower_identity_clause(constraint)?);
        Ok(values)
    }

    fn lower_identity_clause(&mut self, constraint: &pg::Constraint) -> Result<Vec<Sexp>> {
        let generation_kind =
            self.generation_kind(&constraint.generated_when, "identity constraint")?;
        if !matches!(
            pg::ConstrType::try_from(constraint.contype),
            Ok(pg::ConstrType::ConstrIdentity)
        ) || !constraint.conname.is_empty()
            || constraint.deferrable
            || constraint.initdeferred
            || constraint.skip_validation
            || constraint.is_no_inherit
            || constraint.raw_expr.is_some()
            || !constraint.cooked_expr.is_empty()
            || constraint.nulls_not_distinct
            || !constraint.keys.is_empty()
            || !constraint.including.is_empty()
            || !constraint.exclusions.is_empty()
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
            return self.unsupported_error("identity constraint");
        }

        let mut values = vec![self.symbol(generation_kind)];

        let lowered_options = self.lower_sequence_options(&constraint.options)?;
        self.optional_clause(&mut values, "options", lowered_options);

        Ok(values)
    }

    fn lower_identity_options(&mut self, options: &[pg::Node]) -> Result<Vec<Sexp>> {
        options
            .iter()
            .map(|node| {
                let PgNode::DefElem(option) =
                    self.required_kind(node.node.as_ref(), "identity option")?
                else {
                    return self.unsupported_error("identity option");
                };
                if !option.defnamespace.is_empty()
                    || !matches!(
                        pg::DefElemAction::try_from(option.defaction),
                        Ok(pg::DefElemAction::DefelemUnspec)
                    )
                {
                    return self.unsupported_error("identity option");
                }
                match option.defname.as_str() {
                    "generated" => {
                        let argument =
                            self.required_node(option.arg.as_deref(), "identity generation")?;
                        let PgNode::Integer(integer) =
                            self.required_kind(argument.node.as_ref(), "identity generation")?
                        else {
                            return self.unsupported_error("identity generation");
                        };
                        let generated = match integer.ival {
                            value if value == i32::from(b'a') => "always",
                            value if value == i32::from(b'd') => "by-default",
                            _ => return self.unsupported_error("identity generation"),
                        };
                        Ok(self.symbol(generated))
                    }
                    "restart" => {
                        let mut values = vec![self.symbol("restart")];

                        if let Some(argument) = option.arg.as_deref() {
                            values.push(self.lower(argument)?);
                        }
                        Ok(Sexp::list(values))
                    }
                    _ => self.lower_definition_option(option),
                }
            })
            .collect()
    }

    pub(super) fn generation_kind<'a>(&mut self, when: &str, context: &str) -> Result<&'a str> {
        match when {
            "a" => Ok("always"),
            "d" => Ok("by-default"),
            _ => self.unsupported_error(context),
        }
    }

    fn lower_replica_identity(&mut self, identity: &pg::ReplicaIdentityStmt) -> Result<Sexp> {
        match identity.identity_type.as_str() {
            "n" if identity.name.is_empty() => {
                let nothing_replica_identity = [self.symbol("nothing")];
                Ok(self.list("replica-identity", nothing_replica_identity))
            }
            "f" if identity.name.is_empty() => {
                let full_replica_identity = [self.symbol("full")];
                Ok(self.list("replica-identity", full_replica_identity))
            }
            "d" if identity.name.is_empty() => {
                let default_replica_identity = [self.symbol("default")];
                Ok(self.list("replica-identity", default_replica_identity))
            }
            "i" if !identity.name.is_empty() => {
                let replica_identity_string =
                    [self.list("using-index", [Sexp::string(identity.name.clone())])];
                Ok(self.list("replica-identity", replica_identity_string))
            }
            _ => self.unsupported("replica identity"),
        }
    }

    fn lower_alter_constraint(&mut self, constraint: &pg::AtAlterConstraint) -> Result<Sexp> {
        if constraint.conname.is_empty()
            || (!constraint.alter_deferrability
                && (constraint.deferrable || constraint.initdeferred))
        {
            return self.unsupported_error("ALTER CONSTRAINT");
        }

        let mut values = vec![
            self.symbol("alter-constraint"),
            Sexp::string(constraint.conname.clone()),
        ];

        if constraint.deferrable {
            values.push(self.symbol("deferrable"));
        }
        if constraint.initdeferred {
            values.push(self.symbol("initially-deferred"));
        }
        if constraint.alter_enforceability {
            values.push(self.symbol(if constraint.is_enforced {
                "enforced"
            } else {
                "not-enforced"
            }));
        }
        if constraint.alter_inheritability {
            values.push(self.symbol(if constraint.noinherit {
                "no-inherit"
            } else {
                "inherit"
            }));
        }

        Ok(Sexp::list(values))
    }

    fn lower_drop_alter_action(&mut self, name: &str, command: &pg::AlterTableCmd) -> Result<Sexp> {
        let behavior = match pg::DropBehavior::try_from(command.behavior) {
            Ok(pg::DropBehavior::DropRestrict) => None,
            Ok(pg::DropBehavior::DropCascade) => Some("cascade"),
            _ => return self.unsupported("ALTER TABLE drop behavior"),
        };
        let mut values = vec![self.symbol(name), Sexp::string(command.name.clone())];

        if command.missing_ok {
            values.push(self.symbol("if-exists"));
        }
        if let Some(behavior) = behavior {
            values.push(self.symbol(behavior));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_rename_table(&mut self, stmt: &pg::RenameStmt) -> Result<Sexp> {
        match pg::ObjectType::try_from(stmt.rename_type) {
            Ok(pg::ObjectType::ObjectTable) => {
                let table = self.required(stmt.relation.as_ref(), "renamed table")?;
                let alter_table = [
                    self.lower_relation(table)?,
                    self.list("rename-table", [Sexp::string(stmt.newname.clone())]),
                ];
                Ok(self.list("alter-table", alter_table))
            }
            Ok(pg::ObjectType::ObjectColumn) => {
                let table = self.required(stmt.relation.as_ref(), "renamed table")?;
                let alter_table = [
                    self.lower_relation(table)?,
                    self.list(
                        "rename-column",
                        [
                            Sexp::string(stmt.subname.clone()),
                            Sexp::string(stmt.newname.clone()),
                        ],
                    ),
                ];
                Ok(self.list("alter-table", alter_table))
            }
            Ok(pg::ObjectType::ObjectTabconstraint) => {
                let table = self.required(stmt.relation.as_ref(), "renamed table")?;
                let alter_table = [
                    self.lower_relation(table)?,
                    self.list(
                        "rename-constraint",
                        [
                            Sexp::string(stmt.subname.clone()),
                            Sexp::string(stmt.newname.clone()),
                        ],
                    ),
                ];
                Ok(self.list("alter-table", alter_table))
            }
            Ok(pg::ObjectType::ObjectRole) => Ok(self.list(
                "rename-role",
                [
                    Sexp::string(stmt.subname.clone()),
                    Sexp::string(stmt.newname.clone()),
                ],
            )),
            Ok(kind @ (pg::ObjectType::ObjectTrigger | pg::ObjectType::ObjectRule)) => {
                self.validate_rename_restrict(stmt.behavior)?;
                let table = self.required(stmt.relation.as_ref(), "renamed object table")?;
                let on = [self.lower_relation(table)?];
                let renamed_object_parts = [
                    Sexp::string(stmt.subname.clone()),
                    self.list("on", on),
                    self.list("rename", [Sexp::string(stmt.newname.clone())]),
                ];
                Ok(self.list(
                    match kind {
                        pg::ObjectType::ObjectTrigger => "rename-trigger",
                        _ => "rename-rule",
                    },
                    renamed_object_parts,
                ))
            }
            Ok(pg::ObjectType::ObjectDomconstraint) => {
                self.validate_rename_restrict(stmt.behavior)?;
                let object = self.required_node(stmt.object.as_deref(), "renamed domain")?;
                let names = match self.required_kind(object.node.as_ref(), "renamed domain")? {
                    PgNode::List(names) => &names.items,
                    _ => return self.unsupported_error("renamed domain"),
                };
                let alter_domain = [
                    self.lower_qualified_name(names)?,
                    self.list(
                        "rename-constraint",
                        [
                            Sexp::string(stmt.subname.clone()),
                            Sexp::string(stmt.newname.clone()),
                        ],
                    ),
                ];
                Ok(self.list("alter-domain", alter_domain))
            }
            Ok(pg::ObjectType::ObjectAttribute) => {
                let renamed_type = self.required(stmt.relation.as_ref(), "renamed type")?;
                let mut values = vec![
                    self.lower_type_relation_name(renamed_type)?,
                    self.list(
                        "rename-attribute",
                        [
                            Sexp::string(stmt.subname.clone()),
                            Sexp::string(stmt.newname.clone()),
                        ],
                    ),
                ];

                if let Some(behavior) = self.drop_behavior(&stmt.behavior)? {
                    values.push(self.symbol(behavior));
                }
                Ok(self.list("alter-type", values))
            }
            Ok(kind) => self.lower_rename_object(kind, stmt),
            Err(_) => self.unsupported("RENAME object type"),
        }
    }

    fn validate_rename_restrict(&mut self, behavior: i32) -> Result<()> {
        if !matches!(
            pg::DropBehavior::try_from(behavior),
            Ok(pg::DropBehavior::DropRestrict)
        ) {
            return self.unsupported_error("RENAME behavior");
        }
        Ok(())
    }

    fn lower_rename_object(
        &mut self,
        object_type: pg::ObjectType,
        stmt: &pg::RenameStmt,
    ) -> Result<Sexp> {
        let alter_object_symbol = format!("alter-{}", self.object_type_name(object_type)?);
        if !matches!(
            pg::DropBehavior::try_from(stmt.behavior),
            Ok(pg::DropBehavior::DropRestrict)
        ) {
            return self.unsupported("RENAME behavior");
        }

        let mut values = vec![self.symbol(&alter_object_symbol)];

        match (stmt.object.as_ref(), stmt.relation.as_ref()) {
            (Some(object), _) => values.extend(self.lower_alter_object_name(object_type, object)?),
            (None, Some(relation)) => values.push(self.lower_relation(relation)?),
            (None, None) if !stmt.subname.is_empty() => {
                values.push(Sexp::string(stmt.subname.clone()))
            }
            (None, None) => return self.unsupported("RENAME object"),
        };
        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        values.push(self.list("rename", [Sexp::string(stmt.newname.clone())]));

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_create_policy(&mut self, stmt: &pg::CreatePolicyStmt) -> Result<Sexp> {
        let table = self.required(stmt.table.as_ref(), "policy table")?;
        let on = [self.lower_relation(table)?];
        let mut values = vec![
            self.symbol("create-policy"),
            Sexp::string(stmt.policy_name.clone()),
            self.list("on", on),
        ];

        let command = match stmt.cmd_name.as_str() {
            "all" => None,
            "select" => Some("select"),
            "insert" => Some("insert"),
            "update" => Some("update"),
            "delete" => Some("delete"),
            _ => return self.unsupported("policy command"),
        };
        if let Some(command) = command {
            let for_value = [self.symbol(command)];
            values.push(self.list("for", for_value));
        }
        if !stmt.permissive {
            let restrictive_as = [self.symbol("restrictive")];
            values.push(self.list("as", restrictive_as));
        }
        self.push_policy_roles(&mut values, &stmt.roles)?;
        self.optional_node_clause(&mut values, "using", stmt.qual.as_deref())?;
        self.optional_node_clause(&mut values, "with-check", stmt.with_check.as_deref())?;

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alter_policy(&mut self, stmt: &pg::AlterPolicyStmt) -> Result<Sexp> {
        let table = self.required(stmt.table.as_ref(), "policy table")?;
        let on = [self.lower_relation(table)?];
        let mut values = vec![
            self.symbol("alter-policy"),
            Sexp::string(stmt.policy_name.clone()),
            self.list("on", on),
        ];

        self.push_policy_roles(&mut values, &stmt.roles)?;
        self.optional_node_clause(&mut values, "using", stmt.qual.as_deref())?;
        self.optional_node_clause(&mut values, "with-check", stmt.with_check.as_deref())?;

        Ok(Sexp::list(values))
    }

    fn push_policy_roles(&mut self, values: &mut Vec<Sexp>, roles: &[pg::Node]) -> Result<()> {
        let to = self.lower_role_spec_nodes(roles)?;
        if roles.is_empty() || is_single_public_role(roles) {
            return Ok(());
        }

        values.push(self.list("to", to));

        Ok(())
    }

    pub(super) fn lower_rename_policy(&mut self, stmt: &pg::RenameStmt) -> Result<Sexp> {
        let table = self.required(stmt.relation.as_ref(), "renamed policy table")?;
        let on = [self.lower_relation(table)?];
        if !matches!(
            pg::DropBehavior::try_from(stmt.behavior),
            Ok(pg::DropBehavior::DropRestrict)
        ) {
            return self.unsupported("RENAME behavior");
        }

        let mut values = vec![
            self.symbol("rename-policy"),
            Sexp::string(stmt.subname.clone()),
            self.list("on", on),
        ];

        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        values.push(Sexp::string(stmt.newname.clone()));

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_create_rule(&mut self, stmt: &pg::RuleStmt) -> Result<Sexp> {
        let event = match pg::CmdType::try_from(stmt.event) {
            Ok(pg::CmdType::CmdSelect) => "on-select",
            Ok(pg::CmdType::CmdUpdate) => "on-update",
            Ok(pg::CmdType::CmdInsert) => "on-insert",
            Ok(pg::CmdType::CmdDelete) => "on-delete",
            _ => return self.unsupported("rule event"),
        };
        let mut values = vec![
            self.symbol("create-rule"),
            Sexp::string(stmt.rulename.clone()),
        ];

        if stmt.replace {
            values.push(self.symbol("or-replace"));
        }

        values.push(self.symbol(event));

        let table = self.required(stmt.relation.as_ref(), "rule table")?;
        let to = [self.lower_relation(table)?];
        values.push(self.list("to", to));
        if stmt.instead {
            values.push(self.symbol("instead"));
        }
        self.optional_node_clause(&mut values, "when", stmt.where_clause.as_deref())?;
        if stmt.actions.is_empty() {
            values.push(self.list("nothing", []));
        } else {
            let actions = self.lower_nodes(&stmt.actions)?;
            values.push(self.list("actions", actions));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alter_object_schema(
        &mut self,
        stmt: &pg::AlterObjectSchemaStmt,
    ) -> Result<Sexp> {
        let object_type = pg::ObjectType::try_from(stmt.object_type)
            .map_err(|_| ParseError::new("unsupported ALTER object type", 0))?;
        let object = match (stmt.object.as_ref(), stmt.relation.as_ref()) {
            (Some(object), _) => self.lower_alter_object_name(object_type, object)?,
            (None, Some(relation)) => vec![self.lower_relation(relation)?],
            (None, None) => return self.unsupported("ALTER object schema target"),
        };
        let alter_statement_name = format!("alter-{}", self.object_type_name(object_type)?);
        let mut values = vec![self.symbol(&alter_statement_name)];

        values.extend(object);
        if stmt.missing_ok {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        values.push(self.list("set-schema", [Sexp::string(stmt.newschema.clone())]));

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alter_owner(&mut self, stmt: &pg::AlterOwnerStmt) -> Result<Sexp> {
        let object_type = pg::ObjectType::try_from(stmt.object_type).map_err(|_| {
            ParseError::new("unsupported PostgreSQL AST: ALTER OWNER object type", 0)
        })?;
        let command = self.alter_database_command(stmt.object_type, "ALTER OWNER object type")?;
        let object = self.required_node(stmt.object.as_deref(), "ALTER OWNER object")?;
        let mut values = vec![self.symbol(command)];

        let role = self.required(stmt.newowner.as_ref(), "ALTER OWNER role")?;
        let owner = [self.lower_role_spec(role)?];
        match self.required_kind(object.node.as_ref(), "ALTER OWNER object")? {
            PgNode::String(name) => values.push(Sexp::string(name.sval.clone())),
            PgNode::Integer(oid) if object_type == pg::ObjectType::ObjectLargeobject => {
                let oid = [self.symbol(&oid.ival.to_string())];
                values.push(self.list("oid", oid))
            }
            _ => values.extend(self.lower_alter_object_name(object_type, object)?),
        }

        values.push(self.list("owner", owner));

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_create_extension(
        &mut self,
        stmt: &pg::CreateExtensionStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-extension"),
            Sexp::string(stmt.extname.clone()),
        ];

        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-not-exists")]));
        }
        for node in &stmt.options {
            let option = match self.required_kind(node.node.as_ref(), "CREATE EXTENSION option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error("CREATE EXTENSION option"),
            };
            match option.defname.as_str() {
                "new_version" => {
                    let version_node =
                        self.required_node(option.arg.as_deref(), "CREATE EXTENSION version")?;
                    let extension_version = [Sexp::string(self.string_node(version_node)?)];
                    values.push(self.list("version", extension_version))
                }
                "schema" => {
                    let schema_node =
                        self.required_node(option.arg.as_deref(), "CREATE EXTENSION schema")?;
                    let extension_schema = [Sexp::string(self.string_node(schema_node)?)];
                    values.push(self.list("schema", extension_schema))
                }
                "cascade" => {
                    if self.lower_flag_argument(option, "CREATE EXTENSION cascade")? {
                        values.push(self.symbol("cascade"));
                    }
                }
                other => return self.unsupported_error(format!("CREATE EXTENSION option {other}")),
            }
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alter_extension(&mut self, stmt: &pg::AlterExtensionStmt) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("alter-extension"),
            Sexp::string(stmt.extname.clone()),
        ];

        for node in &stmt.options {
            let option = match self.required_kind(node.node.as_ref(), "ALTER EXTENSION option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error("ALTER EXTENSION option"),
            };
            match option.defname.as_str() {
                "new_version" => {
                    let version_node =
                        self.required_node(option.arg.as_deref(), "ALTER EXTENSION version")?;
                    let extension_version = [Sexp::string(self.string_node(version_node)?)];
                    values.push(self.list("update-to", extension_version))
                }
                other => return self.unsupported_error(format!("ALTER EXTENSION option {other}")),
            }
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_alter_extension_contents(
        &mut self,
        stmt: &pg::AlterExtensionContentsStmt,
    ) -> Result<Sexp> {
        let action = match stmt.action {
            1 => "add",
            -1 => "drop",
            _ => return self.unsupported("ALTER EXTENSION action"),
        };
        let node = self.required_node(stmt.object.as_deref(), "ALTER EXTENSION object")?;
        let object = match self.required_kind(node.node.as_ref(), "ALTER EXTENSION object")? {
            PgNode::ObjectWithArgs(object) => self.lower_object_with_args(object)?,
            PgNode::List(_) | PgNode::String(_) | PgNode::RangeVar(_) => {
                self.lower_object_name(node)?
            }
            _ => return self.unsupported_error("ALTER EXTENSION object"),
        };
        let alter_extension_string = [
            Sexp::string(stmt.extname.clone()),
            self.list(action, [object]),
        ];

        Ok(self.list("alter-extension", alter_extension_string))
    }

    pub(crate) fn object_type_name(&mut self, object_type: pg::ObjectType) -> Result<&'static str> {
        match object_type {
            pg::ObjectType::ObjectSchema => Ok("schema"),
            pg::ObjectType::ObjectTable => Ok("table"),
            pg::ObjectType::ObjectIndex => Ok("index"),
            pg::ObjectType::ObjectView => Ok("view"),
            pg::ObjectType::ObjectMatview => Ok("materialized-view"),
            pg::ObjectType::ObjectForeignTable => Ok("foreign-table"),
            pg::ObjectType::ObjectSequence => Ok("sequence"),
            pg::ObjectType::ObjectDomain => Ok("domain"),
            pg::ObjectType::ObjectType => Ok("type"),
            pg::ObjectType::ObjectCollation => Ok("collation"),
            pg::ObjectType::ObjectConversion => Ok("conversion"),
            pg::ObjectType::ObjectStatisticExt => Ok("statistics"),
            pg::ObjectType::ObjectFunction => Ok("function"),
            pg::ObjectType::ObjectProcedure => Ok("procedure"),
            pg::ObjectType::ObjectRoutine => Ok("routine"),
            pg::ObjectType::ObjectAggregate => Ok("aggregate"),
            pg::ObjectType::ObjectOperator => Ok("operator"),
            pg::ObjectType::ObjectOpclass => Ok("operator-class"),
            pg::ObjectType::ObjectOpfamily => Ok("operator-family"),
            pg::ObjectType::ObjectLanguage => Ok("language"),
            pg::ObjectType::ObjectTransform => Ok("transform"),
            pg::ObjectType::ObjectFdw => Ok("foreign-data-wrapper"),
            pg::ObjectType::ObjectForeignServer => Ok("server"),
            pg::ObjectType::ObjectTsconfiguration => Ok("text-search-configuration"),
            pg::ObjectType::ObjectTsdictionary => Ok("text-search-dictionary"),
            pg::ObjectType::ObjectTsparser => Ok("text-search-parser"),
            pg::ObjectType::ObjectTstemplate => Ok("text-search-template"),
            pg::ObjectType::ObjectTrigger => Ok("trigger"),
            pg::ObjectType::ObjectRule => Ok("rule"),
            pg::ObjectType::ObjectEventTrigger => Ok("event-trigger"),
            pg::ObjectType::ObjectLargeobject => Ok("large-object"),
            _ => Err(ParseError::new(
                "unsupported PostgreSQL AST: object type",
                0,
            )),
        }
    }

    fn lower_alter_options(&mut self, command: &pg::AlterTableCmd) -> Result<Vec<Sexp>> {
        let node = self.required_node(command.def.as_deref(), "ALTER TABLE options")?;
        let PgNode::List(options) =
            self.required_kind(node.node.as_ref(), "ALTER TABLE options")?
        else {
            return self.unsupported_error("ALTER TABLE options");
        };
        self.lower_nodes(&options.items)
    }

    fn lower_type_relation_name(&mut self, relation: &pg::RangeVar) -> Result<Sexp> {
        let mut values = vec![self.symbol("name")];

        if !relation.catalogname.is_empty() {
            values.push(Sexp::string(relation.catalogname.clone()));
        }
        if !relation.schemaname.is_empty() {
            values.push(Sexp::string(relation.schemaname.clone()));
        }

        if relation.relname.is_empty() || relation.alias.is_some() {
            return self.unsupported("composite type name");
        }

        values.push(Sexp::string(relation.relname.clone()));

        Ok(Sexp::list(values))
    }

    fn lower_type_node(&mut self, node: &pg::Node) -> Result<Sexp> {
        match self.required_kind(node.node.as_ref(), "type")? {
            PgNode::TypeName(typename) => self.lower_type(typename),
            _ => self.unsupported("column type"),
        }
    }
}
