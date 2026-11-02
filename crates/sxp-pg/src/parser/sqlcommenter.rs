use super::*;

pub(super) struct Tag {
    offset: usize,
    key: String,
    value: String,
}

pub(super) fn sqlcommenter_tags(input: &str) -> Vec<Tag> {
    scan_block_comments(input)
        .into_iter()
        .flat_map(|(offset, text)| {
            parse_tags(&text)
                .into_iter()
                .map(move |(key, value)| Tag { offset, key, value })
        })
        .collect()
}

fn scan_block_comments(input: &str) -> Vec<(usize, String)> {
    let Ok(result) = pg_query::scan(input) else {
        return Vec::new();
    };
    result
        .tokens
        .iter()
        .filter(|token| token.token == pg::Token::CComment as i32)
        .filter_map(|token| comment_text(input, token))
        .collect()
}

fn comment_text(input: &str, token: &pg::ScanToken) -> Option<(usize, String)> {
    let start = usize::try_from(token.start).ok()?;
    let end = usize::try_from(token.end).ok()?;
    if start > end
        || end > input.len()
        || !input.is_char_boundary(start)
        || !input.is_char_boundary(end)
    {
        return None;
    }
    Some((start, input[start..end].to_owned()))
}

pub(super) fn statement_spans(stmts: &[pg::RawStmt], input: &str) -> Vec<(usize, usize)> {
    stmts
        .iter()
        .map(|stmt| {
            let start = usize::try_from(stmt.stmt_location)
                .unwrap_or(0)
                .min(input.len());
            let length = usize::try_from(stmt.stmt_len).unwrap_or(0);
            let end = if length == 0 {
                input.len()
            } else {
                start.saturating_add(length).min(input.len())
            };
            (start, end)
        })
        .collect()
}

fn comment_owner(spans: &[(usize, usize)], offset: usize) -> Option<usize> {
    spans
        .iter()
        .position(|(start, end)| offset >= *start && offset < *end)
        .or_else(|| spans.iter().position(|(start, _)| offset < *start))
        .or_else(|| spans.len().checked_sub(1))
}

impl PostgresParser {
    pub(super) fn attach_tags(
        &mut self,
        stmts: Vec<Sexp>,
        spans: &[(usize, usize)],
        tags: &[Tag],
    ) -> Vec<Sexp> {
        if tags.is_empty() {
            return stmts;
        }

        let mut grouped: Vec<Vec<&Tag>> = vec![Vec::new(); stmts.len()];
        for tag in tags {
            if let Some(group) =
                comment_owner(spans, tag.offset).and_then(|index| grouped.get_mut(index))
            {
                group.push(tag);
            }
        }
        stmts
            .into_iter()
            .zip(grouped)
            .map(|(stmt, tags)| self.insert_sqlcommenter(stmt, &tags))
            .collect()
    }

    fn insert_sqlcommenter(&mut self, stmt: Sexp, tags: &[&Tag]) -> Sexp {
        match stmt {
            Sexp::List(values) if !tags.is_empty() && !values.is_empty() => {
                let mut values = values.into_vec();
                values.insert(1, self.sqlcommenter_clause(tags.iter().copied()));
                Sexp::List(values.into_boxed_slice())
            }
            statement => statement,
        }
    }

    pub(super) fn sqlcommenter_clause<'a>(
        &mut self,
        tags: impl IntoIterator<Item = &'a Tag>,
    ) -> Sexp {
        let tag_sexps: Vec<Sexp> = tags
            .into_iter()
            .map(|tag| self.list("tag", [Sexp::string(&tag.key), Sexp::string(&tag.value)]))
            .collect();
        self.list("sqlcommenter", tag_sexps)
    }
}

fn parse_tags(text: &str) -> Vec<(String, String)> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut tags = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match parse_tag(&chars, index) {
            Some((tag, next)) => {
                tags.push(tag);
                index = next;
            }
            None => index += 1,
        }
    }
    tags
}

