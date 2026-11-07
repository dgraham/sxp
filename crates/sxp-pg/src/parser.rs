use super::*;
use pg_query::protobuf as pg;
use pg_query::protobuf::{a_const, node::Node as PgNode};

mod copy;
mod ddl;
mod dml;
mod explain;
mod expression;
mod foreign;
mod json;
mod maintenance;
mod objects;
mod opclass;
mod procedural;
mod query;
mod relation;
mod schema;
mod session;
mod sqlcommenter;
#[cfg(test)]
mod tests;
mod txn;
mod xml;

impl PostgresParser {
    pub(super) fn parse_statements(&mut self, input: &str) -> Result<Vec<Sexp>> {
        let result =
            pg_query::parse(input, 0).map_err(|error| ParseError::new(error.to_string(), 0))?;
        let statements = result
            .protobuf
            .stmts
            .iter()
            .map(|stmt| {
                self.required_node(stmt.stmt.as_deref(), "statement")
                    .and_then(|node| self.lower(node))
            })
            .collect::<Result<Vec<_>>>()?;

        let tags = if self.options.sqlcommenter {
            sqlcommenter::sqlcommenter_tags(input)
        } else {
            Vec::new()
        };
        let mut statements = self.attach_tags(
            statements,
            &sqlcommenter::statement_spans(&result.protobuf.stmts, input),
            &tags,
        );

        if statements.is_empty() {
            if tags.is_empty() {
                return Err(ParseError::new("PostgreSQL parser returned no tree", 0));
            }
            statements.push(self.sqlcommenter_clause(tags.iter()));
        }
        Ok(statements)
    }
}

