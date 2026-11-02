use super::*;

impl PostgresParser {
    pub(super) fn lower_json_format(&mut self, format: &pg::JsonFormat) -> Result<Option<Sexp>> {
        let kind = match pg::JsonFormatType::try_from(format.format_type) {
            Ok(pg::JsonFormatType::Undefined | pg::JsonFormatType::JsFormatDefault) => {
                return Ok(None);
            }
            Ok(pg::JsonFormatType::JsFormatJson) => "json",
            Ok(pg::JsonFormatType::JsFormatJsonb) => "jsonb",
            _ => return self.unsupported_error("JSON format type"),
        };
        let encoding = match pg::JsonEncoding::try_from(format.encoding) {
            Ok(pg::JsonEncoding::Undefined | pg::JsonEncoding::JsEncDefault) => None,
            Ok(pg::JsonEncoding::JsEncUtf8) => Some("utf8"),
            Ok(pg::JsonEncoding::JsEncUtf16) => Some("utf16"),
            Ok(pg::JsonEncoding::JsEncUtf32) => Some("utf32"),
            _ => return self.unsupported_error("JSON encoding"),
        };
        let mut values = vec![self.symbol(kind)];

        if let Some(encoding) = encoding {
            values.push(self.symbol(encoding));
        }

        Ok(Some(self.list("format", values)))
    }

    pub(super) fn lower_json_returning(&mut self, returning: &pg::JsonReturning) -> Result<Sexp> {
        let mut values = Vec::new();
        if returning.typid != 0 {
            let oid = [self.symbol(&returning.typid.to_string())];
            values.push(self.list("oid", oid));
        }
        if returning.typmod != 0 && returning.typmod != -1 {
            let typmod = [self.symbol(&returning.typmod.to_string())];
            values.push(self.list("typmod", typmod));
        }
        if let Some(format) = returning
            .format
            .as_ref()
            .map(|x| self.lower_json_format(x))
            .transpose()?
            .flatten()
        {
            values.push(format);
        }

        Ok(self.list("returning", values))
    }

    pub(super) fn lower_json_output(&mut self, output: &pg::JsonOutput) -> Result<Sexp> {
        let returning_type = self.required(output.type_name.as_ref(), "RETURNING type")?;
        let mut values = vec![self.lower_type(returning_type)?];

        if let Some(format) = output
            .returning
            .as_ref()
            .and_then(|returning| returning.format.as_ref())
            .map(|x| self.lower_json_format(x))
            .transpose()?
            .flatten()
        {
            values.push(format);
        }

        Ok(self.list("returning", values))
    }

