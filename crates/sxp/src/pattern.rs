use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};

use crate::{Atom, ParserOptions, Sexp, SexpParser};

/// Scalar categories that a pattern can match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomType {
    Symbol,
    String,
    Integer,
    Float,
    Boolean,
    Nil,
}

/// A pattern over an S-expression tree.
///
/// A list pattern matches children in order. [`Pattern::Repeat`] matches zero
/// or more children, [`Pattern::OneOrMore`] matches one or more children, and
/// [`Pattern::Optional`] matches zero or one child. [`Pattern::Reference`]
/// resolves through the enclosing [`Grammar`].
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Any,
    Atom(AtomType),
    Exact(Sexp),
    List(Vec<Pattern>),
    Repeat(Box<Pattern>),
    OneOrMore(Box<Pattern>),
    Optional(Box<Pattern>),
    OneOf(Vec<Pattern>),
    Capture(String, Box<Pattern>),
    Reference(String),
}

/// A named set of patterns with one designated root rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Grammar {
    name: String,
    root: String,
    rules: Vec<(String, Pattern)>,
}

/// A node matched by a query and the captures made while matching it.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryMatch<'a> {
    pub node: &'a Sexp,
    pub captures: Vec<(String, &'a Sexp)>,
}

/// An invalid or malformed pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternError(String);

impl Pattern {
    /// Parses the external S-expression notation for a pattern.
    ///
    /// Bare atoms are exact matches. Compound forms are `(any)`, `(symbol)`,
    /// `(string)`, `(integer)`, `(float)`, `(boolean)`, `(null)`,
    /// `(exact VALUE)`, `(list PATTERN ...)`, `(repeat PATTERN)`,
    /// `(one-or-more PATTERN)`, `(optional PATTERN)`,
    /// `(or PATTERN ...)`, `(capture NAME PATTERN)`, and `(ref NAME)`.
    pub fn parse(input: &str) -> Result<Self, PatternError> {
        let mut parser = SexpParser::with_options(ParserOptions::default().typed_atoms(true));
        let sexp = parser
            .parse(input)
            .map_err(|error| PatternError(error.to_string()))?;
        Self::from_sexp(&sexp)
    }

    /// Converts an S-expression pattern description into a pattern.
    pub fn from_sexp(sexp: &Sexp) -> Result<Self, PatternError> {
        match sexp {
            Sexp::Atom(_) => Ok(Self::Exact(sexp.clone())),
            Sexp::List(items) => {
                let Some(Sexp::Atom(Atom::Symbol(form))) = items.first() else {
                    return Err(PatternError(
                        "pattern form must begin with a symbol".to_owned(),
                    ));
                };

                match form.as_ref() {
                    "any" => {
                        expect_len(items, 1, "any")?;
                        Ok(Self::Any)
                    }
                    "symbol" => atom_pattern(items, AtomType::Symbol),
                    "string" => atom_pattern(items, AtomType::String),
                    "integer" => atom_pattern(items, AtomType::Integer),
                    "float" => atom_pattern(items, AtomType::Float),
                    "boolean" => atom_pattern(items, AtomType::Boolean),
                    "null" => atom_pattern(items, AtomType::Nil),
                    "exact" => {
                        expect_len(items, 2, "exact")?;
                        Ok(Self::Exact(items[1].clone()))
                    }
                    "list" => items[1..]
                        .iter()
                        .map(Self::from_sexp)
                        .collect::<Result<Vec<_>, _>>()
                        .map(Self::List),
                    "repeat" => {
                        expect_len(items, 2, "repeat")?;
                        Ok(Self::Repeat(Box::new(Self::from_sexp(&items[1])?)))
                    }
                    "one-or-more" => {
                        expect_len(items, 2, "one-or-more")?;
                        Ok(Self::OneOrMore(Box::new(Self::from_sexp(&items[1])?)))
                    }
                    "optional" => {
                        expect_len(items, 2, "optional")?;
                        Ok(Self::Optional(Box::new(Self::from_sexp(&items[1])?)))
                    }
                    "or" => {
                        if items.len() < 2 {
                            return Err(PatternError(
                                "or pattern must contain at least one alternative".to_owned(),
                            ));
                        }
                        items[1..]
                            .iter()
                            .map(Self::from_sexp)
                            .collect::<Result<Vec<_>, _>>()
                            .map(Self::OneOf)
                    }
                    "capture" => {
                        expect_len(items, 3, "capture")?;
                        let name = pattern_name(&items[1], "capture name")?;
                        Ok(Self::Capture(name, Box::new(Self::from_sexp(&items[2])?)))
                    }
                    "ref" => {
                        expect_len(items, 2, "ref")?;
                        Ok(Self::Reference(pattern_name(&items[1], "rule name")?))
                    }
                    _ => Err(PatternError(format!("unknown pattern form {form:?}"))),
                }
            }
        }
    }

