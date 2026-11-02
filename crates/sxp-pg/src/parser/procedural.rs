use super::*;

impl PostgresParser {
    pub(super) fn lower_return(&mut self, stmt: &pg::ReturnStmt) -> Result<Sexp> {
        let return_exp = self.required_node(stmt.returnval.as_deref(), "RETURN expression")?;
        let ret = [self.lower(return_exp)?];
        Ok(self.list("return", ret))
    }

    pub(super) fn lower_do(&mut self, stmt: &pg::DoStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("do")];

        for node in &stmt.args {
            let option = match self.required_kind(node.node.as_ref(), "DO option")? {
                PgNode::DefElem(option) => option,
                _ => return self.unsupported_error("DO option"),
            };
            match option.defname.as_str() {
                "language" => {
                    let language_node = self.required_node(option.arg.as_deref(), "DO language")?;
                    let language_name = [Sexp::string(self.string_node(language_node)?)];
                    values.push(self.list("language", language_name))
                }
                "as" => {
                    let code_node = self.required_node(option.arg.as_deref(), "DO code")?;
                    let code_string = [Sexp::string(self.string_node(code_node)?)];
                    values.push(self.list("sql", code_string))
                }
                other => return self.unsupported_error(format!("DO option {other}")),
            }
        }

        Ok(Sexp::list(values))
    }

    pub(super) fn lower_call(&mut self, stmt: &pg::CallStmt) -> Result<Sexp> {
        let function = self.required(stmt.funccall.as_deref(), "CALL function")?;

        if stmt.funcexpr.is_some() {
            return self.unsupported("CALL function expression");
        }

        let mut call = match self.lower_function(function)? {
            Sexp::List(values) => Vec::from(values),
            _ => return self.unsupported("CALL function"),
        };
        if !stmt.outargs.is_empty() {
            let outargs = self.lower_nodes(&stmt.outargs)?;
            call.push(self.list("outargs", outargs));
        }

        Ok(Sexp::list(call))
    }
}
