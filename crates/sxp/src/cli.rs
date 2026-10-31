use std::env;
use std::error::Error;
use std::io::{self, ErrorKind};

use getopts::Options;

/// Returns the process arguments as UTF-8 strings.
pub fn args() -> io::Result<Vec<String>> {
    env::args_os()
        .map(|arg| {
            arg.into_string().map_err(|arg| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("Invalid UTF-8 argument: {}", arg.to_string_lossy()),
                )
            })
        })
        .collect()
}

/// Returns whether an error represents a broken pipe.
pub fn is_broken_pipe(error: &(dyn Error + 'static)) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == ErrorKind::BrokenPipe)
}

/// Builds the standard command-line options used by sxp commands.
pub struct OptionsBuilder {
    command_name: String,
    options: Options,
}

impl OptionsBuilder {
    /// Creates a builder with the standard help and version flags.
    pub fn new(command_name: impl Into<String>) -> Self {
        let mut options = Options::new();
        options.optflag("h", "help", "Print this help message");
        options.optflag("V", "version", "Print version information");
        Self {
            command_name: command_name.into(),
            options,
        }
    }

    /// Adds grammar printing, validation, and query options.
    pub fn grammar(mut self) -> Self {
        self.options.optflag(
            "",
            "print-pattern",
            &format!(
                "Print the grammar for S-expressions emitted by {}",
                self.command_name
            ),
        );
        self.options.optflag(
            "",
            "validate",
            &format!(
                "Validate stdin as an S-expression emitted by {}",
                self.command_name
            ),
        );
        self.options.optopt(
            "",
            "query",
            "Find matching subtrees in an S-expression using a pattern",
            "PATTERN",
        );
        self
    }

    /// Adds output format, width, and color selection options.
    pub fn formatting(mut self) -> Self {
        self.options.optopt(
            "",
            "format",
            "Output format: compact, minified, horizontal, vertical, token, or inspect",
            "FORMAT",
        );
        self.options.optopt(
            "w",
            "width",
            "Target line width for --format=horizontal",
            "COLUMNS",
        );
        self.options.optopt(
            "",
            "color",
            "When to colorize output: auto, always, or never",
            "WHEN",
        );
        self
    }

    /// Adds the repeatable color customization option.
    pub fn colors(mut self) -> Self {
        self.options.optmulti(
            "",
            "colors",
            "Color for an element: TYPE:ATTRIBUTE:VALUE (types: atom, string, punct)",
            "SPEC",
        );
        self
    }

    /// Adds the symbol interning limit option.
    pub fn symbol_pool(mut self) -> Self {
        self.options.optopt(
            "",
            "max-symbol-entries",
            "Maximum number of symbols retained for interning (0 disables caching)",
            "COUNT",
        );
        self
    }

    /// Returns the configured `getopts::Options`.
    pub fn build(self) -> Options {
        self.options
    }
}

#[cfg(test)]
mod tests {
    use super::is_broken_pipe;
    use std::io::{Error, ErrorKind};

    #[test]
    fn identifies_broken_pipe_errors() {
        assert!(is_broken_pipe(&Error::from(ErrorKind::BrokenPipe)));
        assert!(!is_broken_pipe(&Error::from(ErrorKind::Other)));
    }
}
