//! Protocol shared by `radeau` and its external `radeau-*` plugins.
//!
//! Completion spec: `radeau completions <shell>` runs
//! `radeau-<name> <name> --radeau-completion-spec`. A plugin that understands the
//! flag prints its clap command tree as JSON ([`to_spec`]); radeau rebuilds it
//! ([`from_spec`]) and attaches it as a subcommand before generating the
//! shell script, so plugin flags complete like built-in ones. A plugin that
//! does not know the flag just exits non-zero and radeau falls back to
//! completing its name only.
//!
//! Interactive prompts: see [`interactive()`], an opt-in helper for plugins
//! that radeau also uses for its own fallback.
//!
//! Plugin `main`: [`run()`] wires all of the above in one call.

pub mod tui;
pub use tui::interactive;

use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, Command, CommandFactory, ValueHint};
use serde_json::{Value, json};

/// Hidden flag radeau passes (after the command-name echo) to ask for the spec.
pub const SPEC_FLAG: &str = "--radeau-completion-spec";

/// Plugin side. Call at the top of `main` with argv *after* stripping the
/// `<name>` echo: if radeau asked for the spec, prints it and returns true.
pub fn answer_spec_request<C: CommandFactory>(args: &[String]) -> bool {
    if args.get(1).map(String::as_str) != Some(SPEC_FLAG) {
        return false;
    }
    println!("{}", to_spec(&C::command()));
    true
}

/// Plugin `main` in one call. radeau invokes a plugin as
/// `radeau-<name> <echo> <rest>` (`radeau-init-<format> <format> <rest>` for init
/// importers): strips that `echo` (same trick cargo-fmt uses for `cargo fmt`;
/// direct `radeau-<name> <rest>` works unchanged), answers a completion-spec
/// request, fills missing args interactively, parses, then runs `body` and
/// prints any error with its cause chain.
pub fn run<C, E>(echo: &str, body: impl FnOnce(C) -> Result<(), E>) -> std::process::ExitCode
where
    C: clap::Parser,
    E: Into<Box<dyn std::error::Error>>,
{
    use std::process::ExitCode;

    let mut args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(echo) {
        args.remove(1);
    }
    if answer_spec_request::<C>(&args) {
        return ExitCode::SUCCESS;
    }
    let Some(args) = interactive::<C>(args) else {
        return ExitCode::FAILURE;
    };

    match body(C::parse_from(args)).map_err(Into::into) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            let mut source = e.source();
            while let Some(cause) = source {
                eprintln!("  Caused by: {cause}");
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}

/// Serialize the parts of a clap command that shell completion uses.
pub fn to_spec(cmd: &Command) -> Value {
    let args: Vec<Value> = cmd
        .get_arguments()
        .filter(|a| !a.is_hide_set())
        .filter(|a| {
            !matches!(
                a.get_action(),
                ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
            )
        })
        .map(|a| {
            json!({
                "id": a.get_id().as_str(),
                "long": a.get_long(),
                "short": a.get_short().map(String::from),
                "help": a.get_help().map(|h| h.to_string()),
                "value_name": a.get_value_names().and_then(|v| v.first()).map(|v| v.to_string()),
                "takes_value": a.get_action().takes_values(),
                "multiple": matches!(a.get_action(), ArgAction::Append),
                "required": a.is_required_set(),
                "values": a.get_possible_values().iter().filter(|p| !p.is_hide_set()).map(|p| p.get_name()).collect::<Vec<_>>(),
                "hint": format!("{:?}", a.get_value_hint()).to_ascii_lowercase(),
            })
        })
        .collect();
    let subcommands: Vec<Value> = cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
        .map(to_spec)
        .collect();
    json!({
        "name": cmd.get_name(),
        "about": cmd.get_about().map(|s| s.to_string()),
        "args": args,
        "subcommands": subcommands,
    })
}

/// radeau side. Rebuild a command named `name` from a plugin's spec.
/// Unknown or malformed fields are skipped rather than failing the whole spec.
pub fn from_spec(name: &str, spec: &Value) -> Command {
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    let flag = |v: &Value, k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);

    let mut cmd = Command::new(name.to_string());
    if let Some(about) = str_of(spec, "about") {
        cmd = cmd.about(about);
    }

    for a in spec
        .get("args")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = str_of(a, "id") else { continue };
        let mut arg = Arg::new(id).action(if flag(a, "multiple") {
            ArgAction::Append
        } else if flag(a, "takes_value") {
            ArgAction::Set
        } else {
            ArgAction::SetTrue
        });
        if let Some(l) = str_of(a, "long") {
            arg = arg.long(l);
        }
        if let Some(s) = str_of(a, "short").and_then(|s| s.chars().next()) {
            arg = arg.short(s);
        }
        if let Some(h) = str_of(a, "help") {
            arg = arg.help(h);
        }
        if let Some(v) = str_of(a, "value_name") {
            arg = arg.value_name(v);
        }
        if let Some(hint) = str_of(a, "hint").and_then(|h| h.parse::<ValueHint>().ok()) {
            arg = arg.value_hint(hint);
        }
        let values: Vec<String> = a
            .get("values")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        if !values.is_empty() {
            arg = arg.value_parser(PossibleValuesParser::new(values));
        }
        cmd = cmd.arg(arg.required(flag(a, "required")));
    }

    for sub in spec
        .get("subcommands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(sub_name) = str_of(sub, "name") {
            cmd = cmd.subcommand(from_spec(&sub_name, sub));
        }
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_round_trips_what_completion_needs() {
        let original = Command::new("radeau-demo")
            .about("Demo plugin")
            .arg(Arg::new("path").value_hint(ValueHint::DirPath))
            .arg(Arg::new("out").long("out").short('o').help("Output"))
            .arg(
                Arg::new("verbose")
                    .long("verbose")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("fmt")
                    .long("fmt")
                    .action(ArgAction::Append)
                    .value_parser(["svg", "png"]),
            )
            .subcommand(Command::new("serve").arg(Arg::new("port").long("port")));

        let spec = to_spec(&original);
        let rebuilt = from_spec("demo", &spec);
        // Must survive a JSON text round-trip, as it does over the pipe.
        let reparsed: Value = serde_json::from_str(&spec.to_string()).unwrap();
        assert_eq!(to_spec(&from_spec("demo", &reparsed)), to_spec(&rebuilt));

        assert_eq!(rebuilt.get_name(), "demo");
        assert_eq!(rebuilt.get_about().unwrap().to_string(), "Demo plugin");
        let arg = |id: &str| rebuilt.get_arguments().find(|a| a.get_id() == id).unwrap();
        assert_eq!(arg("path").get_value_hint(), ValueHint::DirPath);
        assert!(arg("path").is_positional());
        assert_eq!(arg("out").get_short(), Some('o'));
        assert!(!arg("verbose").get_action().takes_values());
        assert_eq!(arg("fmt").get_possible_values().len(), 2);
        assert!(rebuilt.find_subcommand("serve").is_some());

        rebuilt.clone().debug_assert();
    }
}
