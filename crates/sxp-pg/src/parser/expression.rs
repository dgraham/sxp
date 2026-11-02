use super::*;

impl PostgresParser {
    pub(super) fn lower_target(&mut self, target: &pg::ResTarget) -> Result<Sexp> {
        let target_expression_node =
            self.required_node(target.val.as_deref(), "target expression")?;
        if !target.indirection.is_empty() {
            return self.unsupported("indirected target");
        }

        let value = self.lower(target_expression_node)?;
        if target.name.is_empty() {
            Ok(value)
        } else {
            Ok(self.list("as", [value, Sexp::string(target.name.clone())]))
        }
    }

    pub(super) fn lower_column_ref(&mut self, reference: &pg::ColumnRef) -> Result<Sexp> {
        let fields = reference
            .fields
            .iter()
            .map(
                |field| match self.required_kind(field.node.as_ref(), "column field")? {
                    PgNode::String(value) => Ok(Sexp::string(value.sval.clone())),
                    PgNode::AStar(_) => Ok(self.symbol("*")),
                    _ => self.unsupported("column reference field"),
                },
            )
            .collect::<Result<Vec<_>>>()?;

        Ok(self.list("column", fields))
    }

    pub(super) fn lower_constant(&mut self, value: &pg::AConst) -> Result<Sexp> {
        if value.isnull {
            return Ok(self.symbol("null"));
        }
        match value.val.as_ref() {
            Some(a_const::Val::Ival(value)) => Ok(self.symbol(&value.ival.to_string())),
            Some(a_const::Val::Fval(value)) => Ok(self.symbol(&value.fval)),
            Some(a_const::Val::Boolval(value)) => Ok(Sexp::boolean(value.boolval)),
            Some(a_const::Val::Sval(value)) => Ok(Sexp::string(value.sval.clone())),
            Some(a_const::Val::Bsval(value)) => {
                Ok(self.list("bit-string", [Sexp::string(value.bsval.clone())]))
            }
            None => self.unsupported("constant without value"),
        }
    }

