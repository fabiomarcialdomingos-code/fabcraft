//! `fabdim` — Cotar Desenho: the single command that turns a Part into a fully dimensioned
//! fabrication drawing (design spec §5). v1 implements the Plate template end to end: overall
//! dimensions, a hole table, a material/thickness callout, and a sheet chosen by
//! `layout_fit::choose_sheet_and_scale`. Profile/Tube/BentProfile templates are not implemented
//! yet in this build (see `not_yet` below) — their smart drawing commands and, for BentProfile,
//! the flat-pattern math already exist; only the automatic dimension *placement* for those kinds
//! is still open, and it fails loudly rather than guessing.

use cadcraft_doc::{Block, EntityKind, Handle};
use cadcraft_engine::cmd::{CommandSpec, has_doc};
use cadcraft_engine::{EngineError, Result, Session};
use cadcraft_geom::{Bounds2, Vec2};
use serde_json::{Value, json};

use crate::jsonutil::bad;
use crate::layout_fit::choose_sheet_and_scale;
use crate::part::{PartData, PartKind};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("fabdim", "Cotar Desenho", run_fabdim)
            .enabled(has_doc)
            .params("{handle (a Part's block reference, from plate/profile/tube/bent-profile)} -> {dimensions: [handle...], holeTable: handle?, layout: name, sheet, scale}"),
    ]
}

fn handle_param(p: &Value) -> Result<Handle> {
    p.get("handle")
        .and_then(Value::as_str)
        .and_then(Handle::parse_hex)
        .ok_or_else(|| {
            bad(
                "fabdim",
                "`handle` must be the hex handle of a Part's block reference",
            )
        })
}

/// The outline's bounding box and every hole's (center, diameter), all in world coordinates.
/// fabcraft's own smart commands always create a Part at `base == insert.insert` (scale 1,
/// rotation 0), so a plain translation by `insert.insert - block.base` reconstructs world
/// coordinates correctly even if the part was moved afterwards (MOVE only changes `insert.insert`).
/// A part later scaled or rotated by a general edit is a known v1 limitation, not handled here.
fn part_geometry(block: &Block, world_offset: Vec2) -> (Bounds2, Vec<(Vec2, f64)>) {
    let mut outline_bounds = Bounds2::EMPTY;
    let mut holes = Vec::new();
    for e in block.entities.iter() {
        match &e.kind {
            EntityKind::LwPolyline(pl) => {
                for v in &pl.vertices {
                    outline_bounds.add(v.p + world_offset);
                }
            }
            EntityKind::Circle(c) => {
                holes.push((c.center.xy() + world_offset, c.radius * 2.0));
            }
            _ => {}
        }
    }
    (outline_bounds, holes)
}

fn run_fabdim(s: &mut Session, p: &Value) -> Result<Value> {
    let handle = handle_param(p)?;

    let (part, block_name, world_offset) = {
        let doc = s.doc()?;
        let entity = doc
            .entity(handle)
            .ok_or_else(|| bad("fabdim", "no object with that handle"))?;
        let insert = match &entity.kind {
            EntityKind::Insert(ins) => ins.clone(),
            _ => {
                return Err(bad(
                    "fabdim",
                    "`handle` must be a block reference (a Part), not some other kind of object",
                ));
            }
        };
        let part = PartData::from_insert(&insert)
            .ok_or_else(|| bad("fabdim", "this object has no fabcraft Part data — draw it with plate/profile/tube/bent-profile, or classify it first"))?;
        let block = doc
            .blocks
            .get(&insert.block)
            .ok_or_else(|| bad("fabdim", "the Part's block definition is missing"))?;
        let offset = insert.insert.xy() - block.base.xy();
        (part, insert.block.clone(), offset)
    };

    match part.kind {
        PartKind::Plate { thickness } => {
            dim_plate(s, &block_name, world_offset, thickness, &part.material)
        }
        PartKind::Profile { .. } => not_yet("fabdim", "Profile"),
        PartKind::Tube { .. } => not_yet("fabdim", "Tube"),
        PartKind::BentProfile { .. } => {
            not_yet("fabdim", "BentProfile (flat-pattern dimensioning)")
        }
    }
}