    fn to_sexp(&self) -> Sexp {
        match self {
            Self::Any => pattern_form("any", []),
            Self::Atom(kind) => pattern_form(
                match kind {
                    AtomType::Symbol => "symbol",
                    AtomType::String => "string",
                    AtomType::Integer => "integer",
                    AtomType::Float => "float",
                    AtomType::Boolean => "boolean",
                    AtomType::Nil => "null",
                },
                [],
            ),
            Self::Exact(value) => pattern_form("exact", [value.clone()]),
            Self::List(items) => pattern_form("list", items.iter().map(Self::to_sexp)),
            Self::Repeat(pattern) => pattern_form("repeat", [pattern.to_sexp()]),
            Self::OneOrMore(pattern) => pattern_form("one-or-more", [pattern.to_sexp()]),
            Self::Optional(pattern) => pattern_form("optional", [pattern.to_sexp()]),
            Self::OneOf(patterns) => pattern_form("or", patterns.iter().map(Self::to_sexp)),
            Self::Capture(name, pattern) => {
                pattern_form("capture", [Sexp::symbol(name.clone()), pattern.to_sexp()])
            }
            Self::Reference(name) => pattern_form("ref", [Sexp::symbol(name.clone())]),
        }
    }
}

impl Grammar {
    /// Creates a grammar after checking its rules and references.
    pub fn new(
        name: impl Into<String>,
        root: impl Into<String>,
        rules: impl IntoIterator<Item = (String, Pattern)>,
    ) -> Result<Self, PatternError> {
        let grammar = Self {
            name: name.into(),
            root: root.into(),
            rules: rules.into_iter().collect(),
        };
        grammar.validate()?;
        Ok(grammar)
    }

    /// Parses the external S-expression notation for a grammar.
    ///
    /// The form is `(grammar (name "NAME") (root NAME) (rule NAME PATTERN) ...)`.
    pub fn parse(input: &str) -> Result<Self, PatternError> {
        let mut parser = SexpParser::with_options(ParserOptions::default().typed_atoms(true));
        let sexp = parser
            .parse(input)
            .map_err(|error| PatternError(error.to_string()))?;
        Self::from_sexp(&sexp)
    }

    /// Converts a grammar description into a grammar.
    pub fn from_sexp(sexp: &Sexp) -> Result<Self, PatternError> {
        let Sexp::List(items) = sexp else {
            return Err(PatternError("grammar must be a list".to_owned()));
        };
        if !is_symbol(items.first(), "grammar") {
            return Err(PatternError(
                "grammar must begin with the symbol `grammar`".to_owned(),
            ));
        }

        let mut name = None;
        let mut root = None;
        let mut rules = Vec::new();
        for clause in &items[1..] {
            let Sexp::List(parts) = clause else {
                return Err(PatternError("grammar clause must be a list".to_owned()));
            };
            match parts.first() {
                Some(Sexp::Atom(Atom::Symbol(form))) if form.as_ref() == "name" => {
                    expect_len(parts, 2, "name")?;
                    if name.is_some() {
                        return Err(PatternError("grammar has more than one name".to_owned()));
                    }
                    let Sexp::Atom(Atom::String(value)) = &parts[1] else {
                        return Err(PatternError(
                            "grammar name must be a string atom".to_owned(),
                        ));
                    };
                    name = Some(value.to_string());
                }
                Some(Sexp::Atom(Atom::Symbol(form))) if form.as_ref() == "root" => {
                    expect_len(parts, 2, "root")?;
                    if root.is_some() {
                        return Err(PatternError("grammar has more than one root".to_owned()));
                    }
                    root = Some(pattern_name(&parts[1], "root rule name")?);
                }
                Some(Sexp::Atom(Atom::Symbol(form))) if form.as_ref() == "rule" => {
                    expect_len(parts, 3, "rule")?;
                    rules.push((
                        pattern_name(&parts[1], "rule name")?,
                        Pattern::from_sexp(&parts[2])?,
                    ));
                }
                _ => {
                    return Err(PatternError(
                        "grammar clauses must be name, root, or rule forms".to_owned(),
                    ));
                }
            }
        }

        Self::new(
            name.ok_or_else(|| PatternError("grammar is missing a name".to_owned()))?,
            root.ok_or_else(|| PatternError("grammar is missing a root rule".to_owned()))?,
            rules,
        )
    }

