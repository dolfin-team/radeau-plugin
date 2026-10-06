//! Interactive TUI for missing (or entirely absent) CLI arguments, shared by
//! radeau and any plugin that opts in.
//!
//! The user is prompted for exactly what clap reports missing. Answers are
//! appended to argv and argv is re-parsed, so everything already typed is
//! kept. Once argv parses, the remaining options of the chosen (sub)command
//! are offered. Prompts come from the clap definitions, never listed here.

use std::error::Error;
use std::io::IsTerminal;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::parser::ValueSource;
use clap::{Arg, ArgAction, Command, CommandFactory};
use cliclack::{input, intro, multiselect, outro, outro_cancel, select};

/// Returns true if stdin is an interactive terminal (not a pipe or redirect).
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
}

/// True for clap errors this module can fix by asking the user.
pub fn is_missing(err: &clap::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::MissingRequiredArgument
            | ErrorKind::MissingSubcommand
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    )
}

/// Plugin side, opt-in. Call in `main` after stripping the `<name>` echo and
/// before parsing. When run with no arguments (or with required ones missing)
/// in a terminal, prompts for them and returns the completed argv; otherwise
/// returns `args` unchanged. `None` means the user cancelled.
///
/// ```ignore
/// let Some(args) = radeau_plugin::interactive::<Cli>(args) else {
///     return ExitCode::FAILURE;
/// };
/// let cli = Cli::parse_from(args);
/// ```
pub fn interactive<C: CommandFactory>(args: Vec<String>) -> Option<Vec<String>> {
    let cmd = C::command();
    let wanted = args.len() <= 1
        || matches!(cmd.clone().try_get_matches_from(&args), Err(e) if is_missing(&e));
    if !wanted || !is_interactive() {
        return Some(args);
    }

    // Invoked through radeau (`radeau viz`) or directly (`radeau-viz`)?
    let name = cmd.get_name().to_string();
    let shown = match (std::env::var_os("RADEAU"), name.strip_prefix("radeau-")) {
        // `radeau-init-<format>` importers run as `radeau init <format>`.
        (Some(_), Some(short)) => format!("radeau {}", short.replacen("init-", "init ", 1)),
        _ => name.clone(),
    };

    intro(format!(" {shown} ")).ok()?;
    match complete(&cmd, args, &shown, &[]) {
        Ok(Outcome::Args(argv)) => Some(argv),
        // Parse error already printed; let clap report it again on parse.
        Ok(Outcome::Failed(argv)) => Some(argv),
        Ok(Outcome::Extra(_)) | Err(_) => None,
    }
}

/// Result of [`complete`].
pub enum Outcome {
    /// argv now parses.
    Args(Vec<String>),
    /// The user picked one of the `extra` top-level choices (its value).
    Extra(String),
    /// Could not complete; the clap error was printed. argv as far as it got.
    Failed(Vec<String>),
}

/// A menu entry that is not a subcommand of `cmd`: (value, label, hint).
pub type ExtraChoice = (String, String, String);

/// An [`ExtraChoice`] scoped to the (sub)command named by the `String`.
/// Under a command with subcommands it joins that command's menu (radeau:
/// plugins under `radeau`). Under a leaf it is offered as an alternative before
/// the leaf's missing arguments are prompted (radeau: importers under `init`).
pub type ScopedChoice = (String, ExtraChoice);

pub type TuiResult<T> = Result<T, Box<dyn Error>>;

/// Prompt until `argv` parses against `cmd`, then offer the remaining
/// options. `shown` is the command prefix used in the final "Running:" line.
/// `Err` means the user cancelled (cliclack already printed it).
pub fn complete(
    cmd: &Command,
    mut argv: Vec<String>,
    shown: &str,
    extra: &[ScopedChoice],
) -> TuiResult<Outcome> {
    let mut asked: Vec<String> = Vec::new();
    let mut offered = false;
    let extras_for = |name: &str| -> Vec<ExtraChoice> {
        extra
            .iter()
            .filter(|(scope, _)| scope == name)
            .map(|(_, c)| c.clone())
            .collect()
    };

    loop {
        let err = match cmd.clone().try_get_matches_from(&argv) {
            Ok(_) => break,
            Err(e) => e,
        };
        let (leaf, insert_at) = leaf_command(cmd, &argv);
        match err.kind() {
            ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                if leaf.has_subcommands() =>
            {
                let top = extras_for(leaf.get_name());
                let choice = pick_subcommand(leaf, &top)?;
                if top.iter().any(|(v, _, _)| *v == choice) {
                    return Ok(Outcome::Extra(choice));
                }
                argv.insert(insert_at, choice);
            }
            ErrorKind::MissingRequiredArgument
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                let alternatives = extras_for(leaf.get_name());
                if !offered && !alternatives.is_empty() {
                    offered = true;
                    if let Some(choice) = pick_alternative(leaf, &alternatives)? {
                        return Ok(Outcome::Extra(choice));
                    }
                }
                let args: Vec<&Arg> = missing_in(leaf, &err)
                    .into_iter()
                    .filter(|a| !asked.contains(&a.get_id().to_string()))
                    .collect();
                if args.is_empty() {
                    // Nothing we can prompt for (or already prompted): give up.
                    return give_up(err, argv);
                }
                for a in args {
                    asked.push(a.get_id().to_string());
                    prompt_arg(a, true, &mut argv)?;
                }
            }
            _ => return give_up(err, argv),
        }
    }

    prompt_optional(cmd, &mut argv)?;

    match cmd.clone().try_get_matches_from(&argv) {
        Ok(_) => {
            outro(format!("Running: {}", shell_line(shown, &argv)))?;
            Ok(Outcome::Args(argv))
        }
        Err(e) => give_up(e, argv),
    }
}

