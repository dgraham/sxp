use super::*;

impl PostgresParser {
    pub(crate) fn lower_define_stmt(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        let kind = pg::ObjectType::try_from(stmt.kind)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: DEFINE statement", 0))?;
        match kind {
            pg::ObjectType::ObjectAggregate => self.lower_define_aggregate(stmt),
            pg::ObjectType::ObjectOperator => self.lower_define_operator(stmt),
            pg::ObjectType::ObjectCollation => self.lower_define_collation(stmt),
            pg::ObjectType::ObjectType => self.lower_define_type(stmt),
            pg::ObjectType::ObjectTsconfiguration => self.lower_define_ts_configuration(stmt),
            pg::ObjectType::ObjectTsdictionary => self.lower_define_ts_dictionary(stmt),
            pg::ObjectType::ObjectTsparser => self.lower_define_ts_parser(stmt),
            pg::ObjectType::ObjectTstemplate => self.lower_define_ts_template(stmt),
            _ => self.unsupported(format!("DEFINE object type {kind:?}")),
        }
    }

    fn lower_define_collation(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        if stmt.replace {
            return self.unsupported("CREATE OR REPLACE COLLATION");
        }

        let mut values = vec![
            self.symbol("create-collation"),
            self.lower_qualified_name(&stmt.defnames)?,
        ];

        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        if !stmt.args.is_empty() {
            return self.unsupported("CREATE COLLATION arguments");
        }

        for node in &stmt.definition {
            let option = self.define_option(node, "CREATE COLLATION option")?;
            values.push(self.lower_collation_option(option)?);
        }

        Ok(Sexp::list(values))
    }

    fn lower_collation_option(&mut self, option: &pg::DefElem) -> Result<Sexp> {
        match option.defname.to_ascii_lowercase().as_str() {
            "from" => {
                let node = self.required_node(option.arg.as_deref(), "CREATE COLLATION source")?;
                let source =
                    match self.required_kind(node.node.as_ref(), "CREATE COLLATION source")? {
                        PgNode::List(names) => self.lower_qualified_name(&names.items)?,
                        _ => {
                            let name = self.lower_scalar_node(node)?;
                            self.list("name", [name])
                        }
                    };
                Ok(self.list("from", [source]))
            }
            "provider" | "locale" | "lc_collate" | "lc_ctype" | "collversion" | "version"
            | "rules" | "deterministic" => {
                let value = self.define_scalar_argument(option, "CREATE COLLATION option value")?;
                Ok(self.list(&self.kebab_name(&option.defname), [value]))
            }
            other => self.unsupported(format!("CREATE COLLATION option {other}")),
        }
    }

    fn lower_define_type(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        if stmt.replace {
            return self.unsupported("CREATE OR REPLACE TYPE");
        }

        let mut values = vec![
            self.symbol("create-type"),
            self.lower_qualified_name(&stmt.defnames)?,
        ];

        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        if !stmt.args.is_empty() {
            return self.unsupported("CREATE TYPE arguments");
        }

        for node in &stmt.definition {
            let option = self.define_option(node, "CREATE TYPE option")?;
            values.push(self.lower_type_definition_option(option)?);
        }

        Ok(Sexp::list(values))
    }

    fn lower_type_definition_option(&mut self, option: &pg::DefElem) -> Result<Sexp> {
        match option.defname.to_ascii_lowercase().as_str() {
            "internallength" | "category" | "preferred" | "default" | "delimiter"
            | "collatable" | "alignment" | "storage" => {
                let value = self.define_scalar_argument(option, "CREATE TYPE option value")?;
                Ok(self.list(&self.kebab_name(&option.defname), [value]))
            }
            "input" | "output" | "receive" | "send" | "typmod_in" | "typmod_out" | "analyze" => {
                let value = self.define_name_argument(option, "CREATE TYPE option value")?;
                Ok(self.list(&self.kebab_name(&option.defname), [value]))
            }
            "element"
            | "like"
            | "subtype_opclass"
            | "canonical"
            | "subtype_diff"
            | "multirange_type_name" => {
                let value = self.define_type_argument(option, "CREATE TYPE option value")?;
                Ok(self.list(&self.kebab_name(&option.defname), [value]))
            }
            "passbyvalue" | "passedbyvalue" => {
                if option.arg.is_some() {
                    return self.unsupported("CREATE TYPE passedbyvalue option value");
                }
                Ok(self.symbol(&self.kebab_name(&option.defname)))
            }
            other => self.unsupported(format!("CREATE TYPE option {other}")),
        }
    }

    fn lower_define_ts_configuration(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        self.lower_define_ts_object(
            stmt,
            "create-text-search-configuration",
            |parser, option| match option.defname.to_ascii_lowercase().as_str() {
                "copy" => {
                    let value = parser.define_name_argument(
                        option,
                        "CREATE TEXT SEARCH CONFIGURATION copy option",
                    )?;
                    Ok(parser.list("copy", [value]))
                }
                "parser" => {
                    let value = parser.define_name_argument(
                        option,
                        "CREATE TEXT SEARCH CONFIGURATION parser option",
                    )?;
                    Ok(parser.list("parser", [value]))
                }
                other => {
                    parser.unsupported(format!("CREATE TEXT SEARCH CONFIGURATION option {other}"))
                }
            },
        )
    }

    fn lower_define_ts_dictionary(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        self.lower_define_ts_object(stmt, "create-text-search-dictionary", |parser, option| {
            match option.defname.to_ascii_lowercase().as_str() {
                "template" => {
                    let value = parser.define_name_argument(
                        option,
                        "CREATE TEXT SEARCH DICTIONARY template option",
                    )?;
                    Ok(parser.list("template", [value]))
                }
                "dictionary" => {
                    let value = parser.define_name_argument(
                        option,
                        "CREATE TEXT SEARCH DICTIONARY dictionary option",
                    )?;
                    Ok(parser.list("dictionary", [value]))
                }
                "dictfile" | "afffile" | "stopwords" | "synonyms" | "initfile" => {
                    let value = parser.define_scalar_argument(
                        option,
                        "CREATE TEXT SEARCH DICTIONARY option value",
                    )?;
                    Ok(parser.list(&parser.kebab_name(&option.defname), [value]))
                }
                _ => parser.lower_generic_option(option),
            }
        })
    }

    fn lower_define_ts_parser(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        self.lower_define_ts_object(stmt, "create-text-search-parser", |parser, option| {
            let name = parser.kebab_name(&option.defname);
            match option.defname.to_ascii_lowercase().as_str() {
                "start" | "gettoken" | "end" | "lextypes" | "headword" => {
                    let value = parser
                        .define_name_argument(option, "CREATE TEXT SEARCH PARSER option value")?;
                    Ok(parser.list(&name, [value]))
                }
                other => parser.unsupported(format!("CREATE TEXT SEARCH PARSER option {other}")),
            }
        })
    }

    fn lower_define_ts_template(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        self.lower_define_ts_object(stmt, "create-text-search-template", |parser, option| {
            match option.defname.to_ascii_lowercase().as_str() {
                "init" | "lexize" => {
                    let name = parser.kebab_name(&option.defname);
                    let value = parser
                        .define_name_argument(option, "CREATE TEXT SEARCH TEMPLATE option value")?;
                    Ok(parser.list(&name, [value]))
                }
                other => parser.unsupported(format!("CREATE TEXT SEARCH TEMPLATE option {other}")),
            }
        })
    }

