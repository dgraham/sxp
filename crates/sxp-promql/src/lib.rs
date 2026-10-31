mod grammar;
pub use grammar::grammar;

use std::time::{Duration, UNIX_EPOCH};

use promql_parser::label::{Matcher, Matchers};
use promql_parser::parser::{
    self, AggregateExpr, AtModifier, BinModifier, BinaryExpr, Call, Expr, LabelModifier,
    MatrixSelector, NumberLiteral, ParenExpr, SubqueryExpr, UnaryExpr, VectorMatchCardinality,
    VectorSelector,
};
use promql_parser::util::display_duration;
use sxp::{ParseError, Parser, Result, Sexp, SymbolPool};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParserOptions {
    max_symbol_entries: Option<usize>,
}

impl ParserOptions {
    /// Sets the maximum number of distinct symbols retained for reuse.
    /// `None` leaves the pool uncapped. `Some(0)` disables symbol caching.
    pub fn max_symbol_entries(mut self, max_entries: Option<usize>) -> Self {
        self.max_symbol_entries = max_entries;
        self
    }
}

#[derive(Default)]
pub struct PromqlParser {
    pool: SymbolPool,
}

impl PromqlParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: ParserOptions) -> Self {
        let pool = options
            .max_symbol_entries
            .map(SymbolPool::with_max_entries)
            .unwrap_or_default();
        Self { pool }
    }

    pub fn parse(&mut self, input: &str) -> Result<Sexp> {
        let expr = parser::parse(input).map_err(|message| ParseError::new(message, 0))?;
        Ok(self.lower_expr(&expr))
    }

    fn symbol(&mut self, text: &str) -> Sexp {
        self.pool.symbol(text)
    }

    fn node(&mut self, tag: &str, fields: Vec<Sexp>) -> Sexp {
        Sexp::list(std::iter::once(self.symbol(tag)).chain(fields))
    }

    fn field(&mut self, name: &str, value: Sexp) -> Sexp {
        Sexp::list([self.symbol(name), value])
    }

    fn lower_expr(&mut self, expr: &Expr) -> Sexp {
        match expr {
            Expr::Aggregate(expr) => self.aggregate(expr),
            Expr::Unary(expr) => self.unary(expr),
            Expr::Binary(expr) => self.binary(expr),
            Expr::Paren(expr) => self.paren(expr),
            Expr::Subquery(expr) => self.subquery(expr),
            Expr::NumberLiteral(expr) => self.number(expr),
            Expr::StringLiteral(expr) => Sexp::string(&expr.val),
            Expr::VectorSelector(expr) => self.vector_selector(expr),
            Expr::MatrixSelector(expr) => self.matrix_selector(expr),
            Expr::Call(expr) => self.call(expr),
            Expr::Extension(expr) => {
                let name = self.field("name", Sexp::string(expr.expr.name()));
                self.node("extension", vec![name])
            }
        }
    }

    fn aggregate(&mut self, expr: &AggregateExpr) -> Sexp {
        let op = self.symbol(&expr.op.to_string());
        let mut fields = vec![self.field("op", op)];
        if let Some(modifier) = &expr.modifier {
            fields.push(self.grouping(modifier));
        }
        if let Some(param) = &expr.param {
            let param = self.lower_expr(param);
            fields.push(self.field("param", param));
        }
        let value = self.lower_expr(&expr.expr);
        fields.push(self.field("expr", value));
        self.node("aggregate", fields)
    }

    fn grouping(&mut self, modifier: &LabelModifier) -> Sexp {
        let (kind, labels) = match modifier {
            LabelModifier::Include(labels) => ("by", &labels.labels),
            LabelModifier::Exclude(labels) => ("without", &labels.labels),
        };
        let tag = self.symbol("grouping");
        let kind = self.symbol(kind);
        let mut fields = vec![tag, kind];
        fields.extend(labels.iter().map(Sexp::string));
        Sexp::list(fields)
    }

    fn unary(&mut self, expr: &UnaryExpr) -> Sexp {
        let minus = self.symbol("-");
        let op = self.field("op", minus);
        let value = self.lower_expr(&expr.expr);
        let value = self.field("expr", value);
        self.node("unary", vec![op, value])
    }

    fn binary(&mut self, expr: &BinaryExpr) -> Sexp {
        let left = self.lower_expr(&expr.lhs);
        let left = self.field("left", left);
        let op = self.symbol(&expr.op.to_string());
        let op = self.field("op", op);
        let right = self.lower_expr(&expr.rhs);
        let right = self.field("right", right);
        let mut fields = vec![left, op, right];
        if let Some(modifier) = &expr.modifier {
            fields.push(self.bin_modifier(modifier));
        }
        self.node("binary", fields)
    }

    fn bin_modifier(&mut self, modifier: &BinModifier) -> Sexp {
        let mut fields = Vec::new();

        if let Some(matching) = &modifier.matching {
            let (kind, labels) = match matching {
                LabelModifier::Include(labels) => ("on", &labels.labels),
                LabelModifier::Exclude(labels) => ("ignoring", &labels.labels),
            };
            let tag = self.symbol("matching");
            let kind = self.symbol(kind);
            let mut matching_fields = vec![tag, kind];
            matching_fields.extend(labels.iter().map(Sexp::string));
            fields.push(Sexp::list(matching_fields));
        }

        let tag = self.symbol("cardinality");
        let kind = self.symbol(match &modifier.card {
            VectorMatchCardinality::OneToOne => "one-to-one",
            VectorMatchCardinality::ManyToMany => "many-to-many",
            VectorMatchCardinality::ManyToOne(_) => "group-left",
            VectorMatchCardinality::OneToMany(_) => "group-right",
        });
        let mut cardinality = vec![tag, kind];
        if let Some(labels) = modifier.card.labels() {
            cardinality.extend(labels.labels.iter().map(Sexp::string));
        }
        fields.push(Sexp::list(cardinality));

        if modifier.return_bool {
            let flag = self.symbol("bool");
            fields.push(Sexp::list([flag]));
        }

        if modifier.fill_values.lhs.is_some() || modifier.fill_values.rhs.is_some() {
            let mut fill = Vec::new();
            if let Some(value) = modifier.fill_values.lhs {
                fill.push(self.field("left", Sexp::float(float_text(value))));
            }
            if let Some(value) = modifier.fill_values.rhs {
                fill.push(self.field("right", Sexp::float(float_text(value))));
            }
            fields.push(self.node("fill", fill));
        }

        self.node("modifier", fields)
    }

    fn paren(&mut self, expr: &ParenExpr) -> Sexp {
        let value = self.lower_expr(&expr.expr);
        let value = self.field("expr", value);
        self.node("paren", vec![value])
    }

    fn subquery(&mut self, expr: &SubqueryExpr) -> Sexp {
        let value = self.lower_expr(&expr.expr);
        let expr_field = self.field("expr", value);
        let range = Sexp::string(display_duration(&expr.range));
        let range = self.field("range", range);
        let mut fields = vec![expr_field, range];
        if let Some(step) = &expr.step {
            let step = Sexp::string(display_duration(step));
            fields.push(self.field("step", step));
        }
        if let Some(at) = &expr.at {
            let at = self.at(at);
            fields.push(at);
        }
        if let Some(offset) = &expr.offset {
            let offset = Sexp::string(offset.to_string());
            fields.push(self.field("offset", offset));
        }
        self.node("subquery", fields)
    }

    fn at(&mut self, at: &AtModifier) -> Sexp {
        let value = match at {
            AtModifier::Start => self.symbol("start"),
            AtModifier::End => self.symbol("end"),
            AtModifier::At(time) => {
                let secs = time
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_secs_f64();
                Sexp::float(format!("{secs:.3}"))
            }
        };
        self.node("at", vec![value])
    }

    fn number(&mut self, expr: &NumberLiteral) -> Sexp {
        self.node("float", vec![Sexp::float(float_text(expr.val))])
    }

    fn vector_selector(&mut self, expr: &VectorSelector) -> Sexp {
        let mut fields = Vec::new();
        if let Some(name) = &expr.name {
            fields.push(self.field("name", Sexp::string(name)));
        }
        if !expr.matchers.matchers.is_empty() || !expr.matchers.or_matchers.is_empty() {
            let matchers = self.matchers(&expr.matchers);
            fields.push(matchers);
        }
        if let Some(offset) = &expr.offset {
            let offset = Sexp::string(offset.to_string());
            fields.push(self.field("offset", offset));
        }
        if let Some(at) = &expr.at {
            let at = self.at(at);
            fields.push(at);
        }
        self.node("vector-selector", fields)
    }

    fn matchers(&mut self, matchers: &Matchers) -> Sexp {
        let mut fields: Vec<Sexp> = matchers.matchers.iter().map(|m| self.matcher(m)).collect();
        for group in &matchers.or_matchers {
            let matches: Vec<Sexp> = group.iter().map(|m| self.matcher(m)).collect();
            fields.push(self.node("or", matches));
        }
        self.node("matchers", fields)
    }

    fn matcher(&mut self, matcher: &Matcher) -> Sexp {
        let op = self.symbol(&matcher.op.to_string());
        let fields = vec![
            op,
            Sexp::string(&matcher.name),
            Sexp::string(&matcher.value),
        ];
        self.node("match", fields)
    }

    fn matrix_selector(&mut self, expr: &MatrixSelector) -> Sexp {
        let selector = self.vector_selector(&expr.vs);
        let selector = self.field("expr", selector);
        let range = Sexp::string(display_duration(&expr.range));
        let range = self.field("range", range);
        self.node("matrix-selector", vec![selector, range])
    }

    fn call(&mut self, expr: &Call) -> Sexp {
        let func = self.field("func", Sexp::string(expr.func.name));
        let args: Vec<Sexp> = expr
            .args
            .args
            .iter()
            .map(|arg| self.lower_expr(arg))
            .collect();
        let args = self.node("args", args);
        self.node("call", vec![func, args])
    }
}

