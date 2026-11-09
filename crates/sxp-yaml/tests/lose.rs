use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn lose_emits_each_document_as_a_top_level_expression() {
    let output = run(
        &["--lose", "--color", "never"],
        b"---\na: 1\n---\n- x\n- y\n",
    );

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "(map (\"a\" 1))\n(seq \"x\" \"y\")\n"
    );
}

#[test]
fn lose_emits_a_single_document_without_a_wrapper() {
    let output = run(&["--lose", "--color", "never"], b"a: 1\n");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "(map (\"a\" 1))\n"
    );
}

#[test]
fn lose_emits_nil_for_empty_input() {
    let output = run(&["--lose", "--color", "never"], b"");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "nil\n");
}

#[test]
fn lose_rejects_non_line_oriented_formats() {
    let output = run(&["--lose", "--format=vertical"], b"a: 1\n");

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("--lose supports only --format=compact or --format=minified")
    );
}

fn run(args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sxp-yaml"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start sxp-yaml");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input)
        .expect("write YAML input");
    child.wait_with_output().expect("wait for sxp-yaml")
}