fn parse_tag(chars: &[char], start: usize) -> Option<((String, String), usize)> {
    let mut key = String::new();
    let mut index = start;
    while let Some(&ch) = chars.get(index) {
        if ch == '\\' && chars.get(index + 1) == Some(&'\'') {
            key.push('\\');
            key.push('\'');
            index += 2;
        } else if is_key_char(ch) {
            key.push(ch);
            index += 1;
        } else {
            break;
        }
    }

    if key.is_empty() {
        return None;
    }

    while chars.get(index).is_some_and(|ch| ch.is_whitespace()) {
        index += 1;
    }
    if chars.get(index) != Some(&'=') {
        return None;
    }
    index += 1;
    while chars.get(index).is_some_and(|ch| ch.is_whitespace()) {
        index += 1;
    }
    if chars.get(index) != Some(&'\'') {
        return None;
    }
    index += 1;

    let mut value = String::new();
    loop {
        match chars.get(index) {
            Some('\\') if chars.get(index + 1) == Some(&'\'') => {
                value.push('\'');
                index += 2;
            }
            Some('\'') => {
                index += 1;
                break;
            }
            Some(ch) => {
                value.push(*ch);
                index += 1;
            }
            None => return None,
        }
    }

    Some((
        (url_decode(&unescape_quotes(&key)), url_decode(&value)),
        index,
    ))
}

fn is_key_char(ch: char) -> bool {
    !ch.is_whitespace() && !matches!(ch, '=' | '\'' | ',' | '/' | '*')
}

fn unescape_quotes(text: &str) -> String {
    text.replace("\\'", "'")
}

