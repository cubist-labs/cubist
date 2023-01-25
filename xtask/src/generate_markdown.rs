//! A module for generating a Markdown documentation for a [`clap::Command`]. This module is
//! heavily inspired by the `clap_mangen` crate.
use clap::{Arg, ArgAction, Command};
use color_eyre::eyre::Result;

use std::io::Write;

pub fn run(f: &mut impl Write, cmd: &Command) -> Result<()> {
    let empty: String = "".into();
    command_docs(f, &empty, cmd)?;
    Ok(())
}

fn command_docs(f: &mut impl Write, cmd_prefix: &String, cmd: &Command) -> Result<()> {
    // The name of a subcommand including its prefix, e.g., `cubist gen` instead of `gen`.
    let full_cmd_name = format!("{} {}", cmd_prefix, cmd.get_name())
        .trim()
        .to_string();

    synopsis(f, &full_cmd_name, cmd)?;
    subcommands(f, &full_cmd_name, cmd)?;
    options(f, &full_cmd_name, cmd)?;

    writeln!(f, "---")?;

    for sub in cmd.get_subcommands().filter(|s| !s.is_hide_set()) {
        command_docs(f, &full_cmd_name, sub)?;
    }

    Ok(())
}

/// Renders the "Usage" line and the description of the command
fn synopsis(f: &mut impl Write, full_cmd_name: &String, cmd: &Command) -> Result<()> {
    writeln!(f, "## `{}` {{#{}}}", &full_cmd_name, anchor(full_cmd_name))?;
    writeln!(f)?;
    write!(f, "Usage: `{}` ", full_cmd_name)?;

    // Render all the non-hidden arguments of the command
    for opt in cmd.get_arguments().filter(|i| !i.is_hide_set()) {
        let (lhs, rhs) = markers(opt.is_required_set());
        match (opt.get_short(), opt.get_long()) {
            (Some(short), Some(long)) => {
                write!(f, "`{}-{}|--{}{}`", lhs, short, long, rhs)?;
            }
            (Some(short), None) => {
                write!(f, "`{}-{}{}`", lhs, short, rhs)?;
            }
            (None, Some(long)) => {
                write!(f, "`{}--{}{}`", lhs, long, rhs)?;
            }
            (None, None) => continue,
        };

        if matches!(opt.get_action(), ArgAction::Count) {
            write!(f, "`...`")?;
        }
        write!(f, " ")?;
    }

    for arg in cmd.get_positionals() {
        positional(f, arg)?;
    }

    // Render a link to the subcommand list
    if cmd.has_subcommands() {
        let (lhs, rhs) = markers(cmd.is_subcommand_required_set());
        write!(
            f,
            "[`{}{}{}`](#{}-commands)",
            lhs,
            cmd.get_subcommand_value_name()
                .unwrap_or_else(|| subcommand_heading(cmd))
                .to_lowercase(),
            rhs,
            anchor(full_cmd_name)
        )?;
    }
    writeln!(f)?;
    writeln!(f)?;

    // Render the description of the command
    if let Some(about) = cmd.get_about().or_else(|| cmd.get_long_about()) {
        writeln!(f, "{}", about)?;
    }

    writeln!(f)?;
    writeln!(f)?;

    Ok(())
}

// Renders a list of subcommands
fn subcommands(f: &mut impl Write, sub_prefix: &String, cmd: &Command) -> Result<()> {
    let subs = cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set())
        .collect::<Vec<_>>();
    if subs.is_empty() {
        return Ok(());
    }
    writeln!(f, "### Commands {{#{}-commands}}", anchor(sub_prefix))?;
    writeln!(f)?;

    // Render each subcommand with its description
    for sub in subs {
        let sub_name = format!("{} {}", sub_prefix, sub.get_name());
        write!(f, "- [`{}`](#{}): ", sub.get_name(), anchor(&sub_name))?;

        if let Some(about) = sub.get_about().or_else(|| sub.get_long_about()) {
            for line in about.to_string().lines() {
                write!(f, "{}", line)?;
            }
        }
        writeln!(f)?;
    }
    Ok(())
}

