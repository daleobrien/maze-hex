//! Command-line arguments, read by hand. The crate has no dependencies beyond
//! `lib`, so there is no argument parser to lean on; the flags are few enough
//! that a match on each one stays easy to read.

use lib::maze::GenerateOptions;
use lib::tiling::source::source_by_name;
use lib::tiling::Shape;
use lib::track::TrackStyle;

/// What the program does once the arguments are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Generate,
    ListTilings,
    Help,
}

/// Every setting the command line can change.
#[derive(Clone, Debug)]
pub struct Options {
    pub mode: Mode,
    pub tiling: String,
    /// `None` means the caller picks a seed from the clock.
    pub seed: Option<u64>,
    /// In edge lengths, because "columns" means nothing in a mixed tiling.
    pub width: f64,
    pub height: f64,
    pub shape: Shape,
    /// The file name without its extension; `.svg` is added if missing.
    pub output: String,
    pub solution: bool,
    pub boundaries: bool,
    pub numbers: bool,
    pub ports: bool,
    pub style: TrackStyle,
    pub generate: GenerateOptions,
}

/// The tiling drawn when `--tiling` is not given.
const DEFAULT_TILING: &str = "hexagonal";

impl Default for Options {
    fn default() -> Self {
        Options {
            mode: Mode::Generate,
            tiling: DEFAULT_TILING.to_string(),
            seed: None,
            width: 30.0,
            height: 20.0,
            shape: Shape::Rectangle,
            output: "maze".to_string(),
            solution: false,
            boundaries: false,
            numbers: false,
            ports: false,
            style: TrackStyle::default(),
            generate: GenerateOptions::default(),
        }
    }
}

/// Reads the arguments after the program name. An error is a message for the
/// user; the caller adds the hint about `--help`.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            // Help returns at once, so `maze-hex --help` works even when the
            // other flags would fail.
            "-h" | "--help" => {
                return Ok(Options {
                    mode: Mode::Help,
                    ..Options::default()
                })
            }
            "--tiling" => {
                let name = value(arg, &mut rest)?;
                if name == "list" {
                    options.mode = Mode::ListTilings;
                } else if source_by_name(name).is_some() {
                    options.tiling = name.to_string();
                } else {
                    return Err(format!("unknown tiling '{name}'"));
                }
            }
            "--seed" => {
                let text = value(arg, &mut rest)?;
                let seed = text
                    .parse::<u64>()
                    .map_err(|_| format!("--seed needs a whole number, got '{text}'"))?;
                options.seed = Some(seed);
            }
            "-W" | "--width" => options.width = positive(arg, &mut rest)?,
            "-H" | "--height" => options.height = positive(arg, &mut rest)?,
            "--shape" => {
                let name = value(arg, &mut rest)?;
                options.shape = match name {
                    "rectangle" => Shape::Rectangle,
                    "hexagon" => Shape::Hexagon,
                    "circle" => Shape::Circle,
                    _ => {
                        return Err(format!(
                            "unknown shape '{name}' (use rectangle, hexagon or circle)"
                        ))
                    }
                };
            }
            "-o" | "--output" => options.output = value(arg, &mut rest)?.to_string(),
            "-S" | "--solution" => options.solution = true,
            "-B" | "--boundaries" => options.boundaries = true,
            "-N" | "--numbers" => options.numbers = true,
            "-T" | "--ports" => options.ports = true,
            "--track-width" => options.style.width = positive(arg, &mut rest)?,
            "--cap-depth" => options.style.cap_depth = at_least_zero(arg, &mut rest)?,
            "--pass-through" => options.generate.pass_through = fraction(arg, &mut rest)?,
            "--straightness" => options.generate.straightness = fraction(arg, &mut rest)?,
            other if other.starts_with('-') => return Err(format!("unknown flag '{other}'")),
            other => return Err(format!("unexpected argument '{other}'")),
        }
    }
    Ok(options)
}

/// The next argument, which must be there and must not be another flag. A
/// negative number is a value, so `-W -5` reaches the number check instead.
fn value<'a>(flag: &str, rest: &mut std::slice::Iter<'a, String>) -> Result<&'a str, String> {
    match rest.next() {
        Some(v) if !v.starts_with('-') || v.parse::<f64>().is_ok() => Ok(v.as_str()),
        _ => Err(format!("{flag} needs a value")),
    }
}

/// The next argument as a finite number.
fn number(flag: &str, rest: &mut std::slice::Iter<'_, String>) -> Result<f64, String> {
    let text = value(flag, rest)?;
    text.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("{flag} needs a number, got '{text}'"))
}

fn positive(flag: &str, rest: &mut std::slice::Iter<'_, String>) -> Result<f64, String> {
    let n = number(flag, rest)?;
    if n > 0.0 {
        Ok(n)
    } else {
        Err(format!("{flag} must be greater than 0, got {n}"))
    }
}

