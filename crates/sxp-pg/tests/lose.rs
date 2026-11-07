use std::io::Cursor;
use std::io::Write;
use std::process::{Command, Stdio};

use sxp_pg::PostgresParser;

#[test]
fn sql_stream_yields_each_statement_individually() {
    let mut parser = PostgresParser::new();
    let mut stream = parser.stream(Cursor::new("SELECT 1; SELECT 2;")).unwrap();

    assert_eq!(
        stream.read_next().unwrap().unwrap().to_string(),
        "(select (columns 1))"
    );
    assert_eq!(
        stream.read_next().unwrap().unwrap().to_string(),
        "(select (columns 2))"
    );
    assert_eq!(stream.read_next().unwrap(), None);

    assert_eq!(
        parser.parse("SELECT 1; SELECT 2;").unwrap().to_string(),
        "(statements (select (columns 1)) (select (columns 2)))"
    );
}

#[test]
fn lose_emits_each_statement_as_a_top_level_expression() {
    let output = run(&["--lose", "--color", "never"], b"SELECT 1; SELECT 2;");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "(select (columns 1))\n(select (columns 2))\n"
    );
}

#[test]
fn lose_emits_single_statements_without_a_wrapper() {
    let output = run(&["--lose", "--color", "never"], b"SELECT 1");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "(select (columns 1))\n"
    );
}

#[test]
fn lose_rejects_non_line_oriented_formats() {
    let output = run(&["--lose", "--format=vertical"], b"SELECT 1; SELECT 2;");

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("--lose supports only --format=compact or --format=minified")
    );
}

fn run(args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sxp-pg"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start sxp-pg");
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin.write_all(input).expect("write SQL input");
    drop(stdin);
    child.wait_with_output().expect("wait for sxp-pg")
}