    pub(super) fn lower_json_behavior(&mut self, behavior: &pg::JsonBehavior) -> Result<Sexp> {
        let name = match pg::JsonBehaviorType::try_from(behavior.btype) {
            Ok(pg::JsonBehaviorType::JsonBehaviorNull) => "null",
            Ok(pg::JsonBehaviorType::JsonBehaviorError) => "error",
            Ok(pg::JsonBehaviorType::JsonBehaviorEmpty) => "empty",
            Ok(pg::JsonBehaviorType::JsonBehaviorTrue) => "true",
            Ok(pg::JsonBehaviorType::JsonBehaviorFalse) => "false",
            Ok(pg::JsonBehaviorType::JsonBehaviorUnknown) => "unknown",
            Ok(pg::JsonBehaviorType::JsonBehaviorEmptyArray) => "empty-array",
            Ok(pg::JsonBehaviorType::JsonBehaviorEmptyObject) => "empty-object",
            Ok(pg::JsonBehaviorType::JsonBehaviorDefault) => "default",
            _ => return self.unsupported_error("JSON behavior type"),
        };
        let mut values = vec![self.symbol(name)];

        if let Some(expr) = behavior.expr.as_deref() {
            values.push(self.lower(expr)?);
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_json_value_expr(&mut self, expr: &pg::JsonValueExpr) -> Result<Sexp> {
        let value_expression_node =
            self.required_node(expr.raw_expr.as_deref(), "JSON value expression")?;
        let expression = self.lower(value_expression_node)?;
        let format = expr
            .format
            .as_ref()
            .map(|x| self.lower_json_format(x))
            .transpose()?
            .flatten();
        match (expression, format) {
            (expression, Some(format)) => Ok(Sexp::list([expression, format])),
            (expression, None) => Ok(expression),
        }
    }

    pub(super) fn lower_json_key_value(&mut self, pair: &pg::JsonKeyValue) -> Result<Sexp> {
        let object_value = self.required(pair.value.as_ref(), "JSON object value")?;
        let json_value = [self.lower_json_value_expr(object_value)?];
        let key_node = self.required_node(pair.key.as_deref(), "JSON object key")?;
        let key = [self.lower(key_node)?];
        let entry = [self.list("key", key), self.list("value", json_value)];
        Ok(self.list("entry", entry))
    }

    pub(super) fn lower_json_argument(&mut self, argument: &pg::JsonArgument) -> Result<Sexp> {
        let passing_argument = self.required(argument.val.as_ref(), "JSON passing argument")?;
        let arg = [
            Sexp::string(argument.name.clone()),
            self.lower_json_value_expr(passing_argument)?,
        ];
        Ok(self.list("arg", arg))
    }

    pub(super) fn lower_json_wrapper(&mut self, wrapper: i32) -> Result<Option<Sexp>> {
        let name = match pg::JsonWrapper::try_from(wrapper) {
            Ok(pg::JsonWrapper::Undefined | pg::JsonWrapper::JswUnspec) => {
                return Ok(None);
            }
            Ok(pg::JsonWrapper::JswNone) => "without-wrapper",
            Ok(pg::JsonWrapper::JswConditional) => "with-conditional-wrapper",
            Ok(pg::JsonWrapper::JswUnconditional) => "with-unconditional-wrapper",
            _ => return self.unsupported_error("JSON wrapper"),
        };

        Ok(Some(self.symbol(name)))
    }

    pub(super) fn lower_json_quotes(&mut self, quotes: i32) -> Result<Option<Sexp>> {
        let name = match pg::JsonQuotes::try_from(quotes) {
            Ok(pg::JsonQuotes::Undefined | pg::JsonQuotes::JsQuotesUnspec) => {
                return Ok(None);
            }
            Ok(pg::JsonQuotes::JsQuotesKeep) => "keep-quotes",
            Ok(pg::JsonQuotes::JsQuotesOmit) => "omit-quotes",
            _ => return self.unsupported_error("JSON quotes"),
        };
        Ok(Some(self.symbol(name)))
    }

    pub(super) fn lower_json_parse_expr(&mut self, expr: &pg::JsonParseExpr) -> Result<Sexp> {
        let parse_expression = self.required(expr.expr.as_ref(), "JSON_PARSE expression")?;
        let mut values = vec![self.lower_json_value_expr(parse_expression)?];

        if expr.unique_keys {
            values.push(self.symbol("unique-keys"));
        }
        if let Some(output) = expr.output.as_ref() {
            values.push(self.lower_json_output(output)?);
        }
        Ok(self.list("json-parse", values))
    }

    pub(super) fn lower_json_scalar_expr(&mut self, expr: &pg::JsonScalarExpr) -> Result<Sexp> {
        let scalar_expression_node =
            self.required_node(expr.expr.as_deref(), "JSON_SCALAR expression")?;
        let mut values = vec![self.lower(scalar_expression_node)?];

        if let Some(output) = expr.output.as_ref() {
            values.push(self.lower_json_output(output)?);
        }

        Ok(self.list("json-scalar", values))
    }

    pub(super) fn lower_json_serialize_expr(
        &mut self,
        expr: &pg::JsonSerializeExpr,
    ) -> Result<Sexp> {
        let serialize_expression =
            self.required(expr.expr.as_ref(), "JSON_SERIALIZE expression")?;
        let mut values = vec![self.lower_json_value_expr(serialize_expression)?];

        if let Some(output) = expr.output.as_ref() {
            values.push(self.lower_json_output(output)?);
        }

        Ok(self.list("json-serialize", values))
    }

    pub(super) fn lower_json_object_constructor(
        &mut self,
        constructor: &pg::JsonObjectConstructor,
    ) -> Result<Sexp> {
        let mut values = self.lower_nodes(&constructor.exprs)?;
        if constructor.absent_on_null {
            values.push(self.symbol("absent-on-null"));
        }
        if constructor.unique {
            values.push(self.symbol("unique-keys"));
        }
        if let Some(output) = constructor.output.as_ref() {
            values.push(self.lower_json_output(output)?);
        }

        Ok(self.list("json-object", values))
    }

    pub(super) fn lower_json_array_constructor(
        &mut self,
        constructor: &pg::JsonArrayConstructor,
    ) -> Result<Sexp> {
        let items = self.lower_nodes(&constructor.exprs)?;
        let mut values = vec![self.list("items", items)];

        if constructor.absent_on_null {
            values.push(self.symbol("absent-on-null"));
        }
        if let Some(output) = constructor.output.as_ref() {
            values.push(self.lower_json_output(output)?);
        }

        Ok(self.list("json-array", values))
    }

    pub(super) fn lower_json_array_query_constructor(
        &mut self,
        constructor: &pg::JsonArrayQueryConstructor,
    ) -> Result<Sexp> {
        let array_query_node =
            self.required_node(constructor.query.as_deref(), "JSON array query")?;
        let query = [self.lower(array_query_node)?];
        let mut values = vec![self.list("query", query)];

        if let Some(format) = constructor
            .format
            .as_ref()
            .map(|x| self.lower_json_format(x))
            .transpose()?
            .flatten()
        {
            values.push(format);
        }
        if constructor.absent_on_null {
            values.push(self.symbol("absent-on-null"));
        }
        if let Some(output) = constructor.output.as_ref() {
            values.push(self.lower_json_output(output)?);
        }

        Ok(self.list("json-array", values))
    }

    pub(super) fn lower_json_constructor_expr(
        &mut self,
        expr: &pg::JsonConstructorExpr,
    ) -> Result<Sexp> {
        let name = match pg::JsonConstructorType::try_from(expr.r#type) {
            Ok(pg::JsonConstructorType::JsctorJsonObject) => "json-object",
            Ok(pg::JsonConstructorType::JsctorJsonArray) => "json-array",
            Ok(pg::JsonConstructorType::JsctorJsonParse) => "json-parse",
            Ok(pg::JsonConstructorType::JsctorJsonScalar) => "json-scalar",
            Ok(pg::JsonConstructorType::JsctorJsonSerialize) => "json-serialize",
            _ => return self.unsupported("analyzed JSON aggregate constructor"),
        };
        let mut values = if name == "json-object" {
            let arguments = self.lower_nodes(&expr.args)?;
            if arguments.len() % 2 != 0 {
                return self.unsupported("analyzed JSON_OBJECT arguments");
            }
            arguments
                .chunks(2)
                .map(|entry| {
                    let entry = [
                        self.list("key", [entry[0].clone()]),
                        self.list("value", [entry[1].clone()]),
                    ];
                    self.list("entry", entry)
                })
                .collect()
        } else if name == "json-array" {
            let items = self.lower_nodes(&expr.args)?;
            vec![self.list("items", items)]
        } else {
            let constructor_argument_node =
                self.required_node(expr.args.first(), "analyzed JSON constructor argument")?;
            let argument = self.lower(constructor_argument_node)?;
            vec![argument]
        };
        if expr.absent_on_null {
            values.push(self.symbol("absent-on-null"));
        }
        if expr.unique {
            values.push(self.symbol("unique-keys"));
        }
        if let Some(returning) = expr.returning.as_ref() {
            values.push(self.lower_json_returning(returning)?);
        }

        Ok(self.list(name, values))
    }

    pub(super) fn lower_json_aggregate_constructor(
        &mut self,
        constructor: &pg::JsonAggConstructor,
    ) -> Result<Vec<Sexp>> {
        let mut values = Vec::new();
        if !constructor.agg_order.is_empty() {
            let order_by = self.lower_nodes(&constructor.agg_order)?;
            values.push(self.list("order-by", order_by));
        }
        if let Some(filter) = constructor.agg_filter.as_deref() {
            let lowered_filter = [self.lower(filter)?];
            values.push(self.list("filter", lowered_filter));
        }
        if let Some(window) = constructor.over.as_ref() {
            let over = self.lower_over_items(window)?;
            values.push(self.list("over", over));
        }
        if let Some(output) = constructor.output.as_ref() {
            values.push(self.lower_json_output(output)?);
        }

        Ok(values)
    }

    pub(super) fn lower_json_array_agg(&mut self, aggregate: &pg::JsonArrayAgg) -> Result<Sexp> {
        let arrayagg_argument = self.required(aggregate.arg.as_ref(), "JSON_ARRAYAGG argument")?;
        let agg = [self.lower_json_value_expr(arrayagg_argument)?];
        let mut values = vec![self.list("agg", agg)];

        let constructor =
            self.required(aggregate.constructor.as_ref(), "JSON_ARRAYAGG constructor")?;
        values.extend(self.lower_json_aggregate_constructor(constructor)?);
        if aggregate.absent_on_null {
            values.push(self.symbol("absent-on-null"));
        }

        Ok(self.list("json-arrayagg", values))
    }

    pub(super) fn lower_json_object_agg(&mut self, aggregate: &pg::JsonObjectAgg) -> Result<Sexp> {
        let objectagg_argument =
            self.required(aggregate.arg.as_ref(), "JSON_OBJECTAGG argument")?;
        let agg = [self.lower_json_key_value(objectagg_argument)?];
        let mut values = vec![self.list("agg", agg)];

        let constructor =
            self.required(aggregate.constructor.as_ref(), "JSON_OBJECTAGG constructor")?;
        values.extend(self.lower_json_aggregate_constructor(constructor)?);
        if aggregate.absent_on_null {
            values.push(self.symbol("absent-on-null"));
        }
        if aggregate.unique {
            values.push(self.symbol("unique-keys"));
        }

        Ok(self.list("json-objectagg", values))
    }

    pub(super) fn lower_json_func_expr(&mut self, expr: &pg::JsonFuncExpr) -> Result<Sexp> {
        let name = match pg::JsonExprOp::try_from(expr.op) {
            Ok(pg::JsonExprOp::JsonExistsOp) => "json-exists",
            Ok(pg::JsonExprOp::JsonQueryOp) => "json-query",
            Ok(pg::JsonExprOp::JsonValueOp) => "json-value",
            _ => return self.unsupported("JSON function expression"),
        };
        let specification_node =
            self.required_node(expr.pathspec.as_deref(), "JSON path specification")?;
        let item = self.required(expr.context_item.as_ref(), "JSON context item")?;
        let mut values = vec![
            self.lower_json_value_expr(item)?,
            self.lower(specification_node)?,
        ];

        let passing = self.lower_nodes(&expr.passing)?;
        self.optional_clause(&mut values, "passing", passing);
        if let Some(output) = expr.output.as_ref() {
            values.push(self.lower_json_output(output)?);
        }
        if let Some(wrapper) = self.lower_json_wrapper(expr.wrapper)? {
            values.push(wrapper);
        }
        if let Some(quotes) = self.lower_json_quotes(expr.quotes)? {
            values.push(quotes);
        }
        if let Some(on_empty) = expr.on_empty.as_ref() {
            let lowered_on_empty = [self.lower_json_behavior(on_empty)?];
            values.push(self.list("on-empty", lowered_on_empty));
        }
        if let Some(on_error) = expr.on_error.as_ref() {
            let lowered_on_error = [self.lower_json_behavior(on_error)?];
            values.push(self.list("on-error", lowered_on_error));
        }

        Ok(self.list(name, values))
    }

    pub(super) fn lower_json_expr(&mut self, expr: &pg::JsonExpr) -> Result<Sexp> {
        let name = match pg::JsonExprOp::try_from(expr.op) {
            Ok(pg::JsonExprOp::JsonExistsOp) => "json-exists",
            Ok(pg::JsonExprOp::JsonQueryOp) => "json-query",
            Ok(pg::JsonExprOp::JsonValueOp) => "json-value",
            _ => return self.unsupported("analyzed JSON expression"),
        };
        let specification_node =
            self.required_node(expr.path_spec.as_deref(), "JSON path specification")?;
        let json_expression_node =
            self.required_node(expr.formatted_expr.as_deref(), "JSON expression")?;
        let mut values = vec![
            self.lower(json_expression_node)?,
            self.lower(specification_node)?,
        ];

        let mut passing = Vec::new();

        if expr.passing_names.len() != expr.passing_values.len() {
            return self.unsupported_error("JSON passing arguments");
        }

        for (name, value) in expr.passing_names.iter().zip(&expr.passing_values) {
            let json_argument = [Sexp::string(self.string_node(name)?), self.lower(value)?];
            passing.push(self.list("arg", json_argument));
        }
        self.optional_clause(&mut values, "passing", passing);
        if let Some(returning) = expr.returning.as_ref() {
            values.push(self.lower_json_returning(returning)?);
        }
        if let Some(wrapper) = self.lower_json_wrapper(expr.wrapper)? {
            values.push(wrapper);
        }
        if expr.omit_quotes {
            values.push(self.symbol("omit-quotes"));
        }
        if let Some(on_empty) = expr.on_empty.as_ref() {
            let lowered_on_empty = [self.lower_json_behavior(on_empty)?];
            values.push(self.list("on-empty", lowered_on_empty));
        }
        if let Some(on_error) = expr.on_error.as_ref() {
            let lowered_on_error = [self.lower_json_behavior(on_error)?];
            values.push(self.list("on-error", lowered_on_error));
        }

        Ok(self.list(name, values))
    }

    pub(super) fn lower_json_is_predicate(
        &mut self,
        predicate: &pg::JsonIsPredicate,
    ) -> Result<Sexp> {
        let json_expression_node =
            self.required_node(predicate.expr.as_deref(), "IS JSON expression")?;
        let mut values = vec![self.lower(json_expression_node)?];

        match pg::JsonValueType::try_from(predicate.item_type) {
            Ok(pg::JsonValueType::Undefined | pg::JsonValueType::JsTypeAny) => {}
            Ok(pg::JsonValueType::JsTypeObject) => values.push(self.symbol("object")),
            Ok(pg::JsonValueType::JsTypeArray) => values.push(self.symbol("array")),
            Ok(pg::JsonValueType::JsTypeScalar) => values.push(self.symbol("scalar")),
            _ => return self.unsupported_error("IS JSON item type"),
        }
        if let Some(format) = predicate
            .format
            .as_ref()
            .map(|x| self.lower_json_format(x))
            .transpose()?
            .flatten()
        {
            values.push(format);
        }
        if predicate.unique_keys {
            values.push(self.symbol("with-unique-keys"));
        }

        Ok(self.list("is-json", values))
    }

    pub(super) fn lower_json_table_path_spec(
        &mut self,
        spec: &pg::JsonTablePathSpec,
    ) -> Result<Sexp> {
        let string_node = self.required_node(spec.string.as_deref(), "JSON_TABLE path string")?;
        let mut values = vec![self.lower(string_node)?];

        if !spec.name.is_empty() {
            values.push(self.list("name", [Sexp::string(spec.name.clone())]));
        }
        Ok(self.list("path", values))
    }

    pub(super) fn lower_json_table_columns(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        nodes
            .iter()
            .map(
                |node| match self.required_kind(node.node.as_ref(), "JSON_TABLE column")? {
                    PgNode::JsonTableColumn(column) => self.lower_json_table_column(column),
                    _ => self.unsupported_error("JSON_TABLE column"),
                },
            )
            .collect()
    }

    pub(super) fn lower_json_table(&mut self, table: &pg::JsonTable) -> Result<Sexp> {
        let path = self.required(table.pathspec.as_ref(), "JSON_TABLE path")?;
        let item = self.required(table.context_item.as_ref(), "JSON_TABLE context item")?;
        let mut values = vec![
            self.lower_json_value_expr(item)?,
            self.lower_json_table_path_spec(path)?,
        ];

        let passing = self.lower_nodes(&table.passing)?;
        self.optional_clause(&mut values, "passing", passing);
        let columns = self.lower_json_table_columns(&table.columns)?;
        values.push(self.list("columns", columns));
        if let Some(on_error) = table.on_error.as_ref() {
            let lowered_on_error = [self.lower_json_behavior(on_error)?];
            values.push(self.list("on-error", lowered_on_error));
        }
        if let Some(alias) = table.alias.as_ref() {
            values.push(self.lower_alias(alias)?);
        }
        if table.lateral {
            values.push(self.symbol("lateral"));
        }

        Ok(self.list("json-table", values))
    }

    pub(super) fn lower_json_table_column(&mut self, column: &pg::JsonTableColumn) -> Result<Sexp> {
        let mut values = vec![Sexp::string(column.name.clone())];

        match pg::JsonTableColumnType::try_from(column.coltype) {
            Ok(pg::JsonTableColumnType::Undefined | pg::JsonTableColumnType::JtcRegular) => {}
            Ok(pg::JsonTableColumnType::JtcForOrdinality) => {
                values.push(self.symbol("for-ordinality"));
            }
            Ok(pg::JsonTableColumnType::JtcExists) => values.push(self.symbol("exists")),
            Ok(pg::JsonTableColumnType::JtcFormatted) => values.push(self.symbol("formatted")),
            Ok(pg::JsonTableColumnType::JtcNested) => values.push(self.symbol("nested")),
            _ => return self.unsupported_error("JSON_TABLE column type"),
        }
        if let Some(type_name) = column.type_name.as_ref() {
            values.push(self.lower_type(type_name)?);
        }
        if let Some(path_spec) = column.pathspec.as_ref() {
            values.push(self.lower_json_table_path_spec(path_spec)?);
        }
        if let Some(format) = column
            .format
            .as_ref()
            .map(|x| self.lower_json_format(x))
            .transpose()?
            .flatten()
        {
            values.push(format);
        }
        if let Some(wrapper) = self.lower_json_wrapper(column.wrapper)? {
            values.push(wrapper);
        }
        if let Some(quotes) = self.lower_json_quotes(column.quotes)? {
            values.push(quotes);
        }
        if !column.columns.is_empty() {
            let columns = self.lower_json_table_columns(&column.columns)?;
            values.push(self.list("columns", columns));
        }
        if let Some(on_empty) = column.on_empty.as_ref() {
            let lowered_on_empty = [self.lower_json_behavior(on_empty)?];
            values.push(self.list("on-empty", lowered_on_empty));
        }
        if let Some(on_error) = column.on_error.as_ref() {
            let lowered_on_error = [self.lower_json_behavior(on_error)?];
            values.push(self.list("on-error", lowered_on_error));
        }

        Ok(self.list("json-column", values))
    }

    pub(super) fn lower_json_table_plan(&mut self, node: &pg::Node) -> Result<Sexp> {
        match self.required_kind(node.node.as_ref(), "JSON_TABLE plan")? {
            PgNode::JsonTablePathScan(scan) => self.lower_json_table_path_scan(scan),
            PgNode::JsonTableSiblingJoin(join) => self.lower_json_table_sibling_join(join),
            _ => self.unsupported("JSON_TABLE plan"),
        }
    }

    pub(super) fn lower_json_table_path_scan(
        &mut self,
        scan: &pg::JsonTablePathScan,
    ) -> Result<Sexp> {
        let path = [Sexp::string(
            self.required(scan.path.as_ref(), "JSON_TABLE scan path")?
                .name
                .clone(),
        )];
        let mut values = vec![self.list("path", path)];

        if scan.error_on_error {
            values.push(self.symbol("error-on-error"));
        }
        if let Some(child) = scan.child.as_deref() {
            values.push(self.lower_json_table_plan(child)?);
        }

        Ok(self.list("json-table-path-scan", values))
    }

    pub(super) fn lower_json_table_sibling_join(
        &mut self,
        join: &pg::JsonTableSiblingJoin,
    ) -> Result<Sexp> {
        let right_plan_node = self.required_node(join.rplan.as_deref(), "JSON_TABLE right plan")?;
        let left_plan_node = self.required_node(join.lplan.as_deref(), "JSON_TABLE left plan")?;
        let json_table_sibling_join = [
            self.lower_json_table_plan(left_plan_node)?,
            self.lower_json_table_plan(right_plan_node)?,
        ];

        Ok(self.list("json-table-sibling-join", json_table_sibling_join))
    }
}