    pub(super) fn lower_operator(&mut self, expr: &pg::AExpr) -> Result<Sexp> {
        let operator = expr
            .name
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?
            .join(".");
        let kind = pg::AExprKind::try_from(expr.kind)
            .map_err(|_| ParseError::new("unsupported operator expression", 0))?;
        let name = match kind {
            pg::AExprKind::AexprOp => "op",
            pg::AExprKind::AexprOpAny => "any-op",
            pg::AExprKind::AexprOpAll => "all-op",
            pg::AExprKind::AexprIn => "in",
            pg::AExprKind::AexprLike => "like",
            pg::AExprKind::AexprIlike => "ilike",
            pg::AExprKind::AexprDistinct => "is-distinct-from",
            pg::AExprKind::AexprNotDistinct => "is-not-distinct-from",
            pg::AExprKind::AexprNullif => "nullif",
            pg::AExprKind::AexprBetween => "between",
            pg::AExprKind::AexprNotBetween => "not-between",
            pg::AExprKind::AexprBetweenSym => "between-symmetric",
            pg::AExprKind::AexprNotBetweenSym => "not-between-symmetric",
            pg::AExprKind::AexprSimilar => "similar-to",
            _ => return self.unsupported(format!("operator expression {}", kind.as_str_name())),
        };
        if let Some(PgNode::SubLink(link)) = expr.rexpr.as_ref().and_then(|node| node.node.as_ref())
            && matches!(
                kind,
                pg::AExprKind::AexprIn | pg::AExprKind::AexprOpAny | pg::AExprKind::AexprOpAll
            )
        {
            let left_expression_node =
                self.required_node(expr.lexpr.as_deref(), "subquery operator left expression")?;
            let left = self.lower(left_expression_node)?;
            let query = self.lower_sublink_query(link)?;
            return match kind {
                pg::AExprKind::AexprIn => Ok(self.list("in-subquery", [left, query])),
                pg::AExprKind::AexprOpAny => {
                    Ok(self.list("any-subquery", [Sexp::string(operator), left, query]))
                }
                _ => Ok(self.list("all-subquery", [Sexp::string(operator), left, query])),
            };
        }
        if let Some(PgNode::List(values)) = expr.rexpr.as_ref().and_then(|node| node.node.as_ref())
            && let [subquery] = values.items.as_slice()
            && let PgNode::SubLink(link) =
                self.required_kind(subquery.node.as_ref(), "IN subquery")?
            && (matches!(kind, pg::AExprKind::AexprIn)
                || (matches!(kind, pg::AExprKind::AexprOp) && operator == "="))
        {
            let left_expression_node =
                self.required_node(expr.lexpr.as_deref(), "IN subquery left expression")?;
            let in_subquery = [
                self.lower(left_expression_node)?,
                self.lower_sublink_query(link)?,
            ];
            return Ok(self.list("in-subquery", in_subquery));
        }
        if matches!(
            kind,
            pg::AExprKind::AexprBetween
                | pg::AExprKind::AexprNotBetween
                | pg::AExprKind::AexprBetweenSym
                | pg::AExprKind::AexprNotBetweenSym
        ) {
            let between_expression_node =
                self.required_node(expr.lexpr.as_deref(), "BETWEEN expression")?;
            let left = self.lower(between_expression_node)?;
            let bounds = match expr.rexpr.as_ref().and_then(|node| node.node.as_ref()) {
                Some(PgNode::List(bounds)) if bounds.items.len() == 2 => {
                    self.lower_nodes(&bounds.items)?
                }
                _ => return self.unsupported("BETWEEN bounds"),
            };
            return Ok(self.list(name, std::iter::once(left).chain(bounds)));
        }
        if kind == pg::AExprKind::AexprSimilar {
            let to_expression_node =
                self.required_node(expr.lexpr.as_deref(), "SIMILAR TO expression")?;
            let left = self.lower(to_expression_node)?;
            let mut values = vec![self.symbol("similar-to"), left];

            let pattern = self.required_node(expr.rexpr.as_deref(), "SIMILAR TO pattern")?;
            match self.required_kind(pattern.node.as_ref(), "SIMILAR TO pattern")? {
                PgNode::FuncCall(call)
                    if call.funcname.len() == 2
                        && self.string_node(&call.funcname[0])? == "pg_catalog"
                        && self.string_node(&call.funcname[1])? == "similar_to_escape" =>
                {
                    if call.args.len() > 2 {
                        return self.unsupported("SIMILAR TO pattern");
                    }

                    let mut args = call.args.iter();
                    let pattern_expression_node =
                        self.required_node(args.next(), "SIMILAR TO pattern expression")?;
                    values.push(self.lower(pattern_expression_node)?);
                    if let Some(escape) = args.next() {
                        values.push(self.lower(escape)?);
                    }
                }
                _ => values.push(self.lower(pattern)?),
            }
            return Ok(Sexp::list(values));
        }

        let mut values = vec![self.symbol(name)];

        if matches!(name, "op" | "any-op" | "all-op") {
            values.push(Sexp::string(operator));
        }
        if let Some(left) = expr.lexpr.as_ref() {
            values.push(self.lower(left)?);
        }
        if let Some(right) = expr.rexpr.as_ref() {
            values.push(self.lower(right)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_indirection(&mut self, indirection: &pg::AIndirection) -> Result<Sexp> {
        let indirection_argument_node =
            self.required_node(indirection.arg.as_deref(), "indirection argument")?;
        let mut values = vec![
            self.symbol("indirection"),
            self.lower(indirection_argument_node)?,
        ];

        values.extend(
            indirection
                .indirection
                .iter()
                .map(|x| self.lower_indirection_item(x))
                .collect::<Result<Vec<_>>>()?,
        );

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_indirection_item(&mut self, node: &pg::Node) -> Result<Sexp> {
        match self.required_kind(node.node.as_ref(), "indirection item")? {
            PgNode::String(field) => Ok(self.list("field", [Sexp::string(field.sval.clone())])),
            PgNode::AIndices(indices) => self.lower_indices(indices),
            PgNode::AStar(_) => Ok(self.symbol("*")),
            _ => self.unsupported("indirection item"),
        }
    }

    pub(super) fn lower_indices(&mut self, indices: &pg::AIndices) -> Result<Sexp> {
        if indices.is_slice {
            let mut values = vec![self.symbol("slice")];

            if let Some(lower_bound) = indices.lidx.as_deref() {
                values.push(self.lower(lower_bound)?);
            }
            if let Some(upper_bound) = indices.uidx.as_deref() {
                values.push(self.lower(upper_bound)?);
            }
            return Ok(Sexp::list(values));
        }

        if indices.lidx.is_some() || indices.uidx.is_none() {
            return self.unsupported("array index");
        }

        let index = self.required_node(indices.uidx.as_deref(), "array index")?;
        let lowered = [self.lower(index)?];

        Ok(self.list("index", lowered))
    }

    pub(super) fn lower_case(&mut self, expr: &pg::CaseExpr) -> Result<Sexp> {
        let mut values = vec![self.symbol("case")];

        if let Some(argument) = expr.arg.as_deref() {
            let arg = [self.lower(argument)?];
            values.push(self.list("argument", arg));
        }

        values.extend(self.lower_nodes(&expr.args)?);
        if let Some(default) = expr.defresult.as_deref() {
            let els = [self.lower(default)?];
            values.push(self.list("else", els));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_case_when(&mut self, expr: &pg::CaseWhen) -> Result<Sexp> {
        let result_node = self.required_node(expr.result.as_deref(), "CASE WHEN result")?;
        let when_condition = self.required_node(expr.expr.as_deref(), "CASE WHEN condition")?;
        let when = [self.lower(when_condition)?, self.lower(result_node)?];
        Ok(self.list("when", when))
    }

    pub(super) fn lower_binary_expression(
        &mut self,
        name: &str,
        arguments: &[pg::Node],
    ) -> Result<Sexp> {
        let [left, right] = arguments else {
            return self.unsupported(format!("{name} argument count"));
        };
        let operands = [self.lower(left)?, self.lower(right)?];
        Ok(self.list(name, operands))
    }

    pub(super) fn lower_scalar_array_op(&mut self, expr: &pg::ScalarArrayOpExpr) -> Result<Sexp> {
        let name = if expr.use_or {
            "any-array"
        } else {
            "all-array"
        };
        self.lower_binary_expression(name, &expr.args)
    }

    pub(super) fn lower_row_compare(&mut self, expr: &pg::RowCompareExpr) -> Result<Sexp> {
        if expr.largs.is_empty() || expr.largs.len() != expr.rargs.len() {
            return self.unsupported("row comparison arguments");
        }

        let comparison = match pg::CompareType::try_from(expr.cmptype) {
            Ok(pg::CompareType::CompareLt) => "lt",
            Ok(pg::CompareType::CompareLe) => "le",
            Ok(pg::CompareType::CompareEq) => "eq",
            Ok(pg::CompareType::CompareGe) => "ge",
            Ok(pg::CompareType::CompareGt) => "gt",
            Ok(pg::CompareType::CompareNe) => "ne",
            Ok(pg::CompareType::CompareOverlap) => "overlaps",
            Ok(pg::CompareType::CompareContainedBy) => "contained-by",
            _ => return self.unsupported("row comparison type"),
        };
        let right = self.lower_nodes(&expr.rargs)?;
        let left = self.lower_nodes(&expr.largs)?;
        let row_compare = [
            self.symbol(comparison),
            self.list("left", left),
            self.list("right", right),
        ];

        Ok(self.list("row-compare", row_compare))
    }

    pub(super) fn lower_sublink(&mut self, link: &pg::SubLink) -> Result<Sexp> {
        let query = self.lower_sublink_query(link)?;
        match pg::SubLinkType::try_from(link.sub_link_type) {
            Ok(pg::SubLinkType::ExistsSublink) => Ok(self.list("exists", [query])),
            Ok(pg::SubLinkType::ExprSublink) => Ok(self.list("scalar-subquery", [query])),
            Ok(pg::SubLinkType::ArraySublink) => Ok(self.list("array-subquery", [query])),
            Ok(pg::SubLinkType::AnySublink) if link.oper_name.is_empty() => {
                let test_expression_node =
                    self.required_node(link.testexpr.as_deref(), "IN subquery test expression")?;
                let in_subquery = [self.lower(test_expression_node)?, query];
                Ok(self.list("in-subquery", in_subquery))
            }
            Ok(pg::SubLinkType::AnySublink) => self.lower_test_sublink("any-subquery", link, query),
            Ok(pg::SubLinkType::AllSublink) => self.lower_test_sublink("all-subquery", link, query),
            Ok(pg::SubLinkType::RowcompareSublink) => {
                self.lower_test_sublink("row-compare-subquery", link, query)
            }
            _ => self.unsupported("subquery type"),
        }
    }

    pub(super) fn lower_sublink_query(&mut self, link: &pg::SubLink) -> Result<Sexp> {
        let subquery_node = self.required_node(link.subselect.as_deref(), "subquery")?;
        self.lower(subquery_node)
    }

    pub(super) fn lower_test_sublink(
        &mut self,
        name: &str,
        link: &pg::SubLink,
        query: Sexp,
    ) -> Result<Sexp> {
        let operator = link
            .oper_name
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?
            .join(".");
        if operator.is_empty() {
            return self.unsupported("subquery operator");
        }

        let test_expression_node =
            self.required_node(link.testexpr.as_deref(), "subquery test expression")?;
        let sublink_values = [
            Sexp::string(operator),
            self.lower(test_expression_node)?,
            query,
        ];

        Ok(self.list(name, sublink_values))
    }

    pub(super) fn lower_boolean(&mut self, expr: &pg::BoolExpr) -> Result<Sexp> {
        let name = match pg::BoolExprType::try_from(expr.boolop) {
            Ok(pg::BoolExprType::AndExpr) => "and",
            Ok(pg::BoolExprType::OrExpr) => "or",
            Ok(pg::BoolExprType::NotExpr) => "not",
            _ => return self.unsupported("boolean expression"),
        };
        let args = self.lower_nodes(&expr.args)?;
        Ok(self.list(name, args))
    }

    pub(super) fn lower_null_test(&mut self, test: &pg::NullTest) -> Result<Sexp> {
        if test.argisrow {
            return self.unsupported("row null test");
        }

        let name = match pg::NullTestType::try_from(test.nulltesttype) {
            Ok(pg::NullTestType::IsNull) => "is-null",
            Ok(pg::NullTestType::IsNotNull) => "is-not-null",
            _ => return self.unsupported("null test"),
        };
        let test_argument_node = self.required_node(test.arg.as_deref(), "null test argument")?;
        let null_test_argument = [self.lower(test_argument_node)?];

        Ok(self.list(name, null_test_argument))
    }

    pub(super) fn lower_boolean_test(&mut self, test: &pg::BooleanTest) -> Result<Sexp> {
        let name = match pg::BoolTestType::try_from(test.booltesttype) {
            Ok(pg::BoolTestType::IsTrue) => "is-true",
            Ok(pg::BoolTestType::IsNotTrue) => "is-not-true",
            Ok(pg::BoolTestType::IsFalse) => "is-false",
            Ok(pg::BoolTestType::IsNotFalse) => "is-not-false",
            Ok(pg::BoolTestType::IsUnknown) => "is-unknown",
            Ok(pg::BoolTestType::IsNotUnknown) => "is-not-unknown",
            _ => return self.unsupported("boolean test"),
        };
        let test_argument_node =
            self.required_node(test.arg.as_deref(), "boolean test argument")?;
        let boolean_test_argument = [self.lower(test_argument_node)?];

        Ok(self.list(name, boolean_test_argument))
    }

    pub(super) fn lower_function(&mut self, call: &pg::FuncCall) -> Result<Sexp> {
        let name = call
            .funcname
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?
            .join(".");
        let mut values = vec![self.symbol("call"), Sexp::string(name)];

        if call.agg_star && call.agg_distinct {
            return self.unsupported("DISTINCT aggregate star");
        }

        if call.agg_star {
            values.push(self.symbol("*"));
        } else {
            let mut arguments = call
                .args
                .iter()
                .map(|x| self.lower(x))
                .collect::<Result<Vec<_>>>()?;
            if call.func_variadic {
                let last = match arguments.pop() {
                    Some(last) => last,
                    None => return self.unsupported("VARIADIC call without arguments"),
                };
                arguments.push(self.list("variadic", [last]));
            }
            if call.agg_distinct {
                values.push(self.list("distinct", arguments));
            } else {
                values.extend(arguments);
            }
        }
        if call.agg_within_group {
            if call.agg_order.is_empty() {
                return self.unsupported("ordered-set aggregate without ORDER BY");
            }

            values.push(self.symbol("within-group"));
        }
        if !call.agg_order.is_empty() {
            let order_by = self.lower_nodes(&call.agg_order)?;
            values.push(self.list("order-by", order_by));
        }
        if let Some(filter) = call.agg_filter.as_deref() {
            let lowered_filter = [self.lower(filter)?];
            values.push(self.list("filter", lowered_filter));
        }
        if let Some(window) = call.over.as_ref() {
            let over = self.lower_over_items(window)?;
            values.push(self.list("over", over));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_min_max(&mut self, expr: &pg::MinMaxExpr) -> Result<Sexp> {
        let name = match pg::MinMaxOp::try_from(expr.op) {
            Ok(pg::MinMaxOp::IsGreatest) => "greatest",
            Ok(pg::MinMaxOp::IsLeast) => "least",
            _ => return self.unsupported("GREATEST/LEAST operator"),
        };
        let mut values = vec![self.symbol("call"), Sexp::string(name)];

        values.extend(self.lower_nodes(&expr.args)?);
        Ok(Sexp::list(values))
    }

    pub(super) fn lower_window(&mut self, window: &pg::WindowDef) -> Result<Sexp> {
        let lowered_window = self.lower_window_items(window)?;
        Ok(self.list("window", lowered_window))
    }

    pub(super) fn lower_over_items(&mut self, window: &pg::WindowDef) -> Result<Vec<Sexp>> {
        if window.refname.is_empty() && !window.name.is_empty() {
            return self.lower_window_items(&pg::WindowDef {
                name: String::new(),
                refname: window.name.clone(),
                ..window.clone()
            });
        }
        self.lower_window_items(window)
    }

    pub(super) fn lower_window_items(&mut self, window: &pg::WindowDef) -> Result<Vec<Sexp>> {
        let mut values = Vec::new();
        let partition_by = self.lower_nodes(&window.partition_clause)?;
        if !window.name.is_empty() {
            values.push(self.list("name", [Sexp::string(window.name.clone())]));
        }
        if !window.refname.is_empty() {
            values.push(self.list("window", [Sexp::string(window.refname.clone())]));
        }
        self.optional_clause(&mut values, "partition-by", partition_by);
        let order_by = self.lower_nodes(&window.order_clause)?;
        self.optional_clause(&mut values, "order-by", order_by);
        if window.frame_options != DEFAULT_WINDOW_FRAME_OPTIONS
            || window.start_offset.is_some()
            || window.end_offset.is_some()
        {
            values.push(self.lower_window_frame(window)?);
        }

        Ok(values)
    }

    pub(super) fn lower_window_frame(&mut self, window: &pg::WindowDef) -> Result<Sexp> {
        use WindowFrameBound::{
            CurrentRow, Following, Preceding, UnboundedFollowing, UnboundedPreceding,
        };
        let mode = match window.frame_options & (FRAME_RANGE | FRAME_ROWS | FRAME_GROUPS) {
            FRAME_RANGE => "range",
            FRAME_ROWS => "rows",
            FRAME_GROUPS => "groups",
            _ => return self.unsupported("window frame mode"),
        };
        let start = match window.frame_options & FRAME_START_BOUNDS {
            FRAME_START_UNBOUNDED_PRECEDING => UnboundedPreceding,
            FRAME_START_CURRENT_ROW => CurrentRow,
            FRAME_START_OFFSET_PRECEDING => Preceding,
            FRAME_START_OFFSET_FOLLOWING => Following,
            _ => return self.unsupported("window frame start bound"),
        };
        let start = self.lower_window_bound(start, window.start_offset.as_deref(), "start")?;
        let mut values = vec![self.symbol("frame"), self.symbol(mode)];

        if window.frame_options & FRAME_BETWEEN != 0 {
            let end = match window.frame_options & FRAME_END_BOUNDS {
                FRAME_END_UNBOUNDED_PRECEDING => UnboundedPreceding,
                FRAME_END_UNBOUNDED_FOLLOWING => UnboundedFollowing,
                FRAME_END_CURRENT_ROW => CurrentRow,
                FRAME_END_OFFSET_PRECEDING => Preceding,
                FRAME_END_OFFSET_FOLLOWING => Following,
                _ => return self.unsupported("window frame end bound"),
            };
            let end = self.lower_window_bound(end, window.end_offset.as_deref(), "end")?;
            values.push(self.list("between", [start, end]));
        } else {
            values.push(start);
        }
        match window.frame_options & FRAME_EXCLUSIONS {
            0 => {}
            FRAME_EXCLUDE_CURRENT_ROW => {
                let current_row_exclude = [self.symbol("current-row")];
                values.push(self.list("exclude", current_row_exclude))
            }
            FRAME_EXCLUDE_GROUP => {
                let group_exclude = [self.symbol("group")];
                values.push(self.list("exclude", group_exclude))
            }
            FRAME_EXCLUDE_TIES => {
                let ties_exclude = [self.symbol("ties")];
                values.push(self.list("exclude", ties_exclude))
            }
            _ => return self.unsupported("window frame exclusion"),
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_window_bound(
        &mut self,
        bound: WindowFrameBound,
        offset: Option<&pg::Node>,
        side: &str,
    ) -> Result<Sexp> {
        let name = match bound {
            WindowFrameBound::UnboundedPreceding => "unbounded-preceding",
            WindowFrameBound::UnboundedFollowing => "unbounded-following",
            WindowFrameBound::CurrentRow => "current-row",
            WindowFrameBound::Preceding => "preceding",
            WindowFrameBound::Following => "following",
        };
        match bound {
            WindowFrameBound::Preceding | WindowFrameBound::Following => {
                let offset_node =
                    self.required_node(offset, &format!("window frame {side} offset"))?;
                let offset = self.lower(offset_node)?;
                Ok(self.list(name, [offset]))
            }
            _ => Ok(Sexp::list([self.symbol(name)])),
        }
    }

    pub(super) fn lower_sort(&mut self, sort: &pg::SortBy) -> Result<Sexp> {
        let direction = if sort.use_op.is_empty() {
            match pg::SortByDir::try_from(sort.sortby_dir) {
                Ok(pg::SortByDir::SortbyDefault | pg::SortByDir::SortbyAsc) => self.symbol("asc"),
                Ok(pg::SortByDir::SortbyDesc) => self.symbol("desc"),
                _ => return self.unsupported("ORDER BY direction"),
            }
        } else {
            let operator = sort
                .use_op
                .iter()
                .map(|x| self.string_node(x))
                .collect::<Result<Vec<_>>>()?
                .join(".");
            self.list("using", [Sexp::string(operator)])
        };
        let nulls = match pg::SortByNulls::try_from(sort.sortby_nulls) {
            Ok(pg::SortByNulls::SortbyNullsDefault) => None,
            Ok(pg::SortByNulls::SortbyNullsFirst) => Some("nulls-first"),
            Ok(pg::SortByNulls::SortbyNullsLast) => Some("nulls-last"),
            _ => return self.unsupported("ORDER BY null ordering"),
        };
        let sort_expression_node = self.required_node(sort.node.as_deref(), "sort expression")?;
        let mut values = vec![
            self.symbol("sort"),
            self.lower(sort_expression_node)?,
            direction,
        ];

        if let Some(nulls) = nulls {
            values.push(self.symbol(nulls));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_sqlvalue_function(
        &mut self,
        function: &pg::SqlValueFunction,
    ) -> Result<Sexp> {
        let op = pg::SqlValueFunctionOp::try_from(function.op)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: SQL value function", 0))?;
        let name = match op {
            pg::SqlValueFunctionOp::SvfopCurrentDate => "current-date",
            pg::SqlValueFunctionOp::SvfopCurrentTime
            | pg::SqlValueFunctionOp::SvfopCurrentTimeN => "current-time",
            pg::SqlValueFunctionOp::SvfopCurrentTimestamp
            | pg::SqlValueFunctionOp::SvfopCurrentTimestampN => "current-timestamp",
            pg::SqlValueFunctionOp::SvfopLocaltime | pg::SqlValueFunctionOp::SvfopLocaltimeN => {
                "localtime"
            }
            pg::SqlValueFunctionOp::SvfopLocaltimestamp
            | pg::SqlValueFunctionOp::SvfopLocaltimestampN => "local-timestamp",
            pg::SqlValueFunctionOp::SvfopCurrentRole => "current-role",
            pg::SqlValueFunctionOp::SvfopCurrentUser => "current-user",
            pg::SqlValueFunctionOp::SvfopUser => "user",
            pg::SqlValueFunctionOp::SvfopSessionUser => "session-user",
            pg::SqlValueFunctionOp::SvfopCurrentCatalog => "current-catalog",
            pg::SqlValueFunctionOp::SvfopCurrentSchema => "current-schema",
            _ => return self.unsupported("SQL value function"),
        };
        let mut values = vec![self.symbol(name)];

        if function.typmod >= 0 {
            let precision = [self.symbol(&function.typmod.to_string())];
            values.push(self.list("precision", precision));
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_collate(&mut self, clause: &pg::CollateClause) -> Result<Sexp> {
        let name = clause
            .collname
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?
            .join(".");
        let collate_argument = self.required_node(clause.arg.as_deref(), "COLLATE argument")?;
        let collate = [self.lower(collate_argument)?, Sexp::string(name)];
        Ok(self.list("collate", collate))
    }

    pub(super) fn lower_grouping_set(&mut self, set: &pg::GroupingSet) -> Result<Sexp> {
        let kind = pg::GroupingSetKind::try_from(set.kind)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: grouping set", 0))?;
        match kind {
            pg::GroupingSetKind::GroupingSetRollup => {
                let rollup = self.lower_nodes(&set.content)?;
                Ok(self.list("rollup", rollup))
            }
            pg::GroupingSetKind::GroupingSetCube => {
                let cube = self.lower_nodes(&set.content)?;
                Ok(self.list("cube", cube))
            }
            pg::GroupingSetKind::GroupingSetSimple => {
                let grouping_set = self.lower_nodes(&set.content)?;
                Ok(self.list("grouping-set", grouping_set))
            }
            pg::GroupingSetKind::GroupingSetEmpty => Ok(Sexp::list([self.symbol("grouping-set")])),
            pg::GroupingSetKind::GroupingSetSets => {
                let grouping_sets = set
                    .content
                    .iter()
                    .map(|node| {
                        match self.required_kind(node.node.as_ref(), "grouping set element")? {
                            PgNode::GroupingSet(inner) => self.lower_grouping_set(inner),
                            PgNode::List(values) => {
                                let grouping_set = self.lower_nodes(&values.items)?;
                                Ok(self.list("grouping-set", grouping_set))
                            }
                            _ => {
                                let grouping_set = [self.lower(node)?];
                                Ok(self.list("grouping-set", grouping_set))
                            }
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(self.list("grouping-sets", grouping_sets))
            }
            _ => self.unsupported("grouping set"),
        }
    }
}
