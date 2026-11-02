use super::*;

impl PostgresParser {
    pub(super) fn lower_copy(&mut self, stmt: &pg::CopyStmt) -> Result<Sexp> {
        let mut values = vec![self.symbol("copy")];

        if let Some(relation) = stmt.relation.as_ref() {
            values.push(self.lower_relation(relation)?);
        } else if let Some(query) = stmt.query.as_deref() {
            let query = [self.lower(query)?];
            values.push(self.list("query", query));
        } else {
            return self.unsupported("COPY source");
        }

        if !stmt.attlist.is_empty() {
            let columns = self.lower_nodes(&stmt.attlist)?;
            values.push(self.list("columns", columns));
        }

        values.push(self.symbol(if stmt.is_from { "from" } else { "to" }));
        if stmt.is_program {
            values.push(self.list("program", [Sexp::string(stmt.filename.clone())]));
        } else if !stmt.filename.is_empty() {
            values.push(self.list("file", [Sexp::string(stmt.filename.clone())]));
        } else {
            values.push(self.symbol(if stmt.is_from { "stdin" } else { "stdout" }));
        }

        if let Some(where_clause) = stmt.where_clause.as_deref() {
            let lowered_where = [self.lower(where_clause)?];
            values.push(self.list("where", lowered_where));
        }
        if !stmt.options.is_empty() {
            let options = stmt
                .options
                .iter()
                .map(
                    |node| match self.required_kind(node.node.as_ref(), "COPY option")? {
                        PgNode::DefElem(option) => self.lower_definition_option(option),
                        _ => self.unsupported("COPY option"),
                    },
                )
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("options", options));
        }

        Ok(Sexp::list(values))
    }
}
