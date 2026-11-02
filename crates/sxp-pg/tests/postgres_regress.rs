use std::fs;
use std::path::Path;

// https://github.com/postgres/postgres/tree/REL_18_STABLE/src/test/regress/sql

const REGRESS_SQL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sql");

#[test]
fn postgres_18_regression_sql_parses_and_lowers() {
    let mut files = fs::read_dir(REGRESS_SQL_DIR)
        .unwrap_or_else(|error| panic!("failed to read {REGRESS_SQL_DIR}: {error}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("failed to read regression fixture entry: {error}"))
                .path()
        })
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("sql"))
        .collect::<Vec<_>>();
    files.sort();
    assert!(!files.is_empty(), "no PostgreSQL regression fixtures found");

    let mut lowered_count = 0;
    let mut rejected_count = 0;
    let mut expected_parse_error_count = 0;
    let mut expected_lowering_error_count = 0;
    let mut failures = Vec::new();

    for path in &files {
        let bytes = fs::read(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(error)
                if path.file_name().and_then(|name| name.to_str())
                    == Some("collate.windows.win1252.sql") =>
            {
                decode_windows_1252(&error.into_bytes())
            }
            Err(error) => panic!("{} is not UTF-8: {error}", path.display()),
        };
        let sql = sql_without_psql_commands_and_copy_data(&source);
        let statements = split_statements(&sql)
            .unwrap_or_else(|error| panic!("failed to split {}: {error}", path.display()));

        for (index, statement) in statements.iter().enumerate() {
            let statement = statement.trim();
            if statement.is_empty() {
                rejected_count += 1;
                continue;
            }
            if let Err(error) = pg_query::parse(statement, 0) {
                rejected_count += 1;
                if is_expected_parse_error(path, statement, &error.to_string()) {
                    expected_parse_error_count += 1;
                } else {
                    failures.push(format!(
                        "{} statement {} was unexpectedly rejected by PostgreSQL's parser: \
                         {error}\n{statement}",
                        path.display(),
                        index + 1
                    ));
                }
                continue;
            }

            match sxp_pg::PostgresParser::new().parse(statement) {
                Ok(sexp) if !sexp.to_string().is_empty() => lowered_count += 1,
                Ok(_) => failures.push(format!(
                    "{} statement {} lowered to empty output:\n{statement}",
                    path.display(),
                    index + 1
                )),
                Err(error) if is_expected_lowering_error(path, &error.to_string()) => {
                    expected_lowering_error_count += 1;
                }
                Err(error) => failures.push(format!(
                    "{} statement {} failed to lower: {error}\n{statement}",
                    path.display(),
                    index + 1
                )),
            }
        }
    }

    assert_eq!(
        expected_lowering_error_count, 3,
        "expected the domain identity error and two duplicate RETURNING alias errors"
    );
    assert!(
        lowered_count >= 50_000,
        "only {lowered_count} PostgreSQL-parseable regression statements were lowered"
    );
    assert!(
        failures.is_empty(),
        "{} parser/lowering failures across {} fixture files \
         ({lowered_count} lowered, {expected_parse_error_count} expected parse errors, \
         {expected_lowering_error_count} expected lowering errors, \
         {rejected_count} total non-SQL or parser-rejected fragments):\n{}",
        failures.len(),
        files.len(),
        failures
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

fn is_expected_parse_error(path: &Path, statement: &str, error: &str) -> bool {
    let comments = statement
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            if line.starts_with("--") {
                Some(line)
            } else {
                line.find("--").map(|index| &line[index..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    if [
        "error",
        "fail",
        "invalid",
        "not supported",
        "doesn't support",
        "does not support",
        "not allowed",
        "cannot",
        "can't",
        "reject",
        "syntax error",
        "syntax does not allow",
        "missing",
        "bad ",
        "no such",
        "only accept valid",
        "maybe someday",
        "not ok",
        "nonexistent",
        "disallowed",
        "don't work",
        "illegal",
        "incompatible",
        "incorrectly",
        "multiple values clause",
        "no longer",
        "not matched",
        "postquel",
        "bogus",
        "wrong",
        "tablename",
    ]
    .iter()
    .any(|marker| comments.contains(marker))
    {
        return true;
    }

    let file_name = path.file_name().and_then(|name| name.to_str());
    let collate_cast = matches!(
        file_name,
        Some(
            "collate.sql"
                | "collate.icu.utf8.sql"
                | "collate.linux.utf8.sql"
                | "collate.windows.win1252.sql"
        )
    ) && statement.contains("SELECT CAST('42' AS text COLLATE \"C\")")
        && error.contains("syntax error at or near \"COLLATE\"");
    collate_cast
        || file_name == Some("errors.sql")
        || (file_name == Some("rules.sql") && statement.trim_start().starts_with("**"))
        || (file_name == Some("stats_ext.sql")
            && statement.to_ascii_lowercase().contains("create statistics")
            && error.contains("syntax error"))
        || (file_name == Some("numerology.sql")
            && (error.contains("numeric literal")
                || error.contains("binary integer")
                || error.contains("octal integer")
                || error.contains("hexadecimal integer")
                || error.contains("parameter number too large")
                || (statement.contains("SELECT _1_000.5")
                    && error.contains("syntax error at or near \".5\""))))
        || (file_name == Some("merge.sql")
            && (comments.contains("select query for insert")
                || comments.contains("multiple values clause")
                || comments.contains("not matched")))
        || (matches!(
            file_name,
            Some("psql.sql" | "psql_crosstab.sql" | "psql_pipeline.sql")
        ) && statement
            .lines()
            .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with("--"))
            .is_some_and(|line| matches!(line.trim(), "not okay" | "still not okay")))
        || error.contains("column number must be in range")
        || error.contains("cannot use multiple ORDER BY clauses with WITHIN GROUP")
        || error.contains("WHERE clause not allowed with COPY TO")
        || error.contains("zero-length delimited identifier")
        || error.contains("constraints cannot be altered to be NOT VALID")
        || error.contains("CHECK constraints cannot be marked NOT ENFORCED")
        || error.contains("a column list with SET NULL is only supported")
        || error.contains("constraint declared INITIALLY DEFERRED must be DEFERRABLE")
        || error.contains("conflicting constraint properties")
        || error.contains("SKIP LOCKED and WITH TIES options cannot be used together")
        || error.contains("invalid table name")
        || error.contains("invalid schema name")
        || error.contains("invalid publication object list")
        || error.contains("WHERE clause not allowed for schema")
        || error.contains("column specification not allowed for schema")
        || error.contains("unrecognized row security option")
        || error.contains("unrecognized JSON encoding")
        || (file_name == Some("publication.sql")
            && (error.contains("syntax error at or near \"WHERE\"")
                || error.contains("syntax error at or near \"CURRENT_SCHEMA\"")))
        || (file_name == Some("sqljson.sql")
            && error.contains("syntax error at or near \")\"")
            && ["JSON()", "JSON_SCALAR()", "JSON_SERIALIZE()"]
                .iter()
                .any(|call| statement.contains(call)))
        || (file_name == Some("sqljson_jsontable.sql")
            && statement.contains("exists empty object on empty")
            && error.contains("syntax error at or near \"empty\""))
        || (file_name == Some("strings.sql")
            && (error.contains("invalid Unicode")
                || error.contains("UESCAPE must be followed")
                || error.contains("invalid Unicode escape character")
                || error.contains("syntax error at or near \"' as f2, '\"")))
        || (file_name == Some("tablesample.sql") && statement.contains("TABLESAMPLE BERNOULLI"))
        || (file_name == Some("triggers.sql")
            && (statement.contains("error_ins_a") || statement.contains("on only grandparent")))
        || (file_name == Some("tuplesort.sql") && statement.contains("EXPLAIN (COSTS OFF) 0"))
        || (file_name == Some("vacuum.sql") && statement.contains("ANALYZE (nonexistent-arg)"))
        || (file_name == Some("window.sql")
            && error.contains("syntax error at or near \"ORDER\"")
            && statement.contains("rank()"))
        || (file_name == Some("with.sql")
            && statement
                .to_ascii_lowercase()
                .contains("create table foo (with"))
        || (file_name == Some("xml.sql")
            && error.contains("option name \"__pg__is_not_null\" cannot be used"))
        || (file_name == Some("foreign_data.sql") && error.contains("syntax error at end of input"))
        || (file_name == Some("create_index.sql")
            && (statement.contains("CREATE INDEX IF NOT EXISTS ON")
                || statement.contains("REINDEX SCHEMA CONCURRENTLY 0")))
}

fn is_expected_lowering_error(path: &Path, error: &str) -> bool {
    let file_name = path.file_name().and_then(|name| name.to_str());
    match (file_name, error) {
        (Some("domain.sql"), error) => {
            error.contains("unsupported PostgreSQL AST: identity constraint")
        }
        (Some("returning.sql"), error) => {
            error.contains("unsupported PostgreSQL AST: duplicate RETURNING alias")
        }
        _ => false,
    }
}

fn sql_without_psql_commands_and_copy_data(source: &str) -> String {
    let mut sql = String::with_capacity(source.len());
    let mut copy_data_remaining = 0;

    for line in source.lines() {
        let trimmed = line.trim_start();
        if copy_data_remaining > 0 {
            if trimmed == "\\." {
                copy_data_remaining -= 1;
            }
            continue;
        }
        if trimmed.starts_with('\\') {
            if trimmed.to_ascii_lowercase().starts_with("\\copy ")
                && trimmed.to_ascii_lowercase().contains("from stdin")
            {
                copy_data_remaining += 1;
            }
            if ["\\gexec", "\\gset", "\\gx", "\\gdesc", "\\g"]
                .iter()
                .any(|command| is_psql_command(trimmed, command))
                && !sql.trim_end().ends_with(';')
                && !sql.trim().is_empty()
            {
                sql.push(';');
            }
            continue;
        }

        let line = strip_inline_psql_command(line);
        let line = line.replace("\\;", ";");
        let line = replace_psql_variables(&line);
        sql.push_str(&line);
        sql.push('\n');
        copy_data_remaining += line
            .split(';')
            .filter(|statement| is_copy_data(statement))
            .count();
    }

    sql
}

fn split_statements(sql: &str) -> Result<Vec<String>, String> {
    // Keep source offsets stable while making known-invalid lexical tokens scannable.
    let mut scanner_sql = sql
        .replace("\"\"", "aa")
        .replace("\\u", "\\\\")
        .replace("\\U", "\\\\");

    loop {
        match pg_query::split_with_scanner(&scanner_sql) {
            Ok(statements) => {
                let statements = statements
                    .into_iter()
                    .map(|statement| {
                        let start = statement.as_ptr() as usize - scanner_sql.as_ptr() as usize;
                        sql.get(start..start + statement.len())
                            .ok_or_else(|| "invalid source span from PostgreSQL scanner".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(coalesce_sql_standard_function_bodies(statements));
            }
            Err(error) => {
                let message = error.to_string();
                let token = message
                    .split_once("at or near \"")
                    .and_then(|(_, suffix)| suffix.split_once('"').map(|(token, _)| token));
                let Some(token) = token.filter(|token| !token.is_empty()) else {
                    return Err(message);
                };
                let replacement = "a".repeat(token.len());
                let sanitized = scanner_sql.replace(token, &replacement);
                if sanitized == scanner_sql {
                    return Err(message);
                }
                scanner_sql = sanitized;
            }
        }
    }
}

fn strip_inline_psql_command(line: &str) -> String {
    let trimmed = line.trim_end();
    if let Some(prefix) = trimmed.strip_suffix('\\') {
        return format!("{};", prefix.trim_end());
    }

    let commands = [
        "\\gexec",
        "\\gset",
        "\\gx",
        "\\gdesc",
        "\\bind",
        "\\sendpipeline",
        "\\parse",
        "\\crosstabview",
        "\\g",
    ];
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut previous = None;
    let mut characters = trimmed.char_indices().peekable();

    while let Some((index, character)) = characters.next() {
        if in_single_quote {
            if character == '\'' {
                if characters.peek().is_some_and(|(_, next)| *next == '\'') {
                    characters.next();
                } else {
                    in_single_quote = false;
                }
            }
            previous = Some(character);
            continue;
        }
        if in_double_quote {
            if character == '"' {
                if characters.peek().is_some_and(|(_, next)| *next == '"') {
                    characters.next();
                } else {
                    in_double_quote = false;
                }
            }
            previous = Some(character);
            continue;
        }
        if character == '-' && characters.peek().is_some_and(|(_, next)| *next == '-') {
            break;
        }
        match character {
            '\'' => in_single_quote = true,
            '"' => in_double_quote = true,
            '\\' if previous.is_none_or(char::is_whitespace) => {
                for command in commands {
                    let Some(suffix) = trimmed[index..].strip_prefix(command) else {
                        continue;
                    };
                    if suffix.is_empty() || suffix.starts_with(char::is_whitespace) {
                        let prefix = trimmed[..index].trim_end();
                        if prefix.is_empty() {
                            return String::new();
                        }
                        return if prefix.ends_with(';') {
                            prefix.to_owned()
                        } else {
                            format!("{prefix};")
                        };
                    }
                }
            }
            _ => {}
        }
        previous = Some(character);
    }

    line.to_owned()
}

fn is_psql_command(text: &str, command: &str) -> bool {
    text == command
        || text.strip_prefix(command).is_some_and(|suffix| {
            suffix.is_empty() || suffix.starts_with(char::is_whitespace) || suffix.starts_with("--")
        })
}

fn replace_psql_variables(line: &str) -> String {
    let characters = line.chars().collect::<Vec<_>>();
    let mut result = String::with_capacity(line.len());
    let mut index = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while index < characters.len() {
        let character = characters[index];
        if in_single_quote {
            result.push(character);
            index += 1;
            if character == '\'' {
                if characters.get(index) == Some(&'\'') {
                    result.push('\'');
                    index += 1;
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }
        if in_double_quote {
            result.push(character);
            index += 1;
            if character == '"' {
                if characters.get(index) == Some(&'"') {
                    result.push('"');
                    index += 1;
                } else {
                    in_double_quote = false;
                }
            }
            continue;
        }
        if character == '-' && characters.get(index + 1) == Some(&'-') {
            result.extend(characters[index..].iter().copied());
            break;
        }
        if character == '\'' {
            in_single_quote = true;
            result.push(character);
            index += 1;
            continue;
        }
        if character == '"' {
            in_double_quote = true;
            result.push(character);
            index += 1;
            continue;
        }
        if character == ':'
            && characters.get(index + 1) == Some(&'\'')
            && (index == 0 || characters[index - 1] != ':')
        {
            result.push('\'');
            index += 2;
            while index < characters.len() {
                let current = characters[index];
                result.push(current);
                index += 1;
                if current == '\'' {
                    if characters.get(index) == Some(&'\'') {
                        result.push('\'');
                        index += 1;
                    } else {
                        break;
                    }
                }
            }
            continue;
        }
        if character == ':'
            && characters.get(index + 1) == Some(&'"')
            && (index == 0 || characters[index - 1] != ':')
        {
            result.push('"');
            index += 2;
            while index < characters.len() {
                let current = characters[index];
                result.push(current);
                index += 1;
                if current == '"' {
                    if characters.get(index) == Some(&'"') {
                        result.push('"');
                        index += 1;
                    } else {
                        break;
                    }
                }
            }
            continue;
        }
        if character == ':'
            && characters
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_alphabetic() || *next == '_')
            && (index == 0 || characters[index - 1] != ':')
        {
            result.push_str(if index > 0 && characters[index - 1] == '.' {
                "placeholder"
            } else {
                "0"
            });
            index += 2;
            while characters
                .get(index)
                .is_some_and(|next| next.is_ascii_alphanumeric() || *next == '_')
            {
                index += 1;
            }
            continue;
        }

        result.push(character);
        index += 1;
    }

    result
}

fn coalesce_sql_standard_function_bodies(statements: Vec<&str>) -> Vec<String> {
    let mut result = Vec::with_capacity(statements.len());
    let mut body: Option<String> = None;

    for statement in statements {
        if let Some(current) = &mut body {
            current.push_str(statement);
            if is_sql_standard_body_end(statement) {
                current.push(';');
                result.push(body.take().expect("SQL standard body is present"));
            } else {
                current.push(';');
            }
            continue;
        }

        let lower = statement.to_ascii_lowercase();
        if lower.contains("begin atomic")
            && (lower.contains("create function") || lower.contains("create procedure"))
        {
            let mut initial = statement.to_owned();
            if !lower.trim_end().ends_with("begin atomic") {
                initial.push(';');
            }
            body = Some(initial);
            if is_sql_standard_body_end(statement) {
                result.push(body.take().expect("SQL standard body is present"));
            }
        } else {
            result.push(statement.to_owned());
        }
    }

    if let Some(body) = body {
        result.push(body);
    }
    result
}

fn is_sql_standard_body_end(statement: &str) -> bool {
    statement
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("--"))
        .and_then(|line| {
            line.split_once("--")
                .map_or(Some(line), |(code, _)| Some(code.trim()))
        })
        .is_some_and(|line| line.trim_end_matches(';').trim() == "END")
}

fn decode_windows_1252(bytes: &[u8]) -> String {
    const EXTENDED: [char; 32] = [
        '€', '\u{0081}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{008d}', 'Ž',
        '\u{008f}', '\u{0090}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ',
        '\u{009d}', 'ž', 'Ÿ',
    ];

    bytes
        .iter()
        .map(|byte| match byte {
            0x80..=0x9f => EXTENDED[usize::from(*byte - 0x80)],
            _ => char::from(*byte),
        })
        .collect()
}

fn is_copy_data(line: &str) -> bool {
    let lower = line.trim_start().to_ascii_lowercase();
    let words = lower
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words.first() == Some(&"copy")
        && words
            .windows(2)
            .any(|pair| pair == ["from", "stdin"] || pair == ["from", "stdout"])
}
