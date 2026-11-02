use super::*;

const OPCLASS_ITEM_OPERATOR: i32 = 1;
const OPCLASS_ITEM_FUNCTION: i32 = 2;
const OPCLASS_ITEM_STORAGE: i32 = 3;

impl PostgresParser {
    pub(crate) fn lower_create_opclass(&mut self, stmt: &pg::CreateOpClassStmt) -> Result<Sexp> {
        let name = self.lower_qualified_name(&stmt.opclassname)?;
        let mut values = vec![self.symbol("create-operator-class"), name];

        if stmt.is_default {
            values.push(Sexp::list([self.symbol("default")]));
        }

        let datatype = self.required(stmt.datatype.as_ref(), "operator class data type")?;
        let datatype_sexp = self.lower_type(datatype)?;
        let for_node = self.list("for", [datatype_sexp]);
        values.push(for_node);
        values.push(self.list("using", [Sexp::string(stmt.amname.clone())]));
        if !stmt.opfamilyname.is_empty() {
            let family = self.lower_qualified_name(&stmt.opfamilyname)?;
            values.push(self.list("family", [family]));
        }
        if !stmt.items.is_empty() {
            let items = self.lower_opclass_items(&stmt.items)?;
            values.push(self.list("as", items));
        }

        Ok(Sexp::list(values))
    }

    pub(crate) fn lower_create_opfamily(&mut self, stmt: &pg::CreateOpFamilyStmt) -> Result<Sexp> {
        let name = self.lower_qualified_name(&stmt.opfamilyname)?;
        let using = self.list("using", [Sexp::string(stmt.amname.clone())]);
        Ok(self.list("create-operator-family", [name, using]))
    }

    pub(crate) fn lower_alter_opfamily(&mut self, stmt: &pg::AlterOpFamilyStmt) -> Result<Sexp> {
        let name = self.lower_qualified_name(&stmt.opfamilyname)?;
        let using = self.list("using", [Sexp::string(stmt.amname.clone())]);
        let items = self.lower_opclass_items(&stmt.items)?;
        let items_node = self.list(if stmt.is_drop { "drop" } else { "add" }, items);
        Ok(self.list("alter-operator-family", [name, using, items_node]))
    }

    pub(crate) fn lower_alter_operator(&mut self, stmt: &pg::AlterOperatorStmt) -> Result<Sexp> {
        let opername = self.required(stmt.opername.as_ref(), "operator name")?;
        let operator = self.lower_object_with_args(opername)?;
        let mut values = vec![self.symbol("alter-operator"), operator];

        for node in &stmt.options {
            let option = self.define_option(node, "ALTER OPERATOR option")?;
            values.push(self.lower_alter_operator_option(option)?);
        }

        Ok(Sexp::list(values))
    }

    fn lower_alter_operator_option(&mut self, option: &pg::DefElem) -> Result<Sexp> {
        let name = option.defname.to_lowercase();
        match name.as_str() {
            "restrict" | "join" => {
                let value = match option.arg.as_deref() {
                    None => self.symbol("none"),
                    Some(node) => {
                        match self
                            .required_kind(node.node.as_ref(), "ALTER OPERATOR option value")?
                        {
                            PgNode::TypeName(typename) => self.lower_signature_type(typename)?,
                            PgNode::String(value) => Sexp::string(value.sval.clone()),
                            _ => return self.unsupported("ALTER OPERATOR option value"),
                        }
                    }
                };
                Ok(self.list(name.as_str(), [value]))
            }
            "commutator" | "negator" => self.define_name_list_option(option, name.as_str()),
            "merges" | "hashes" => match option.arg.as_deref() {
                None => Ok(self.symbol(name.as_str())),
                Some(node) => {
                    match self.required_kind(node.node.as_ref(), "ALTER OPERATOR option value")? {
                        PgNode::String(value) => {
                            let atom = self.symbol(value.sval.as_str());
                            Ok(self.list(name.as_str(), [atom]))
                        }
                        _ => self.unsupported("ALTER OPERATOR option value"),
                    }
                }
            },
            other => self.unsupported(format!("ALTER OPERATOR option {other}")),
        }
    }