    /// Returns this grammar's descriptive name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the name of the designated root rule.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Returns this grammar in its external S-expression notation.
    pub fn to_sexp(&self) -> Sexp {
        let clauses = std::iter::once(Sexp::list([
            Sexp::symbol("name"),
            Sexp::string(self.name.clone()),
        ]))
        .chain(std::iter::once(Sexp::list([
            Sexp::symbol("root"),
            Sexp::symbol(self.root.clone()),
        ])))
        .chain(self.rules.iter().map(|(name, pattern)| {
            Sexp::list([
                Sexp::symbol("rule"),
                Sexp::symbol(name.clone()),
                pattern.to_sexp(),
            ])
        }));
        Sexp::list(std::iter::once(Sexp::symbol("grammar")).chain(clauses))
    }

    /// Returns whether the entire value matches the designated root rule.
    pub fn matches_root(&self, value: &Sexp) -> bool {
        self.rule(&self.root).is_some_and(|pattern| {
            !self
                .match_pattern(pattern, value, &mut HashSet::new())
                .is_empty()
        })
    }

    /// Finds every matching node in pre-order, including the root.
    pub fn query<'a>(
        &self,
        root: &'a Sexp,
        pattern: &Pattern,
    ) -> Result<Vec<QueryMatch<'a>>, PatternError> {
        validate_pattern(pattern, &self.rule_names())?;
        let mut matches = Vec::new();
        self.query_node(root, pattern, &mut matches);
        Ok(matches)
    }

    fn query_node<'a>(&self, node: &'a Sexp, pattern: &Pattern, matches: &mut Vec<QueryMatch<'a>>) {
        for captures in self.match_pattern(pattern, node, &mut HashSet::new()) {
            matches.push(QueryMatch { node, captures });
        }
        if let Sexp::List(children) = node {
            for child in children.iter() {
                self.query_node(child, pattern, matches);
            }
        }
    }

    fn validate(&self) -> Result<(), PatternError> {
        if self.name.trim().is_empty() {
            return Err(PatternError("grammar name must not be empty".to_owned()));
        }
        let mut names = HashSet::new();
        for (name, pattern) in &self.rules {
            if !names.insert(name.as_str()) {
                return Err(PatternError(format!("duplicate rule {name:?}")));
            }
            validate_pattern(pattern, &HashSet::new())?;
        }
        if !names.contains(self.root.as_str()) {
            return Err(PatternError(format!(
                "root rule {:?} is not defined",
                self.root
            )));
        }

        let names = self.rule_names();
        for (_, pattern) in &self.rules {
            validate_pattern(pattern, &names)?;
        }
        Ok(())
    }

    fn rule_names(&self) -> HashSet<&str> {
        self.rules.iter().map(|(name, _)| name.as_str()).collect()
    }

    fn rule(&self, name: &str) -> Option<&Pattern> {
        self.rules
            .iter()
            .find_map(|(rule_name, pattern)| (rule_name == name).then_some(pattern))
    }

    fn match_pattern<'a>(
        &self,
        pattern: &Pattern,
        value: &'a Sexp,
        active_rules: &mut HashSet<(String, *const Sexp)>,
    ) -> Vec<Vec<(String, &'a Sexp)>> {
        match pattern {
            Pattern::Any => vec![Vec::new()],
            Pattern::Atom(kind) if atom_matches(kind, value) => vec![Vec::new()],
            Pattern::Atom(_) => Vec::new(),
            Pattern::Exact(expected) if expected == value => vec![Vec::new()],
            Pattern::Exact(_) => Vec::new(),
            Pattern::List(patterns) => {
                let Sexp::List(values) = value else {
                    return Vec::new();
                };
                self.match_list_patterns(patterns, values, 0, 0, Vec::new(), active_rules)
            }
            Pattern::Repeat(_) | Pattern::OneOrMore(_) | Pattern::Optional(_) => Vec::new(),
            Pattern::OneOf(alternatives) => alternatives
                .iter()
                .flat_map(|alternative| self.match_pattern(alternative, value, active_rules))
                .collect(),
            Pattern::Capture(name, pattern) => self
                .match_pattern(pattern, value, active_rules)
                .into_iter()
                .map(|mut captures| {
                    captures.push((name.clone(), value));
                    captures
                })
                .collect(),
            Pattern::Reference(name) => {
                let Some(rule) = self.rule(name) else {
                    return Vec::new();
                };
                let key = (name.clone(), value as *const Sexp);
                if !active_rules.insert(key.clone()) {
                    return Vec::new();
                }
                let matches = self.match_pattern(rule, value, active_rules);
                active_rules.remove(&key);
                matches
            }
        }
    }

    fn match_list_patterns<'a>(
        &self,
        patterns: &[Pattern],
        values: &'a [Sexp],
        pattern_index: usize,
        value_index: usize,
        captures: Vec<(String, &'a Sexp)>,
        active_rules: &mut HashSet<(String, *const Sexp)>,
    ) -> Vec<Vec<(String, &'a Sexp)>> {
        if pattern_index == patterns.len() {
            return (value_index == values.len())
                .then_some(captures)
                .into_iter()
                .collect();
        }

        match &patterns[pattern_index] {
            Pattern::Optional(pattern) => {
                let mut matches = self.match_list_patterns(
                    patterns,
                    values,
                    pattern_index + 1,
                    value_index,
                    captures.clone(),
                    active_rules,
                );
                if let Some(value) = values.get(value_index) {
                    for subcaptures in self.match_pattern(pattern, value, active_rules) {
                        let mut next_captures = captures.clone();
                        next_captures.extend(subcaptures);
                        matches.extend(self.match_list_patterns(
                            patterns,
                            values,
                            pattern_index + 1,
                            value_index + 1,
                            next_captures,
                            active_rules,
                        ));
                    }
                }
                matches
            }
            Pattern::Repeat(pattern) | Pattern::OneOrMore(pattern) => {
                let one_or_more = matches!(&patterns[pattern_index], Pattern::OneOrMore(_));
                let mut states = vec![captures];
                if pattern_index + 1 == patterns.len() {
                    if one_or_more && value_index == values.len() {
                        return Vec::new();
                    }
                    for value in &values[value_index..] {
                        states = self.combine_matches(states, pattern, value, active_rules);
                        if states.is_empty() {
                            break;
                        }
                    }
                    return states;
                }

                let mut matches = Vec::new();
                let mut next_value_index = value_index;
                if one_or_more {
                    let Some(value) = values.get(next_value_index) else {
                        return Vec::new();
                    };
                    states = self.combine_matches(states, pattern, value, active_rules);
                    if states.is_empty() {
                        return Vec::new();
                    }
                    next_value_index += 1;
                }
                loop {
                    for state in &states {
                        matches.extend(self.match_list_patterns(
                            patterns,
                            values,
                            pattern_index + 1,
                            next_value_index,
                            state.clone(),
                            active_rules,
                        ));
                    }
                    let Some(value) = values.get(next_value_index) else {
                        break;
                    };
                    states = self.combine_matches(states, pattern, value, active_rules);
                    if states.is_empty() {
                        break;
                    }
                    next_value_index += 1;
                }
                matches
            }
            pattern => {
                let Some(value) = values.get(value_index) else {
                    return Vec::new();
                };
                self.match_pattern(pattern, value, active_rules)
                    .into_iter()
                    .flat_map(|subcaptures| {
                        let mut next_captures = captures.clone();
                        next_captures.extend(subcaptures);
                        self.match_list_patterns(
                            patterns,
                            values,
                            pattern_index + 1,
                            value_index + 1,
                            next_captures,
                            active_rules,
                        )
                    })
                    .collect()
            }
        }
    }

    fn combine_matches<'a>(
        &self,
        states: Vec<Vec<(String, &'a Sexp)>>,
        pattern: &Pattern,
        value: &'a Sexp,
        active_rules: &mut HashSet<(String, *const Sexp)>,
    ) -> Vec<Vec<(String, &'a Sexp)>> {
        let submatches = self.match_pattern(pattern, value, active_rules);
        states
            .into_iter()
            .flat_map(|state| {
                submatches.iter().map(move |submatch| {
                    let mut combined = state.clone();
                    combined.extend(submatch.iter().cloned());
                    combined
                })
            })
            .collect()
    }
}