impl PostgresParser {
    #[allow(unreachable_patterns)]
    pub(super) fn lower(&mut self, node: &pg::Node) -> Result<Sexp> {
        match self.required_kind(node.node.as_ref(), "node")? {
            PgNode::SelectStmt(stmt) => self.lower_select(stmt),
            PgNode::InsertStmt(stmt) => self.lower_insert(stmt),
            PgNode::UpdateStmt(stmt) => self.lower_update(stmt),
            PgNode::DeleteStmt(stmt) => self.lower_delete(stmt),
            PgNode::MergeStmt(stmt) => self.lower_merge(stmt),
            PgNode::ExplainStmt(stmt) => self.lower_explain(stmt),
            PgNode::ReturnStmt(stmt) => self.lower_return(stmt),
            PgNode::CreateStmt(stmt) => self.lower_create_table(stmt),
            PgNode::AlterTableStmt(stmt) => self.lower_alter_table(stmt),
            PgNode::RenameStmt(stmt) if stmt.rename_type == pg::ObjectType::ObjectPolicy as i32 => {
                self.lower_rename_policy(stmt)
            }
            PgNode::RenameStmt(stmt)
                if stmt.rename_type == pg::ObjectType::ObjectDatabase as i32
                    || stmt.rename_type == pg::ObjectType::ObjectTablespace as i32
                    || stmt.rename_type == pg::ObjectType::ObjectPublication as i32
                    || stmt.rename_type == pg::ObjectType::ObjectSubscription as i32 =>
            {
                self.lower_alter_rename(stmt)
            }
            PgNode::RenameStmt(stmt) => self.lower_rename_table(stmt),
            PgNode::AlterObjectSchemaStmt(stmt) => self.lower_alter_object_schema(stmt),
            PgNode::CreateSchemaStmt(stmt) => self.lower_create_schema(stmt),
            PgNode::DropStmt(stmt) => self.lower_drop(stmt),
            PgNode::IndexStmt(stmt) => self.lower_create_index(stmt),
            PgNode::ViewStmt(stmt) => self.lower_create_view(stmt),
            PgNode::CreateTableAsStmt(stmt) => match pg::ObjectType::try_from(stmt.objtype) {
                Ok(pg::ObjectType::ObjectTable) => self.lower_create_table_as(stmt),
                Ok(pg::ObjectType::ObjectMatview) => self.lower_create_materialized_view(stmt),
                _ => self.unsupported("CREATE TABLE AS statement"),
            },
            PgNode::CreateSeqStmt(stmt) => self.lower_create_sequence(stmt),
            PgNode::AlterSeqStmt(stmt) => self.lower_alter_sequence(stmt),
            PgNode::CreateDomainStmt(stmt) => self.lower_create_domain(stmt),
            PgNode::AlterDomainStmt(stmt) => self.lower_alter_domain(stmt),
            PgNode::CompositeTypeStmt(stmt) => self.lower_create_composite_type(stmt),
            PgNode::CreateEnumStmt(stmt) => self.lower_create_enum(stmt),
            PgNode::AlterEnumStmt(stmt) => self.lower_alter_enum(stmt),
            PgNode::RangeVar(relation) => self.lower_relation(relation),
            PgNode::JoinExpr(join) => self.lower_join(join),
            PgNode::RangeSubselect(subselect) => self.lower_range_subselect(subselect),
            PgNode::ResTarget(target) => self.lower_target(target),
            PgNode::ColumnRef(reference) => self.lower_column_ref(reference),
            PgNode::ParamRef(reference) => {
                let parameter = [self.symbol(&reference.number.to_string())];
                Ok(self.list("parameter", parameter))
            }
            PgNode::AConst(value) => self.lower_constant(value),
            PgNode::AExpr(expr) => self.lower_operator(expr),
            PgNode::AArrayExpr(expr) => {
                let array = self.lower_nodes(&expr.elements)?;
                Ok(self.list("array", array))
            }
            PgNode::AIndirection(indirection) => self.lower_indirection(indirection),
            PgNode::AIndices(indices) => self.lower_indices(indices),
            PgNode::BoolExpr(expr) => self.lower_boolean(expr),
            PgNode::NullTest(test) => self.lower_null_test(test),
            PgNode::BooleanTest(test) => self.lower_boolean_test(test),
            PgNode::FuncCall(call) => self.lower_function(call),
            PgNode::CaseExpr(expr) => self.lower_case(expr),
            PgNode::CaseWhen(expr) => self.lower_case_when(expr),
            PgNode::ArrayExpr(expr) => {
                let array = self.lower_nodes(&expr.elements)?;
                Ok(self.list("array", array))
            }
            PgNode::RowExpr(expr) => {
                let row = self.lower_nodes(&expr.args)?;
                Ok(self.list("row", row))
            }
            PgNode::RowCompareExpr(expr) => self.lower_row_compare(expr),
            PgNode::CoalesceExpr(expr) => {
                let coalesce = self.lower_nodes(&expr.args)?;
                Ok(self.list("coalesce", coalesce))
            }
            PgNode::NullIfExpr(expr) => self.lower_binary_expression("nullif", &expr.args),
            PgNode::DistinctExpr(expr) => {
                self.lower_binary_expression("is-distinct-from", &expr.args)
            }
            PgNode::ScalarArrayOpExpr(expr) => self.lower_scalar_array_op(expr),
            PgNode::SubLink(link) => self.lower_sublink(link),
            PgNode::SortBy(sort) => self.lower_sort(sort),
            PgNode::WindowDef(window) => self.lower_window(window),
            PgNode::CommonTableExpr(cte) => self.lower_cte(cte),
            PgNode::TypeCast(cast) => {
                let cast_type = self.required(cast.type_name.as_ref(), "cast type")?;
                let cast_argument = self.required_node(cast.arg.as_deref(), "cast argument")?;
                let lowered_cast = [self.lower(cast_argument)?, self.lower_type(cast_type)?];
                Ok(self.list("cast", lowered_cast))
            }
            PgNode::ColumnDef(column) => self.lower_column_definition(column),
            PgNode::Constraint(constraint) => self.lower_constraint(constraint),
            PgNode::IndexElem(element) => self.lower_index_element(element),
            PgNode::DefElem(option) => self.lower_definition_option(option),
            PgNode::PartitionElem(element) => self.lower_partition_element(element),
            PgNode::List(values) => Ok(Sexp::list(self.lower_nodes(&values.items)?)),
            PgNode::Integer(value) => Ok(self.symbol(&value.ival.to_string())),
            PgNode::Float(value) => Ok(self.symbol(&value.fval)),
            PgNode::Boolean(value) => Ok(Sexp::boolean(value.boolval)),
            PgNode::String(value) => Ok(Sexp::string(value.sval.clone())),
            PgNode::BitString(value) => {
                Ok(self.list("bit-string", [Sexp::string(value.bsval.clone())]))
            }
            PgNode::AStar(_) => Ok(self.symbol("*")),
            PgNode::SetToDefault(_) => Ok(self.symbol("default")),
            PgNode::Alias(..) => self.unsupported("node Alias"),
            PgNode::TableFunc(..) => self.unsupported("node TableFunc"),
            PgNode::IntoClause(into) => self.lower_into(into),
            PgNode::Var(..) => self.unsupported("node Var"),
            PgNode::Param(..) => self.unsupported("node Param"),
            PgNode::Aggref(..) => self.unsupported("node Aggref"),
            PgNode::GroupingFunc(function) => {
                let grouping = self.lower_nodes(&function.args)?;
                Ok(self.list("grouping", grouping))
            }
            PgNode::WindowFunc(..) => self.unsupported("node WindowFunc"),
            PgNode::WindowFuncRunCondition(..) => self.unsupported("node WindowFuncRunCondition"),
            PgNode::MergeSupportFunc(..) => Ok(Sexp::list([self.symbol("merge-action")])),
            PgNode::SubscriptingRef(..) => self.unsupported("node SubscriptingRef"),
            PgNode::FuncExpr(..) => self.unsupported("node FuncExpr"),
            PgNode::NamedArgExpr(arg) => {
                let named_arg_node = self.required_node(arg.arg.as_deref(), "named argument")?;
                let named_arg = [Sexp::string(arg.name.clone()), self.lower(named_arg_node)?];
                Ok(self.list("named-arg", named_arg))
            }
            PgNode::OpExpr(..) => self.unsupported("node OpExpr"),
            PgNode::SubPlan(..) => self.unsupported("node SubPlan"),
            PgNode::AlternativeSubPlan(..) => self.unsupported("node AlternativeSubPlan"),
            PgNode::FieldSelect(..) => self.unsupported("node FieldSelect"),
            PgNode::FieldStore(..) => self.unsupported("node FieldStore"),
            PgNode::RelabelType(..) => self.unsupported("node RelabelType"),
            PgNode::CoerceViaIo(..) => self.unsupported("node CoerceViaIo"),
            PgNode::ArrayCoerceExpr(..) => self.unsupported("node ArrayCoerceExpr"),
            PgNode::ConvertRowtypeExpr(..) => self.unsupported("node ConvertRowtypeExpr"),
            PgNode::CollateExpr(..) => self.unsupported("node CollateExpr"),
            PgNode::CaseTestExpr(..) => self.unsupported("node CaseTestExpr"),
            PgNode::MinMaxExpr(expr) => self.lower_min_max(expr),
            PgNode::SqlvalueFunction(function) => self.lower_sqlvalue_function(function),
            PgNode::XmlExpr(expr) => self.lower_xml_expr(expr),
            PgNode::JsonFormat(format) => match self.lower_json_format(format)? {
                Some(sexp) => Ok(sexp),
                None => {
                    let default_format = [self.symbol("default")];
                    Ok(self.list("format", default_format))
                }
            },
            PgNode::JsonReturning(returning) => self.lower_json_returning(returning),
            PgNode::JsonValueExpr(expr) => self.lower_json_value_expr(expr),
            PgNode::JsonConstructorExpr(expr) => self.lower_json_constructor_expr(expr),
            PgNode::JsonIsPredicate(predicate) => self.lower_json_is_predicate(predicate),
            PgNode::JsonBehavior(behavior) => self.lower_json_behavior(behavior),
            PgNode::JsonExpr(expr) => self.lower_json_expr(expr),
            PgNode::JsonTablePath(path) => {
                Ok(self.list("json-table-path", [Sexp::string(path.name.clone())]))
            }
            PgNode::JsonTablePathScan(scan) => self.lower_json_table_path_scan(scan),
            PgNode::JsonTableSiblingJoin(join) => self.lower_json_table_sibling_join(join),
            PgNode::MergeAction(..) => self.unsupported("node MergeAction"),
            PgNode::CoerceToDomain(..) => self.unsupported("node CoerceToDomain"),
            PgNode::CoerceToDomainValue(..) => self.unsupported("node CoerceToDomainValue"),
            PgNode::CurrentOfExpr(expr) => self.lower_current_of(expr),
            PgNode::NextValueExpr(..) => self.unsupported("node NextValueExpr"),
            PgNode::InferenceElem(..) => self.unsupported("node InferenceElem"),
            PgNode::TargetEntry(..) => self.unsupported("node TargetEntry"),
            PgNode::RangeTblRef(..) => self.unsupported("node RangeTblRef"),
            PgNode::FromExpr(..) => self.unsupported("node FromExpr"),
            PgNode::OnConflictExpr(..) => self.unsupported("node OnConflictExpr"),
            PgNode::Query(..) => self.unsupported("node Query"),
            PgNode::TypeName(typename) => self.lower_type(typename),
            PgNode::CollateClause(clause) => self.lower_collate(clause),
            PgNode::RoleSpec(spec) => self.lower_role_spec(spec),
            PgNode::MultiAssignRef(..) => self.unsupported("node MultiAssignRef"),
            PgNode::RangeFunction(function) => self.lower_range_function(function),
            PgNode::RangeTableFunc(function) => self.lower_range_table_func(function),
            PgNode::RangeTableFuncCol(column) => self.lower_range_table_func_col(column),
            PgNode::RangeTableSample(sample) => self.lower_table_sample(sample),
            PgNode::TableLikeClause(clause) => self.lower_table_like_clause(clause),
            PgNode::LockingClause(clause) => self.lower_locking_clause(clause),
            PgNode::XmlSerialize(serialize) => self.lower_xml_serialize(serialize),
            PgNode::PartitionSpec(spec) => self.lower_partition_spec(spec),
            PgNode::PartitionBoundSpec(bound) => self.lower_partition_bound_spec(bound),
            PgNode::PartitionRangeDatum(datum) => self.lower_partition_range_datum(datum),
            PgNode::PartitionCmd(command) => self.lower_partition_command(command, false),
            PgNode::RangeTblEntry(..) => self.unsupported("node RangeTblEntry"),
            PgNode::RtepermissionInfo(..) => self.unsupported("node RtepermissionInfo"),
            PgNode::RangeTblFunction(..) => self.unsupported("node RangeTblFunction"),
            PgNode::TableSampleClause(..) => self.unsupported("node TableSampleClause"),
            PgNode::WithCheckOption(..) => self.unsupported("node WithCheckOption"),
            PgNode::SortGroupClause(..) => self.unsupported("node SortGroupClause"),
            PgNode::GroupingSet(set) => self.lower_grouping_set(set),
            PgNode::WindowClause(..) => self.unsupported("node WindowClause"),
            PgNode::RowMarkClause(..) => self.unsupported("node RowMarkClause"),
            PgNode::WithClause(..) => self.unsupported("node WithClause"),
            PgNode::InferClause(..) => self.unsupported("node InferClause"),
            PgNode::OnConflictClause(..) => self.unsupported("node OnConflictClause"),
            PgNode::CtesearchClause(clause) => self.lower_cte_search(clause),
            PgNode::CtecycleClause(clause) => self.lower_cte_cycle(clause),
            PgNode::MergeWhenClause(..) => self.unsupported("node MergeWhenClause"),
            PgNode::TriggerTransition(..) => self.unsupported("node TriggerTransition"),
            PgNode::JsonOutput(output) => self.lower_json_output(output),
            PgNode::JsonArgument(argument) => self.lower_json_argument(argument),
            PgNode::JsonFuncExpr(expr) => self.lower_json_func_expr(expr),
            PgNode::JsonTablePathSpec(spec) => self.lower_json_table_path_spec(spec),
            PgNode::JsonTable(table) => self.lower_json_table(table),
            PgNode::JsonTableColumn(column) => self.lower_json_table_column(column),
            PgNode::JsonKeyValue(pair) => self.lower_json_key_value(pair),
            PgNode::JsonParseExpr(expr) => self.lower_json_parse_expr(expr),
            PgNode::JsonScalarExpr(expr) => self.lower_json_scalar_expr(expr),
            PgNode::JsonSerializeExpr(expr) => self.lower_json_serialize_expr(expr),
            PgNode::JsonObjectConstructor(constructor) => {
                self.lower_json_object_constructor(constructor)
            }
            PgNode::JsonArrayConstructor(constructor) => {
                self.lower_json_array_constructor(constructor)
            }
            PgNode::JsonArrayQueryConstructor(constructor) => {
                self.lower_json_array_query_constructor(constructor)
            }
            PgNode::JsonAggConstructor(constructor) => Ok(Sexp::list(
                self.lower_json_aggregate_constructor(constructor)?,
            )),
            PgNode::JsonObjectAgg(aggregate) => self.lower_json_object_agg(aggregate),
            PgNode::JsonArrayAgg(aggregate) => self.lower_json_array_agg(aggregate),
            PgNode::RawStmt(..) => self.unsupported("node RawStmt"),
            PgNode::SetOperationStmt(..) => self.unsupported("node SetOperationStmt"),
            PgNode::PlassignStmt(..) => self.unsupported("node PlassignStmt"),
            PgNode::ReplicaIdentityStmt(..) => self.unsupported("node ReplicaIdentityStmt"),
            PgNode::AlterTableCmd(..) => self.unsupported("node AlterTableCmd"),
            PgNode::AlterCollationStmt(stmt) => self.lower_alter_collation(stmt),
            PgNode::GrantStmt(stmt) => self.lower_grant(stmt),
            PgNode::ObjectWithArgs(object) => self.lower_object_with_args(object),
            PgNode::AccessPriv(privilege) => self.lower_access_privilege(privilege),
            PgNode::GrantRoleStmt(stmt) => self.lower_grant_role(stmt),
            PgNode::AlterDefaultPrivilegesStmt(stmt) => self.lower_alter_default_privileges(stmt),
            PgNode::CopyStmt(stmt) => self.lower_copy(stmt),
            PgNode::VariableSetStmt(stmt) => self.lower_variable_set(stmt),
            PgNode::VariableShowStmt(stmt) => {
                Ok(self.list("show", [Sexp::string(stmt.name.clone())]))
            }
            PgNode::CreateTableSpaceStmt(stmt) => self.lower_create_tablespace(stmt),
            PgNode::DropTableSpaceStmt(stmt) => self.lower_drop_tablespace(stmt),
            PgNode::AlterTableSpaceOptionsStmt(stmt) => self.lower_alter_tablespace_options(stmt),
            PgNode::AlterTableMoveAllStmt(stmt) => self.lower_alter_table_move_all(stmt),
            PgNode::CreateExtensionStmt(stmt) => self.lower_create_extension(stmt),
            PgNode::AlterExtensionStmt(stmt) => self.lower_alter_extension(stmt),
            PgNode::AlterExtensionContentsStmt(stmt) => self.lower_alter_extension_contents(stmt),
            PgNode::CreateFdwStmt(stmt) => self.lower_create_fdw(stmt),
            PgNode::AlterFdwStmt(stmt) => self.lower_alter_fdw(stmt),
            PgNode::CreateForeignServerStmt(stmt) => self.lower_create_foreign_server(stmt),
            PgNode::AlterForeignServerStmt(stmt) => self.lower_alter_foreign_server(stmt),
            PgNode::CreateForeignTableStmt(stmt) => self.lower_create_foreign_table(stmt),
            PgNode::CreateUserMappingStmt(stmt) => self.lower_create_user_mapping(stmt),
            PgNode::AlterUserMappingStmt(stmt) => self.lower_alter_user_mapping(stmt),
            PgNode::DropUserMappingStmt(stmt) => self.lower_drop_user_mapping(stmt),
            PgNode::ImportForeignSchemaStmt(stmt) => self.lower_import_foreign_schema(stmt),
            PgNode::CreatePolicyStmt(stmt) => self.lower_create_policy(stmt),
            PgNode::AlterPolicyStmt(stmt) => self.lower_alter_policy(stmt),
            PgNode::CreateAmStmt(stmt) => self.lower_create_am(stmt),
            PgNode::CreateTrigStmt(stmt) => self.lower_create_trigger(stmt),
            PgNode::CreateEventTrigStmt(stmt) => self.lower_create_event_trigger(stmt),
            PgNode::AlterEventTrigStmt(stmt) => self.lower_alter_event_trigger(stmt),
            PgNode::CreatePlangStmt(stmt) => self.lower_create_language(stmt),
            PgNode::CreateRoleStmt(stmt) => self.lower_create_role(stmt),
            PgNode::AlterRoleStmt(stmt) => self.lower_alter_role(stmt),
            PgNode::AlterRoleSetStmt(stmt) => self.lower_alter_role_set(stmt),
            PgNode::DropRoleStmt(stmt) => self.lower_drop_role(stmt),
            PgNode::DefineStmt(stmt) => self.lower_define_stmt(stmt),
            PgNode::CreateOpClassStmt(stmt) => self.lower_create_opclass(stmt),
            PgNode::CreateOpClassItem(..) => self.unsupported("node CreateOpClassItem"),
            PgNode::CreateOpFamilyStmt(stmt) => self.lower_create_opfamily(stmt),
            PgNode::AlterOpFamilyStmt(stmt) => self.lower_alter_opfamily(stmt),
            PgNode::TruncateStmt(stmt) => self.lower_truncate(stmt),
            PgNode::CommentStmt(stmt) => self.lower_comment(stmt),
            PgNode::SecLabelStmt(stmt) => self.lower_security_label(stmt),
            PgNode::DeclareCursorStmt(stmt) => self.lower_declare_cursor(stmt),
            PgNode::ClosePortalStmt(stmt) => self.lower_close_cursor(stmt),
            PgNode::FetchStmt(stmt) => self.lower_fetch(stmt),
            PgNode::CreateStatsStmt(stmt) => self.lower_create_stats(stmt),
            PgNode::StatsElem(element) => self.lower_stats_element(element),
            PgNode::AlterStatsStmt(stmt) => self.lower_alter_stats(stmt),
            PgNode::CreateFunctionStmt(stmt) => self.lower_create_function(stmt),
            PgNode::FunctionParameter(..) => self.unsupported("node FunctionParameter"),
            PgNode::AlterFunctionStmt(stmt) => self.lower_alter_function(stmt),
            PgNode::DoStmt(stmt) => self.lower_do(stmt),
            PgNode::InlineCodeBlock(..) => self.unsupported("node InlineCodeBlock"),
            PgNode::CallStmt(stmt) => self.lower_call(stmt),
            PgNode::CallContext(..) => self.unsupported("node CallContext"),
            PgNode::AlterObjectDependsStmt(stmt) => self.lower_alter_object_depends(stmt),
            PgNode::AlterOwnerStmt(stmt) => self.lower_alter_owner(stmt),
            PgNode::AlterOperatorStmt(stmt) => self.lower_alter_operator(stmt),
            PgNode::AlterTypeStmt(stmt) => self.lower_alter_type(stmt),
            PgNode::RuleStmt(stmt) => self.lower_create_rule(stmt),
            PgNode::NotifyStmt(stmt) => self.lower_notify(stmt),
            PgNode::ListenStmt(stmt) => self.lower_listen(stmt),
            PgNode::UnlistenStmt(stmt) => self.lower_unlisten(stmt),
            PgNode::TransactionStmt(stmt) => self.lower_transaction(stmt),
            PgNode::CreateRangeStmt(stmt) => self.lower_create_range(stmt),
            PgNode::LoadStmt(stmt) => self.lower_load(stmt),
            PgNode::CreatedbStmt(stmt) => self.lower_create_database(stmt),
            PgNode::AlterDatabaseStmt(stmt) => self.lower_alter_database(stmt),
            PgNode::AlterDatabaseRefreshCollStmt(stmt) => {
                self.lower_alter_database_refresh_collation(stmt)
            }
            PgNode::AlterDatabaseSetStmt(stmt) => self.lower_alter_database_set(stmt),
            PgNode::DropdbStmt(stmt) => self.lower_drop_database(stmt),
            PgNode::AlterSystemStmt(stmt) => self.lower_alter_system(stmt),
            PgNode::ClusterStmt(stmt) => self.lower_cluster(stmt),
            PgNode::VacuumStmt(stmt) => self.lower_vacuum(stmt),
            PgNode::VacuumRelation(..) => self.lower_vacuum_relation(node),
            PgNode::RefreshMatViewStmt(stmt) => self.lower_refresh_matview(stmt),
            PgNode::CheckPointStmt(_) => self.lower_checkpoint(),
            PgNode::DiscardStmt(stmt) => self.lower_discard(stmt),
            PgNode::LockStmt(stmt) => self.lower_lock(stmt),
            PgNode::ConstraintsSetStmt(stmt) => self.lower_constraints_set(stmt),
            PgNode::ReindexStmt(stmt) => self.lower_reindex(stmt),
            PgNode::CreateConversionStmt(stmt) => self.lower_create_conversion(stmt),
            PgNode::CreateCastStmt(stmt) => self.lower_create_cast(stmt),
            PgNode::CreateTransformStmt(stmt) => self.lower_create_transform(stmt),
            PgNode::PrepareStmt(stmt) => self.lower_prepare(stmt),
            PgNode::ExecuteStmt(stmt) => self.lower_execute(stmt),
            PgNode::DeallocateStmt(stmt) => self.lower_deallocate(stmt),
            PgNode::DropOwnedStmt(stmt) => self.lower_drop_owned(stmt),
            PgNode::ReassignOwnedStmt(stmt) => self.lower_reassign_owned(stmt),
            PgNode::AlterTsdictionaryStmt(stmt) => self.lower_alter_tsdictionary(stmt),
            PgNode::AlterTsconfigurationStmt(stmt) => self.lower_alter_tsconfiguration(stmt),
            PgNode::PublicationTable(..) => self.unsupported("node PublicationTable"),
            PgNode::PublicationObjSpec(..) => self.unsupported("node PublicationObjSpec"),
            PgNode::CreatePublicationStmt(stmt) => self.lower_create_publication(stmt),
            PgNode::AlterPublicationStmt(stmt) => self.lower_alter_publication(stmt),
            PgNode::CreateSubscriptionStmt(stmt) => self.lower_create_subscription(stmt),
            PgNode::AlterSubscriptionStmt(stmt) => self.lower_alter_subscription(stmt),
            PgNode::DropSubscriptionStmt(stmt) => self.lower_drop_subscription(stmt),
            PgNode::IntList(..) => self.unsupported("node IntList"),
            PgNode::OidList(..) => self.unsupported("node OidList"),
            _ => self.unsupported("node added by a newer pg_query"),
        }
    }
}

