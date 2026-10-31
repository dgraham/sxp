use std::error::Error;
use std::io::{self, Read};
use std::process::ExitCode;

use getopts::Options;
use sxp::cli::{OptionsBuilder, args, is_broken_pipe};
use sxp::pattern::{Pattern, QueryMatch};
use sxp::{ColorChoice, Colors, Format, Sexp, SexpParser, write_stdout};
use sxp_xml::{ParserOptions, XmlParser, grammar};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) if is_broken_pipe(error.as_ref()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let args = args()?;
    let opts = opts();
    let matches = opts.parse(&args[1..])?;

    if matches.opt_present("help") {
        usage(&opts);
        return Ok(ExitCode::SUCCESS);
    }

    if matches.opt_present("version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }

    let print_pattern = matches.opt_present("print-pattern");
    let validate = matches.opt_present("validate");
    let query = matches.opt_str("query");
    let selected_modes =
        usize::from(print_pattern) + usize::from(validate) + usize::from(query.is_some());
    if selected_modes > 1 {
        return Err("choose only one of --print-pattern, --validate, or --query".into());
    }

    let grammar = grammar();
    if print_pattern {
        println!("{}", grammar.to_sexp());
        return Ok(ExitCode::SUCCESS);
    }

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    if validate {
        let value = parse_sexp(&input)?;
        if grammar.matches_root(&value) {
            return Ok(ExitCode::SUCCESS);
        }
        eprintln!("S-expression does not match the sxp-xml output grammar");
        return Ok(ExitCode::FAILURE);
    }

    let max_symbol_entries = matches
        .opt_str("max-symbol-entries")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("invalid max-symbol-entries value {value:?}"))
        })
        .transpose()?;
    let options = ParserOptions::default()
        .preserve_whitespace(matches.opt_present("preserve-whitespace"))
        .preserve_comments(matches.opt_present("preserve-comments"))
        .max_symbol_entries(max_symbol_entries);

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

    if let Some(query_text) = query {
        let tree = parse_sexp(&input)?;
        let pattern = Pattern::parse(&query_text)?;
        let matches = grammar.query(&tree, &pattern)?;
        write_stdout(&query_result(matches), format, &colors, choice)?;
    } else {
        let mut parser = XmlParser::with_options(options);
        write_stdout(&parser.parse(&input)?, format, &colors, choice)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn query_result(matches: Vec<QueryMatch<'_>>) -> Sexp {
    Sexp::list(matches.into_iter().map(|matched| {
        let captures = matched.captures.into_iter().map(|(name, value)| {
            Sexp::list([Sexp::symbol("capture"), Sexp::string(name), value.clone()])
        });
        Sexp::list(
            std::iter::once(Sexp::symbol("match"))
                .chain([Sexp::list([Sexp::symbol("node"), matched.node.clone()])])
                .chain(captures),
        )
    }))
}

fn parse_sexp(input: &str) -> sxp::Result<Sexp> {
    SexpParser::new().parse(input)
}

fn opts() -> Options {
    let mut opts = OptionsBuilder::new(env!("CARGO_PKG_NAME"))
        .grammar()
        .symbol_pool()
        .formatting()
        .colors()
        .build();
    opts.optflag(
        "",
        "preserve-whitespace",
        "Preserve whitespace between and inside nodes",
    );
    opts.optflag("", "preserve-comments", "Preserve comments in the output");
    opts
}

fn usage(opts: &Options) {
    let brief = "Convert XML to an S-expression, validate, or query S-expressions\n\nUsage:\n    sxp-xml [options]";
    println!("{}", opts.usage(brief));
}