impl Display for PatternError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PatternError {}

fn atom_pattern(items: &[Sexp], kind: AtomType) -> Result<Pattern, PatternError> {
    expect_len(items, 1, "atom")?;
    Ok(Pattern::Atom(kind))
}

fn atom_matches(kind: &AtomType, value: &Sexp) -> bool {
    let Sexp::Atom(atom) = value else {
        return false;
    };
    matches!(
        (kind, atom),
        (AtomType::Symbol, Atom::Symbol(_))
            | (AtomType::String, Atom::String(_))
            | (AtomType::Integer, Atom::Integer(_))
            | (AtomType::Float, Atom::Float(_))
            | (AtomType::Boolean, Atom::Boolean(_))
            | (AtomType::Nil, Atom::Nil)
    )
}

fn expect_len(items: &[Sexp], len: usize, form: &str) -> Result<(), PatternError> {
    if items.len() == len {
        Ok(())
    } else {
        Err(PatternError(format!(
            "{form} pattern expects {} item(s), got {}",
            len.saturating_sub(1),
            items.len().saturating_sub(1)
        )))
    }
}

fn pattern_name(value: &Sexp, context: &str) -> Result<String, PatternError> {
    match value {
        Sexp::Atom(Atom::Symbol(name)) => Ok(name.to_string()),
        _ => Err(PatternError(format!("{context} must be a symbol"))),
    }
}

