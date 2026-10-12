//! `fabclassify` — the "single question" path (design spec §3): turns already-drawn freehand
//! geometry (plain `line`/`pline`/`circle` entities a smart tool never touched) into a Part, by
//! filling exactly the same fields a smart tool fills in automatically. Freehand drawing itself
//! never changes — this only runs when the draftsman explicitly asks for it, typically because
//! `fabdim` just refused the same geometry for having no Part data yet.

use cadcraft_doc::entity_bounds;
use cadcraft_engine::cmd::{CommandSpec, has_doc};
use cadcraft_engine::{Result, Session};
use cadcraft_geom::Bounds2;
use serde_json::{Value, json};

use crate::jsonutil::{bad, handles_req, str_or};
use crate::part::{PartData, PartKind};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("fabclassify", "Classificar Peça", run_classify)
            .enabled(has_doc)
            .params(
                "{handles: [hex,...], kind: PLATE|PROFILE|TUBE|BENT, thickness? (required for PLATE/BENT), catalogRef? (PROFILE/TUBE), bendRadius?/kFactor? (BENT, default thickness/0.33), material?} -> {handle, kind}",
            ),
    ]
}

fn run_classify(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "fabclassify";
    let handles = handles_req(cmd, p, "handles")?;
    let kind_tag = p
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| bad(cmd, "`kind` is required: PLATE, PROFILE, TUBE or BENT"))?
        .to_ascii_uppercase();
    let material = str_or(p, "material", "").to_string();
    let thickness = p
        .get("thickness")
        .and_then(Value::as_f64)
        .filter(|t| t.is_finite() && *t > 0.0)
        .unwrap_or(0.0);
    if matches!(kind_tag.as_str(), "PLATE" | "BENT") && thickness <= 0.0 {
        return Err(bad(
            cmd,
            "`thickness` is required (and positive) to classify a PLATE or BENT part",
        ));
    }
    let catalog_ref = p
        .get("catalogRef")
        .and_then(Value::as_str)
        .map(str::to_string);
    let bend_radius = p
        .get("bendRadius")
        .and_then(Value::as_f64)
        .filter(|r| r.is_finite() && *r >= 0.0)
        .unwrap_or(thickness);
    let k_factor = p
        .get("kFactor")
        .and_then(Value::as_f64)
        .filter(|k| k.is_finite() && *k >= 0.0)
        .unwrap_or(0.33);

    let kind = PartKind::from_tag(&kind_tag, thickness, catalog_ref, bend_radius, k_factor)
        .ok_or_else(|| {
            bad(
                cmd,
                format!("`kind` \"{kind_tag}\" is not PLATE, PROFILE, TUBE or BENT"),
            )
        })?;
    let part = PartData::new(kind, material);

    let origin = {
        let doc = s.doc()?;
        let mut bounds = Bounds2::EMPTY;
        for h in &handles {
            let e = doc
                .entity(*h)
                .ok_or_else(|| bad(cmd, format!("no object with handle {}", h.hex())))?;
            bounds = bounds.union(&entity_bounds(doc, e, 0));
        }
        if bounds.is_empty() {
            return Err(bad(cmd, "the given handles have no measurable geometry"));
        }
        bounds.min
    };

    let mut all_handles: Vec<String> = handles.iter().map(|h| h.hex()).collect();
    for (tag, value) in part.attdef_values() {
        let r = s.execute("attdef", &json!({"tag": tag, "default": value, "at": [origin.x, origin.y], "height": 0.01, "invisible": true}))?;
        let h = r
            .get("handle")
            .and_then(Value::as_str)
            .ok_or_else(|| bad(cmd, "expected a `handle` back from attdef"))?;
        all_handles.push(h.to_string());
    }

    let block_name = format!("FABPART_{}", handles[0].hex());
    let result = s.execute("block", &json!({"name": block_name, "base": [origin.x, origin.y], "handles": all_handles, "keep": "convert"}))?;
    let insert_handle = result.get("insert").and_then(Value::as_str).ok_or_else(|| bad(cmd, "block conversion did not produce an insert — were the handles valid, undeleted objects?"))?;
    Ok(json!({"handle": insert_handle, "kind": part.kind.tag_value()}))
}