    fn lower_define_ts_object(
        &mut self,
        stmt: &pg::DefineStmt,
        atom: &str,
        lower_option: impl Fn(&mut Self, &pg::DefElem) -> Result<Sexp>,
    ) -> Result<Sexp> {
        if stmt.replace {
            return self.unsupported(format!("CREATE OR REPLACE {}", atom.to_uppercase()));
        }

        let mut values = vec![
            self.symbol(atom),
            self.lower_qualified_name(&stmt.defnames)?,
        ];

        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        if !stmt.args.is_empty() {
            return self.unsupported(format!("{} arguments", atom.to_uppercase()));
        }

        for node in &stmt.definition {
            let option = self.define_option(node, atom)?;
            values.push(lower_option(self, option)?);
        }

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_create_event_trigger(
        &mut self,
        stmt: &pg::CreateEventTrigStmt,
    ) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-event-trigger"),
            Sexp::string(stmt.trigname.clone()),
            self.list("on", [Sexp::string(stmt.eventname.clone())]),
        ];

        for node in &stmt.whenclause {
            let option = self.define_option(node, "CREATE EVENT TRIGGER filter")?;
            let arg = self.required_node(option.arg.as_deref(), "event trigger filter values")?;
            let filter =
                match self.required_kind(arg.node.as_ref(), "event trigger filter values")? {
                    PgNode::List(items) => &items.items,
                    _ => return self.unsupported("event trigger filter values"),
                };
            let name = if option.defname == "tag" {
                "tags".to_owned()
            } else {
                self.kebab_name(&option.defname)
            };
            let names = filter
                .iter()
                .map(|node| self.string_node(node))
                .collect::<Result<Vec<_>>>()?;
            let when = self.list(&name, names.into_iter().map(Sexp::string));
            values.push(self.list("when", [when]));
        }

        let function = self.lower_qualified_name(&stmt.funcname)?;
        let execute = self.list("function", [function]);
        values.push(self.list("execute", [execute]));

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_alter_event_trigger(
        &mut self,
        stmt: &pg::AlterEventTrigStmt,
    ) -> Result<Sexp> {
        let action = match stmt.tgenabled.as_str() {
            "D" => Sexp::list([self.symbol("disable")]),
            "O" => Sexp::list([self.symbol("enable")]),
            "R" => {
                let state = self.symbol("replica");
                self.list("enable", [state])
            }
            "A" => {
                let state = self.symbol("always");
                self.list("enable", [state])
            }
            other => return self.unsupported(format!("event trigger enable state {other}")),
        };
        Ok(self.list(
            "alter-event-trigger",
            [Sexp::string(stmt.trigname.clone()), action],
        ))
    }

    pub(crate) fn lower_create_am(&mut self, stmt: &pg::CreateAmStmt) -> Result<Sexp> {
        let kind = match stmt.amtype.as_str() {
            "i" => "index",
            "t" => "table",
            other => return self.unsupported(format!("access method type {other}")),
        };
        let atom = self.symbol(kind);
        let type_node = self.list("type", [atom]);
        let handler = self.lower_qualified_name(&stmt.handler_name)?;
        let handler_node = self.list("handler", [handler]);
        Ok(self.list(
            "create-access-method",
            [Sexp::string(stmt.amname.clone()), type_node, handler_node],
        ))
    }

