//! `plate` — Desenhar Chapa. Draws a flat plate (any closed outline, or a convenience
//! width×height rectangle) with optional holes, and tags it as a `PartKind::Plate` so `fabdim`
//! can dimension it automatically later.

use cadcraft_engine::cmd::{CommandSpec, has_doc};
use cadcraft_engine::{Result, Session};
use cadcraft_geom::Vec2;
use serde_json::Value;

use super::shared::build_part;
use crate::jsonutil::{bad, point_or, point_value, positive_req, str_or};
use crate::part::{PartData, PartKind};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("plate", "Desenhar Chapa", run_plate)
            .enabled(has_doc)
            .params("{outline?: [[x,y]...] (>=3 pts) | width & height & at?, thickness, material?, holes?: [{center:[x,y], diameter}]} -> {handle, kind}"),
    ]
}

fn outline_points(cmd: &str, p: &Value) -> Result<Vec<Vec2>> {
    if let Some(arr) = p.get("outline").and_then(Value::as_array) {
        let pts: Vec<Vec2> = arr.iter().filter_map(point_value).collect();
        if pts.len() < 3 || pts.len() != arr.len() {
            return Err(bad(cmd, "`outline` needs at least 3 valid [x, y] points"));
        }
        return Ok(pts);
    }
    let width = positive_req(cmd, p, "width")?;
    let height = positive_req(cmd, p, "height")?;
    let at = point_or(p, "at", Vec2::ZERO);
    Ok(vec![
        at,
        Vec2::new(at.x + width, at.y),
        Vec2::new(at.x + width, at.y + height),
        Vec2::new(at.x, at.y + height),
    ])
}

fn run_plate(s: &mut Session, p: &Value) -> Result<Value> {
    let thickness = positive_req("plate", p, "thickness")?;
    let material = str_or(p, "material", "").to_string();
    let outline = outline_points("plate", p)?;
    build_part(
        s,
        "plate",
        PartData::new(PartKind::Plate { thickness }, material),
        &outline,
        true,
        p,
    )
}

#[cfg(test)]
mod tests {
    use cadcraft_engine::Session;
    use serde_json::json;

    fn session_with_sample() -> Session {
        let mut s = Session::new();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "test", None);
        s
    }

    // `plate` lives in fab-engine's own registry, not cadcraft's — `Session::execute` alone
    // can't find it, so tests go through `dispatch::execute`, exactly like fabcraft's CLI/app do.
    use crate::dispatch::execute as fab_execute;

    #[test]
    fn draws_a_rectangular_plate_with_holes() {
        let mut s = session_with_sample();
        let r = fab_execute(
            &mut s,
            "plate",
            &json!({
                "width": 100.0, "height": 100.0, "thickness": 5.0, "material": "A36",
                "holes": [{"center": [20.0, 20.0], "diameter": 10.0}, {"center": [80.0, 80.0], "diameter": 10.0}],
            }),
        )
        .expect("plate command should succeed on valid input");
        assert_eq!(r["kind"], "PLATE");
        assert!(r["handle"].is_string());
    }

    #[test]
    fn rejects_non_positive_thickness() {
        let mut s = session_with_sample();
        let err = fab_execute(
            &mut s,
            "plate",
            &json!({"width": 10.0, "height": 10.0, "thickness": 0.0}),
        )
        .unwrap_err();
        assert!(err.to_string().contains("thickness"));
    }

    #[test]
    fn rejects_a_degenerate_outline() {
        let mut s = session_with_sample();
        let err = fab_execute(
            &mut s,
            "plate",
            &json!({"outline": [[0.0, 0.0], [1.0, 1.0]], "thickness": 5.0}),
        )
        .unwrap_err();
        assert!(err.to_string().contains("outline"));
    }

    #[test]
    fn hostile_hole_params_never_panic() {
        let mut s = session_with_sample();
        // NaN, negative and missing fields: every one must be a clean error, not a panic.
        for holes in [
            json!([{"center": [f64::NAN, 0.0], "diameter": 5.0}]),
            json!([{"center": [0.0, 0.0], "diameter": -5.0}]),
            json!([{"center": [0.0, 0.0]}]),
        ] {
            let err = fab_execute(
                &mut s,
                "plate",
                &json!({"width": 10.0, "height": 10.0, "thickness": 5.0, "holes": holes}),
            )
            .unwrap_err();
            assert!(!err.to_string().is_empty());
        }
    }
}
