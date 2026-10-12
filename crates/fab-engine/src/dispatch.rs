//! Merges fabcraft's own commands with cadcraft's command set. cadcraft-engine's
//! `Session::execute`/`cmdline` only search cadcraft's own sealed, private static registry (it
//! has no extension point for a host app to add commands) — so this is the single entry point
//! fabcraft's app layer (CLI, GUI, and later MCP) calls instead, routing each id to whichever
//! registry defines it. fabcraft's own ids are fabrication terms (`plate`, `fabdim`, ...), never
//! AutoCAD command names, so the two registries can't collide.
//!
//! **Atomic undo.** A fab command's `run` composes several native cadcraft calls (`pline`,
//! `attdef`, `block`, ...), each already undo-recorded by cadcraft's own `Session::execute` — so
//! a naive dispatch would leave one undo step per primitive, not one for the whole fab command.
//! `DocState::undo`/`redo` and `DocState::push_undo` turned out to be fully public (not the sealed
//! internals they first looked like), so this collapses the N steps the sub-calls pushed into the
//! single one a fab command should be: snapshot the document before running it, let the sub-calls
//! push their own entries as normal, then on success replace those N new entries with one labelled
//! snapshot of the pre-command state. `push_undo`'s own redo-clearing already ran as part of the
//! sub-calls, so nothing else needs doing there.

use cadcraft_engine::{EngineError, Result, Session, Snapshot};
use serde_json::Value;

use crate::commands;

/// Run a command by id, whether it's one of fabcraft's own or one of cadcraft's.
pub fn execute(session: &mut Session, id: &str, params: &Value) -> Result<Value> {
    let lower = id.to_ascii_lowercase();
    let Some(cmd) = commands::find_command(&lower) else {
        return session.execute(&lower, params);
    };
    (cmd.enabled)(session).map_err(|m| EngineError::Disabled(cmd.id.into(), m))?;

    // What to restore the single merged undo step to, if the command succeeds and actually
    // changed anything: the document/selection exactly as they were right before it ran.
    let before = session
        .state()
        .ok()
        .map(|s| (s.doc.clone(), s.selection.clone(), s.undo.len()));

    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (cmd.run)(session, params)));
    let result = match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".into());
            return Err(EngineError::Internal(cmd.id.into(), msg));
        }
    };

    if result.is_ok()
        && cmd.undoable
        && let Some((before_doc, before_selection, before_len)) = before
        && let Ok(st) = session.state_mut()
        && st.undo.len() > before_len
    {
        // The sub-calls pushed `st.undo.len() - before_len` individual steps; replace all of
        // them with one, labelled with the fab command's own name, restoring to the state from
        // before any of them ran.
        st.undo.truncate(before_len);
        st.undo.push(Snapshot {
            label: cmd.label.to_string(),
            doc: before_doc,
            selection: before_selection,
        });
    }
    result
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

    fn session_with_sample() -> Session {
        let mut s = Session::new();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "test", None);
        s
    }

    /// The point of the merge: a `plate` call draws an outline, holes and attribute defs (several
    /// native commands internally) and converts them to a block — many native undo steps — but
    /// one Ctrl+Z-equivalent `Session::undo()` must take the drawing back to exactly how it was
    /// before `plate` ran, in a single step, not partially through the block's own construction.
    #[test]
    fn a_fab_command_undoes_in_a_single_step() {
        let mut s = session_with_sample();
        let before_entities = s.doc().expect("doc").model.len();
        let before_undo_len = s.state().expect("state").undo.len();

        execute(&mut s, "plate", &json!({"width": 50.0, "height": 30.0, "thickness": 2.0, "holes": [{"center": [10.0, 10.0], "diameter": 5.0}]})).expect("plate should succeed");

        let after_undo_len = s.state().expect("state").undo.len();
        assert_eq!(
            after_undo_len,
            before_undo_len + 1,
            "exactly one new undo step, however many primitives `plate` composed internally"
        );

        let label = s
            .undo()
            .expect("undo should succeed")
            .expect("there should be a step to undo");
        assert_eq!(label, "Desenhar Chapa");
        assert_eq!(
            s.doc().expect("doc").model.len(),
            before_entities,
            "undoing once must fully restore the pre-plate drawing, not stop partway through"
        );
    }

    #[test]
    fn a_failed_fab_command_leaves_no_partial_undo_merge() {
        let mut s = session_with_sample();
        let before_undo_len = s.state().expect("state").undo.len();
        // Thickness <= 0 is refused before any drawing primitive runs, so nothing to merge.
        assert!(
            execute(
                &mut s,
                "plate",
                &json!({"width": 10.0, "height": 10.0, "thickness": 0.0})
            )
            .is_err()
        );
        assert_eq!(s.state().expect("state").undo.len(), before_undo_len);
    }
}