    pub(crate) fn lower_create_language(&mut self, stmt: &pg::CreatePLangStmt) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-language"),
            Sexp::string(stmt.plname.clone()),
        ];

        if stmt.replace {
            values.push(self.symbol("or-replace"));
        }
        if stmt.pltrusted {
            values.push(self.symbol("trusted"));
        }
        for (field, name) in [
            (&stmt.plhandler, "handler"),
            (&stmt.plinline, "inline"),
            (&stmt.plvalidator, "validator"),
        ] {
            if !field.is_empty() {
                let node = self.lower_qualified_name(field)?;
                values.push(self.list(name, [node]));
            }
        }

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_create_transform(
        &mut self,
        stmt: &pg::CreateTransformStmt,
    ) -> Result<Sexp> {
        let mut values = vec![self.symbol("create-transform")];

        if stmt.replace {
            values.push(self.symbol("or-replace"));
        }

        let type_node = self.required(stmt.type_name.as_ref(), "transform type")?;
        let type_sexp = self.lower_type(type_node)?;
        let for_node = self.list("for", [type_sexp]);
        values.push(for_node);
        values.push(self.list("language", [Sexp::string(stmt.lang.clone())]));

        let from_sql_node = self.required(stmt.fromsql.as_ref(), "transform from-sql function")?;
        let from_sql = self.lower_object_with_args(from_sql_node)?;
        values.push(self.list("from-sql", [from_sql]));

        let to_sql_node = self.required(stmt.tosql.as_ref(), "transform to-sql function")?;
        let to_sql = self.lower_object_with_args(to_sql_node)?;
        values.push(self.list("to-sql", [to_sql]));

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_create_range(&mut self, stmt: &pg::CreateRangeStmt) -> Result<Sexp> {
        let mut range = Vec::new();
        for node in &stmt.params {
            let option = self.define_option(node, "CREATE TYPE ... AS RANGE option")?;
            let name = self.kebab_name(&option.defname);
            match option.defname.as_str() {
                "subtype"
                | "subtype_opclass"
                | "collation"
                | "canonical"
                | "subtype_diff"
                | "multirange_type_name" => {
                    let value =
                        self.define_type_argument(option, "CREATE TYPE ... AS RANGE option value")?;
                    range.push(self.list(&name, [value]));
                }
                other => {
                    return self.unsupported(format!("CREATE TYPE ... AS RANGE option {other}"));
                }
            }
        }

        let type_name = self.lower_qualified_name(&stmt.type_name)?;
        let as_range = self.list("as-range", range);

        Ok(self.list("create-type", [type_name, as_range]))
    }

    pub(crate) fn lower_alter_collation(&mut self, stmt: &pg::AlterCollationStmt) -> Result<Sexp> {
        let name = self.lower_qualified_name(&stmt.collname)?;
        let refresh = Sexp::list([self.symbol("refresh-version")]);
        Ok(self.list("alter-collation", [name, refresh]))
    }

    pub(crate) fn lower_alter_type(&mut self, stmt: &pg::AlterTypeStmt) -> Result<Sexp> {
        let options = self.lower_storage_options(&stmt.options, "ALTER TYPE option")?;
        let name = self.lower_qualified_name(&stmt.type_name)?;
        let set_options = self.list("set-options", options);
        Ok(self.list("alter-type", [name, set_options]))
    }

    pub(crate) fn lower_alter_table_move_all(
        &mut self,
        stmt: &pg::AlterTableMoveAllStmt,
    ) -> Result<Sexp> {
        let object_type = pg::ObjectType::try_from(stmt.objtype)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: object type", 0))?;
        let object = self.object_type_name(object_type)?;
        let atom = self.symbol(object);
        let type_node = self.list("type", [atom]);
        let from_table = self.list(
            "from-tablespace",
            [Sexp::string(stmt.orig_tablespacename.clone())],
        );
        let mut values = vec![self.symbol("move-all-in-tablespace"), type_node, from_table];

        if !stmt.roles.is_empty() {
            let owner = self.lower_role_spec_nodes(&stmt.roles)?;
            values.push(self.list("owner", owner));
        }

        let to_table = self.list(
            "to-tablespace",
            [Sexp::string(stmt.new_tablespacename.clone())],
        );
        values.push(to_table);
        if stmt.nowait {
            values.push(self.symbol("nowait"));
        }

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_alter_tsconfiguration(
        &mut self,
        stmt: &pg::AlterTsConfigurationStmt,
    ) -> Result<Sexp> {
        let kind = pg::AlterTsConfigType::try_from(stmt.kind)
            .map_err(|_| ParseError::new("unsupported PostgreSQL AST: text search action", 0))?;
        let mut dictionaries = Vec::new();
        for node in &stmt.dicts {
            let names =
                match self.required_kind(node.node.as_ref(), "text search dictionary name")? {
                    PgNode::List(names) => &names.items,
                    _ => return self.unsupported("text search dictionary name"),
                };
            dictionaries.push(self.lower_qualified_name(names)?);
        }

        let action = match kind {
            pg::AlterTsConfigType::AlterTsconfigAddMapping => {
                let tokens = self.ts_token_names(&stmt.tokentype)?;
                let tokens_node = self.list("tokens", tokens);
                let dictionaries_node = self.list("dictionaries", dictionaries);
                self.list("add-mapping", [tokens_node, dictionaries_node])
            }
            pg::AlterTsConfigType::AlterTsconfigAlterMappingForToken => {
                let tokens = self.ts_token_names(&stmt.tokentype)?;
                let tokens_node = self.list("tokens", tokens);
                let dictionaries_node = self.list("dictionaries", dictionaries);
                self.list("alter-mapping", [tokens_node, dictionaries_node])
            }
            pg::AlterTsConfigType::AlterTsconfigReplaceDict
            | pg::AlterTsConfigType::AlterTsconfigReplaceDictForToken => {
                let [old, new] = dictionaries.as_slice() else {
                    return self
                        .unsupported("ALTER TEXT SEARCH CONFIGURATION REPLACE dictionaries");
                };
                let mut values = Vec::new();
                if kind == pg::AlterTsConfigType::AlterTsconfigReplaceDictForToken {
                    let tokens = self.ts_token_names(&stmt.tokentype)?;
                    values.push(self.list("tokens", tokens));
                }

                values.push(self.list("from", [old.clone()]));
                values.push(self.list("to", [new.clone()]));
                Sexp::list(std::iter::once(self.symbol("replace-dictionary")).chain(values))
            }
            pg::AlterTsConfigType::AlterTsconfigDropMapping => {
                let tokens = self.ts_token_names(&stmt.tokentype)?;
                let mut values = vec![self.list("tokens", tokens)];

                if stmt.missing_ok {
                    values.push(Sexp::list([self.symbol("if-exists")]));
                }
                Sexp::list(std::iter::once(self.symbol("drop-mapping")).chain(values))
            }
            pg::AlterTsConfigType::AlterTsconfigTypeUndefined => {
                return self.unsupported("ALTER TEXT SEARCH CONFIGURATION action");
            }
        };
        let name = self.lower_qualified_name(&stmt.cfgname)?;

        Ok(self.list("alter-text-search-configuration", [name, action]))
    }

    pub(crate) fn lower_alter_tsdictionary(
        &mut self,
        stmt: &pg::AlterTsDictionaryStmt,
    ) -> Result<Sexp> {
        let options =
            self.lower_storage_options(&stmt.options, "ALTER TEXT SEARCH DICTIONARY option")?;
        let name = self.lower_qualified_name(&stmt.dictname)?;
        let options_node = self.list("options", options);
        Ok(self.list("alter-text-search-dictionary", [name, options_node]))
    }

    fn ts_token_names(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        let names = nodes
            .iter()
            .map(|node| self.string_node(node))
            .collect::<Result<Vec<_>>>()?;
        Ok(names.into_iter().map(Sexp::string).collect())
    }

    fn lower_storage_options(&mut self, options: &[pg::Node], context: &str) -> Result<Vec<Sexp>> {
        options
            .iter()
            .map(|node| {
                let option = match self.required_kind(node.node.as_ref(), context)? {
                    PgNode::DefElem(option) => option,
                    _ => return self.unsupported_error(context),
                };
                if !option.defnamespace.is_empty() {
                    return self.unsupported_error(context);
                }

                let action = match pg::DefElemAction::try_from(option.defaction) {
                    Ok(pg::DefElemAction::DefelemUnspec) => "option",
                    Ok(pg::DefElemAction::DefelemSet) => "set-option",
                    Ok(pg::DefElemAction::DefelemAdd) => "add-option",
                    Ok(pg::DefElemAction::DefelemDrop) => "drop-option",
                    _ => return self.unsupported_error(context),
                };
                let mut values = vec![self.symbol(action), Sexp::string(option.defname.clone())];

                match option.arg.as_deref() {
                    Some(argument) => values.push(self.lower_scalar_node(argument)?),
                    None if action == "drop-option" => {}
                    None => return self.unsupported_error(context),
                }
                Ok(Sexp::list(values))
            })
            .collect()
    }

    fn lower_scalar_node(&mut self, node: &pg::Node) -> Result<Sexp> {
        match node.node.as_ref() {
            Some(PgNode::String(value)) => Ok(Sexp::string(value.sval.clone())),
            Some(PgNode::TypeName(typename)) => {
                let names = typename
                    .names
                    .iter()
                    .map(|node| self.string_node(node))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Sexp::string(names.join(".")))
            }
            _ => self.lower(node),
        }
    }

    fn define_scalar_argument(&mut self, option: &pg::DefElem, context: &str) -> Result<Sexp> {
        let node = self.required_node(option.arg.as_deref(), context)?;
        self.lower_scalar_node(node)
    }

    fn define_name_argument(&mut self, option: &pg::DefElem, context: &str) -> Result<Sexp> {
        let node = self.required_node(option.arg.as_deref(), context)?;
        let typename = match self.required_kind(node.node.as_ref(), context)? {
            PgNode::TypeName(typename) => typename,
            _ => return self.unsupported_error(context),
        };
        self.lower_qualified_name(&typename.names)
    }

    fn kebab_name(&self, name: &str) -> String {
        name.to_ascii_lowercase().replace('_', "-")
    }

    fn lower_define_aggregate(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        let mut values = vec![
            self.symbol("create-aggregate"),
            self.lower_qualified_name(&stmt.defnames)?,
        ];

        if stmt.replace {
            values.push(self.symbol("or-replace"));
        }
        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }
        match stmt.args.as_slice() {
            [] => {}
            [arguments, direct_count] => {
                let count = match self.required_kind(
                    direct_count.node.as_ref(),
                    "CREATE AGGREGATE direct argument count",
                )? {
                    PgNode::Integer(integer) => integer.ival,
                    _ => return self.unsupported("CREATE AGGREGATE direct argument count"),
                };
                if count < -1 {
                    return self.unsupported("CREATE AGGREGATE direct argument count");
                }

                let parameters = match arguments.node.as_ref() {
                    None => {
                        let wildcard_parameters = [self.symbol("*")];
                        if count != -1 {
                            return self.unsupported("CREATE AGGREGATE arguments");
                        }

                        values.push(self.list("parameters", wildcard_parameters));
                        &[][..]
                    }
                    Some(PgNode::List(list)) => &list.items[..],
                    _ => return self.unsupported("CREATE AGGREGATE arguments"),
                };
                let direct = if count < 0 {
                    parameters.len()
                } else {
                    match usize::try_from(count) {
                        Ok(direct) if direct <= parameters.len() => direct,
                        _ => return self.unsupported("CREATE AGGREGATE direct argument count"),
                    }
                };
                let (direct, ordered_by) = parameters.split_at(direct);
                let params = direct
                    .iter()
                    .map(|x| self.lower_aggregate_parameter(x))
                    .collect::<Result<Vec<_>>>()?;
                self.optional_clause(&mut values, "parameters", params);
                let order_by = ordered_by
                    .iter()
                    .map(|x| self.lower_aggregate_parameter(x))
                    .collect::<Result<Vec<_>>>()?;
                self.optional_clause(&mut values, "order-by", order_by);
            }
            _ => return self.unsupported("CREATE AGGREGATE arguments"),
        }
        for node in &stmt.definition {
            let option = self.define_option(node, "CREATE AGGREGATE option")?;
            values.push(self.lower_aggregate_option(option)?);
        }

        Ok(Sexp::list(values))
    }

    fn lower_aggregate_parameter(&mut self, node: &pg::Node) -> Result<Sexp> {
        let parameter =
            match self.required_kind(node.node.as_ref(), "CREATE AGGREGATE parameter")? {
                PgNode::FunctionParameter(parameter) => parameter,
                _ => return self.unsupported("CREATE AGGREGATE parameter"),
            };
        let parameter_type = self.required(
            parameter.arg_type.as_ref(),
            "CREATE AGGREGATE parameter type",
        )?;
        let type_sexp = self.lower_signature_type(parameter_type)?;
        match pg::FunctionParameterMode::try_from(parameter.mode) {
            Ok(pg::FunctionParameterMode::FuncParamDefault) => Ok(type_sexp),
            Ok(pg::FunctionParameterMode::FuncParamVariadic) => {
                Ok(self.list("variadic", [type_sexp]))
            }
            _ => self.unsupported("CREATE AGGREGATE parameter mode"),
        }
    }

    fn lower_aggregate_option(&mut self, option: &pg::DefElem) -> Result<Sexp> {
        let name = option.defname.to_ascii_lowercase();
        match name.as_str() {
            "sfunc" | "sfunc1" | "stype" | "stype1" | "finalfunc" | "func" | "combinefunc"
            | "serialfunc" | "deserialfunc" | "msfunc" | "minvfunc" | "mstype" | "mstate"
            | "mfinalfunc" | "basetype" => {
                let type_argument =
                    [self.define_type_argument(option, "CREATE AGGREGATE option value")?];
                Ok(self.list(name.as_str(), type_argument))
            }
            "sspace" | "msspace" => {
                let node =
                    self.required_node(option.arg.as_deref(), "CREATE AGGREGATE state size")?;
                let size =
                    match self.required_kind(node.node.as_ref(), "CREATE AGGREGATE state size")? {
                        PgNode::Integer(value) => value.ival,
                        PgNode::AConst(value) => match value.val.as_ref() {
                            Some(a_const::Val::Ival(value)) => value.ival,
                            _ => return self.unsupported_error("CREATE AGGREGATE state size"),
                        },
                        _ => return self.unsupported_error("CREATE AGGREGATE state size"),
                    };
                let size_symbol = [self.symbol(&size.to_string())];
                Ok(self.list(name.as_str(), size_symbol))
            }
            "parallel" => {
                let parallel_option =
                    self.option_string_value(option, "CREATE AGGREGATE PARALLEL option")?;
                let parallel = [self.symbol(&parallel_option)];
                Ok(self.list("parallel", parallel))
            }
            "finalfunc_modify" | "mfinalfunc_modify" => {
                let value =
                    self.option_string_value(option, "CREATE AGGREGATE modification option")?;
                let keyword = match value.as_str() {
                    "read_only" => "read-only",
                    "read_write" => "read-write",
                    "shareable" => "shareable",
                    "modifiable" => "modifiable",
                    _ => return self.unsupported("CREATE AGGREGATE modification option"),
                };
                let modification_symbol = [self.symbol(keyword)];
                Ok(self.list(
                    match name.as_str() {
                        "finalfunc_modify" => "finalfunc-modify",
                        _ => "mfinalfunc-modify",
                    },
                    modification_symbol,
                ))
            }
            "finalfunc_extra" | "mfinalfunc_extra" => {
                let kebab = match name.as_str() {
                    "finalfunc_extra" => "finalfunc-extra",
                    _ => "mfinalfunc-extra",
                };
                let enabled = match option.arg.as_deref() {
                    None => true,
                    Some(_) => self.lower_flag_argument(option, "CREATE AGGREGATE option value")?,
                };
                Ok(self.symbol(&if enabled {
                    kebab.to_owned()
                } else {
                    format!("no-{kebab}")
                }))
            }
            "initcond" | "initcond1" | "initcond2" | "minitcond" => {
                let initial_condition = [Sexp::string(
                    self.define_string_value(option, "CREATE AGGREGATE initial condition")?,
                )];
                Ok(self.list(name.as_str(), initial_condition))
            }
            "sortop" => {
                let value = self.option_string_value(option, "CREATE AGGREGATE SORTOP option")?;
                Ok(self.list("sortop", [Sexp::string(value)]))
            }
            "window" => {
                if option.arg.is_some() {
                    return self.unsupported("CREATE AGGREGATE WINDOW option");
                }
                Ok(self.symbol("window"))
            }
            "hypothetical" => {
                if option.arg.is_some() {
                    return self.unsupported("CREATE AGGREGATE HYPOTHETICAL option");
                }
                Ok(self.symbol("hypothetical"))
            }
            other => self.unsupported(format!("CREATE AGGREGATE option {other}")),
        }
    }

    fn lower_define_operator(&mut self, stmt: &pg::DefineStmt) -> Result<Sexp> {
        if stmt.replace {
            return self.unsupported("CREATE OR REPLACE OPERATOR");
        }

        let mut values = vec![
            self.symbol("create-operator"),
            self.lower_qualified_name(&stmt.defnames)?,
        ];

        if stmt.if_not_exists {
            values.push(Sexp::list([self.symbol("if-exists")]));
        }

        if !stmt.args.is_empty() {
            return self.unsupported("CREATE OPERATOR arguments");
        }

        for node in &stmt.definition {
            let option = self.define_option(node, "CREATE OPERATOR option")?;
            values.push(self.lower_define_operator_option(option)?);
        }

        Ok(Sexp::list(values))
    }

    fn lower_define_operator_option(&mut self, option: &pg::DefElem) -> Result<Sexp> {
        let name = option.defname.to_ascii_lowercase();
        match name.as_str() {
            "leftarg" | "rightarg" | "procedure" | "function" | "restrict" | "join" => {
                let type_argument =
                    [self.define_type_argument(option, "CREATE OPERATOR option value")?];
                Ok(self.list(name.as_str(), type_argument))
            }
            "commutator" | "negator" => self.define_name_list_option(option, name.as_str()),
            "sort1" | "sort2" | "ltcmp" | "gtcmp" => match option.arg.as_deref() {
                None => Ok(self.symbol(&name)),
                Some(_) => self.define_name_list_option(option, name.as_str()),
            },
            "hashes" | "merges" => {
                if option.arg.is_some() {
                    return self.unsupported(format!("CREATE OPERATOR {name} option"));
                }
                Ok(self.symbol(&name))
            }
            _ => self.lower_generic_option(option),
        }
    }

    pub(super) fn define_option<'a>(
        &mut self,
        node: &'a pg::Node,
        context: &str,
    ) -> Result<&'a pg::DefElem> {
        let option = match self.required_kind(node.node.as_ref(), context)? {
            PgNode::DefElem(option) => option,
            _ => return self.unsupported_error(context),
        };
        if !option.defnamespace.is_empty()
            || !matches!(
                pg::DefElemAction::try_from(option.defaction),
                Ok(pg::DefElemAction::DefelemUnspec)
            )
        {
            return self.unsupported_error("DDL option action");
        }

        Ok(option)
    }

    pub(super) fn define_type_argument(
        &mut self,
        option: &pg::DefElem,
        context: &str,
    ) -> Result<Sexp> {
        let node = self.required_node(option.arg.as_deref(), context)?;
        match self.required_kind(node.node.as_ref(), context)? {
            PgNode::TypeName(typename) => self.lower_signature_type(typename),
            PgNode::String(value) => Ok(self.list("type", [Sexp::string(value.sval.clone())])),
            _ => self.unsupported(context),
        }
    }

    fn define_string_value(&mut self, option: &pg::DefElem, context: &str) -> Result<String> {
        let node = self.required_node(option.arg.as_deref(), context)?;
        match self.required_kind(node.node.as_ref(), context)? {
            PgNode::String(value) => Ok(value.sval.clone()),
            PgNode::Integer(value) => Ok(value.ival.to_string()),
            PgNode::Float(value) => Ok(value.fval.clone()),
            PgNode::AConst(value) => match value.val.as_ref() {
                Some(a_const::Val::Sval(value)) => Ok(value.sval.clone()),
                Some(a_const::Val::Ival(value)) => Ok(value.ival.to_string()),
                Some(a_const::Val::Fval(value)) => Ok(value.fval.clone()),
                Some(a_const::Val::Bsval(value)) => Ok(value.bsval.clone()),
                _ => self.unsupported_error(context),
            },
            _ => self.unsupported_error(context),
        }
    }

    pub(super) fn define_name_list_option(
        &mut self,
        option: &pg::DefElem,
        name: &str,
    ) -> Result<Sexp> {
        let node = self.required_node(option.arg.as_deref(), "operator option value")?;
        let names = match self.required_kind(node.node.as_ref(), "operator option value")? {
            PgNode::List(names) => &names.items,
            _ => return self.unsupported_error("operator option value"),
        };
        let name_list = names
            .iter()
            .map(|x| self.string_node(x))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(Sexp::string);

        Ok(self.list(name, name_list))
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
    fn lowers_create_event_trigger() {
        lowers(
            "create event trigger regress_event_trigger on ddl_command_start \
                 execute procedure p()",
            "(create-event-trigger \"regress_event_trigger\" (on \"ddl_command_start\") (execute (function (name \"p\"))))",
        );
        lowers(
            "create event trigger t1 on ddl_command_start \
                 WHEN TAG IN ('CREATE TABLE', 'DROP TABLE') execute procedure p()",
            "(create-event-trigger \"t1\" (on \"ddl_command_start\") (when (tags \"CREATE TABLE\" \"DROP TABLE\")) (execute (function (name \"p\"))))",
        );
    }

    #[test]
    fn lowers_alter_event_trigger() {
        lowers(
            "alter event trigger regress_event_trigger disable",
            "(alter-event-trigger \"regress_event_trigger\" (disable))",
        );
        lowers(
            "alter event trigger regress_event_trigger enable",
            "(alter-event-trigger \"regress_event_trigger\" (enable))",
        );
        lowers(
            "alter event trigger regress_event_trigger enable replica",
            "(alter-event-trigger \"regress_event_trigger\" (enable replica))",
        );
        lowers(
            "alter event trigger regress_event_trigger enable always",
            "(alter-event-trigger \"regress_event_trigger\" (enable always))",
        );
    }

    #[test]
    fn lowers_create_access_method() {
        lowers(
            "CREATE ACCESS METHOD gist2 TYPE INDEX HANDLER gisthandler",
            "(create-access-method \"gist2\" (type index) (handler (name \"gisthandler\")))",
        );
        lowers(
            "CREATE ACCESS METHOD heap2 TYPE TABLE HANDLER heap_tableam_handler",
            "(create-access-method \"heap2\" (type table) (handler (name \"heap_tableam_handler\")))",
        );
    }

    #[test]
    fn lowers_create_language() {
        lowers(
            "CREATE LANGUAGE alt_lang1 HANDLER plpgsql_call_handler",
            "(create-language \"alt_lang1\" (handler (name \"plpgsql_call_handler\")))",
        );
        lowers(
            "CREATE OR REPLACE TRUSTED LANGUAGE plx HANDLER h INLINE i VALIDATOR v",
            "(create-language \"plx\" or-replace trusted (handler (name \"h\")) (inline (name \"i\")) (validator (name \"v\")))",
        );
    }

    #[test]
    fn lowers_create_transform() {
        lowers(
            "CREATE TRANSFORM FOR int LANGUAGE SQL ( \
                 FROM SQL WITH FUNCTION prsd_lextype(internal), \
                 TO SQL WITH FUNCTION int4recv(internal))",
            "(create-transform (for (type \"pg_catalog\" \"int4\")) (language \"sql\") (from-sql (name \"prsd_lextype\" (parameters (type \"internal\")))) (to-sql (name \"int4recv\" (parameters (type \"internal\")))))",
        );
        lowers(
            "CREATE OR REPLACE TRANSFORM FOR int LANGUAGE SQL ( \
                 FROM SQL WITH FUNCTION f(), TO SQL WITH FUNCTION g())",
            "(create-transform or-replace (for (type \"pg_catalog\" \"int4\")) (language \"sql\") (from-sql (name \"f\")) (to-sql (name \"g\")))",
        );
    }

    #[test]
    fn lowers_create_range_type() {
        lowers(
            "create type textrange_c as range(subtype=text, collation=\"C\")",
            "(create-type (name \"textrange_c\") (as-range (subtype (type \"text\")) (collation (type \"C\"))))",
        );
        lowers(
            "CREATE TYPE deptest_range AS RANGE (SUBTYPE = int4)",
            "(create-type (name \"deptest_range\") (as-range (subtype (type \"int4\"))))",
        );
        lowers(
            "create type txr as range (subtype = int4, subtype_opclass = int4_ops, \
                 canonical = fn, subtype_diff = fn2, multirange_type_name = mr)",
            "(create-type (name \"txr\") (as-range (subtype (type \"int4\")) (subtype-opclass (type \"int4_ops\")) (canonical (type \"fn\")) (subtype-diff (type \"fn2\")) (multirange-type-name (type \"mr\"))))",
        );
    }

    #[test]
    fn lowers_alter_collation_refresh_version() {
        lowers(
            "ALTER COLLATION \"en-x-icu\" REFRESH VERSION",
            "(alter-collation (name \"en-x-icu\") (refresh-version))",
        );
    }

    #[test]
    fn lowers_alter_type_set_options() {
        lowers(
            "ALTER TYPE myvarchar SET (storage = extended)",
            "(alter-type (name \"myvarchar\") (set-options (option \"storage\" \"extended\")))",
        );
    }

    #[test]
    fn lowers_alter_table_move_all() {
        lowers(
            "ALTER TABLE ALL IN TABLESPACE regress_tblspace_renamed SET TABLESPACE pg_default",
            "(move-all-in-tablespace (type table) (from-tablespace \"regress_tblspace_renamed\") (to-tablespace \"pg_default\"))",
        );
        lowers(
            "ALTER INDEX ALL IN TABLESPACE ts1 OWNED BY regress_user1 \
                 SET TABLESPACE ts2 NOWAIT",
            "(move-all-in-tablespace (type index) (from-tablespace \"ts1\") (owner (role \"regress_user1\")) (to-tablespace \"ts2\") nowait)",
        );
        lowers(
            "ALTER MATERIALIZED VIEW ALL IN TABLESPACE ts1 SET TABLESPACE ts2",
            "(move-all-in-tablespace (type materialized-view) (from-tablespace \"ts1\") (to-tablespace \"ts2\"))",
        );
    }

    #[test]
    fn lowers_alter_text_search_configuration() {
        lowers(
            "ALTER TEXT SEARCH CONFIGURATION test_tsconfig \
                 ADD MAPPING FOR email WITH simple",
            "(alter-text-search-configuration (name \"test_tsconfig\") (add-mapping (tokens \"email\") (dictionaries (name \"simple\"))))",
        );
        lowers(
            "ALTER TEXT SEARCH CONFIGURATION test_tsconfig \
                 ALTER MAPPING FOR email WITH ispell",
            "(alter-text-search-configuration (name \"test_tsconfig\") (alter-mapping (tokens \"email\") (dictionaries (name \"ispell\"))))",
        );
        lowers(
            "ALTER TEXT SEARCH CONFIGURATION ispell_tst ALTER MAPPING \
                 REPLACE stword WITH stword_nostop",
            "(alter-text-search-configuration (name \"ispell_tst\") (replace-dictionary (from (name \"stword\")) (to (name \"stword_nostop\"))))",
        );
        lowers(
            "ALTER TEXT SEARCH CONFIGURATION ispell_tst ALTER MAPPING FOR word, numword, \
                 asciiword REPLACE stword WITH stword_nostop",
            "(alter-text-search-configuration (name \"ispell_tst\") (replace-dictionary (tokens \"word\" \"numword\" \"asciiword\") (from (name \"stword\")) (to (name \"stword_nostop\"))))",
        );
        lowers(
            "ALTER TEXT SEARCH CONFIGURATION test_tsconfig DROP MAPPING FOR email",
            "(alter-text-search-configuration (name \"test_tsconfig\") (drop-mapping (tokens \"email\")))",
        );
        lowers(
            "ALTER TEXT SEARCH CONFIGURATION test_tsconfig \
                 DROP MAPPING IF EXISTS FOR email",
            "(alter-text-search-configuration (name \"test_tsconfig\") (drop-mapping (tokens \"email\") (if-exists)))",
        );
    }

    #[test]
    fn lowers_alter_text_search_dictionary() {
        lowers(
            "ALTER TEXT SEARCH DICTIONARY synonym (CaseSensitive = 1)",
            "(alter-text-search-dictionary (name \"synonym\") (options (option \"casesensitive\" 1)))",
        );
        lowers(
            "ALTER TEXT SEARCH DICTIONARY synonym (CaseSensitive = off)",
            "(alter-text-search-dictionary (name \"synonym\") (options (option \"casesensitive\" \"off\")))",
        );
    }

    #[test]
    fn lowers_define_collation() {
        lowers(
            "CREATE COLLATION mycoll1 FROM \"C\"",
            "(create-collation (name \"mycoll1\") (from (name \"C\")))",
        );
        lowers(
            "CREATE COLLATION test0 (provider = icu, locale = 'de-u-co-phonebk', \
                 deterministic = false)",
            "(create-collation (name \"test0\") (provider \"icu\") (locale \"de-u-co-phonebk\") (deterministic \"false\"))",
        );
        lowers(
            "CREATE COLLATION case_coll (\"Lc_Collate\" = \"POSIX\", \
                 \"Lc_Ctype\" = \"POSIX\")",
            "(create-collation (name \"case_coll\") (lc-collate \"POSIX\") (lc-ctype \"POSIX\"))",
        );
        lowers(
            "CREATE COLLATION IF NOT EXISTS coll1 (provider = libc)",
            "(create-collation (name \"coll1\") (if-exists) (provider \"libc\"))",
        );
    }

    #[test]
    fn lowers_define_shell_type() {
        lowers(
            "CREATE TYPE myvarchar",
            "(create-type (name \"myvarchar\"))",
        );
    }

    #[test]
    fn lowers_define_text_search_objects() {
        lowers(
            "CREATE TEXT SEARCH CONFIGURATION alt_ts_conf1 (copy=english)",
            "(create-text-search-configuration (name \"alt_ts_conf1\") (copy (name \"english\")))",
        );
        lowers(
            "CREATE TEXT SEARCH DICTIONARY alt_ts_dict1 (template=simple)",
            "(create-text-search-dictionary (name \"alt_ts_dict1\") (template (name \"simple\")))",
        );
        lowers(
            "CREATE TEXT SEARCH PARSER alt_ts_prs1 \
                 (start = prsd_start, gettoken = prsd_nexttoken, end = prsd_end, \
                 lextypes = prsd_lextype)",
            "(create-text-search-parser (name \"alt_ts_prs1\") (start (name \"prsd_start\")) (gettoken (name \"prsd_nexttoken\")) (end (name \"prsd_end\")) (lextypes (name \"prsd_lextype\")))",
        );
        lowers(
            "CREATE TEXT SEARCH TEMPLATE alt_ts_temp1 (lexize=dsimple_lexize)",
            "(create-text-search-template (name \"alt_ts_temp1\") (lexize (name \"dsimple_lexize\")))",
        );
    }

    #[test]
    fn lowers_define_type_definition() {
        lowers(
            "CREATE TYPE widget ( internallength = 24, input = widget_in, \
                 output = widget_out, typmod_in = numerictypmodin, \
                 typmod_out = numerictypmodout, alignment = double )",
            "(create-type (name \"widget\") (internallength 24) (input (name \"widget_in\")) (output (name \"widget_out\")) (typmod-in (name \"numerictypmodin\")) (typmod-out (name \"numerictypmodout\")) (alignment \"double\"))",
        );
        lowers(
            "CREATE TYPE int42 ( internallength = 4, input = int42_in, \
                 output = int42_out, alignment = int4, default = 42, passedbyvalue )",
            "(create-type (name \"int42\") (internallength 4) (input (name \"int42_in\")) (output (name \"int42_out\")) (alignment \"int4\") (default 42) passedbyvalue)",
        );
        lowers(
            "CREATE TYPE bogus_type ( \"Internallength\" = 4, \"Input\" = int42_in, \
                 \"Output\" = int42_out, \"Alignment\" = int4, \"Default\" = 42, \
                 PASSEDBYVALUE )",
            "(create-type (name \"bogus_type\") (internallength 4) (input (name \"int42_in\")) (output (name \"int42_out\")) (alignment \"int4\") (default 42) passedbyvalue)",
        );
        lowers(
            "create type int8alias1 ( input = int8alias1in, output = int8alias1out, like = int8 )",
            "(create-type (name \"int8alias1\") (input (name \"int8alias1in\")) (output (name \"int8alias1out\")) (like (type \"int8\")))",
        );
        lowers(
            "CREATE TYPE myvarchar ( input = myvarcharin, output = myvarcharout, \
                 alignment = integer, storage = main )",
            "(create-type (name \"myvarchar\") (input (name \"myvarcharin\")) (output (name \"myvarcharout\")) (alignment \"pg_catalog.int4\") (storage \"main\"))",
        );
        lowers(
            "CREATE TYPE text_w_default ( internallength = variable, \
                 input = text_w_default_in, output = text_w_default_out, \
                 default = 'zippo' )",
            "(create-type (name \"text_w_default\") (internallength \"variable\") (input (name \"text_w_default_in\")) (output (name \"text_w_default_out\")) (default \"zippo\"))",
        );
        lowers(
            "CREATE TYPE city_budget ( internallength = 16, input = int44in, \
                 output = int44out, element = int4, category = 'x', preferred = 'true' )",
            "(create-type (name \"city_budget\") (internallength 16) (input (name \"int44in\")) (output (name \"int44out\")) (element (type \"int4\")) (category \"x\") (preferred \"true\"))",
        );
        lowers(
            "CREATE TYPE sqljson_mystr ( INPUT = sqljson_mystr_in, \
                 OUTPUT = sqljson_mystr_out, LIKE = text, CATEGORY = 'S' )",
            "(create-type (name \"sqljson_mystr\") (input (name \"sqljson_mystr_in\")) (output (name \"sqljson_mystr_out\")) (like (type \"text\")) (category \"S\"))",
        );
    }

    #[test]
    fn lowers_define_text_search_dictionary_file_options() {
        lowers(
            "CREATE TEXT SEARCH DICTIONARY hunspell ( Template=ispell, \
                 DictFile=ispell_sample, AffFile=ispell_sample )",
            "(create-text-search-dictionary (name \"hunspell\") (template (name \"ispell\")) (dictfile \"ispell_sample\") (afffile \"ispell_sample\"))",
        );
        lowers(
            "CREATE TEXT SEARCH DICTIONARY synonym ( Template=synonym, Synonyms=synonym_sample )",
            "(create-text-search-dictionary (name \"synonym\") (template (name \"synonym\")) (synonyms \"synonym_sample\"))",
        );
        lowers(
            "CREATE TEXT SEARCH DICTIONARY thesaurus ( Template=thesaurus, \
                 DictFile=thesaurus_sample, Dictionary=english_stem )",
            "(create-text-search-dictionary (name \"thesaurus\") (template (name \"thesaurus\")) (dictfile \"thesaurus_sample\") (dictionary (name \"english_stem\")))",
        );
        lowers(
            "CREATE TEXT SEARCH DICTIONARY tsdict_case ( Template = ispell, \
                 \"DictFile\" = ispell_sample, \"AffFile\" = ispell_sample, \
                 \"StopWords\" = english )",
            "(create-text-search-dictionary (name \"tsdict_case\") (template (name \"ispell\")) (dictfile \"ispell_sample\") (afffile \"ispell_sample\") (stopwords \"english\"))",
        );
    }

    #[test]
    fn lowers_custom_text_search_dictionary_options() {
        lowers(
            "CREATE TEXT SEARCH DICTIONARY custom_dict \
                 (template = simple, custom_option = 'custom value', custom_count = 42)",
            "(create-text-search-dictionary (name \"custom_dict\") (template (name \"simple\")) (option \"custom_option\" \"custom value\") (option \"custom_count\" 42))",
        );
    }

    #[test]
    fn lowers_define_text_search_template_init() {
        lowers(
            "create text search template alter1.tmpl(init = dsimple_init, lexize = dsimple_lexize)",
            "(create-text-search-template (name \"alter1\" \"tmpl\") (init (name \"dsimple_init\")) (lexize (name \"dsimple_lexize\")))",
        );
        lowers(
            "CREATE TEXT SEARCH TEMPLATE tstemp_case ( INIT = init_function )",
            "(create-text-search-template (name \"tstemp_case\") (init (name \"init_function\")))",
        );
    }

    #[test]
    fn lowers_define_text_search_configuration_parser() {
        lowers(
            "create text search configuration alter1.cfg(parser = alter1.prs)",
            "(create-text-search-configuration (name \"alter1\" \"cfg\") (parser (name \"alter1\" \"prs\")))",
        );
    }

    #[test]
    fn lowers_define_collation_rules_version_and_source() {
        lowers(
            "CREATE COLLATION testcoll_rules1 (provider = icu, locale = '', \
                 rules = '&a < g')",
            "(create-collation (name \"testcoll_rules1\") (provider \"icu\") (locale \"\") (rules \"&a < g\"))",
        );
        lowers(
            "CREATE COLLATION coll_dup_chk (FROM = \"C\", VERSION = \"1\")",
            "(create-collation (name \"coll_dup_chk\") (from (name \"C\")) (version \"1\"))",
        );
        lowers(
            "CREATE COLLATION coll_dup_chk (VERSION = '1', VERSION = \"NONSENSE\", \
                 LOCALE = '')",
            "(create-collation (name \"coll_dup_chk\") (version \"1\") (version \"NONSENSE\") (locale \"\"))",
        );
    }

    #[test]
    fn lowers_create_event_trigger_non_tag_filter() {
        lowers(
            "create event trigger regress_event_trigger2 on ddl_command_start \
                 WHEN food in ('sandwich') execute procedure p()",
            "(create-event-trigger \"regress_event_trigger2\" (on \"ddl_command_start\") (when (food \"sandwich\")) (execute (function (name \"p\"))))",
        );
    }

    #[test]
    fn lower_create_aggregate_old_options() {
        lowers(
            "CREATE AGGREGATE agg(int) (SFUNC = sfunc, STYPE = int)",
            "(create-aggregate (name \"agg\") (parameters (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\")))",
        );
    }

    #[test]
    fn lower_create_aggregate_schema_qualified() {
        lowers(
            "CREATE AGGREGATE public.agg(int4) (SFUNC = sfunc, STYPE = int4, FINALFUNC = ffunc, INITCOND = '0')",
            "(create-aggregate (name \"public\" \"agg\") (parameters (type \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"int4\")) (finalfunc (type \"ffunc\")) (initcond \"0\"))",
        );
    }

    #[test]
    fn lower_create_aggregate_new_style_func() {
        lowers(
            "CREATE AGGREGATE agg(int) (FUNC = f, STYPE = int)",
            "(create-aggregate (name \"agg\") (parameters (type \"pg_catalog\" \"int4\")) (func (type \"f\")) (stype (type \"pg_catalog\" \"int4\")))",
        );
    }

    #[test]
    fn lower_create_aggregate_order_by_func_typmods() {
        lowers(
            "CREATE AGGREGATE agg(float8, float8 ORDER BY float8) (FUNC = f(int, int), STYPE = float8)",
            "(create-aggregate (name \"agg\") (parameters (type \"float8\") (type \"float8\")) (order-by (type \"float8\")) (func (type \"f\" (modifiers (column \"int\") (column \"int\")))) (stype (type \"float8\")))",
        );
    }

    #[test]
    fn lower_create_aggregate_parallel() {
        lowers(
            "CREATE AGGREGATE agg(int) (SFUNC = sfunc, STYPE = int, PARALLEL = SAFE)",
            "(create-aggregate (name \"agg\") (parameters (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\")) (parallel safe))",
        );
    }

    #[test]
    fn lower_create_aggregate_sortop() {
        lowers(
            "CREATE AGGREGATE pct(float ORDER BY int) (SFUNC = sfunc, STYPE = float, SORTOP = \">\")",
            "(create-aggregate (name \"pct\") (parameters (type \"pg_catalog\" \"float8\")) (order-by (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"float8\")) (sortop \">\"))",
        );
    }

    #[test]
    fn lower_create_aggregate_order_by_multi() {
        lowers(
            "CREATE AGGREGATE pct(float, int ORDER BY int) (SFUNC = sfunc, STYPE = float)",
            "(create-aggregate (name \"pct\") (parameters (type \"pg_catalog\" \"float8\") (type \"pg_catalog\" \"int4\")) (order-by (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"float8\")))",
        );
    }

    #[test]
    fn lower_create_aggregate_order_by_only() {
        lowers(
            "CREATE AGGREGATE agg(ORDER BY int) (SFUNC = sfunc, STYPE = int)",
            "(create-aggregate (name \"agg\") (order-by (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\")))",
        );
    }

    #[test]
    fn lower_create_aggregate_window() {
        lowers(
            "CREATE AGGREGATE agg(int) (SFUNC = sfunc, STYPE = int, WINDOW)",
            "(create-aggregate (name \"agg\") (parameters (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\")) window)",
        );
    }

    #[test]
    fn lower_create_aggregate_moving() {
        lowers(
            "CREATE AGGREGATE agg(float8) (SFUNC = sfunc, STYPE = float8, MSFUNC = msfunc, MINVFUNC = minvfunc, MSTATE = float8, MFINALFUNC = mfinalfunc, MINITCOND = '0')",
            "(create-aggregate (name \"agg\") (parameters (type \"float8\")) (sfunc (type \"sfunc\")) (stype (type \"float8\")) (msfunc (type \"msfunc\")) (minvfunc (type \"minvfunc\")) (mstate (type \"float8\")) (mfinalfunc (type \"mfinalfunc\")) (minitcond \"0\"))",
        );
    }

    #[test]
    fn lower_create_aggregate_finalfunc_modify() {
        lowers(
            "CREATE AGGREGATE agg(int) (SFUNC = sfunc, STYPE = int, FINALFUNC_MODIFY = SHAREABLE)",
            "(create-aggregate (name \"agg\") (parameters (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\")) (finalfunc-modify shareable))",
        );
    }

    #[test]
    fn lower_create_aggregate_combine_serial_deserial() {
        lowers(
            "CREATE AGGREGATE agg(int) (SFUNC = sfunc, STYPE = int, COMBINEFUNC = cfunc, SERIALFUNC = sfn, DESERIALFUNC = dfn)",
            "(create-aggregate (name \"agg\") (parameters (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\")) (combinefunc (type \"cfunc\")) (serialfunc (type \"sfn\")) (deserialfunc (type \"dfn\")))",
        );
    }

    #[test]
    fn lower_create_aggregate_star() {
        lowers(
            "CREATE AGGREGATE agg(*) (SFUNC = sfunc, STYPE = int)",
            "(create-aggregate (name \"agg\") (parameters *) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\")))",
        );
    }

    #[test]
    fn lower_create_aggregate_basetype() {
        lowers(
            "CREATE AGGREGATE agg (BASETYPE = int, SFUNC = sfunc, STYPE = int)",
            "(create-aggregate (name \"agg\") (basetype (type \"pg_catalog\" \"int4\")) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\")))",
        );
    }

    #[test]
    fn lower_create_aggregate_variadic() {
        lowers(
            "CREATE AGGREGATE agg(VARIADIC int[]) (SFUNC = sfunc, STYPE = int[])",
            "(create-aggregate (name \"agg\") (parameters (variadic (type \"pg_catalog\" \"int4\" (array)))) (sfunc (type \"sfunc\")) (stype (type \"pg_catalog\" \"int4\" (array))))",
        );
    }

    #[test]
    fn lower_create_operator_basic() {
        lowers(
            "CREATE OPERATOR + (LEFTARG = int, RIGHTARG = int, PROCEDURE = fn)",
            "(create-operator (name \"+\") (leftarg (type \"pg_catalog\" \"int4\")) (rightarg (type \"pg_catalog\" \"int4\")) (procedure (type \"fn\")))",
        );
    }

    #[test]
    fn lower_create_operator_commutator_negator() {
        lowers(
            "CREATE OPERATOR <<< (LEFTARG = int, RIGHTARG = int, PROCEDURE = fn, COMMUTATOR = >>>, NEGATOR = !)",
            "(create-operator (name \"<<<\") (leftarg (type \"pg_catalog\" \"int4\")) (rightarg (type \"pg_catalog\" \"int4\")) (procedure (type \"fn\")) (commutator \">>>\") (negator \"!\"))",
        );
    }

    #[test]
    fn lower_create_operator_hashes_merges() {
        lowers(
            "CREATE OPERATOR &&& (LEFTARG = int, RIGHTARG = int, PROCEDURE = fn, HASHES, MERGES)",
            "(create-operator (name \"&&&\") (leftarg (type \"pg_catalog\" \"int4\")) (rightarg (type \"pg_catalog\" \"int4\")) (procedure (type \"fn\")) hashes merges)",
        );
    }

    #[test]
    fn lower_create_operator_restrict_join() {
        lowers(
            "CREATE OPERATOR |>> (LEFTARG = int, RIGHTARG = int, PROCEDURE = fn, RESTRICT = rproc, JOIN = jproc)",
            "(create-operator (name \"|>>\") (leftarg (type \"pg_catalog\" \"int4\")) (rightarg (type \"pg_catalog\" \"int4\")) (procedure (type \"fn\")) (restrict (type \"rproc\")) (join (type \"jproc\")))",
        );
    }

    #[test]
    fn lower_create_operator_qualified_commutator() {
        lowers(
            "CREATE OPERATOR << (LEFTARG = int, RIGHTARG = int, PROCEDURE = fn, COMMUTATOR = OPERATOR(public.>>))",
            "(create-operator (name \"<<\") (leftarg (type \"pg_catalog\" \"int4\")) (rightarg (type \"pg_catalog\" \"int4\")) (procedure (type \"fn\")) (commutator \"public\" \">>\"))",
        );
    }

    #[test]
    fn lower_create_operator_leftarg_only() {
        lowers(
            "CREATE OPERATOR + (LEFTARG = int, PROCEDURE = fn)",
            "(create-operator (name \"+\") (leftarg (type \"pg_catalog\" \"int4\")) (procedure (type \"fn\")))",
        );
    }

    #[test]
    fn define_object_types_supported() {
        assert!(parse("CREATE COLLATION c (LC_COLLATE = \"de_DE\")").is_ok());
        assert!(parse("CREATE TYPE x").is_ok());
    }

    #[test]
    fn unsupported_define_options() {
        assert!(parse("CREATE AGGREGATE agg(int) (BOGUS = 1)").is_err());
    }

    #[test]
    fn lowers_aggregate_old_syntax_options() {
        lowers(
            "CREATE AGGREGATE alt_agg1 (sfunc1 = int4pl, basetype = int4, \
                     stype1 = int4, initcond = '0')",
            "(create-aggregate (name \"alt_agg1\") (sfunc1 (type \"int4pl\")) (basetype (type \"int4\")) (stype1 (type \"int4\")) (initcond \"0\"))",
        );
    }

    #[test]
    fn lowers_aggregate_parallel_value() {
        lowers(
            "CREATE AGGREGATE mysum (int) ( stype = int, sfunc = int4pl, \
                     parallel = pear )",
            "(create-aggregate (name \"mysum\") (parameters (type \"pg_catalog\" \"int4\")) (stype (type \"pg_catalog\" \"int4\")) (sfunc (type \"int4pl\")) (parallel pear))",
        );
    }

    #[test]
    fn lowers_aggregate_sspace() {
        lowers(
            "CREATE AGGREGATE spaced(int8) (sfunc = int8pl, stype = int8, \
                     sspace = 1048576)",
            "(create-aggregate (name \"spaced\") (parameters (type \"int8\")) (sfunc (type \"int8pl\")) (stype (type \"int8\")) (sspace 1048576))",
        );
    }

    #[test]
    fn lowers_aggregate_finalfunc_extra_flag() {
        lowers(
            "CREATE AGGREGATE extra(int8) (sfunc = int8pl, stype = int8, \
                     finalfunc_extra)",
            "(create-aggregate (name \"extra\") (parameters (type \"int8\")) (sfunc (type \"int8pl\")) (stype (type \"int8\")) finalfunc-extra)",
        );
        lowers(
            "CREATE AGGREGATE noextra(int8) (sfunc = int8pl, stype = int8, \
                     finalfunc_extra = false)",
            "(create-aggregate (name \"noextra\") (parameters (type \"int8\")) (sfunc (type \"int8pl\")) (stype (type \"int8\")) no-finalfunc-extra)",
        );
    }

    #[test]
    fn lowers_aggregate_finalfunc_modify_read_write() {
        lowers(
            "CREATE AGGREGATE writing(float8) (sfunc = ordered_set_transition, \
                     stype = internal, finalfunc_modify = read_write)",
            "(create-aggregate (name \"writing\") (parameters (type \"float8\")) (sfunc (type \"ordered_set_transition\")) (stype (type \"internal\")) (finalfunc-modify read-write))",
        );
    }

    #[test]
    fn lowers_aggregate_hypothetical() {
        lowers(
            "CREATE AGGREGATE hypo(float8 ORDER BY float8) (sfunc = ordered_set_transition, \
                     stype = internal, hypothetical)",
            "(create-aggregate (name \"hypo\") (parameters (type \"float8\")) (order-by (type \"float8\")) (sfunc (type \"ordered_set_transition\")) (stype (type \"internal\")) hypothetical)",
        );
    }

    #[test]
    fn lowers_create_or_replace_aggregate() {
        lowers(
            "CREATE OR REPLACE AGGREGATE agg(int8) (sfunc = int8pl, stype = int8)",
            "(create-aggregate (name \"agg\") or-replace (parameters (type \"int8\")) (sfunc (type \"int8pl\")) (stype (type \"int8\")))",
        );
    }

    #[test]
    fn lowers_operator_sort_flags() {
        lowers(
            "CREATE OPERATOR === (LEFTARG = box, RIGHTARG = box, \
                     PROCEDURE = area_equal_procedure, SORT1, SORT2, HASHES)",
            "(create-operator (name \"===\") (leftarg (type \"box\")) (rightarg (type \"box\")) (procedure (type \"area_equal_procedure\")) sort1 sort2 hashes)",
        );
    }

    #[test]
    fn lowers_operator_sort_flag_values() {
        lowers(
            "CREATE OPERATOR === (LEFTARG = box, RIGHTARG = box, \
                     PROCEDURE = area_equal_procedure, COMMUTATOR = ====, \
                     MERGES, LTCMP = <, GTCMP = >)",
            "(create-operator (name \"===\") (leftarg (type \"box\")) (rightarg (type \"box\")) (procedure (type \"area_equal_procedure\")) (commutator \"====\") merges (ltcmp \"<\") (gtcmp \">\"))",
        );
    }
}
