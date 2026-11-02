use super::*;

impl PostgresParser {
    pub(super) fn lower_xml_expr(&mut self, expr: &pg::XmlExpr) -> Result<Sexp> {
        match pg::XmlExprOp::try_from(expr.op) {
            Ok(pg::XmlExprOp::IsXmlconcat) => {
                let xml_concat = self.lower_nodes(&expr.args)?;
                Ok(self.list("xml-concat", xml_concat))
            }
            Ok(pg::XmlExprOp::IsXmlelement) => self.lower_xml_element(expr),
            Ok(pg::XmlExprOp::IsXmlforest) => self.lower_xml_forest(expr),
            Ok(pg::XmlExprOp::IsXmlparse) => self.lower_xml_parse(expr),
            Ok(pg::XmlExprOp::IsXmlpi) => self.lower_xml_pi(expr),
            Ok(pg::XmlExprOp::IsXmlroot) => self.lower_xml_root(expr),
            Ok(pg::XmlExprOp::IsXmlserialize) => {
                if expr.args.len() != 1 {
                    return self.unsupported_error("XMLSERIALIZE arguments");
                }

                let option = self.lower_xml_option(expr.xmloption, "XMLSERIALIZE option")?;
                let serialize_expr = self.required(expr.args.first(), "XMLSERIALIZE expression")?;
                let xml_serialize = [self.symbol(option), self.lower(serialize_expr)?];
                Ok(self.list("xml-serialize", xml_serialize))
            }
            Ok(pg::XmlExprOp::IsDocument) => {
                let is_document = self.lower_nodes(&expr.args)?;
                Ok(self.list("is-document", is_document))
            }
            _ => self.unsupported("XML expression"),
        }
    }