fn give_up(err: clap::Error, argv: Vec<String>) -> TuiResult<Outcome> {
    let _ = outro_cancel("Could not complete the command.");
    err.print()?;
    Ok(Outcome::Failed(argv))
}

/// Deepest subcommand named in argv, and the argv index right after it
/// (where a missing nested subcommand name must be inserted).
fn leaf_command<'a>(root: &'a Command, argv: &[String]) -> (&'a Command, usize) {
    let mut cmd = root;
    let mut at = 1;
    for (i, tok) in argv.iter().enumerate().skip(1) {
        if let Some(sub) = cmd.find_subcommand(tok) {
            cmd = sub;
            at = i + 1;
        }
    }
    (cmd, at)
}

fn pick_subcommand(cmd: &Command, extra: &[ExtraChoice]) -> TuiResult<String> {
    let mut menu = select(format!("{} — what would you like to do?", cmd.get_name()));
    for sub in cmd.get_subcommands().filter(|s| s.get_name() != "help") {
        let about = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
        menu = menu.item(sub.get_name().to_string(), sub.get_name(), about);
    }
    for (value, label, hint) in extra {
        menu = menu.item(value.clone(), label, hint);
    }
    Ok(menu.interact()?)
}

/// Let the user run `leaf` itself (`None`) or one of its `alternatives`.
fn pick_alternative(leaf: &Command, alternatives: &[ExtraChoice]) -> TuiResult<Option<String>> {
    let about = leaf.get_about().map(|s| s.to_string()).unwrap_or_default();
    let mut menu =
        select(format!("{} — how?", leaf.get_name())).item(String::new(), leaf.get_name(), about);
    for (value, label, hint) in alternatives {
        menu = menu.item(value.clone(), label, hint);
    }
    let choice: String = menu.interact()?;
    Ok((!choice.is_empty()).then_some(choice))
}

/// Args of `leaf` that `err` reports missing. clap only gives their rendered
/// form (`<NAME>`, `--range <RANGE>`), so match on that.
fn missing_in<'a>(leaf: &'a Command, err: &clap::Error) -> Vec<&'a Arg> {
    let missing = match err.get(ContextKind::InvalidArg) {
        Some(ContextValue::Strings(v)) => v.clone(),
        Some(ContextValue::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    };
    leaf.get_arguments()
        .filter(|a| missing.iter().any(|m| renders_as(a, m)))
        .collect()
}

fn renders_as(a: &Arg, rendered: &str) -> bool {
    let head = rendered.split_whitespace().next().unwrap_or("");
    if a.is_positional() {
        // Variadic positionals render as `<FILE>...`.
        head.trim_end_matches("...") == format!("<{}>", value_name(a))
    } else {
        a.get_long().is_some_and(|l| head == format!("--{l}"))
            || a.get_short().is_some_and(|s| head == format!("-{s}"))
    }
}

fn value_name(a: &Arg) -> String {
    a.get_value_names()
        .and_then(|v| v.first())
        .map(|v| v.to_string())
        .unwrap_or_else(|| a.get_id().to_string())
}

/// First sentence of an arg's doc comment.
fn help_line(a: &Arg) -> String {
    a.get_help()
        .map(|h| h.to_string())
        .and_then(|h| h.split(". ").next().map(str::to_string))
        .unwrap_or_default()
}

fn display_name(a: &Arg) -> String {
    match (a.get_long(), a.get_short()) {
        (Some(l), _) => format!("--{l}"),
        (None, Some(s)) => format!("-{s}"),
        _ => format!("<{}>", value_name(a)),
    }
}

fn is_meta(a: &Arg) -> bool {
    a.is_hide_set()
        || matches!(
            a.get_action(),
            ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
        )
}

