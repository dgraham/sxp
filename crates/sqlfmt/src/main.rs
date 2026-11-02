use std::error::Error;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use getopts::Options;
use sxp::cli::args;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = args()?;
    let opts = opts();
    let matches = opts.parse(&args[1..])?;

    if matches.opt_present("help") {
        usage(&opts);
        return Ok(());
    }

    if matches.opt_present("version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    match matches.free.first().map(String::as_str) {
        Some("split") => split(),
        Some(command) => Err(format!("unknown command {command:?}").into()),
        None => Err("expected a command, try `sqlfmt --help`".into()),
    }
}

fn split() -> Result<(), Box<dyn Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    for statement in pg_query::split_with_parser(&input)?.iter() {
        out.write_all(statement.trim().as_bytes())?;
        out.write_all(b"\0")?;
    }
    out.flush()?;
    Ok(())
}

fn opts() -> Options {
    let mut opts = Options::new();
    opts.optflag("h", "help", "Print this help message");
    opts.optflag("V", "version", "Print version information");
    opts
}

fn usage(opts: &Options) {
    let brief = "Format SQL\n\nUsage:\n    sqlfmt [options] COMMAND\n\nCommands:\n    split    Read SQL statements from stdin and print them terminated by null bytes";
    println!("{}", opts.usage(brief));
}