#[cfg(test)]
mod tests {
    use crate::dispatch::execute as fab_execute;
    use cadcraft_engine::Session;
    use serde_json::json;

    fn session_with_sample() -> Session {
        let mut s = Session::new();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "test", None);
        s
    }

    #[test]
    fn classifies_a_freehand_rectangle_as_a_plate_and_fabdim_accepts_it() {
        let mut s = session_with_sample();
        let pline = s.execute("pline", &json!({"vertices": [[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]], "closed": true})).expect("pline should draw");
        let handle = pline["handle"].as_str().expect("handle").to_string();

        let r = fab_execute(
            &mut s,
            "fabclassify",
            &json!({"handles": [handle], "kind": "plate", "thickness": 3.0, "material": "A36"}),
        )
        .expect("classifying freehand geometry must succeed");
        assert_eq!(r["kind"], "PLATE");

        // Proof it's a real Part now: fabdim accepts it exactly like a smart-tool plate.
        let dim = fab_execute(&mut s, "fabdim", &json!({"handle": r["handle"]}))
            .expect("fabdim must now accept the classified geometry");
        assert!(dim["sheet"].is_string());
    }

    #[test]
    fn classifies_freehand_geometry_plus_holes_together() {
        let mut s = session_with_sample();
        let pline = s.execute("pline", &json!({"vertices": [[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]], "closed": true})).expect("pline should draw");
        let circle = s
            .execute("circle", &json!({"center": [30.0, 20.0], "diameter": 8.0}))
            .expect("circle should draw");
        let handles = vec![
            pline["handle"].as_str().expect("handle").to_string(),
            circle["handle"].as_str().expect("handle").to_string(),
        ];

        let r = fab_execute(
            &mut s,
            "fabclassify",
            &json!({"handles": handles, "kind": "PLATE", "thickness": 2.0}),
        )
        .expect("classifying geometry + a hole must succeed");
        let dim = fab_execute(&mut s, "fabdim", &json!({"handle": r["handle"]}))
            .expect("fabdim must see the hole too");
        assert!(dim["holeTable"].is_string());
    }

    #[test]
    fn rejects_plate_without_a_thickness() {
        let mut s = session_with_sample();
        let pline = s
            .execute(
                "pline",
                &json!({"vertices": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], "closed": true}),
            )
            .expect("pline should draw");
        let handle = pline["handle"].as_str().expect("handle").to_string();
        let err = fab_execute(
            &mut s,
            "fabclassify",
            &json!({"handles": [handle], "kind": "PLATE"}),
        )
        .unwrap_err();
        assert!(err.to_string().contains("thickness"));
    }

    #[test]
    fn rejects_an_unknown_kind_and_hostile_handles_without_panicking() {
        let mut s = session_with_sample();
        let pline = s
            .execute(
                "pline",
                &json!({"vertices": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], "closed": true}),
            )
            .expect("pline should draw");
        let handle = pline["handle"].as_str().expect("handle").to_string();

        assert!(
            fab_execute(
                &mut s,
                "fabclassify",
                &json!({"handles": [handle], "kind": "NOT_A_KIND", "thickness": 1.0})
            )
            .is_err()
        );
        assert!(
            fab_execute(
                &mut s,
                "fabclassify",
                &json!({"handles": ["FFFFFF"], "kind": "PLATE", "thickness": 1.0})
            )
            .is_err()
        );
        assert!(
            fab_execute(
                &mut s,
                "fabclassify",
                &json!({"handles": [], "kind": "PLATE", "thickness": 1.0})
            )
            .is_err()
        );
        assert!(
            fab_execute(
                &mut s,
                "fabclassify",
                &json!({"handles": ["not-hex"], "kind": "PLATE", "thickness": 1.0})
            )
            .is_err()
        );
    }
}
