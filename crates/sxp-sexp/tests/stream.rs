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