fn at_least_zero(flag: &str, rest: &mut std::slice::Iter<'_, String>) -> Result<f64, String> {
    let n = number(flag, rest)?;
    if n >= 0.0 {
        Ok(n)
    } else {
        Err(format!("{flag} must be 0 or more, got {n}"))
    }
}

/// A probability or weight: the generator treats anything outside 0 to 1 as
/// the nearest end, so a typo like 7 would be silent without this check.
fn fraction(flag: &str, rest: &mut std::slice::Iter<'_, String>) -> Result<f64, String> {
    let n = number(flag, rest)?;
    if (0.0..=1.0).contains(&n) {
        Ok(n)
    } else {
        Err(format!("{flag} must be between 0 and 1, got {n}"))
    }
}

/// The file name to write: `.svg` is added unless the name already ends in it.
pub fn output_path(name: &str) -> String {
    if name.ends_with(".svg") {
        name.to_string()
    } else {
        format!("{name}.svg")
    }
}

/// The help text. The defaults come from `Options::default` and the lib
/// settings, so the text cannot drift from what the program does.
pub fn usage() -> String {
    let d = Options::default();
    format!(
        "maze-hex: mazes on tilings of equal-edged polygons

usage: maze-hex [options]

  --tiling NAME       tiling to draw on (default {tiling}); `list` prints every name
  --seed N            random seed, so the same flags give the same maze
                      (default: from the clock, and printed to stderr)
  -W, --width N       width in edge lengths (default {width})
  -H, --height N      height in edge lengths (default {height})
  --shape SHAPE       rectangle, hexagon or circle (default rectangle)
  -o, --output NAME   file to write, NAME.svg (default {output})
  -S, --solution      draw the route from entrance to exit in red
  -B, --boundaries    draw faint tile outlines
  -N, --numbers       draw each tile's number
  -T, --ports         draw a dot on every open port
  --track-width F     width of the track (default {track_width})
  --cap-depth F       how far a dead end reaches into its tile (default {cap_depth})
  --pass-through F    how often a tile may take a second track, 0 to 1 (default {pass_through})
  --straightness F    preference for going straight through a tile, 0 to 1 (default {straightness})
  -h, --help          print this help and exit
",
        tiling = d.tiling,
        width = d.width,
        height = d.height,
        output = d.output,
        track_width = d.style.width,
        cap_depth = d.style.cap_depth,
        pass_through = d.generate.pass_through,
        straightness = d.generate.straightness,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Options, String> {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse(&owned)
    }

    #[test]
    fn no_arguments_gives_the_defaults() {
        let o = parse_strs(&[]).expect("defaults parse");
        assert_eq!(o.mode, Mode::Generate);
        assert_eq!(o.tiling, "hexagonal");
        assert_eq!(o.seed, None);
        assert_eq!(o.width, 30.0);
        assert_eq!(o.height, 20.0);
        assert_eq!(o.shape, Shape::Rectangle);
        assert_eq!(o.output, "maze");
        assert!(!o.solution && !o.boundaries && !o.numbers && !o.ports);
        assert_eq!(o.style.width, TrackStyle::default().width);
        assert_eq!(o.style.cap_depth, TrackStyle::default().cap_depth);
        assert_eq!(
            o.generate.pass_through,
            GenerateOptions::default().pass_through
        );
        assert_eq!(
            o.generate.straightness,
            GenerateOptions::default().straightness
        );
    }

    #[test]
    fn every_flag_is_read() {
        let o = parse_strs(&[
            "--tiling",
            "square",
            "--seed",
            "42",
            "--width",
            "12.5",
            "-H",
            "8",
            "--shape",
            "circle",
            "--output",
            "out",
            "-S",
            "-B",
            "-N",
            "-T",
            "--track-width",
            "0.25",
            "--cap-depth",
            "0.5",
            "--pass-through",
            "0.2",
            "--straightness",
            "1",
        ])
        .expect("every flag parses");
        assert_eq!(o.mode, Mode::Generate);
        assert_eq!(o.tiling, "square");
        assert_eq!(o.seed, Some(42));
        assert_eq!(o.width, 12.5);
        assert_eq!(o.height, 8.0);
        assert_eq!(o.shape, Shape::Circle);
        assert_eq!(o.output, "out");
        assert!(o.solution && o.boundaries && o.numbers && o.ports);
        assert_eq!(o.style.width, 0.25);
        assert_eq!(o.style.cap_depth, 0.5);
        assert_eq!(o.generate.pass_through, 0.2);
        assert_eq!(o.generate.straightness, 1.0);
    }

    #[test]
    fn short_forms_match_the_long_ones() {
        let short = parse_strs(&["-W", "5", "-H", "6", "-o", "x", "-S", "-B", "-N", "-T"]).unwrap();
        let long = parse_strs(&[
            "--width",
            "5",
            "--height",
            "6",
            "--output",
            "x",
            "--solution",
            "--boundaries",
            "--numbers",
            "--ports",
        ])
        .unwrap();
        assert_eq!(short.width, long.width);
        assert_eq!(short.height, long.height);
        assert_eq!(short.output, long.output);
        assert!(short.solution && short.boundaries && short.numbers && short.ports);
    }

    #[test]
    fn negative_numbers_are_values_not_flags() {
        // A negative width is a value, so it reaches the positivity check.
        let err = parse_strs(&["-W", "-5"]).unwrap_err();
        assert!(err.contains("greater than 0"), "{err}");
    }

    #[test]
    fn tiling_list_and_help_are_modes() {
        assert_eq!(
            parse_strs(&["--tiling", "list"]).unwrap().mode,
            Mode::ListTilings
        );
        assert_eq!(parse_strs(&["-h"]).unwrap().mode, Mode::Help);
        assert_eq!(parse_strs(&["--help"]).unwrap().mode, Mode::Help);
        // Help wins over flags after it, so it never fails on a bad value.
        assert_eq!(
            parse_strs(&["--help", "--seed", "x"]).unwrap().mode,
            Mode::Help
        );
    }

    #[test]
    fn unknown_flag_is_an_error() {
        let err = parse_strs(&["--nope"]).unwrap_err();
        assert_eq!(err, "unknown flag '--nope'");
    }

    #[test]
    fn bare_word_is_an_error() {
        let err = parse_strs(&["maze"]).unwrap_err();
        assert_eq!(err, "unexpected argument 'maze'");
    }

    #[test]
    fn missing_value_is_an_error() {
        assert_eq!(parse_strs(&["--seed"]).unwrap_err(), "--seed needs a value");
        // The next token is another flag, so the value is missing, not wrong.
        assert_eq!(parse_strs(&["-W", "-S"]).unwrap_err(), "-W needs a value");
        assert_eq!(parse_strs(&["-o", "-B"]).unwrap_err(), "-o needs a value");
    }

    #[test]
    fn unparsable_numbers_are_errors() {
        assert!(parse_strs(&["--seed", "abc"])
            .unwrap_err()
            .contains("whole number"));
        assert!(parse_strs(&["--seed", "-1"])
            .unwrap_err()
            .contains("whole number"));
        assert!(parse_strs(&["-W", "wide"])
            .unwrap_err()
            .contains("needs a number"));
        assert!(parse_strs(&["-W", "inf"])
            .unwrap_err()
            .contains("needs a number"));
        assert!(parse_strs(&["--track-width", "NaN"])
            .unwrap_err()
            .contains("needs a number"));
    }

    #[test]
    fn out_of_range_values_are_errors() {
        assert!(parse_strs(&["-H", "0"])
            .unwrap_err()
            .contains("greater than 0"));
        assert!(parse_strs(&["--track-width", "0"])
            .unwrap_err()
            .contains("greater than 0"));
        assert!(parse_strs(&["--cap-depth", "-0.1"])
            .unwrap_err()
            .contains("0 or more"));
        assert!(parse_strs(&["--pass-through", "1.5"])
            .unwrap_err()
            .contains("between 0 and 1"));
        assert!(parse_strs(&["--straightness", "-0.5"])
            .unwrap_err()
            .contains("between 0 and 1"));
    }

    #[test]
    fn unknown_tiling_and_shape_are_errors() {
        assert_eq!(
            parse_strs(&["--tiling", "penrose"]).unwrap_err(),
            "unknown tiling 'penrose'"
        );
        assert!(parse_strs(&["--shape", "star"])
            .unwrap_err()
            .contains("unknown shape 'star'"));
    }

    #[test]
    fn output_gets_an_svg_extension_once() {
        assert_eq!(output_path("maze"), "maze.svg");
        assert_eq!(output_path("maze.svg"), "maze.svg");
        assert_eq!(output_path("docs/sample"), "docs/sample.svg");
    }

    #[test]
    fn usage_names_every_flag() {
        let text = usage();
        for flag in [
            "--tiling",
            "--seed",
            "-W, --width",
            "-H, --height",
            "--shape",
            "-o, --output",
            "-S, --solution",
            "-B, --boundaries",
            "-N, --numbers",
            "-T, --ports",
            "--track-width",
            "--cap-depth",
            "--pass-through",
            "--straightness",
            "-h, --help",
        ] {
            assert!(text.contains(flag), "usage is missing {flag}");
        }
        assert!(text.contains("default hexagonal"));
    }
}