    pub(super) fn lower_xml_option(&mut self, option: i32, context: &str) -> Result<&'static str> {
        match pg::XmlOptionType::try_from(option) {
            Ok(pg::XmlOptionType::XmloptionDocument) => Ok("document"),
            Ok(pg::XmlOptionType::XmloptionContent) => Ok("content"),
            _ => self.unsupported_error(context),
        }
    }

    pub(super) fn lower_xml_derived_name(&mut self, node: &pg::Node) -> Result<Option<String>> {
        match self.required_kind(node.node.as_ref(), "XML name")? {
            PgNode::ColumnRef(reference) => {
                let field = self.required(reference.fields.last(), "XML name field")?;
                Ok(Some(self.string_node(field)?))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn lower_xml_attribute(&mut self, node: &pg::Node) -> Result<Sexp> {
        let target = match self.required_kind(node.node.as_ref(), "XML attribute")? {
            PgNode::ResTarget(target) => target,
            _ => return self.unsupported_error("XML attribute"),
        };
        let value = self.required_node(target.val.as_deref(), "XML attribute expression")?;
        let name = if target.name.is_empty() {
            self.lower_xml_derived_name(value)?
        } else {
            Some(target.name.clone())
        };
        Ok(match name {
            Some(name) => {
                let attr = [self.list("name", [Sexp::string(name)]), self.lower(value)?];
                self.list("attr", attr)
            }
            None => {
                let attr = [self.lower(value)?];
                self.list("attr", attr)
            }
        })
    }

    pub(super) fn lower_xml_element(&mut self, expr: &pg::XmlExpr) -> Result<Sexp> {
        let mut values = vec![self.list("name", [Sexp::string(expr.name.clone())])];

        if !expr.named_args.is_empty() {
            let attributes = expr
                .named_args
                .iter()
                .map(|x| self.lower_xml_attribute(x))
                .collect::<Result<Vec<_>>>()?;
            values.push(self.list("attributes", attributes));
        }

        values.extend(self.lower_nodes(&expr.args)?);

        Ok(self.list("xml-element", values))
    }

    pub(super) fn lower_xml_forest(&mut self, expr: &pg::XmlExpr) -> Result<Sexp> {
        let values = expr
            .named_args
            .iter()
            .map(|node| {
                let target = match self.required_kind(node.node.as_ref(), "XML forest element")? {
                    PgNode::ResTarget(target) => target,
                    _ => return self.unsupported_error("XML forest element"),
                };
                let value = self.required_node(target.val.as_deref(), "XML forest expression")?;
                let name = if target.name.is_empty() {
                    match self.lower_xml_derived_name(value)? {
                        Some(name) => name,
                        None => return self.unsupported_error("XML name"),
                    }
                } else {
                    target.name.clone()
                };
                let elem = [self.list("name", [Sexp::string(name)]), self.lower(value)?];
                Ok(self.list("elem", elem))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(self.list("xml-forest", values))
    }

    pub(super) fn lower_xml_parse(&mut self, expr: &pg::XmlExpr) -> Result<Sexp> {
        if expr.args.len() != 2 {
            return self.unsupported_error("XMLPARSE arguments");
        }

        let option = self.lower_xml_option(expr.xmloption, "XMLPARSE option")?;
        let preserve =
            match self.required_kind(expr.args[1].node.as_ref(), "XMLPARSE whitespace option")? {
                PgNode::AConst(value) => match value.val.as_ref() {
                    Some(a_const::Val::Boolval(boolean)) => boolean.boolval,
                    _ => return self.unsupported_error("XMLPARSE whitespace option"),
                },
                _ => return self.unsupported_error("XMLPARSE whitespace option"),
            };
        let mut values = vec![self.symbol(option), self.lower(&expr.args[0])?];

        if preserve {
            values.push(self.symbol("preserve-whitespace"));
        }

        Ok(self.list("xml-parse", values))
    }

    pub(super) fn lower_xml_pi(&mut self, expr: &pg::XmlExpr) -> Result<Sexp> {
        if expr.args.len() > 1 {
            return self.unsupported_error("XMLPI arguments");
        }

        let mut values = vec![self.list("name", [Sexp::string(expr.name.clone())])];

        if let Some(content) = expr.args.first() {
            values.push(self.lower(content)?);
        }
        Ok(self.list("xml-pi", values))
    }

    pub(super) fn lower_xml_root(&mut self, expr: &pg::XmlExpr) -> Result<Sexp> {
        let version = [self.lower(&expr.args[1])?];

        if expr.args.len() != 3 {
            return self.unsupported_error("XMLROOT arguments");
        }

        let mut values = vec![self.lower(&expr.args[0])?, self.list("version", version)];

        match self.required_kind(expr.args[2].node.as_ref(), "XMLROOT standalone")? {
            PgNode::AConst(value) => match value.val.as_ref() {
                Some(a_const::Val::Ival(integer)) => match integer.ival {
                    0 => {
                        let yes_standalone = [self.symbol("yes")];
                        values.push(self.list("standalone", yes_standalone))
                    }
                    1 => {
                        let no_standalone = [self.symbol("no")];
                        values.push(self.list("standalone", no_standalone))
                    }
                    2 => {
                        let no_value_standalone = [self.symbol("no-value")];
                        values.push(self.list("standalone", no_value_standalone))
                    }
                    3 => {}
                    _ => return self.unsupported("XMLROOT standalone value"),
                },
                _ => return self.unsupported_error("XMLROOT standalone value"),
            },
            _ => return self.unsupported_error("XMLROOT standalone"),
        }

        Ok(self.list("xml-root", values))
    }

    pub(super) fn lower_xml_serialize(&mut self, serialize: &pg::XmlSerialize) -> Result<Sexp> {
        let option = self.lower_xml_option(serialize.xmloption, "XMLSERIALIZE option")?;
        let xmlserialize_expression_node =
            self.required_node(serialize.expr.as_deref(), "XMLSERIALIZE expression")?;
        let mut values = vec![
            self.symbol(option),
            self.lower(xmlserialize_expression_node)?,
        ];

        if let Some(type_name) = serialize.type_name.as_ref() {
            let serialized_type = [self.lower_type(type_name)?];
            values.push(self.list("as", serialized_type));
        }
        if serialize.indent {
            values.push(self.symbol("indent"));
        }

        Ok(self.list("xml-serialize", values))
    }
}
