//! `fabcraft-cli`: headless fabcraft, mirroring cadcraft-cli's own shape.
//!
//! ```text
//! fabcraft-cli run [--sample] [--cmd 'id {json}']... [--save OUT.dxf]
//! fabcraft-cli commands [FILTER]
//! ```
//!
//! fabcraft is CLI-first in this build: the native GUI (reusing `cadcraft-ui-egui`) is explicitly
//! the next step, not yet wired up (see the repo README). Every fab command is reachable here via
//! `--cmd`, exactly as the MCP/control-channel path reaches cadcraft's own commands — so this is
//! enough to draw Parts and dimension them end to end, script it, and inspect the result.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![forbid(unsafe_code)]

use std::process::ExitCode;

use cadcraft_engine::Session;
use serde_json::Value;

const USAGE: &str = "usage:
  fabcraft-cli run [--sample] [--cmd 'id {json}']... [--save OUT.dxf]
  fabcraft-cli commands [FILTER]
  fabcraft-cli --version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fabcraft-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("fabcraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("commands") => cmd_commands(args.get(1).map(String::as_str)),
        Some("run") => cmd_run(&args[1..]),
        _ => Err(USAGE.to_string()),
    }
}

fn cmd_commands(filter: Option<&str>) -> Result<(), String> {
    let mut rows: Vec<(&str, &str, bool)> = cadcraft_engine::command_specs()
        .iter()
        .map(|c| (c.id, c.label, false))
        .chain(
            fab_engine::commands::command_specs()
                .iter()
                .map(|c| (c.id, c.label, true)),
        )
        .collect();
    rows.sort_by_key(|(id, _, _)| *id);
    for (id, label, is_fab) in rows {
        if filter.is_some_and(|f| {
            !id.contains(f) && !label.to_ascii_lowercase().contains(&f.to_ascii_lowercase())
        }) {
            continue;
        }
        println!("{id}\t{label}{}", if is_fab { "\t(fab)" } else { "" });
    }
    Ok(())
}

fn cmd_run(args: &[String]) -> Result<(), String> {
    let mut session = Session::new();
    let mut save: Option<String> = None;
    let mut sample = false;
    let mut cmds: Vec<(String, Value)> = Vec::new();

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sample" => sample = true,
            "--save" => save = Some(it.next().ok_or("--save needs a path")?.clone()),
            "--cmd" => {
                let raw = it.next().ok_or("--cmd needs 'id {json}'")?;
                let (id, json_part) = raw.split_once(' ').unwrap_or((raw.as_str(), "{}"));
                let params: Value = serde_json::from_str(json_part)
                    .map_err(|e| format!("--cmd '{raw}': invalid JSON params: {e}"))?;
                cmds.push((id.to_string(), params));
            }
            other => return Err(format!("unrecognized argument: {other}\n\n{USAGE}")),
        }
    }

    if sample {
        session.open_drawing(cadcraft_engine::sample::default_sample(), "Bracket", None);
    } else {
        return Err(format!(
            "fabcraft-cli run needs --sample for now (opening a file is not wired up yet)\n\n{USAGE}"
        ));
    }

    for (id, params) in &cmds {
        let result = fab_engine::dispatch::execute(&mut session, id, params)
            .map_err(|e| format!("`{id}`: {e}"))?;
        println!("{id} -> {result}");
    }

    if let Some(path) = save {
        let doc = session.doc().map_err(|e| e.to_string())?;
        let bytes = cadcraft_io::write(doc, &path).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| format!("writing {path}: {e}"))?;
    }
    Ok(())
}