/// Renders the options of a command
fn options(f: &mut impl Write, full_cmd_name: &str, cmd: &Command) -> Result<()> {
    let items: Vec<_> = cmd.get_arguments().filter(|i| !i.is_hide_set()).collect();
    if items.is_empty() {
        return Ok(());
    }

    writeln!(f, "### Options {{#{}-options}}", anchor(full_cmd_name))?;
    writeln!(f)?;

    for arg in items.iter() {
        write!(f, "- ")?;

        if arg.is_positional() {
            positional(f, arg)?;
        } else {
            match (arg.get_short(), arg.get_long()) {
                (Some(short), Some(long)) => write!(f, "`-{}, --{}", short, long)?,
                (Some(short), None) => write!(f, "`-{}", short)?,
                (None, Some(long)) => write!(f, "`--{}", long)?,
                (None, None) => (),
            };

            if let Some(value) = &arg.get_value_names() {
                write!(f, " <{}>", value.join(" "))?;
            }
            write!(f, "`")?;
        }

        option_default_values(f, arg)?;
        option_help(f, arg)?;
        option_possible_values(f, arg)?;
        option_environment(f, arg)?;
        writeln!(f)?;
    }

    writeln!(f)?;
    Ok(())
}

/// Renders the default values of an option
fn option_default_values(f: &mut impl Write, opt: &Arg) -> Result<()> {
    if opt.is_hide_default_value_set() || !opt.get_action().takes_values() {
        return Ok(());
    }

    if !opt.get_default_values().is_empty() {
        let values = opt
            .get_default_values()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(",");

        write!(f, " (default: `{}`)", values)?;
    }

    Ok(())
}

/// Renders the help text of an option
fn option_help(f: &mut impl Write, opt: &Arg) -> Result<()> {
    if !opt.is_hide_long_help_set() {
        let long_help = opt.get_long_help();
        if let Some(help) = long_help {
            write!(f, ": {}", help)?;
        }
    }
    if !opt.is_hide_short_help_set() {
        if let Some(help) = opt.get_help() {
            write!(f, ": {}", help)?;
        }
    }

    Ok(())
}

/// Renders a list of possible values for an option
fn option_possible_values(f: &mut impl Write, arg: &Arg) -> Result<()> {
    let possibles = &arg.get_possible_values();
    let possibles: Vec<&clap::builder::PossibleValue> =
        possibles.iter().filter(|pos| !pos.is_hide_set()).collect();

    if possibles.is_empty() || arg.is_hide_possible_values_set() {
        return Ok(());
    }

    writeln!(f)?;
    writeln!(f)?;
    writeln!(f, "  Possible values:")?;

    for value in possibles {
        let val_name = value.get_name();
        match value.get_help() {
            Some(help) => writeln!(f, "  - `{}`: {}", val_name, help)?,
            None => writeln!(f, "{}", val_name)?,
        }
    }
    Ok(())
}

/// Renders a description that indicates that an option can be set using an environment variable
fn option_environment(f: &mut impl Write, opt: &Arg) -> Result<()> {
    if opt.is_hide_env_set() {
        return Ok(());
    }
    if let Some(env) = opt.get_env() {
        writeln!(
            f,
            "May also be specified with the `{}` environment variable.",
            env.to_string_lossy()
        )?;
    }
    Ok(())
}

/// Renders a positional argument in the "Usage" string
fn positional(f: &mut impl Write, arg: &Arg) -> Result<()> {
    let (lhs, rhs) = markers(arg.is_required_set());
    write!(f, "`{}", lhs)?;
    if let Some(value) = arg.get_value_names() {
        write!(f, "{}", value.join(" "))?;
    } else {
        write!(f, "{}", arg.get_id())?;
    }
    write!(f, "{}` ", rhs)?;
    Ok(())
}

/// Renders the name of the subcommand placeholder
fn subcommand_heading(cmd: &Command) -> &str {
    match cmd.get_subcommand_help_heading() {
        Some(title) => title,
        None => "COMMAND",
    }
}

/// Renders markers for the "Usage" string, e.g., `"<"`, `">"` for required arguments
fn markers(required: bool) -> (&'static str, &'static str) {
    if required {
        ("<", ">")
    } else {
        ("[", "]")
    }
}

/// Turns a string into an Markdown anchor (i.e., replaces spaces with dashes)
fn anchor(s: &str) -> String {
    s.replace(' ', "-")
}
