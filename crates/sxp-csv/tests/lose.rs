use std::io::{BufRead, BufReader, Cursor, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use sxp::SexpReader;

#[test]
fn lose_emits_each_record_as_a_top_level_expression() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sxp-csv"))
        .args(["--lose", "--color", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start sxp-csv");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(b"name,age\nAda,36\nGrace,45\n")
        .expect("write CSV input");
    let output = child.wait_with_output().expect("wait for sxp-csv");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "(name age)\n(\"Ada\" 36)\n(\"Grace\" 45)\n"
    );

    let mut reader = SexpReader::new(Cursor::new(
        "(name age)\n(\"Ada\" 36)\n(\"Grace\" 45)\n".as_bytes(),
    ));
    assert_eq!(
        reader.read_next().unwrap().unwrap().to_string(),
        "(name age)"
    );
    assert_eq!(
        reader.read_next().unwrap().unwrap().to_string(),
        r#"("Ada" 36)"#
    );
    assert_eq!(
        reader.read_next().unwrap().unwrap().to_string(),
        r#"("Grace" 45)"#
    );
    assert_eq!(reader.read_next().unwrap(), None);
}

#[test]
fn lose_honors_no_headers() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sxp-csv"))
        .args(["--lose", "--no-headers", "--color", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start sxp-csv");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(b"Ada,36\n")
        .expect("write CSV input");
    let output = child.wait_with_output().expect("wait for sxp-csv");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "(\"Ada\" 36)\n");
}

#[test]
fn lose_writes_records_before_stdin_reaches_eof() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sxp-csv"))
        .args(["--lose", "--color", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start sxp-csv");
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
    stdin.write_all(b"name,age\n").expect("write header");
    stdin.flush().expect("flush header");
    assert_eq!(read_line(&mut child, &receiver), "(name age)\n");

    stdin.write_all(b"Ada,36\n").expect("write record");
    stdin.flush().expect("flush record");
    assert_eq!(read_line(&mut child, &receiver), "(\"Ada\" 36)\n");

    drop(stdin);
    assert!(child.wait().expect("wait for sxp-csv").success());
    reader.join().expect("join stdout reader");
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
