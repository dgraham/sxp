use super::*;

fn explain_option_default(name: &str, analyze: bool) -> bool {
    match name {
        "analyze" | "verbose" | "settings" | "buffers" | "wal" | "generic_plan" | "memory" => false,
        "costs" | "timing" => true,
        "summary" => analyze,
        _ => false,
    }
}

enum ExplainOption<'a> {
    Boolean(&'a str, bool),
    Format(&'a str),
    Serialize(&'a str),
}

impl PostgresParser {
    pub(super) fn lower_explain(&mut self, stmt: &pg::ExplainStmt) -> Result<Sexp> {
        let options = stmt
            .options
            .iter()
            .map(|x| self.explain_option(x))
            .collect::<Result<Vec<_>>>()?;
        let mut analyze = false;
        for option in &options {
            if let ExplainOption::Boolean("analyze", value) = option {
                analyze = *value;
            }
        }

        let mut values = vec![self.symbol("explain")];

        values.extend(
            options
                .iter()
                .filter_map(|option| self.lower_explain_option(option, analyze)),
        );
        let query_node = self.required_node(stmt.query.as_deref(), "EXPLAIN query")?;
        let query = [self.lower(query_node)?];
        values.push(self.list("query", query));

        Ok(Sexp::list(values))
    }

    fn lower_explain_option(&mut self, option: &ExplainOption<'_>, analyze: bool) -> Option<Sexp> {
        match option {
            ExplainOption::Format("text") => None,
            ExplainOption::Format(format) => {
                let format_value = [self.symbol(format)];
                Some(self.list("format", format_value))
            }
            ExplainOption::Serialize(mode) => {
                let serialize_value = [self.symbol(mode)];
                Some(self.list("serialize", serialize_value))
            }
            ExplainOption::Boolean(name, value) => {
                let default = explain_option_default(name, analyze);
                (value != &default).then(|| {
                    let name = match *name {
                        "generic_plan" => "generic-plan",
                        name => name,
                    };
                    if *value {
                        self.symbol(name)
                    } else {
                        let false_symbol = [self.symbol("false")];
                        self.list(name, false_symbol)
                    }
                })
            }
        }
    }

    fn explain_option<'a>(&mut self, node: &'a pg::Node) -> Result<ExplainOption<'a>> {
        let option = match self.required_kind(node.node.as_ref(), "EXPLAIN option")? {
            PgNode::DefElem(option) => option,
            _ => return self.unsupported_error("EXPLAIN option"),
        };
        if !option.defnamespace.is_empty()
            || !matches!(
                pg::DefElemAction::try_from(option.defaction),
                Ok(pg::DefElemAction::DefelemUnspec)
            )
        {
            return self.unsupported_error("EXPLAIN option");
        }
        match option.defname.as_str() {
            "analyze" | "verbose" | "costs" | "settings" | "buffers" | "wal" | "timing"
            | "summary" | "generic_plan" | "memory" => Ok(ExplainOption::Boolean(
                option.defname.as_str(),
                match option.arg.as_deref() {
                    Some(node) => self.lower_explain_boolean(node)?,
                    None => true,
                },
            )),
            "format" => self.lower_explain_format(option.arg.as_deref()),
            "serialize" => self.lower_explain_serialize(option.arg.as_deref()),
            name => self.unsupported_error(format!("EXPLAIN option {name}")),
        }
    }

    fn lower_explain_serialize<'a>(
        &mut self,
        node: Option<&'a pg::Node>,
    ) -> Result<ExplainOption<'a>> {
        let mode = match node {
            None => "none",
            Some(node) => {
                match self.required_kind(node.node.as_ref(), "EXPLAIN SERIALIZE option")? {
                    PgNode::String(value) => value.sval.as_str(),
                    PgNode::AConst(value) => match value.val.as_ref() {
                        Some(a_const::Val::Sval(value)) => value.sval.as_str(),
                        _ => return self.unsupported_error("EXPLAIN SERIALIZE option"),
                    },
                    _ => return self.unsupported_error("EXPLAIN SERIALIZE option"),
                }
            }
        };
        match mode {
            "none" | "text" | "binary" => Ok(ExplainOption::Serialize(mode)),
            _ => self.unsupported_error("EXPLAIN SERIALIZE option"),
        }
    }

    fn lower_explain_format<'a>(
        &mut self,
        node: Option<&'a pg::Node>,
    ) -> Result<ExplainOption<'a>> {
        let format = match self
            .required_node(node, "EXPLAIN FORMAT option")?
            .node
            .as_ref()
        {
            Some(PgNode::String(value)) => value.sval.as_str(),
            Some(PgNode::AConst(value)) => match value.val.as_ref() {
                Some(a_const::Val::Sval(value)) => value.sval.as_str(),
                _ => return self.unsupported_error("EXPLAIN FORMAT option"),
            },
            _ => return self.unsupported_error("EXPLAIN FORMAT option"),
        };
        match format {
            "text" | "xml" | "json" | "yaml" => Ok(ExplainOption::Format(format)),
            _ => self.unsupported_error("EXPLAIN FORMAT option"),
        }
    }

    fn lower_explain_boolean(&mut self, node: &pg::Node) -> Result<bool> {
        match self.required_kind(node.node.as_ref(), "EXPLAIN boolean option")? {
            PgNode::Boolean(value) => Ok(value.boolval),
            PgNode::Integer(value) => self.explain_boolean_integer(value.ival),
            PgNode::String(value) => self.explain_boolean_string(&value.sval),
            PgNode::AConst(value) => match value.val.as_ref() {
                Some(a_const::Val::Boolval(value)) => Ok(value.boolval),
                Some(a_const::Val::Ival(value)) => self.explain_boolean_integer(value.ival),
                Some(a_const::Val::Sval(value)) => self.explain_boolean_string(&value.sval),
                _ => self.unsupported_error("EXPLAIN boolean option"),
            },
            _ => self.unsupported_error("EXPLAIN boolean option"),
        }
    }

    fn explain_boolean_string(&mut self, value: &str) -> Result<bool> {
        match value {
            "true" | "on" => Ok(true),
            "false" | "off" => Ok(false),
            _ => self.unsupported_error("EXPLAIN boolean option"),
        }
    }

    fn explain_boolean_integer(&mut self, value: i32) -> Result<bool> {
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => self.unsupported_error("EXPLAIN boolean option"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::PostgresParser;

    fn parse(input: &str) -> sxp::Result<sxp::Sexp> {
        PostgresParser::new().parse(input)
    }

    #[test]
    fn lowers_explain_semantically() {
        assert_eq!(
            parse("EXPLAIN ANALYZE SELECT id FROM users").map(|value| value.to_string()),
            Ok(
                "(explain analyze (query (select (columns (column \"id\")) (from (table (\"users\"))))))"
                    .to_owned()
            )
        );
    }

    #[test]
    fn lowers_generic_plan_options_and_defaults() {
        let generic_plan = parse("EXPLAIN (GENERIC_PLAN) SELECT 1")
            .unwrap()
            .to_string();
        assert!(generic_plan.contains("generic-plan"), "{generic_plan}");

        let generic_plan_true = parse("EXPLAIN (GENERIC_PLAN TRUE) SELECT 1")
            .unwrap()
            .to_string();
        assert!(
            generic_plan_true.contains("generic-plan"),
            "{generic_plan_true}"
        );

        let generic_plan_false = parse("EXPLAIN (GENERIC_PLAN FALSE) SELECT 1")
            .unwrap()
            .to_string();
        assert!(
            !generic_plan_false.contains("generic-plan"),
            "{generic_plan_false}"
        );
        assert_eq!(
            generic_plan_false,
            parse("EXPLAIN SELECT 1").unwrap().to_string(),
            "false is GENERIC_PLAN's default"
        );
    }

    #[test]
    fn lowers_serialize_modes() {
        for (syntax, lowered) in [
            ("SERIALIZE", "(serialize none)"),
            ("SERIALIZE NONE", "(serialize none)"),
            ("SERIALIZE TEXT", "(serialize text)"),
            ("SERIALIZE BINARY", "(serialize binary)"),
        ] {
            let value = parse(&format!("EXPLAIN ({syntax}) SELECT 1"))
                .unwrap()
                .to_string();
            assert!(value.contains(lowered), "{syntax}: {value}");
        }
    }

    #[test]
    fn lowers_memory_option_default_and_argument() {
        let memory = parse("EXPLAIN (MEMORY) SELECT 1").unwrap().to_string();
        assert!(memory.contains("memory"), "{memory}");

        let memory_true = parse("EXPLAIN (MEMORY TRUE) SELECT 1").unwrap().to_string();
        assert!(memory_true.contains("memory"), "{memory_true}");

        let memory_false = parse("EXPLAIN (MEMORY FALSE) SELECT 1")
            .unwrap()
            .to_string();
        assert!(
            !memory_false.contains("memory"),
            "false is the default: {memory_false}"
        );
        assert_eq!(memory_false, parse("EXPLAIN SELECT 1").unwrap().to_string());
    }

    #[test]
    fn rejects_unsupported_explain_options() {
        assert!(parse("EXPLAIN (UNKNOWN_OPTION) SELECT 1").is_err());
        assert!(parse("EXPLAIN (SERIALIZE XML) SELECT 1").is_err());
    }
}