    fn lower_opclass_items(&mut self, items: &[pg::Node]) -> Result<Vec<Sexp>> {
        items
            .iter()
            .map(
                |node| match self.required_kind(node.node.as_ref(), "operator class item")? {
                    PgNode::CreateOpClassItem(item) => self.lower_opclass_item(item),
                    _ => self.unsupported("operator class item"),
                },
            )
            .collect()
    }

    fn lower_opclass_item(&mut self, item: &pg::CreateOpClassItem) -> Result<Sexp> {
        match item.itemtype {
            OPCLASS_ITEM_OPERATOR | OPCLASS_ITEM_FUNCTION => {
                let kind = if item.itemtype == OPCLASS_ITEM_OPERATOR {
                    "operator"
                } else {
                    "function"
                };
                let mut values = vec![self.symbol(kind), self.symbol(&item.number.to_string())];

                if let Some(name) = item.name.as_ref() {
                    let node = self.lower_object_with_args(name)?;
                    values.push(node);
                }
                if !item.class_args.is_empty() {
                    let types = self.lower_opclass_type_nodes(&item.class_args)?;
                    values.push(self.list("types", types));
                }
                if !item.order_family.is_empty() {
                    let name = self.lower_qualified_name(&item.order_family)?;
                    values.push(self.list("order-by", [name]));
                }
                Ok(Sexp::list(values))
            }
            OPCLASS_ITEM_STORAGE => {
                let storedtype =
                    self.required(item.storedtype.as_ref(), "operator class storage type")?;
                let storedtype_sexp = self.lower_type(storedtype)?;
                Ok(self.list("storage", [storedtype_sexp]))
            }
            other => self.unsupported(format!("operator class item type {other}")),
        }
    }

