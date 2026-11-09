use std::io::{self, IsTerminal, Write};

use termcolor::{Color, ColorSpec, StandardStream, WriteColor};

use crate::{Format, Sexp};

/// When to emit ANSI color escape sequences.
pub use termcolor::ColorChoice;

/// Resolves the color choice for stdout. Disables `Auto` when stdout
/// is not a terminal and respects `TERM` and `NO_COLOR`. Explicit
/// choices override the environment.
pub(crate) fn resolve(choice: ColorChoice) -> ColorChoice {
    match choice {
        ColorChoice::Auto if !io::stdout().is_terminal() => ColorChoice::Never,
        choice => choice,
    }
}

/// The color theme for S-expression output. By default strings print in
/// magenta, parentheses print dimmed, and atoms keep the terminal's default
/// foreground color.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Colors {
    pub(crate) atom: ColorSpec,
    pub(crate) string: ColorSpec,
    pub(crate) punct: ColorSpec,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            atom: ColorSpec::new().set_reset(false).clone(),
            string: ColorSpec::new()
                .set_fg(Some(Color::Magenta))
                .set_reset(false)
                .clone(),
            punct: ColorSpec::new().set_dimmed(true).set_reset(false).clone(),
        }
    }
}

impl Colors {
    /// Applies one ripgrep-style `TYPE:ATTRIBUTE:VALUE` specification, where
    /// type is `atom`, `string`, or `punct`, attribute is `fg`, `bg`, or
    /// `style`, and value is a color name, `ansi:N`, a comma-separated style
    /// list, or `none`. Later specifications override earlier ones.
    pub fn set(&mut self, spec: &str) -> Result<(), String> {
        let invalid = || format!("invalid --colors spec {spec:?} (expected TYPE:ATTRIBUTE:VALUE)");
        let (kind, rest) = spec.split_once(':').ok_or_else(invalid)?;
        let (attribute, value) = rest.split_once(':').ok_or_else(invalid)?;
        let style = match kind {
            "atom" => &mut self.atom,
            "string" => &mut self.string,
            "punct" => &mut self.punct,
            _ => {
                return Err(format!(
                    "invalid --colors type {kind:?} (expected atom, string, or punct)"
                ));
            }
        };
        apply(style, attribute, value)
    }
}

fn apply(style: &mut ColorSpec, attribute: &str, value: &str) -> Result<(), String> {
    match attribute {
        "fg" => {
            style.set_fg(parse_color(value)?);
        }
        "bg" => {
            style.set_bg(parse_color(value)?);
        }
        "style" => {
            if value == "none" {
                style
                    .set_bold(false)
                    .set_dimmed(false)
                    .set_italic(false)
                    .set_underline(false)
                    .set_strikethrough(false);
            } else {
                for name in value.split(',') {
                    match name {
                        "bold" => {
                            style.set_bold(true);
                        }
                        "dimmed" => {
                            style.set_dimmed(true);
                        }
                        "italic" => {
                            style.set_italic(true);
                        }
                        "underline" => {
                            style.set_underline(true);
                        }
                        "strikethrough" => {
                            style.set_strikethrough(true);
                        }
                        _ => return Err(format!("invalid style {name:?}")),
                    }
                }
            }
        }
        _ => {
            return Err(format!(
                "invalid color attribute {attribute:?} (expected fg, bg, or style)"
            ));
        }
    }
    style.set_reset(false);
    Ok(())
}

/// Parses a color value: a name, `ansi:N`, or `none` to clear.
fn parse_color(value: &str) -> Result<Option<Color>, String> {
    if value == "none" {
        return Ok(None);
    }
    if let Some(index) = value.strip_prefix("ansi:") {
        return index
            .parse::<u8>()
            .map(|index| Some(Color::Ansi256(index)))
            .map_err(|_| format!("invalid ANSI color index in {value:?} (expected 0-255)"));
    }
    let color = match value {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "bright-black" => Color::Ansi256(8),
        "bright-red" => Color::Ansi256(9),
        "bright-green" => Color::Ansi256(10),
        "bright-yellow" => Color::Ansi256(11),
        "bright-blue" => Color::Ansi256(12),
        "bright-magenta" => Color::Ansi256(13),
        "bright-cyan" => Color::Ansi256(14),
        "bright-white" => Color::Ansi256(15),
        _ => {
            return Err(format!(
                "invalid color {value:?} (expected a named color or ansi:0-255)"
            ));
        }
    };
    Ok(Some(color))
}