fn url_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let (Some(high), Some(low)) = (
                bytes.get(index + 1).copied().and_then(hex_value),
                bytes.get(index + 2).copied().and_then(hex_value),
            )
        {
            decoded.push(high << 4 | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| text.to_owned())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ParserOptions, PostgresParser};

    fn parse(input: &str) -> sxp::Result<sxp::Sexp> {
        PostgresParser::new().parse(input)
    }

    fn lowers(sql: &str, expected: &str) {
        assert_eq!(
            parse(sql).map(|sexp| sexp.to_string()),
            Ok(expected.to_owned())
        );
    }

    #[test]
    fn ignores_comments_without_sqlcommenter_flag() {
        for input in [
            "SELECT 1",
            "SELECT 1 /*app='web'*/",
            "SELECT 1 -- app='web'",
            "-- leading\nSELECT 1; SELECT 2 /*a='1'*/",
        ] {
            assert_eq!(
                parse(input).map(|sexp| sexp.to_string()),
                PostgresParser::with_options(ParserOptions::default())
                    .parse(input)
                    .map(|sexp| sexp.to_string()),
                "{input}"
            );
        }
        lowers("SELECT 1 /*app='web'*/", "(select (columns 1))");
        lowers(
            "SELECT 1; SELECT 2 /*a='1'*/",
            "(statements (select (columns 1)) (select (columns 2)))",
        );
    }

    #[test]
    fn lowers_sqlcommenter_tags() {
        assert_eq!(
                parse_sqlcommenter("SELECT * FROM users /*app='web',route='%2Fapi%2Fusers'*/;"),
                Ok(
                    "(select (sqlcommenter (tag \"app\" \"web\") (tag \"route\" \"/api/users\")) (columns (column *)) (from (table (\"users\"))))"
                        .to_owned()
                )
            );
    }

    #[test]
    fn skips_malformed_pairs() {
        assert_eq!(
            parse_sqlcommenter("SELECT 1 /* a='1',b='2' trailer */"),
            Ok(
                "(select (sqlcommenter (tag \"a\" \"1\") (tag \"b\" \"2\")) (columns 1))"
                    .to_owned()
            )
        );
        assert_eq!(
            parse_sqlcommenter("SELECT 1 /*a='1',b='2',*/"),
            Ok(
                "(select (sqlcommenter (tag \"a\" \"1\") (tag \"b\" \"2\")) (columns 1))"
                    .to_owned()
            )
        );
        for input in [
            "SELECT 1 /* no pairs here */",
            "SELECT 1 /*= 'v'*/",
            "SELECT 1 /*a='*/",
            "SELECT 1 /*a=1*/",
            "SELECT 1 /*a '1'*/",
        ] {
            assert_eq!(
                parse_sqlcommenter(input),
                Ok("(select (columns 1))".to_owned()),
                "{input}"
            );
        }
    }

    #[test]
    fn ignores_line_comments_with_sqlcommenter_flag() {
        assert_eq!(
            parse_sqlcommenter("SELECT 1 -- a='1'"),
            Ok("(select (columns 1))".to_owned())
        );
        assert_eq!(
            parse_sqlcommenter("SELECT 1 -- a='1'\n"),
            Ok("(select (columns 1))".to_owned())
        );
    }

    #[test]
    fn decodes_sqlcommenter_keys_and_values() {
        assert_eq!(
            parse_sqlcommenter("SELECT 1 /*route%20parameter='v'*/"),
            Ok("(select (sqlcommenter (tag \"route parameter\" \"v\")) (columns 1))".to_owned())
        );
        assert_eq!(
            parse_sqlcommenter("SELECT 1 /*a%27b='v'*/"),
            Ok("(select (sqlcommenter (tag \"a'b\" \"v\")) (columns 1))".to_owned())
        );
        assert_eq!(
            parse_sqlcommenter("SELECT 1 /*a='it\\'s'*/"),
            Ok("(select (sqlcommenter (tag \"a\" \"it's\")) (columns 1))".to_owned())
        );
        assert_eq!(
            parse_sqlcommenter("SELECT 1 /*x='a+b%20c'*/"),
            Ok("(select (sqlcommenter (tag \"x\" \"a+b c\")) (columns 1))".to_owned())
        );
    }

    #[test]
    fn leaves_invalid_percent_sequences_alone() {
        assert_eq!(
                parse_sqlcommenter("SELECT 1 /*a='%ZZ',b='%A',c='x%'*/"),
                Ok(
                    "(select (sqlcommenter (tag \"a\" \"%ZZ\") (tag \"b\" \"%A\") (tag \"c\" \"x%\")) (columns 1))"
                        .to_owned()
                )
            );
    }

    #[test]
    fn keeps_non_utf8_percent_sequences_raw() {
        assert_eq!(
            parse_sqlcommenter("SELECT 1 /*a='%FF',b='%C3%A9'*/"),
            Ok(
                "(select (sqlcommenter (tag \"a\" \"%FF\") (tag \"b\" \"\u{e9}\")) (columns 1))"
                    .to_owned()
            )
        );
    }

    #[test]
    fn collects_tags_from_several_comments_in_source_order() {
        assert_eq!(
            parse_sqlcommenter("SELECT 1 /*a='1'*/ /*b='2'*/"),
            Ok(
                "(select (sqlcommenter (tag \"a\" \"1\") (tag \"b\" \"2\")) (columns 1))"
                    .to_owned()
            )
        );
    }

    #[test]
    fn attaches_tags_to_the_owning_statement() {
        assert_eq!(
                parse_sqlcommenter("SELECT 1 /*a='1'*/; SELECT 2 /*b='2'*/; SELECT 3"),
                Ok(
                    "(statements (select (sqlcommenter (tag \"a\" \"1\")) (columns 1)) (select (sqlcommenter (tag \"b\" \"2\")) (columns 2)) (select (columns 3)))"
                        .to_owned()
                )
            );
    }

    #[test]
    fn attaches_trailing_comment_to_last_statement() {
        assert_eq!(
                parse_sqlcommenter("SELECT 1; SELECT 2; /*a='1'*/"),
                Ok(
                    "(statements (select (columns 1)) (select (sqlcommenter (tag \"a\" \"1\")) (columns 2)))"
                        .to_owned()
                )
            );
    }

    #[test]
    fn ignores_comment_text_inside_string_literals() {
        assert_eq!(
            parse_sqlcommenter("SELECT '/*app=''x''*/'"),
            Ok("(select (columns \"/*app='x'*/\"))".to_owned())
        );
    }

    #[test]
    fn leaves_sqlcommenter_free_input_unchanged() {
        assert_eq!(
            parse_sqlcommenter("SELECT 1"),
            Ok("(select (columns 1))".to_owned())
        );
        assert_eq!(
                parse_sqlcommenter("SELECT u.id FROM users AS u WHERE u.active"),
                Ok(
                    "(select (columns (column \"u\" \"id\")) (from (table (\"users\") (alias \"u\"))) (where (column \"u\" \"active\")))"
                        .to_owned()
                )
            );
    }

    #[test]
    fn lowers_tags_without_statements() {
        assert_eq!(
            parse_sqlcommenter("/*app='web'*/"),
            Ok("(sqlcommenter (tag \"app\" \"web\"))".to_owned())
        );
        assert_eq!(
            parse_sqlcommenter("/* no pairs here */").map_err(|error| error.to_string()),
            Err("PostgreSQL parser returned no tree at byte 0".to_owned())
        );
    }

    fn parse_sqlcommenter(input: &str) -> sxp::Result<String> {
        PostgresParser::with_options(ParserOptions::default().sqlcommenter(true))
            .parse(input)
            .map(|sexp| sexp.to_string())
    }
}