    fn lower_opclass_type_nodes(&mut self, nodes: &[pg::Node]) -> Result<Vec<Sexp>> {
        nodes
            .iter()
            .map(|node| {
                match self.required_kind(node.node.as_ref(), "operator class argument type")? {
                    PgNode::TypeName(typename) => self.lower_signature_type(typename),
                    _ => self.unsupported("operator class argument type"),
                }
            })
            .collect()
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
    fn lowers_create_operator_family() {
        lowers(
            "CREATE OPERATOR FAMILY alt_opf1 USING hash",
            "(create-operator-family (name \"alt_opf1\") (using \"hash\"))",
        );
        lowers(
            "CREATE OPERATOR FAMILY nsp.opf USING btree",
            "(create-operator-family (name \"nsp\" \"opf\") (using \"btree\"))",
        );
    }

    #[test]
    fn lowers_create_operator_class_storage() {
        lowers(
            "CREATE OPERATOR CLASS alt_opc1 FOR TYPE uuid USING hash AS STORAGE uuid",
            "(create-operator-class (name \"alt_opc1\") (for (type \"uuid\")) (using \"hash\") \
             (as (storage (type \"uuid\"))))",
        );
        lowers(
            "CREATE OPERATOR CLASS evttrigopclass FOR TYPE int USING btree AS STORAGE int",
            "(create-operator-class (name \"evttrigopclass\") (for (type \"pg_catalog\" \"int4\")) \
             (using \"btree\") (as (storage (type \"pg_catalog\" \"int4\"))))",
        );
    }

    #[test]
    fn lowers_create_operator_class_default() {
        lowers(
            "CREATE OPERATOR CLASS box_ops DEFAULT FOR TYPE box USING gist2 AS OPERATOR 1 <<",
            "(create-operator-class (name \"box_ops\") (default) (for (type \"box\")) \
             (using \"gist2\") (as (operator 1 (name \"<<\"))))",
        );
    }

    #[test]
    fn lowers_create_operator_class_family_and_items() {
        lowers(
            "CREATE OPERATOR CLASS opc FOR TYPE int4 USING btree FAMILY alt_opf1 \
             AS OPERATOR 1 < (int4, int4)",
            "(create-operator-class (name \"opc\") (for (type \"int4\")) (using \"btree\") \
             (family (name \"alt_opf1\")) \
             (as (operator 1 (name \"<\" (parameters (type \"int4\") (type \"int4\"))))))",
        );
    }

    #[test]
    fn lowers_create_operator_class_operators_and_function() {
        lowers(
            "CREATE OPERATOR CLASS test_int4_ops FOR TYPE int4 USING btree AS \
             OPERATOR 1 < (int4,int4), OPERATOR 2 <= (int4,int4), \
             OPERATOR 3 = (int4,int4), OPERATOR 4 >= (int4,int4), \
             OPERATOR 5 > (int4,int4), FUNCTION 1 my_int4_sort(int4,int4)",
            "(create-operator-class (name \"test_int4_ops\") (for (type \"int4\")) (using \"btree\") \
             (as \
             (operator 1 (name \"<\" (parameters (type \"int4\") (type \"int4\")))) \
             (operator 2 (name \"<=\" (parameters (type \"int4\") (type \"int4\")))) \
             (operator 3 (name \"=\" (parameters (type \"int4\") (type \"int4\")))) \
             (operator 4 (name \">=\" (parameters (type \"int4\") (type \"int4\")))) \
             (operator 5 (name \">\" (parameters (type \"int4\") (type \"int4\")))) \
             (function 1 (name \"my_int4_sort\" (parameters (type \"int4\") (type \"int4\"))))))",
        );
    }

    #[test]
    fn lowers_alter_operator_family_add() {
        lowers(
            "ALTER OPERATOR FAMILY alt_opf4 USING btree ADD \
             OPERATOR 1 < (int4, int2), OPERATOR 2 <= (int4, int2), \
             FUNCTION 1 btint42cmp(int4, int2)",
            "(alter-operator-family (name \"alt_opf4\") (using \"btree\") \
             (add \
             (operator 1 (name \"<\" (parameters (type \"int4\") (type \"int2\")))) \
             (operator 2 (name \"<=\" (parameters (type \"int4\") (type \"int2\")))) \
             (function 1 (name \"btint42cmp\" (parameters (type \"int4\") (type \"int2\"))))))",
        );
    }

    #[test]
    fn lowers_alter_operator_family_add_function_op_types() {
        lowers(
            "ALTER OPERATOR FAMILY alt_opf4 USING btree ADD \
             FUNCTION 1 (int4, int2) btint42cmp(int4, int2)",
            "(alter-operator-family (name \"alt_opf4\") (using \"btree\") \
             (add (function 1 (name \"btint42cmp\" (parameters (type \"int4\") (type \"int2\"))) \
             (types (type \"int4\") (type \"int2\")))))",
        );
    }

    #[test]
    fn lowers_alter_operator_family_drop() {
        lowers(
            "ALTER OPERATOR FAMILY alt_opf4 USING btree DROP \
             OPERATOR 1 (int4, int2), FUNCTION 1 (int4, int2)",
            "(alter-operator-family (name \"alt_opf4\") (using \"btree\") \
             (drop (operator 1 (types (type \"int4\") (type \"int2\"))) \
             (function 1 (types (type \"int4\") (type \"int2\")))))",
        );
    }

    #[test]
    fn lowers_alter_operator_family_order_by() {
        lowers(
            "ALTER OPERATOR FAMILY alt_opf9 USING gist ADD \
             OPERATOR 1 < (int4, int4) FOR ORDER BY float_ops",
            "(alter-operator-family (name \"alt_opf9\") (using \"gist\") \
             (add (operator 1 (name \"<\" (parameters (type \"int4\") (type \"int4\"))) \
             (order-by (name \"float_ops\")))))",
        );
    }

    #[test]
    fn lowers_alter_operator_restrict_and_join() {
        lowers(
            "ALTER OPERATOR === (boolean, boolean) SET (RESTRICT = NONE)",
            "(alter-operator (name \"===\" (parameters (type \"pg_catalog\" \"bool\") \
             (type \"pg_catalog\" \"bool\"))) (restrict none))",
        );
        lowers(
            "ALTER OPERATOR === (boolean, boolean) SET (RESTRICT = contsel, JOIN = contjoinsel)",
            "(alter-operator (name \"===\" (parameters (type \"pg_catalog\" \"bool\") \
             (type \"pg_catalog\" \"bool\"))) (restrict (type \"contsel\")) \
             (join (type \"contjoinsel\")))",
        );
    }

    #[test]
    fn lowers_alter_operator_quoted_option_names() {
        lowers(
            "ALTER OPERATOR & (bit, bit) SET \
             (\"Restrict\" = _int_contsel, \"Join\" = _int_contjoinsel)",
            "(alter-operator (name \"&\" (parameters (type \"pg_catalog\" \"bit\" (modifiers 1)) \
             (type \"pg_catalog\" \"bit\" (modifiers 1)))) (restrict (type \"_int_contsel\")) \
             (join (type \"_int_contjoinsel\")))",
        );
    }

    #[test]
    fn lowers_alter_operator_commutator_and_negator() {
        lowers(
            "ALTER OPERATOR === (boolean, real) SET (COMMUTATOR = ====, NEGATOR = !====)",
            "(alter-operator (name \"===\" (parameters (type \"pg_catalog\" \"bool\") \
             (type \"pg_catalog\" \"float4\"))) (commutator \"====\") (negator \"!====\"))",
        );
    }

    #[test]
    fn lowers_alter_operator_merges_and_hashes() {
        lowers(
            "ALTER OPERATOR === (boolean, real) SET (MERGES)",
            "(alter-operator (name \"===\" (parameters (type \"pg_catalog\" \"bool\") \
             (type \"pg_catalog\" \"float4\"))) merges)",
        );
        lowers(
            "ALTER OPERATOR === (boolean, real) SET (HASHES = false)",
            "(alter-operator (name \"===\" (parameters (type \"pg_catalog\" \"bool\") \
             (type \"pg_catalog\" \"float4\"))) (hashes false))",
        );
    }

    #[test]
    fn parses_create_index_opclass() {
        lowers(
            "CREATE INDEX i ON t USING gin (x gin_trgm_ops);",
            "(create-index (name \"i\") (on (table (\"t\"))) (using \"gin\") (columns (index-column \"x\" (opclass \"gin_trgm_ops\"))))",
        );
        lowers(
            "CREATE INDEX i ON t USING gin (x public.gin_trgm_ops);",
            "(create-index (name \"i\") (on (table (\"t\"))) (using \"gin\") (columns (index-column \"x\" (opclass \"public\" \"gin_trgm_ops\"))))",
        );
    }

    #[test]
    fn parses_create_index_opclass_options() {
        lowers(
            "CREATE INDEX i ON t USING gin (x gin_trgm_ops (siglen = 8));",
            "(create-index (name \"i\") (on (table (\"t\"))) (using \"gin\") (columns (index-column \"x\" (opclass \"gin_trgm_ops\" (options (option \"siglen\" 8))))))",
        );
    }

    #[test]
    fn parses_create_index_collate_opclass() {
        lowers(
            "CREATE INDEX i ON t (x COLLATE \"C\" varchar_pattern_ops);",
            "(create-index (name \"i\") (on (table (\"t\"))) (columns (index-column \"x\" (collate \"C\") (opclass \"varchar_pattern_ops\"))))",
        );
    }

    #[test]
    fn lowers_alter_operator_class_rename() {
        lowers(
            "ALTER OPERATOR CLASS alt_op_class1 USING alt_op_index RENAME TO alt_op_class2",
            "(alter-operator-class (using \"alt_op_index\") (name \"alt_op_class1\") (rename \"alt_op_class2\"))",
        );
    }
}