fn not_yet(cmd: &str, kind: &str) -> Result<Value> {
    Err(EngineError::Other(format!(
        "`{cmd}`: automatic dimensioning for {kind} parts is not available yet in this build — only Plate is implemented so far"
    )))
}

fn dim_plate(
    s: &mut Session,
    block_name: &str,
    world_offset: Vec2,
    thickness: f64,
    material: &str,
) -> Result<Value> {
    let (bounds, holes) = {
        let doc = s.doc()?;
        let block = doc
            .blocks
            .get(block_name)
            .ok_or_else(|| bad("fabdim", "the Part's block definition is missing"))?;
        part_geometry(block, world_offset)
    };
    if bounds.is_empty() {
        return Err(bad(
            "fabdim",
            "this Plate has no outline geometry to dimension",
        ));
    }

    let margin = (bounds.width().max(bounds.height()) * 0.15).max(10.0);
    let mut dim_handles = Vec::new();

    // Overall width, below the part.
    let width_at = Vec2::new(bounds.center().x, bounds.min.y - margin);
    let r = s.execute("dimlinear", &json!({"p1": [bounds.min.x, bounds.min.y], "p2": [bounds.max.x, bounds.min.y], "at": [width_at.x, width_at.y]}))?;
    dim_handles.push(r["handle"].clone());

    // Overall height, left of the part.
    let height_at = Vec2::new(bounds.min.x - margin, bounds.center().y);
    let r = s.execute("dimlinear", &json!({"p1": [bounds.min.x, bounds.min.y], "p2": [bounds.min.x, bounds.max.y], "at": [height_at.x, height_at.y]}))?;
    dim_handles.push(r["handle"].clone());

    // Thickness/material callout, above the part.
    let callout = if material.is_empty() {
        format!("e = {thickness}")
    } else {
        format!("e = {thickness}  —  {material}")
    };
    let callout_height = (bounds.width().max(bounds.height()) * 0.03).max(2.5);
    s.execute("text", &json!({"at": [bounds.min.x, bounds.max.y + margin * 0.3], "height": callout_height, "text": callout}))?;

    // The hole table, to the right of the part (one row per hole, datum = the part's own origin
    // corner — `bounds.min` — so X/Y read the same way a draftsman would dimension them by hand).
    let mut hole_table_handle = None;
    if !holes.is_empty() {
        let cells: Vec<Value> = holes
            .iter()
            .enumerate()
            .map(|(i, (center, diameter))| {
                json!([
                    format!("{}", i + 1),
                    format!("{diameter:.1}"),
                    format!("{:.1}", center.x - bounds.min.x),
                    format!("{:.1}", center.y - bounds.min.y)
                ])
            })
            .collect();
        let r = s.execute(
            "table",
            &json!({
                "at": [bounds.max.x + margin, bounds.max.y],
                "rows": holes.len(),
                "cols": 4,
                "header": ["FURO", "Ø", "X", "Y"],
                "cells": cells,
                "title": "TABELA DE FUROS",
            }),
        )?;
        hole_table_handle = r.get("handle").cloned();
    }

    // Everything that needs to appear (the part plus the margin dimensions/callout/table need);
    // a generous extra margin stands in for the dimension/table extents this pass doesn't
    // measure precisely yet — the sheet-choice algorithm itself (layout_fit) is fully exact and
    // tested; this is a conservative over-estimate of what it's fed.
    let content = Bounds2::new(
        Vec2::new(bounds.min.x - margin * 2.0, bounds.min.y - margin * 2.0),
        Vec2::new(bounds.max.x + margin * 4.0, bounds.max.y + margin * 2.0),
    );
    let sheet = choose_sheet_and_scale(content, 15.0, 100.0)
        .ok_or_else(|| bad("fabdim", "could not choose a sheet size for this drawing"))?;

    let layout_name = format!("FAB_{block_name}");
    let layout_result = s.execute("layout.new", &json!({"name": layout_name}))?;

    Ok(json!({
        "dimensions": dim_handles,
        "holeTable": hole_table_handle,
        "layout": layout_result.get("name").cloned().unwrap_or(json!(layout_name)),
        "sheet": sheet.sheet_name,
        "scale": format!("1:{}", sheet.scale_divisor),
    }))
}

