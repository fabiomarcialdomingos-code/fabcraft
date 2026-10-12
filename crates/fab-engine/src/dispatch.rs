//! Merges fabcraft's own commands with cadcraft's command set. cadcraft-engine's
//! `Session::execute`/`cmdline` only search cadcraft's own sealed, private static registry (it
//! has no extension point for a host app to add commands) — so this is the single entry point
//! fabcraft's app layer (CLI, and later UI/MCP) calls instead, routing each id to whichever
//! registry defines it. fabcraft's own ids are fabrication terms (`plate`, `fabdim`, ...), never
//! AutoCAD command names, so the two registries can't collide.

use cadcraft_engine::{EngineError, Result, Session};
use serde_json::Value;

use crate::commands;

/// Run a command by id, whether it's one of fabcraft's own or one of cadcraft's.
pub fn execute(session: &mut Session, id: &str, params: &Value) -> Result<Value> {
    let lower = id.to_ascii_lowercase();
    if let Some(cmd) = commands::find_command(&lower) {
        (cmd.enabled)(session).map_err(|m| EngineError::Disabled(cmd.id.into(), m))?;
        // A fab command's own `run` composes several native cadcraft calls (pline, attdef,
        // block...), each already guarded and undo-recorded by cadcraft's own `Session::execute`.
        // This outer guard only has to catch a bug in fab-engine's own orchestration code, since
        // cadcraft-engine's internal undo/assoc bookkeeping (`cmd::groups::after_command` and
        // friends) is private to that crate and out of reach from here — an honest limitation:
        // a fab command leaves one undo step per primitive it called, not a single atomic one.
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (cmd.run)(session, params)));
        return match result {
            Ok(r) => r,
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".into());
                Err(EngineError::Internal(cmd.id.into(), msg))
            }
        };
    }
    session.execute(&lower, params)
}

/// Whether `id` names one of fabcraft's own commands (as opposed to one of cadcraft's).
pub fn is_fab_command(id: &str) -> bool {
    commands::find_command(&id.to_ascii_lowercase()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_engine::Session;
    use serde_json::json;

    #[test]
    fn routes_a_fab_command_and_a_cadcraft_command_both() {
        let mut s = Session::new();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "test", None);
        assert!(
            execute(
                &mut s,
                "plate",
                &json!({"width": 10.0, "height": 10.0, "thickness": 2.0})
            )
            .is_ok()
        );
        // "line" is a native cadcraft command fabcraft never redefines — falls through untouched.
        assert!(
            execute(
                &mut s,
                "line",
                &json!({"points": [[0.0, 0.0], [10.0, 10.0]]})
            )
            .is_ok()
        );
    }

    #[test]
    fn unknown_command_is_a_clean_error() {
        let mut s = Session::new();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "test", None);
        assert!(execute(&mut s, "not-a-real-command", &json!({})).is_err());
    }
}