impl Parser for PromqlParser {
    fn parse(&mut self, input: &str) -> Result<Sexp> {
        PromqlParser::parse(self, input)
    }
}

fn float_text(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else {
        format!("{value:?}")
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::PromqlParser;
    use sxp::{Atom, Result, Sexp};

    fn parse(input: &str) -> Result<Sexp> {
        PromqlParser::new().parse(input)
    }

    fn lowers(input: &str, expected: &str) -> sxp::Sexp {
        let value = parse(input).expect("should parse");
        assert_eq!(value.to_string(), expected);
        value
    }

    fn first_symbol(value: &Sexp) -> &Rc<str> {
        value
            .iter()
            .find_map(|node| match node {
                Sexp::Atom(Atom::Symbol(symbol)) => Some(symbol),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn reuses_symbols_across_parses() {
        let mut parser = PromqlParser::new();
        let first = parser.parse("up").unwrap();
        let second = parser.parse("up").unwrap();

        assert!(Rc::ptr_eq(first_symbol(&first), first_symbol(&second)));
    }

    #[test]
    fn parses_selectors_and_matchers() {
        lowers("up", r#"(vector-selector (name "up"))"#);
        lowers(
            r#"http_requests_total{environment=~"staging|testing|development",method!="GET"} offset 5m"#,
            r#"(vector-selector (name "http_requests_total") (matchers (match =~ "environment" "staging|testing|development") (match != "method" "GET")) (offset "5m"))"#,
        );
    }

    #[test]
    fn parses_matrix_selectors_and_at_modifiers() {
        lowers(
            "rate(http_requests_total[5m])",
            r#"(call (func "rate") (args (matrix-selector (expr (vector-selector (name "http_requests_total"))) (range "5m"))))"#,
        );
        lowers(
            "up @ start() offset -2m",
            r#"(vector-selector (name "up") (offset "-2m") (at start))"#,
        );
    }

    #[test]
    fn parses_aggregations() {
        lowers(
            "sum by (job) (rate(http_requests_total[5m]))",
            r#"(aggregate (op sum) (grouping by "job") (expr (call (func "rate") (args (matrix-selector (expr (vector-selector (name "http_requests_total"))) (range "5m"))))))"#,
        );
        lowers(
            "topk(3, sum without (instance) (rate(errors_total[10m])))",
            r#"(aggregate (op topk) (param (float 3.0)) (expr (aggregate (op sum) (grouping without "instance") (expr (call (func "rate") (args (matrix-selector (expr (vector-selector (name "errors_total"))) (range "10m"))))))))"#,
        );
    }

    #[test]
    fn parses_binary_operations() {
        lowers(
            "a + on(instance) group_left b",
            r#"(binary (left (vector-selector (name "a"))) (op +) (right (vector-selector (name "b"))) (modifier (matching on "instance") (cardinality group-left)))"#,
        );
        lowers(
            "a > bool 0",
            r#"(binary (left (vector-selector (name "a"))) (op >) (right (float 0.0)) (modifier (cardinality one-to-one) (bool)))"#,
        );
        lowers(
            "a or b",
            r#"(binary (left (vector-selector (name "a"))) (op or) (right (vector-selector (name "b"))) (modifier (cardinality many-to-many)))"#,
        );
    }

    #[test]
    fn parses_unary_paren_and_subquery() {
        lowers(
            "-(1 + 2)",
            "(unary (op -) (expr (paren (expr (binary (left (float 1.0)) (op +) (right (float 2.0)))))))",
        );
        lowers(
            "max_over_time(up[5m:1m] @ end())",
            r#"(call (func "max_over_time") (args (subquery (expr (vector-selector (name "up"))) (range "5m") (step "1m") (at end))))"#,
        );
    }

    #[test]
    fn preserves_scalar_types() {
        lowers("1.5", "(float 1.5)");
        lowers("-0.25", "(float -0.25)");
        lowers("Inf", "(float inf)");
        lowers("1e400", "(float inf)");
    }

    #[test]
    fn rejects_invalid_promql() {
        assert!(parse("sum(").is_err());
        assert!(parse("http_requests_total{").is_err());
    }
}
