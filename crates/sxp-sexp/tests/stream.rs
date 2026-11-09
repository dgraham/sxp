use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

#[test]
fn stream_formats_consecutive_expressions_before_eof() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sxp-sexp"))
        .args(["--stream", "--format=compact", "--color=never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start sxp-sexp");
    let stdout = child.stdout.take().expect("piped stdout");
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            if stdout.read_line(&mut line).expect("read output") == 0 {
                break;
            }
            if sender.send(line).is_err() {
                break;
            }
        }
    });

    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin
        .write_all(b"(name age)\n")
        .expect("write first expression");
    stdin.flush().expect("flush first expression");
    assert_eq!(read_line(&mut child, &receiver), "(name age)\n");

    stdin
        .write_all(b"(\"Ada\" 36)\n")
        .expect("write second expression");
    stdin.flush().expect("flush second expression");
    assert_eq!(read_line(&mut child, &receiver), "(\"Ada\" 36)\n");

    drop(stdin);
    assert!(child.wait().expect("wait for sxp-sexp").success());
    reader.join().expect("join stdout reader");
}

#[test]
fn first_and_car_select_each_expression() {
    for selector in ["--first", "--car"] {
        let output = run(
            &["--stream", selector, "--format=compact", "--color=never"],
            b"(a b)\n(c d)\n",
        );

        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "a\nc\n");
    }
}

#[test]
fn rest_and_cdr_select_each_expression_tail() {
    for selector in ["--rest", "--cdr"] {
        let output = run(
            &["--stream", selector, "--format=compact", "--color=never"],
            b"(header other)\n(\"only\")\n",
        );

        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "(other)\n()\n");
    }
}

#[test]
fn selectors_reject_non_lists_and_empty_lists() {
    let non_list = run(&["--first"], b"atom");
    assert!(!non_list.status.success());
    assert!(
        String::from_utf8(non_list.stderr)
            .unwrap()
            .contains("--first/--car requires a list")
    );

    let empty_list = run(&["--rest"], b"()");
    assert!(!empty_list.status.success());
    assert!(
        String::from_utf8(empty_list.stderr)
            .unwrap()
            .contains("cannot select from an empty list")
    );
}

#[test]
fn first_and_rest_are_mutually_exclusive() {
    let output = run(&["--first", "--cdr"], b"(a b)");
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("choose only one of --first/--car or --rest/--cdr")
    );
}

fn run(args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sxp-sexp"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start sxp-sexp");
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin.write_all(input).expect("write input");
    drop(stdin);
    child.wait_with_output().expect("wait for sxp-sexp")
}

fn read_line(child: &mut Child, receiver: &Receiver<String>) -> String {
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out waiting for streamed output: {error}");
        }
    }
}
