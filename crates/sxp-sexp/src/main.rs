use std::error::Error;
use std::io::{self, Read};
use std::process::ExitCode;

use getopts::Options;
use sxp::cli::{OptionsBuilder, args, is_broken_pipe};
use sxp::{ColorChoice, Colors, Format, ParserOptions, Sexp, SexpParser, SexpReader, write_stdout};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if is_broken_pipe(error.as_ref()) => ExitCode::SUCCESS,
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

    let max_symbol_entries = matches
        .opt_str("max-symbol-entries")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("invalid max-symbol-entries value {value:?}"))
        })
        .transpose()?;
    let options = ParserOptions::default().max_symbol_entries(max_symbol_entries);

    let first = matches.opt_present("first") || matches.opt_present("car");
    let rest = matches.opt_present("rest") || matches.opt_present("cdr");
    if first && rest {
        return Err("choose only one of --first/--car or --rest/--cdr".into());
    }
    let selector = if first {
        Some(Selector::First)
    } else if rest {
        Some(Selector::Rest)
    } else {
        None
    };

    let choice = match matches.opt_str("color").as_deref() {
        None => ColorChoice::Auto,
        Some(mode) => mode.parse()?,
    };

    let mut colors = Colors::default();
    for spec in matches.opt_strs("colors") {
        colors.set(&spec)?;
    }

    let format: Format = match matches.opt_str("format").as_deref() {
        None => Format::default(),
        Some(text) => text.parse()?,
    };
    let format = match matches.opt_str("width") {
        None => format,
        Some(text) => {
            let width = text
                .parse()
                .map_err(|_| format!("invalid width {text:?}"))?;
            format.with_width(width)
        }
    };

    if matches.opt_present("stream") {
        let stdin = io::stdin();
        let mut reader = SexpReader::with_options(stdin.lock(), options);
        while let Some(sexp) = reader.read_next()? {
            let sexp = match selector {
                Some(selector) => select(sexp, selector)?,
                None => sexp,
            };
            write_stdout(&sexp, format, &colors, choice)?;
        }
    } else {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        let mut parser = SexpParser::with_options(options);
        let sexp = parser.parse(&input)?;
        let sexp = match selector {
            Some(selector) => select(sexp, selector)?,
            None => sexp,
        };
        write_stdout(&sexp, format, &colors, choice)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Selector {
    First,
    Rest,
}

fn select(sexp: Sexp, selector: Selector) -> io::Result<Sexp> {
    let operation = match selector {
        Selector::First => "--first/--car",
        Selector::Rest => "--rest/--cdr",
    };
    let is_list = sexp.is_list();
    let selected = match selector {
        Selector::First => sexp.into_first(),
        Selector::Rest => sexp.into_rest(),
    };
    selected.ok_or_else(|| {
        let message = if is_list {
            format!("{operation} cannot select from an empty list")
        } else {
            format!("{operation} requires a list")
        };
        io::Error::new(io::ErrorKind::InvalidInput, message)
    })
}

fn opts() -> Options {
    let mut opts = OptionsBuilder::new(env!("CARGO_PKG_NAME"))
        .symbol_pool()
        .formatting()
        .colors()
        .build();
    opts.optflag("", "stream", "Format consecutive top-level expressions");
    opts.optflag("", "first", "Select the first item of each top-level list");
    opts.optflag("", "car", "Alias for --first");
    opts.optflag("", "rest", "Select the rest of each top-level list");
    opts.optflag("", "cdr", "Alias for --rest");
    opts
}

fn usage(opts: &Options) {
    let brief = "Format an S-expression or a stream of consecutive S-expressions\n\nUsage:\n    sxp-sexp [options]";
    println!("{}", opts.usage(brief));
}