/// Ask for one argument and append it to argv. Flags (`SetTrue`) are only
/// reached once the user has chosen them, so they are appended directly.
fn prompt_arg(a: &Arg, required: bool, argv: &mut Vec<String>) -> TuiResult<()> {
    let label = format!("{} {}", display_name(a), help_line(a));
    let multi = matches!(a.get_action(), ArgAction::Append);
    let choices = a.get_possible_values();

    let values: Vec<String> = if matches!(a.get_action(), ArgAction::SetTrue) {
        argv.push(display_name(a));
        return Ok(());
    } else if !choices.is_empty() {
        let items: Vec<(String, String, String)> = choices
            .iter()
            .filter(|p| !p.is_hide_set())
            .map(|p| {
                let help = p.get_help().map(|h| h.to_string()).unwrap_or_default();
                (p.get_name().to_string(), p.get_name().to_string(), help)
            })
            .collect();
        if multi {
            multiselect(label)
                .items(&items)
                .required(required)
                .interact()?
        } else {
            vec![select(label).items(&items).interact()?]
        }
    } else {
        let mut prompt = input(label).required(required);
        if let Some(d) = a.get_default_values().first() {
            prompt = prompt.default_input(&d.to_string_lossy());
        }
        if multi {
            prompt = prompt.placeholder("comma-separated");
        }
        let v: String = prompt.interact()?;
        if multi {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        } else if v.trim().is_empty() {
            vec![]
        } else {
            vec![v]
        }
    };

    for v in values {
        match (a.get_long(), a.get_short()) {
            (Some(l), _) => argv.push(format!("--{l}={v}")),
            (None, Some(s)) => argv.extend([format!("-{s}"), v]),
            _ => argv.push(v),
        }
    }
    Ok(())
}

/// Offer every remaining option of the chosen subcommand that was not given
/// on the command line.
fn prompt_optional(root: &Command, argv: &mut Vec<String>) -> TuiResult<()> {
    let matches = root.clone().try_get_matches_from(argv.iter())?;
    let mut m = &matches;
    while let Some((_, sub)) = m.subcommand() {
        m = sub;
    }
    let (leaf, _) = leaf_command(root, argv);

    let unset: Vec<&Arg> = leaf
        .get_arguments()
        .filter(|a| {
            !is_meta(a) && m.value_source(a.get_id().as_str()) != Some(ValueSource::CommandLine)
        })
        .collect();
    if unset.is_empty() {
        return Ok(());
    }

    let mut menu =
        multiselect("Other options (space to toggle, enter to continue)").required(false);
    for a in &unset {
        menu = menu.item(a.get_id().to_string(), display_name(a), help_line(a));
    }
    let chosen: Vec<String> = menu.interact()?;

    for a in unset
        .into_iter()
        .filter(|a| chosen.contains(&a.get_id().to_string()))
    {
        prompt_arg(a, false, argv)?;
    }
    Ok(())
}

/// argv as a copy-pasteable shell line, so the user learns the direct form.
fn shell_line(shown: &str, argv: &[String]) -> String {
    let mut parts = vec![shown.to_string()];
    for a in &argv[1..] {
        if a.is_empty() || a.contains(|c: char| c.is_whitespace() || "'\"$`\\<>|&;*?".contains(c)) {
            parts.push(format!("'{}'", a.replace('\'', r"'\''")));
        } else {
            parts.push(a.clone());
        }
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The TUI relies on mapping clap's "missing argument" text back to
    /// `Arg`s; if clap changes its rendering, prompts silently vanish.
    fn missing_ids(cmd: &Command, argv: &[&str]) -> Vec<String> {
        let err = cmd.clone().try_get_matches_from(argv).unwrap_err();
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        let (leaf, _) = leaf_command(cmd, &argv);
        missing_in(leaf, &err)
            .iter()
            .map(|a| a.get_id().to_string())
            .collect()
    }

    #[test]
    fn maps_missing_args_to_prompts() {
        let cmd = Command::new("demo")
            .subcommand_required(true)
            .subcommand(
                Command::new("add")
                    .arg(Arg::new("name").required(true))
                    .arg(Arg::new("domain").long("domain").short('d'))
                    .arg(Arg::new("range").long("range").required(true)),
            )
            .subcommand(
                Command::new("init")
                    .arg(
                        Arg::new("name")
                            .long("name")
                            .required_unless_present("turtle"),
                    )
                    .arg(Arg::new("turtle").long("with-turtle")),
            )
            .subcommand(
                Command::new("import")
                    .arg(Arg::new("files").required(true).action(ArgAction::Append)),
            );
        assert_eq!(
            missing_ids(&cmd, &["demo", "add", "-d", "A"]),
            ["name", "range"]
        );
        assert_eq!(missing_ids(&cmd, &["demo", "init"]), ["name"]);
        assert_eq!(missing_ids(&cmd, &["demo", "import"]), ["files"]);
        assert_eq!(
            shell_line("radeau viz", &["x".into(), "--out=a b".into()]),
            "radeau viz '--out=a b'"
        );
    }
}