const DEFAULT_WINDOW_FRAME_OPTIONS: i32 =
    FRAME_RANGE | FRAME_START_UNBOUNDED_PRECEDING | FRAME_END_CURRENT_ROW;
const FRAME_RANGE: i32 = 0x0002;
const FRAME_ROWS: i32 = 0x0004;
const FRAME_GROUPS: i32 = 0x0008;
const FRAME_BETWEEN: i32 = 0x0010;
const FRAME_START_UNBOUNDED_PRECEDING: i32 = 0x0020;
const FRAME_END_UNBOUNDED_PRECEDING: i32 = 0x0040;
const FRAME_START_UNBOUNDED_FOLLOWING: i32 = 0x0080;
const FRAME_END_UNBOUNDED_FOLLOWING: i32 = 0x0100;
const FRAME_START_CURRENT_ROW: i32 = 0x0200;
const FRAME_END_CURRENT_ROW: i32 = 0x0400;
const FRAME_START_OFFSET_PRECEDING: i32 = 0x0800;
const FRAME_END_OFFSET_PRECEDING: i32 = 0x1000;
const FRAME_START_OFFSET_FOLLOWING: i32 = 0x2000;
const FRAME_END_OFFSET_FOLLOWING: i32 = 0x4000;
const FRAME_EXCLUDE_CURRENT_ROW: i32 = 0x8000;
const FRAME_EXCLUDE_GROUP: i32 = 0x10000;
const FRAME_EXCLUDE_TIES: i32 = 0x20000;
const FRAME_START_BOUNDS: i32 = FRAME_START_UNBOUNDED_PRECEDING
    | FRAME_START_UNBOUNDED_FOLLOWING
    | FRAME_START_CURRENT_ROW
    | FRAME_START_OFFSET_PRECEDING
    | FRAME_START_OFFSET_FOLLOWING;