/// Wraps `write` in the styling from `spec`.
pub(crate) fn styled(
    out: &mut dyn WriteColor,
    spec: Option<&ColorSpec>,
    write: impl FnOnce(&mut dyn WriteColor) -> io::Result<()>,
) -> io::Result<()> {
    match spec.filter(|spec| !spec.is_none()) {
        Some(spec) => {
            out.set_color(spec)?;
            write(&mut *out)?;
            out.reset()
        }
        None => write(out),
    }
}

/// Writes `sexp` to `out` in `format`, optionally colorized. Display, token,
/// and inspect formats are always plain for machine-readable output.
pub fn write(
    sexp: &Sexp,
    out: &mut dyn WriteColor,
    format: Format,
    colors: Option<&Colors>,
) -> io::Result<()> {
    match format {
        Format::Compact => sexp.fmt_compact(out, colors),
        Format::Minified => sexp.fmt_minified(out, colors),
        Format::Horizontal { width } => sexp.fmt_pretty(out, colors, 0, width),
        Format::Display => sexp.fmt_display(out),
        Format::Token => sexp.fmt_token(out),
        Format::Inspect => sexp.fmt_inspect(out),
        Format::Vertical => sexp.fmt_pretty(out, colors, 0, 0),
    }
}

/// Writes `sexp` to stdout, colorized according to `choice`. Display, token,
/// and inspect formats are always plain.
pub fn write_stdout(
    sexp: &Sexp,
    format: Format,
    colors: &Colors,
    choice: ColorChoice,
) -> io::Result<()> {
    let mut stream = StandardStream::stdout(resolve(choice));
    write(sexp, &mut stream, format, Some(colors))?;
    if !matches!(format, Format::Token | Format::Inspect) {
        stream.write_all(b"\n")?;
    }
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::{ColorChoice, Colors, Format, resolve, write};
    use crate::Sexp;
    use termcolor::{Ansi, ColorSpec, NoColor, WriteColor};

    fn rendered(spec: &ColorSpec) -> String {
        let mut buffer = Vec::new();
        let mut stream = Ansi::new(&mut buffer);
        stream.set_color(spec).unwrap();
        String::from_utf8(buffer).unwrap()
    }

    #[test]
    fn parses_color_modes() {
        assert_eq!("auto".parse::<ColorChoice>().unwrap(), ColorChoice::Auto);
        assert_eq!(
            "always".parse::<ColorChoice>().unwrap(),
            ColorChoice::Always
        );
        assert_eq!("never".parse::<ColorChoice>().unwrap(), ColorChoice::Never);
        assert!("sometimes".parse::<ColorChoice>().is_err());
    }

    #[test]
    fn resolves_explicit_color_choices() {
        assert_eq!(resolve(ColorChoice::Always), ColorChoice::Always);
        assert_eq!(resolve(ColorChoice::Never), ColorChoice::Never);
    }

    #[test]
    fn defaults_print_dim_parentheses_and_magenta_strings() {
        let colors = Colors::default();
        assert_eq!(rendered(&colors.atom), "");
        assert_eq!(rendered(&colors.string), "\x1b[35m");
        assert_eq!(rendered(&colors.punct), "\x1b[2m");
    }

    #[test]
    fn parses_specifications() {
        let mut colors = Colors::default();

        colors.set("atom:fg:ansi:214").unwrap();
        assert_eq!(rendered(&colors.atom), "\x1b[38;5;214m");

        colors.set("string:fg:none").unwrap();
        assert_eq!(rendered(&colors.string), "");

        colors.set("punct:style:bold,italic").unwrap();
        assert_eq!(rendered(&colors.punct), "\x1b[1m\x1b[2m\x1b[3m");

        colors.set("string:bg:cyan").unwrap();
        colors.set("string:style:none").unwrap();
        assert_eq!(rendered(&colors.string), "\x1b[46m");
    }

    #[test]
    fn rejects_invalid_specifications() {
        let mut colors = Colors::default();
        assert!(colors.set("atom").is_err());
        assert!(colors.set("atom:fg").is_err());
        assert!(colors.set("bogus:fg:red").is_err());
        assert!(colors.set("atom:weight:bold").is_err());
        assert!(colors.set("atom:fg:not-a-color").is_err());
        assert!(colors.set("atom:style:blink").is_err());
        assert!(colors.set("atom:style:blinking").is_err());
        assert!(colors.set("atom:fg:ansi:999").is_err());
    }

    #[test]
    fn parses_all_colors() {
        let mut colors = Colors::default();
        colors.set("atom:fg:bright-white").unwrap();
        assert_eq!(rendered(&colors.atom), "\x1b[38;5;15m");
        colors.set("atom:bg:white").unwrap();
        assert_eq!(rendered(&colors.atom), "\x1b[38;5;15m\x1b[47m");
        colors.set("atom:fg:ansi:0").unwrap();
        assert_eq!(rendered(&colors.atom), "\x1b[38;5;0m\x1b[47m");
    }

    #[test]
    fn writes_colored_output() {
        let sexp = Sexp::list([Sexp::symbol("p"), Sexp::string("x")]);
        let mut buffer = Ansi::new(Vec::new());
        write(
            &sexp,
            &mut buffer,
            Format::Compact,
            Some(&Colors::default()),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(buffer.into_inner()).unwrap(),
            "\x1b[2m(\x1b[0mp \x1b[35m\"x\"\x1b[0m\x1b[2m)\x1b[0m"
        );
    }

    #[test]
    fn writes_plain_output_without_colors() {
        let sexp = Sexp::list([Sexp::symbol("p"), Sexp::string("x")]);
        let mut buffer = Ansi::new(Vec::new());
        write(&sexp, &mut buffer, Format::Compact, None).unwrap();
        assert_eq!(String::from_utf8(buffer.into_inner()).unwrap(), "(p \"x\")");
    }

    #[test]
    fn token_output_is_plain_even_with_colors_enabled() {
        let sexp = Sexp::list([Sexp::symbol("p"), Sexp::string("x")]);
        let mut buffer = Ansi::new(Vec::new());
        write(&sexp, &mut buffer, Format::Token, Some(&Colors::default())).unwrap();
        assert_eq!(
            String::from_utf8(buffer.into_inner()).unwrap(),
            "(\np\n\"x\"\n)\n"
        );
    }

    #[test]
    fn writes_colored_minified_output() {
        let sexp = Sexp::list([Sexp::symbol("a"), Sexp::string("b")]);
        let mut buffer = Ansi::new(Vec::new());
        write(
            &sexp,
            &mut buffer,
            Format::Minified,
            Some(&Colors::default()),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(buffer.into_inner()).unwrap(),
            "\x1b[2m(\x1b[0ma\x1b[35m\"b\"\x1b[0m\x1b[2m)\x1b[0m"
        );
    }

    #[test]
    fn minified_output_is_plain_without_colors() {
        let sexp = Sexp::list([Sexp::symbol("a"), Sexp::string("b")]);
        let mut buffer = Ansi::new(Vec::new());
        write(&sexp, &mut buffer, Format::Minified, None).unwrap();
        assert_eq!(String::from_utf8(buffer.into_inner()).unwrap(), "(a\"b\")");
    }

    #[test]
    fn writes_colored_pretty_output() {
        let sexp = Sexp::list([Sexp::symbol("p"), Sexp::string("x")]);
        let mut buffer = Ansi::new(Vec::new());
        write(
            &sexp,
            &mut buffer,
            Format::Vertical,
            Some(&Colors::default()),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(buffer.into_inner()).unwrap(),
            "\x1b[2m(\x1b[0mp\n  \x1b[35m\"x\"\x1b[0m\x1b[2m)\x1b[0m"
        );
    }

    #[test]
    fn omits_escape_sequences_for_cleared_styles() {
        let mut colors = Colors::default();
        colors.set("punct:style:none").unwrap();
        let sexp = Sexp::list([Sexp::symbol("p")]);
        let mut buffer = Ansi::new(Vec::new());
        write(&sexp, &mut buffer, Format::Compact, Some(&colors)).unwrap();
        assert_eq!(String::from_utf8(buffer.into_inner()).unwrap(), "(p)");
    }

    #[test]
    fn writes_plain_output_through_stripping_stream() {
        let sexp = Sexp::list([Sexp::symbol("p"), Sexp::string("x")]);
        let mut buffer = Vec::new();
        {
            let mut out = NoColor::new(&mut buffer);
            write(&sexp, &mut out, Format::Compact, Some(&Colors::default())).unwrap();
        }
        assert_eq!(String::from_utf8(buffer).unwrap(), "(p \"x\")");
    }
}
