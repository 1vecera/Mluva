//! The released bridge's deliberately small command line, including option abbreviations.
pub const USAGE: &str = "usage: mluva-shell [-h] [--overlay]\n                   {watch,record,global-record,cancel,latest,status}\n";
const ACTIONS: &[&str] = &[
    "watch",
    "record",
    "global-record",
    "cancel",
    "latest",
    "status",
];

pub struct Arguments {
    pub action: String,
    pub overlay: bool,
}

fn quoted(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let escaped = value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace(quote, &format!("\\{quote}"));
    format!("{quote}{escaped}{quote}")
}

pub fn parse(arguments: impl Iterator<Item = String>) -> Result<Option<Arguments>, String> {
    let mut action = None;
    let mut overlay = false;
    let mut extras = Vec::new();
    let mut options = true;
    for argument in arguments {
        if options && argument == "--" {
            options = false;
            continue;
        }
        let negative_number = argument.starts_with('-')
            && argument[1..].chars().any(|c| c.is_ascii_digit())
            && argument[1..]
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.')
            && argument.parse::<f64>().is_ok();
        if options && argument.starts_with('-') && argument != "-" && !negative_number {
            let (option, explicit) = argument
                .split_once('=')
                .map_or((argument.as_str(), None), |(a, b)| (a, Some(b)));
            let help = option == "-h" || (option.starts_with("--") && "--help".starts_with(option));
            let preview = option.starts_with("--") && "--overlay".starts_with(option);
            if help || preview {
                if let Some(value) = explicit {
                    return Err(format!(
                        "argument {}: ignored explicit argument {}",
                        if help { "-h/--help" } else { "--overlay" },
                        quoted(value)
                    ));
                }
                if help {
                    print!(
                        "{USAGE}\nDesktop status bridge with an opt-in volatile preview and existing session-bus\nactions.\n\npositional arguments:\n  {{watch,record,global-record,cancel,latest,status}}\n\noptions:\n  -h, --help            show this help message and exit\n  --overlay             Include a volatile preview for the floating widget\n"
                    );
                    return Ok(None);
                }
                overlay = true;
            } else {
                extras.push(argument);
            }
        } else if action.is_none() {
            if !ACTIONS.contains(&argument.as_str()) {
                return Err(format!(
                    "argument action: invalid choice: {} (choose from {})",
                    quoted(&argument),
                    ACTIONS
                        .iter()
                        .map(|a| quoted(a))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            action = Some(argument);
        } else {
            extras.push(argument);
        }
    }
    let action = action.ok_or("the following arguments are required: action")?;
    if !extras.is_empty() {
        return Err(format!("unrecognized arguments: {}", extras.join(" ")));
    }
    Ok(Some(Arguments { action, overlay }))
}
