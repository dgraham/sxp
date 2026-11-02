use super::*;

impl PostgresParser {
    pub(in crate::parser) fn lower_create_conversion(
        &mut self,
        stmt: &pg::CreateConversionStmt,
    ) -> Result<Sexp> {
        let from = [self.lower_qualified_name(&stmt.func_name)?];
        let mut values = vec![
            self.symbol("create-conversion"),
            self.lower_qualified_name(&stmt.conversion_name)?,
            self.list("for", [Sexp::string(stmt.for_encoding_name.clone())]),
            self.list("to", [Sexp::string(stmt.to_encoding_name.clone())]),
            self.list("from", from),
        ];

        if stmt.def {
            values.push(self.symbol("default"));
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_create_cast(
        &mut self,
        stmt: &pg::CreateCastStmt,
    ) -> Result<Sexp> {
        let target_type = self.required(stmt.targettype.as_ref(), "CREATE CAST target type")?;
        let source_type = self.required(stmt.sourcetype.as_ref(), "CREATE CAST source type")?;
        let mut values = vec![
            self.symbol("create-cast"),
            self.lower_type(source_type)?,
            self.lower_type(target_type)?,
        ];

        if stmt.inout {
            values.push(self.symbol("with-inout"));
        }
        if let Some(function) = stmt.func.as_ref() {
            let func = [self.lower_object_with_args(function)?];
            values.push(self.list("function", func));
        }

        let context = match pg::CoercionContext::try_from(stmt.context)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: CREATE CAST context", 0))?
        {
            pg::CoercionContext::CoercionImplicit => "implicit",
            pg::CoercionContext::CoercionAssignment => "assignment",
            pg::CoercionContext::CoercionExplicit => "explicit",
            _ => return self.unsupported("CREATE CAST context"),
        };
        let as_value = [self.symbol(context)];
        values.push(self.list("as", as_value));

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_create_function(
        &mut self,
        stmt: &pg::CreateFunctionStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol(if stmt.is_procedure {
            "create-procedure"
        } else {
            "create-function"
        })];

        if stmt.replace {
            values.push(self.symbol("or-replace"));
        }

        values.push(self.lower_qualified_name(&stmt.funcname)?);
        if !stmt.parameters.is_empty() {
            let lowered_parameters = stmt
                .parameters
                .iter()
                .map(
                    |node| match self.required_kind(node.node.as_ref(), "function parameter")? {
                        PgNode::FunctionParameter(parameter) => {
                            self.lower_function_parameter(parameter)
                        }
                        _ => self.unsupported_error("function parameter"),
                    },
                )
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("parameters", lowered_parameters));
        }
        if let Some(return_type) = stmt.return_type.as_ref() {
            let lowered_returns = [self.lower_signature_type(return_type)?];
            if stmt.is_procedure {
                return self.unsupported("procedure return type");
            }

            values.push(self.list("returns", lowered_returns));
        }

        values.extend(self.lower_function_options(&stmt.options)?);
        if let Some(body) = stmt.sql_body.as_deref() {
            let lowered_sql_body = self.lower_sql_body(body)?;
            values.push(self.list("sql-body", lowered_sql_body));
        }

        Ok(Sexp::list(values))
    }

    fn lower_sql_body(&mut self, body: &pg::Node) -> Result<Vec<Sexp>> {
        match self.required_kind(body.node.as_ref(), "function body")? {
            PgNode::ReturnStmt(stmt) => Ok(vec![self.lower_return(stmt)?]),
            PgNode::List(statements) => {
                let [outer] = statements.items.as_slice() else {
                    return self.unsupported_error("function body statements");
                };
                let statements: &[pg::Node] = match outer.node.as_ref() {
                    Some(PgNode::List(statements)) => &statements.items,
                    None => &[],
                    Some(_) => return self.unsupported_error("function body statements"),
                };
                self.lower_nodes(statements)
            }
            _ => self.unsupported_error("function body"),
        }
    }

    fn lower_function_options(&mut self, options: &[pg::Node]) -> Result<Vec<Sexp>> {
        options
            .iter()
            .map(|node| {
                let option =
                    match self.required_kind(node.node.as_ref(), "CREATE FUNCTION option")? {
                        PgNode::DefElem(option) => option,
                        _ => return self.unsupported_error("CREATE FUNCTION option"),
                    };
                match option.defname.as_str() {
                    "language" => {
                        let language_node =
                            self.required_node(option.arg.as_deref(), "CREATE FUNCTION language")?;
                        let function_language = [Sexp::string(self.string_node(language_node)?)];
                        Ok(self.list("language", function_language))
                    }
                    "as" => {
                        let node =
                            self.required_node(option.arg.as_deref(), "CREATE FUNCTION body")?;
                        let items =
                            match self.required_kind(node.node.as_ref(), "CREATE FUNCTION body")? {
                                PgNode::List(items) => &items.items,
                                _ => return self.unsupported_error("CREATE FUNCTION body"),
                            };
                        let code = items
                            .iter()
                            .map(|x| self.string_node(x))
                            .collect::<Result<Vec<_>>>()?;
                        Ok(self.list("as", code.into_iter().map(Sexp::string)))
                    }
                    "volatility" => {
                        let volatility_node = self
                            .required_node(option.arg.as_deref(), "CREATE FUNCTION volatility")?;
                        let volatility = self.string_node(volatility_node)?;
                        match volatility.as_str() {
                            "volatile" | "stable" | "immutable" => Ok(self.symbol(&volatility)),
                            _ => self.unsupported_error("CREATE FUNCTION volatility"),
                        }
                    }
                    "strict" => {
                        let strict =
                            self.lower_flag_argument(option, "CREATE FUNCTION STRICT option")?;
                        Ok(self.symbol(if strict {
                            "strict"
                        } else {
                            "called-on-null-input"
                        }))
                    }
                    "security" => {
                        let definer =
                            self.lower_flag_argument(option, "CREATE FUNCTION SECURITY option")?;
                        Ok(self.symbol(if definer {
                            "security-definer"
                        } else {
                            "security-invoker"
                        }))
                    }
                    "leakproof" => {
                        let leakproof =
                            self.lower_flag_argument(option, "CREATE FUNCTION LEAKPROOF option")?;
                        Ok(self.symbol(if leakproof {
                            "leakproof"
                        } else {
                            "not-leakproof"
                        }))
                    }
                    "parallel" => {
                        let parallel_node =
                            self.required_node(option.arg.as_deref(), "CREATE FUNCTION parallel")?;
                        let parallel = self.string_node(parallel_node)?;
                        match parallel.as_str() {
                            "safe" | "unsafe" | "restricted" => {
                                let parallel = [self.symbol(&parallel)];
                                Ok(self.list("parallel", parallel))
                            }
                            _ => self.unsupported_error("CREATE FUNCTION parallel"),
                        }
                    }
                    "set" => {
                        let node = self.required_node(
                            option.arg.as_deref(),
                            "CREATE FUNCTION SET statement",
                        )?;
                        let set = match self
                            .required_kind(node.node.as_ref(), "CREATE FUNCTION SET statement")?
                        {
                            PgNode::VariableSetStmt(set) => set,
                            _ => return self.unsupported_error("CREATE FUNCTION SET statement"),
                        };
                        self.lower_variable_set(set)
                    }
                    "cost" | "rows" => {
                        let node = self
                            .required_node(option.arg.as_deref(), "CREATE FUNCTION option value")?;
                        let value = match self
                            .required_kind(node.node.as_ref(), "CREATE FUNCTION option value")?
                        {
                            PgNode::Integer(integer) => self.symbol(&integer.ival.to_string()),
                            PgNode::Float(value) => self.symbol(&value.fval),
                            _ => return self.unsupported_error("CREATE FUNCTION option value"),
                        };
                        Ok(self.list(option.defname.as_str(), [value]))
                    }
                    "support" => {
                        let node = self.required_node(
                            option.arg.as_deref(),
                            "CREATE FUNCTION support function",
                        )?;
                        let names = match self
                            .required_kind(node.node.as_ref(), "CREATE FUNCTION support function")?
                        {
                            PgNode::List(names) => &names.items,
                            _ => return self.unsupported_error("CREATE FUNCTION support function"),
                        };
                        let lowered_support = [self.lower_qualified_name(names)?];
                        Ok(self.list("support", lowered_support))
                    }
                    "transform" => {
                        let node = self.required_node(
                            option.arg.as_deref(),
                            "CREATE FUNCTION transform type",
                        )?;
                        let types = match self
                            .required_kind(node.node.as_ref(), "CREATE FUNCTION transform type")?
                        {
                            PgNode::List(types) => &types.items,
                            _ => return self.unsupported_error("CREATE FUNCTION transform type"),
                        };
                        let lowered_transform = types
                            .iter()
                            .map(|node| {
                                match self.required_kind(
                                    node.node.as_ref(),
                                    "CREATE FUNCTION transform type",
                                )? {
                                    PgNode::TypeName(typename) => {
                                        self.lower_signature_type(typename)
                                    }
                                    _ => self.unsupported_error("CREATE FUNCTION transform type"),
                                }
                            })
                            .collect::<Result<Vec<_>>>()?;
                        Ok(self.list("transform", lowered_transform))
                    }
                    "window" => match option.arg.as_deref() {
                        None => Ok(self.symbol("window")),
                        Some(argument) => match self.required_kind(
                            argument.node.as_ref(),
                            "CREATE FUNCTION window option",
                        )? {
                            PgNode::Boolean(value) if value.boolval => Ok(self.symbol("window")),
                            _ => self.unsupported("CREATE FUNCTION window option"),
                        },
                    },
                    other => self.unsupported_error(format!("CREATE FUNCTION option {other}")),
                }
            })
            .collect()
    }

    fn lower_function_parameter(&mut self, parameter: &pg::FunctionParameter) -> Result<Sexp> {
        let mode = match pg::FunctionParameterMode::try_from(parameter.mode) {
            Ok(pg::FunctionParameterMode::FuncParamDefault) => None,
            Ok(pg::FunctionParameterMode::FuncParamIn) => Some("in"),
            Ok(pg::FunctionParameterMode::FuncParamOut) => Some("out"),
            Ok(pg::FunctionParameterMode::FuncParamInout) => Some("inout"),
            Ok(pg::FunctionParameterMode::FuncParamVariadic) => Some("variadic"),
            Ok(pg::FunctionParameterMode::FuncParamTable) => Some("table"),
            _ => return self.unsupported("function parameter mode"),
        };
        let mut values = vec![self.symbol("parameter")];

        let parameter_type =
            self.required(parameter.arg_type.as_ref(), "function parameter type")?;
        if let Some(mode) = mode {
            values.push(self.symbol(mode));
        }
        if !parameter.name.is_empty() {
            values.push(Sexp::string(parameter.name.clone()));
        }

        values.push(self.lower_signature_type(parameter_type)?);
        if let Some(default) = parameter.defexpr.as_deref() {
            let lowered_default = [self.lower(default)?];
            values.push(self.list("default", lowered_default));
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_function(
        &mut self,
        stmt: &pg::AlterFunctionStmt,
    ) -> Result<Sexp> {
        let command = match pg::ObjectType::try_from(stmt.objtype) {
            Ok(pg::ObjectType::ObjectFunction) => "alter-function",
            Ok(pg::ObjectType::ObjectProcedure) => "alter-procedure",
            Ok(pg::ObjectType::ObjectRoutine) => "alter-routine",
            _ => return self.unsupported("ALTER FUNCTION object type"),
        };
        let function = self.required(stmt.func.as_ref(), "ALTER FUNCTION signature")?;
        let mut values = vec![self.symbol(command), self.lower_object_with_args(function)?];

        values.extend(self.lower_function_options(&stmt.actions)?);

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_create_trigger(
        &mut self,
        stmt: &pg::CreateTrigStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-trigger"),
            Sexp::string(stmt.trigname.clone()),
        ];

        let trigger_timing = self.trigger_timing(stmt.timing)?;
        if stmt.replace {
            values.push(self.symbol("or-replace"));
        }
        if stmt.isconstraint {
            values.push(self.symbol("constraint"));
        }

        values.push(self.symbol(trigger_timing));

        let trigger_relation = self.required(stmt.relation.as_ref(), "trigger relation")?;
        let lowered_on = [self.lower_relation(trigger_relation)?];
        values.push(self.list("on", lowered_on));
        if let Some(constrrel) = stmt.constrrel.as_ref() {
            let lowered_from = [self.lower_relation(constrrel)?];
            values.push(self.list("from", lowered_from));
        }

        values.push(self.trigger_events(stmt.events, &stmt.columns)?);

        let for_each = [self.symbol(if stmt.row { "row" } else { "statement" })];
        values.push(self.list("for-each", for_each));
        self.optional_node_clause(&mut values, "when", stmt.when_clause.as_deref())?;
        if !stmt.transition_rels.is_empty() {
            let lowered_transitions = stmt
                .transition_rels
                .iter()
                .map(
                    |node| match self.required_kind(node.node.as_ref(), "trigger transition")? {
                        PgNode::TriggerTransition(transition) => {
                            self.lower_trigger_transition(transition)
                        }
                        _ => self.unsupported_error("trigger transition"),
                    },
                )
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("transitions", lowered_transitions));
        }
        if stmt.deferrable {
            values.push(self.symbol("deferrable"));
            values.push(self.symbol(if stmt.initdeferred {
                "initially-deferred"
            } else {
                "initially-immediate"
            }));
        }

        let mut function = vec![self.lower_qualified_name(&stmt.funcname)?];

        if !stmt.args.is_empty() {
            let args = stmt
                .args
                .iter()
                .map(|x| self.string_node(x))
                .collect::<Result<Vec<_>>>()?;
            function.push(self.list("args", args.into_iter().map(Sexp::string)));
        }

        values.push(self.list("function", function));

        Ok(Sexp::list(values))
    }

    fn trigger_timing(&mut self, timing: i32) -> Result<&'static str> {
        match timing {
            0 => Ok("after"),
            TRIGGER_TYPE_BEFORE => Ok("before"),
            TRIGGER_TYPE_INSTEAD => Ok("instead"),
            _ => self.unsupported_error("trigger timing"),
        }
    }

    fn trigger_events(&mut self, events: i32, columns: &[pg::Node]) -> Result<Sexp> {
        if events & !TRIGGER_TYPE_EVENTS != 0 {
            return self.unsupported("trigger events");
        }

        let mut items: Vec<Sexp> = Vec::new();
        if events & TRIGGER_TYPE_INSERT != 0 {
            items.push(self.symbol("insert"));
        }
        if events & TRIGGER_TYPE_DELETE != 0 {
            items.push(self.symbol("delete"));
        }
        if events & TRIGGER_TYPE_UPDATE != 0 {
            if columns.is_empty() {
                items.push(self.symbol("update"));
            } else {
                let trigger_update_columns = columns
                    .iter()
                    .map(|x| self.string_node(x))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .map(Sexp::string);
                items.push(self.list("update", trigger_update_columns));
            }
        } else if !columns.is_empty() {
            return self.unsupported("trigger event columns");
        }
        if events & TRIGGER_TYPE_TRUNCATE != 0 {
            items.push(self.symbol("truncate"));
        }

        if items.is_empty() {
            return self.unsupported("trigger events");
        }

        Ok(self.list("events", items))
    }

    fn lower_trigger_transition(&mut self, transition: &pg::TriggerTransition) -> Result<Sexp> {
        let kind = match (transition.is_table, transition.is_new) {
            (true, true) => "new-table",
            (true, false) => "old-table",
            (false, true) => "new-row",
            (false, false) => "old-row",
        };

        Ok(self.list(kind, [Sexp::string(transition.name.clone())]))
    }

    pub(in crate::parser) fn lower_alter_object_depends(
        &mut self,
        stmt: &pg::AlterObjectDependsStmt,
    ) -> Result<Sexp> {
        let command = match pg::ObjectType::try_from(stmt.object_type) {
            Ok(pg::ObjectType::ObjectFunction) => "alter-function-depends",
            Ok(pg::ObjectType::ObjectProcedure) => "alter-procedure-depends",
            _ => return self.unsupported("ALTER DEPENDS object type"),
        };

        if stmt.relation.is_some() {
            return self.unsupported("ALTER DEPENDS relation");
        }

        let object = self.required_node(stmt.object.as_deref(), "ALTER DEPENDS object")?;
        let function = match self.required_kind(object.node.as_ref(), "ALTER DEPENDS object")? {
            PgNode::ObjectWithArgs(function) => function,
            _ => return self.unsupported("ALTER DEPENDS object"),
        };
        let extension_name = [Sexp::string(
            self.required(stmt.extname.as_ref(), "ALTER DEPENDS extension name")?
                .sval
                .clone(),
        )];
        let mut values = vec![
            self.symbol(command),
            self.lower_object_with_args(function)?,
            self.list("depends-on-extension", extension_name),
        ];

        if stmt.remove {
            values.push(self.symbol("remove"));
        }

        Ok(Sexp::list(values))
    }

    pub(in crate::parser) fn lower_alter_object_name(
        &mut self,
        object_type: pg::ObjectType,
        node: &pg::Node,
    ) -> Result<Vec<Sexp>> {
        if !matches!(
            object_type,
            pg::ObjectType::ObjectOpclass | pg::ObjectType::ObjectOpfamily
        ) {
            return Ok(vec![self.lower_object_name(node)?]);
        }

        let names = match self.required_kind(node.node.as_ref(), "DDL object name")? {
            PgNode::List(names) => names,
            _ => return self.unsupported_error("DDL object name"),
        };
        let (access_method, name) = match names.items.split_first() {
            Some(split) => split,
            None => return self.unsupported_error("DDL object name"),
        };
        let access_method = [Sexp::string(self.string_node(access_method)?)];
        Ok(vec![
            self.list("using", access_method),
            self.lower_qualified_name(name)?,
        ])
    }
}