const FRAME_END_BOUNDS: i32 = FRAME_END_UNBOUNDED_PRECEDING
    | FRAME_END_UNBOUNDED_FOLLOWING
    | FRAME_END_CURRENT_ROW
    | FRAME_END_OFFSET_PRECEDING
    | FRAME_END_OFFSET_FOLLOWING;
const FRAME_EXCLUSIONS: i32 = FRAME_EXCLUDE_CURRENT_ROW | FRAME_EXCLUDE_GROUP | FRAME_EXCLUDE_TIES;

enum WindowFrameBound {
    UnboundedPreceding,
    UnboundedFollowing,
    CurrentRow,
    Preceding,
    Following,
}

impl PostgresParser {
    fn lower_generic_options(&mut self, options: &[pg::Node], context: &str) -> Result<Vec<Sexp>> {
        options
            .iter()
            .map(|node| {
                let PgNode::DefElem(option) = self.required_kind(node.node.as_ref(), context)?
                else {
                    return self.unsupported_error(context);
                };
                if !option.defnamespace.is_empty() {
                    return self.unsupported_error(context);
                }

                let name = match pg::DefElemAction::try_from(option.defaction) {
                    Ok(pg::DefElemAction::DefelemUnspec) => "option",
                    Ok(pg::DefElemAction::DefelemSet) => "set-option",
                    Ok(pg::DefElemAction::DefelemAdd) => "add-option",
                    Ok(pg::DefElemAction::DefelemDrop) => "drop-option",
                    _ => return self.unsupported_error(context),
                };
                let mut values = vec![self.symbol(name), Sexp::string(option.defname.clone())];

                match option.arg.as_deref() {
                    Some(argument) => values.push(self.lower(argument)?),
                    None if name == "drop-option" => {}
                    None => return self.unsupported_error(context),
                }
                Ok(Sexp::list(values))
            })
            .collect()
    }

