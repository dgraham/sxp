use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use getopts::{Options, ParsingStyle};
use sxp::cli::{OptionsBuilder, args};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
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
        usage(&opts)?;
        return Ok(ExitCode::SUCCESS);
    }

    if matches.opt_present("version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }

    let Some(subcommand) = matches.free.first() else {
        usage(&opts)?;
        return Ok(ExitCode::from(2));
    };

    let cmd = subcommand;

    let Some(bin) = command_path(&format!("sxp-{cmd}")) else {
        eprintln!("sxp-{cmd} not found in PATH or XDG_BIN_HOME");
        return Ok(ExitCode::from(127));
    };

    match Command::new(bin).args(&matches.free).status() {
        Ok(status) => Ok(ExitCode::from(match status.code() {
            Some(code) => code as u8,
            None => 1,
        })),
        Err(error) => {
            eprintln!("could not execute sxp-{cmd}: {error}");
            Ok(ExitCode::from(127))
        }
    }
}

fn opts() -> Options {
    let mut opts = OptionsBuilder::new(env!("CARGO_PKG_NAME")).build();
    opts.parsing_style(ParsingStyle::StopAtFirstFree);
    opts
}

fn usage(opts: &Options) -> io::Result<()> {
    let subcommands = available_subcommands()?;
    let commands = if subcommands.is_empty() {
        "    (none found)".to_owned()
    } else {
        subcommands
            .iter()
            .map(|subcommand| format!("    {subcommand}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let brief = format!(
        "Convert structured text to S-expressions\n\n\
         Usage:\n    sxp <command> [options]\n\n\
         Available commands:\n{commands}\n\n\
         See 'sxp <command> --help' to read about a specific subcommand."
    );
    println!("{}", opts.usage(&brief));
    Ok(())
}

fn xdg_bin_home() -> Option<PathBuf> {
    env::var_os("XDG_BIN_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|s| !s.is_empty())
                .map(|home| PathBuf::from(home).join(".local/bin"))
        })
}

fn command_path(name: &str) -> Option<PathBuf> {
    for dir in command_directories() {
        let path = dir.join(name);
        if is_executable(&path) {
            return Some(path);
        }
    }

    None
}

fn command_directories() -> Vec<PathBuf> {
    let mut directories = xdg_bin_home().into_iter().collect::<Vec<_>>();
    if let Some(paths) = env::var_os("PATH") {
        directories.extend(env::split_paths(&paths).map(|dir| {
            if dir.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                dir
            }
        }));
    }
    directories
}

fn available_subcommands() -> io::Result<Vec<String>> {
    let subcommands = command_directories()
        .iter()
        .map(|directory| subcommands_in(directory))
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>();
    Ok(subcommands.into_iter().collect())
}

fn subcommands_in(directory: &Path) -> io::Result<Vec<String>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(Vec::new());
        }
        Err(error) => return Err(error),
    };

    entries
        .map(|entry| entry.map(|entry| subcommand_name(&entry)))
        .collect::<io::Result<Vec<_>>>()
        .map(|subcommands| subcommands.into_iter().flatten().collect())
}

fn subcommand_name(entry: &fs::DirEntry) -> Option<String> {
    if !is_executable(&entry.path()) {
        return None;
    }

    entry
        .file_name()
        .to_str()?
        .strip_prefix("sxp-")
        .filter(|subcommand| !subcommand.is_empty())
        .map(str::to_owned)
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