#[cfg(test)]
mod tests {
    use crate::dispatch::execute as fab_execute;
    use cadcraft_engine::Session;
    use serde_json::{Value, json};

    fn session_with_sample() -> Session {
        let mut s = Session::new();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "test", None);
        s
    }

    /// The design spec's own worked example: a 100×100×5mm plate with 7 Ø10mm holes.
    #[test]
    fn dimensions_the_spec_example_plate_end_to_end() {
        let mut s = session_with_sample();
        let holes: Vec<Value> = (0..7)
            .map(|i| json!({"center": [10.0 + i as f64 * 12.0, 50.0], "diameter": 10.0}))
            .collect();
        let plate = fab_execute(&mut s, "plate", &json!({"width": 100.0, "height": 100.0, "thickness": 5.0, "material": "A36", "holes": holes})).expect("drawing the plate must succeed");
        let handle = plate["handle"].as_str().expect("handle").to_string();

        let r = fab_execute(&mut s, "fabdim", &json!({"handle": handle}))
            .expect("dimensioning a fresh Plate must succeed");
        assert_eq!(
            r["dimensions"].as_array().expect("dimensions array").len(),
            2,
            "width and height overall dimensions"
        );
        assert!(
            r["holeTable"].is_string(),
            "7 holes must produce a hole table"
        );
        assert!(r["sheet"].is_string());
        assert!(r["scale"].as_str().expect("scale").starts_with("1:"));
    }

    #[test]
    fn a_plate_without_holes_skips_the_table_but_still_dimensions() {
        let mut s = session_with_sample();
        let plate = fab_execute(
            &mut s,
            "plate",
            &json!({"width": 40.0, "height": 25.0, "thickness": 2.0}),
        )
        .expect("plate without holes must succeed");
        let handle = plate["handle"].as_str().expect("handle").to_string();
        let r = fab_execute(&mut s, "fabdim", &json!({"handle": handle}))
            .expect("dimensioning must still succeed with no holes");
        assert!(r["holeTable"].is_null());
    }

    #[test]
    fn rejects_a_handle_that_is_not_a_part() {
        let mut s = session_with_sample();
        // A plain LINE, drawn the freehand way — never tagged as a Part.
        let line = s
            .execute("line", &json!({"points": [[0.0, 0.0], [10.0, 10.0]]}))
            .expect("line should draw");
        let handle = line["handles"][0].as_str().expect("handles[0]").to_string();
        let err = fab_execute(&mut s, "fabdim", &json!({"handle": handle})).unwrap_err();
        assert!(err.to_string().contains("Part"));
    }

    #[test]
    fn rejects_an_unknown_handle_without_panicking() {
        let mut s = session_with_sample();
        let err = fab_execute(&mut s, "fabdim", &json!({"handle": "FFFFFF"})).unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn profile_and_tube_and_bent_report_not_available_yet_instead_of_guessing() {
        let mut s = session_with_sample();
        let profile = fab_execute(&mut s, "profile", &json!({"length": 500.0, "height": 50.0}))
            .expect("profile should draw");
        let err = fab_execute(&mut s, "fabdim", &json!({"handle": profile["handle"]})).unwrap_err();
        assert!(err.to_string().contains("not available yet"));
    }
}