    fn lower_signature_type(&mut self, typename: &pg::TypeName) -> Result<Sexp> {
        self.lower_type(typename)
    }

    fn lower_nodes(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        nodes.iter().map(|x| self.lower(x)).collect()
    }

    fn lower_object_with_args(&mut self, object: &pg::ObjectWithArgs) -> Result<Sexp> {
        let mut values = vec![self.symbol("name")];

        values.extend(
            object
                .objname
                .iter()
                .map(|x| self.string_node(x))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(Sexp::string),
        );

        if object.args_unspecified {
            values.push(self.symbol("*"));
            return Ok(Sexp::list(values));
        }

        if !object.objfuncargs.is_empty() {
            let argument_type = object
                .objfuncargs
                .iter()
                .map(|node| {
                    let parameter =
                        match self.required_kind(node.node.as_ref(), "function argument")? {
                            PgNode::FunctionParameter(parameter) => parameter,
                            _ => return self.unsupported_error("function argument"),
                        };
                    let argument_type =
                        self.required(parameter.arg_type.as_ref(), "function argument type")?;
                    self.lower_signature_type(argument_type)
                })
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("parameters", argument_type));
            return Ok(Sexp::list(values));
        }

        if !object.objargs.is_empty() {
            let any_parameters = object
                .objargs
                .iter()
                .map(|node| match node.node.as_ref() {
                    Some(PgNode::TypeName(type_name)) => self.lower_signature_type(type_name),
                    None => Ok(self.symbol("any")),
                    _ => self.unsupported("function argument type"),
                })
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("parameters", any_parameters));
        }
        Ok(Sexp::list(values))
    }

    fn lower_role_spec(&mut self, spec: &pg::RoleSpec) -> Result<Sexp> {
        let kind = pg::RoleSpecType::try_from(spec.roletype)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: role", 0))?;
        match kind {
            pg::RoleSpecType::RolespecCstring => {
                Ok(self.list("role", [Sexp::string(spec.rolename.clone())]))
            }
            pg::RoleSpecType::RolespecPublic => {
                let public_role = [self.symbol("public")];
                Ok(self.list("role", public_role))
            }
            pg::RoleSpecType::RolespecCurrentRole => {
                let current_role_role = [self.symbol("current-role")];
                Ok(self.list("role", current_role_role))
            }
            pg::RoleSpecType::RolespecCurrentUser => {
                let current_user_role = [self.symbol("current-user")];
                Ok(self.list("role", current_user_role))
            }
            pg::RoleSpecType::RolespecSessionUser => {
                let session_user_role = [self.symbol("session-user")];
                Ok(self.list("role", session_user_role))
            }
            _ => self.unsupported("role type"),
        }
    }

    fn lower_role_spec_nodes(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        nodes
            .iter()
            .map(
                |node| match self.required_kind(node.node.as_ref(), "role")? {
                    PgNode::RoleSpec(spec) => self.lower_role_spec(spec),
                    _ => self.unsupported("role"),
                },
            )
            .collect()
    }

    fn optional_clause(&mut self, values: &mut Vec<Sexp>, name: &str, items: Vec<Sexp>) {
        if !items.is_empty() {
            values.push(self.list(name, items));
        }
    }

    fn optional_node_clause(
        &mut self,
        values: &mut Vec<Sexp>,
        name: &str,
        node: Option<&pg::Node>,
    ) -> Result<()> {
        if let Some(node) = node {
            let lowered_node = [self.lower(node)?];
            values.push(self.list(name, lowered_node));
        }
        Ok(())
    }

    fn string_node(&mut self, node: &pg::Node) -> Result<String> {
        match self.required_kind(node.node.as_ref(), "name")? {
            PgNode::String(value) => Ok(value.sval.clone()),
            _ => Err(ParseError::new("expected PostgreSQL name", 0)),
        }
    }

    fn required<'a, T>(&mut self, value: Option<&'a T>, name: &str) -> Result<&'a T> {
        value.ok_or_else(|| ParseError::new(format!("PostgreSQL AST missing {name}"), 0))
    }

    fn required_node<'a>(
        &mut self,
        value: Option<&'a pg::Node>,
        name: &str,
    ) -> Result<&'a pg::Node> {
        self.required(value, name)
    }

    fn required_kind<'a>(&mut self, value: Option<&'a PgNode>, name: &str) -> Result<&'a PgNode> {
        self.required(value, name)
    }

    fn list(&mut self, name: &str, values: impl IntoIterator<Item = Sexp>) -> Sexp {
        Sexp::list(std::iter::once(self.symbol(name)).chain(values))
    }

    fn unsupported(&mut self, item: impl Into<String>) -> Result<Sexp> {
        self.unsupported_error(item)
    }

    fn unsupported_error<T>(&mut self, item: impl Into<String>) -> Result<T> {
        Err(ParseError::new(
            format!("unsupported PostgreSQL AST: {}", item.into()),
            0,
        ))
    }
}