fn is_symbol(value: Option<&Sexp>, expected: &str) -> bool {
    matches!(value, Some(Sexp::Atom(Atom::Symbol(name))) if name.as_ref() == expected)
}

fn pattern_form(name: &str, args: impl IntoIterator<Item = Sexp>) -> Sexp {
    Sexp::list(std::iter::once(Sexp::symbol(name)).chain(args))
}

fn validate_pattern(pattern: &Pattern, rule_names: &HashSet<&str>) -> Result<(), PatternError> {
    match pattern {
        Pattern::List(patterns) => {
            for pattern in patterns {
                match pattern {
                    Pattern::Repeat(inner)
                    | Pattern::OneOrMore(inner)
                    | Pattern::Optional(inner) => {
                        validate_pattern(inner, rule_names)?;
                    }
                    _ => validate_pattern(pattern, rule_names)?,
                }
            }
        }
        Pattern::Repeat(_) | Pattern::OneOrMore(_) | Pattern::Optional(_) => {
            return Err(PatternError(
                "repeat, one-or-more, and optional patterns must be children of a list".to_owned(),
            ));
        }
        Pattern::OneOf(alternatives) => {
            if alternatives.is_empty() {
                return Err(PatternError(
                    "or pattern must contain at least one alternative".to_owned(),
                ));
            }
            for pattern in alternatives {
                validate_pattern(pattern, rule_names)?;
            }
        }
        Pattern::Capture(_, pattern) => validate_pattern(pattern, rule_names)?,
        Pattern::Reference(name)
            if !rule_names.is_empty() && !rule_names.contains(name.as_str()) =>
        {
            return Err(PatternError(format!("undefined rule reference {name:?}")));
        }
        Pattern::Any | Pattern::Atom(_) | Pattern::Exact(_) | Pattern::Reference(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AtomType, Grammar, Pattern, PatternError};
    use crate::{Sexp, SexpParser};

    fn grammar() -> Grammar {
        Grammar::new(
            "test-grammar",
            "value",
            [(
                "value".to_owned(),
                Pattern::OneOf(vec![
                    Pattern::Atom(AtomType::Nil),
                    Pattern::Atom(AtomType::Boolean),
                    Pattern::Atom(AtomType::Integer),
                    Pattern::Atom(AtomType::Float),
                    Pattern::Atom(AtomType::String),
                    Pattern::List(vec![
                        Pattern::Exact(Sexp::symbol("array")),
                        Pattern::Repeat(Box::new(Pattern::Reference("value".to_owned()))),
                    ]),
                ]),
            )],
        )
        .unwrap()
    }

    fn parse(input: &str) -> Sexp {
        SexpParser::with_options(crate::ParserOptions::default().typed_atoms(true))
            .parse(input)
            .unwrap()
    }

    #[test]
    fn matches_recursive_root_rules_exhaustively() {
        let grammar = grammar();
        assert!(grammar.matches_root(&parse("true")));
        assert!(grammar.matches_root(&parse("(array 1 false (array nil))")));
        assert!(!grammar.matches_root(&parse("(unknown 1)")));
        assert!(!grammar.matches_root(&parse("(array 1 missing)")));
    }

    #[test]
    fn queries_subtrees_and_returns_captures() {
        let grammar = grammar();
        let tree = parse("(array 1 (array false))");
        let pattern = Pattern::parse("(capture arrays (list array (repeat (ref value))))").unwrap();
        let matches = grammar.query(&tree, &pattern).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].node.to_string(), "(array 1 (array false))");
        assert_eq!(matches[1].node.to_string(), "(array false)");
        assert_eq!(matches[0].captures[0].0, "arrays");
    }

    #[test]
    fn round_trips_grammar_notation() {
        let grammar = grammar();
        let encoded = grammar.to_sexp().to_string();
        assert!(encoded.starts_with(r#"(grammar (name "test-grammar")"#));
        assert_eq!(Grammar::parse(&encoded), Ok(grammar));
    }

    #[test]
    fn requires_one_nonempty_string_name() {
        for input in [
            "(grammar (root value) (rule value (any)))",
            r#"(grammar (name "first") (name "second") (root value) (rule value (any)))"#,
            "(grammar (name named) (root value) (rule value (any)))",
            r#"(grammar (name "") (root value) (rule value (any)))"#,
        ] {
            assert!(Grammar::parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn matches_optional_and_repeated_fields_in_order() {
        let pattern =
            Pattern::parse("(list start (optional middle) (repeat (integer)) end)").unwrap();
        let grammar =
            Grammar::new("sequence-grammar", "root", [("root".to_owned(), pattern)]).unwrap();
        assert!(grammar.matches_root(&parse("(start end)")));
        assert!(grammar.matches_root(&parse("(start middle 1 2 end)")));
        assert!(grammar.matches_root(&parse("(start 1 end)")));
        assert!(!grammar.matches_root(&parse("(start middle middle end)")));
        assert!(!grammar.matches_root(&parse("(start end 1)")));
        assert_eq!(Grammar::parse(&grammar.to_sexp().to_string()), Ok(grammar));
    }

    #[test]
    fn matches_one_or_more_fields_in_order() {
        let pattern = Pattern::parse("(list start (one-or-more (integer)) end)").unwrap();
        let grammar =
            Grammar::new("sequence-grammar", "root", [("root".to_owned(), pattern)]).unwrap();
        assert!(grammar.matches_root(&parse("(start 1 end)")));
        assert!(grammar.matches_root(&parse("(start 1 2 3 end)")));
        assert!(!grammar.matches_root(&parse("(start end)")));
        assert!(!grammar.matches_root(&parse("(start 1 nope end)")));
        assert_eq!(Grammar::parse(&grammar.to_sexp().to_string()), Ok(grammar));
    }

    #[test]
    fn rejects_standalone_repeats_and_undefined_rules() {
        assert_eq!(
            Grammar::new(
                "test-grammar",
                "root",
                [("root".to_owned(), Pattern::Repeat(Box::new(Pattern::Any)))]
            ),
            Err(PatternError(
                "repeat, one-or-more, and optional patterns must be children of a list".to_owned()
            ))
        );
        assert!(
            Grammar::new(
                "test-grammar",
                "root",
                [(
                    "root".to_owned(),
                    Pattern::OneOrMore(Box::new(Pattern::Any))
                )]
            )
            .is_err()
        );
        assert!(
            Grammar::new(
                "test-grammar",
                "root",
                [("root".to_owned(), Pattern::Reference("missing".to_owned()))]
            )
            .is_err()
        );
    }
}
