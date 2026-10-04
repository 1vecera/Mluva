//! Preserve the released helper's two-option CLI without an interpreter.
use std::{ffi::OsString, path::PathBuf};

pub const USAGE: &str = "usage: mluva-install-widget [-h] [--check | --stage STAGE]";
pub const HELP: &str = "Install the widget bundled with this Mluva release without a separate
repository.

options:
  -h, --help     show this help message and exit
  --check        Validate without changing the installation
  --stage STAGE  Prepare a widget folder without contacting the desktop";

pub enum Mode {
    Help,
    Stage(PathBuf),
    Install { check: bool },
}

pub fn parse(args: impl Iterator<Item = OsString>) -> Result<Mode, String> {
    let mut args = args.peekable();
    let mut check = false;
    let mut stage = None;
    let mut unknown = Vec::new();
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        if text == "--" {
            unknown.push(text.into_owned());
            unknown.extend(args.map(|value| value.to_string_lossy().into_owned()));
            break;
        }
        let (name, explicit) = text
            .split_once('=')
            .map_or((text.as_ref(), None), |(name, value)| (name, Some(value)));
        let option = if name.starts_with("--") && name.len() > 2 {
            ["--check", "--stage", "--help"]
                .into_iter()
                .find(|full| full.starts_with(name))
        } else if name.starts_with("-h") {
            Some("--help")
        } else {
            None
        };
        match option {
            Some("--help" | "--check") => {
                let label = if option == Some("--help") {
                    "-h/--help"
                } else {
                    "--check"
                };
                let explicit =
                    explicit.or_else(|| text.strip_prefix("-h").filter(|value| !value.is_empty()));
                if let Some(value) = explicit {
                    return Err(format!(
                        "argument {label}: ignored explicit argument {}",
                        quoted(value)
                    ));
                }
                if option == Some("--help") {
                    return Ok(Mode::Help);
                }
                if stage.is_some() {
                    return Err("argument --check: not allowed with argument --stage".into());
                }
                check = true;
            }
            Some("--stage") => {
                let value = if let Some(value) = explicit {
                    OsString::from(value)
                } else {
                    if args
                        .peek()
                        .is_none_or(|value| is_option(&value.to_string_lossy()))
                    {
                        return Err("argument --stage: expected one argument".into());
                    }
                    args.next().expect("a stage argument was checked")
                };
                if check {
                    return Err("argument --stage: not allowed with argument --check".into());
                }
                stage = Some(PathBuf::from(value));
            }
            _ => unknown.push(text.into_owned()),
        }
    }
    if !unknown.is_empty() {
        return Err(format!("unrecognized arguments: {}", unknown.join(" ")));
    }
    Ok(stage.map_or(Mode::Install { check }, Mode::Stage))
}

fn is_option(value: &str) -> bool {
    if value == "-" || !value.starts_with('-') || value.contains(' ') {
        return false;
    }
    let negative = &value[1..];
    // argparse treats plain negative integers and decimals as values.
    let number = !negative.is_empty() && negative.bytes().all(|byte| byte.is_ascii_digit())
        || negative.split_once('.').is_some_and(|(left, right)| {
            !right.is_empty()
                && left
                    .bytes()
                    .chain(right.bytes())
                    .all(|byte| byte.is_ascii_digit())
        });
    !number
}

fn quoted(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = quote.to_string();
    for character in value.chars() {
        match character {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\\' => result.push_str("\\\\"),
            character if character == quote => {
                result.push('\\');
                result.push(character);
            }
            character => result.push(character),
        }
    }
    result.push(quote);
    result
}
