//! `fabdim` — Cotar Desenho: the single command that turns a Part into a fully dimensioned
//! fabrication drawing (design spec §5). Implements all four PartKind templates: Plate (overall
//! dimensions + hole table), Profile/Tube (overall length + a catalog cross-section mini-view +
//! hole table), and BentProfile (per-flange dimensions on the folded view, bend-angle callouts,
//! and the flat/unfolded view with its own overall length — the flat-pattern length is
//! recomputed from the folded geometry itself plus the bend radius/K-factor persisted on the
//! Part, via `commands::bent_profile::flat_length`). Every template ends by choosing a sheet
//! (`layout_fit::choose_sheet_and_scale`) and wiring it to a real viewport (`place_on_sheet`).

use cadcraft_doc::{Block, EntityKind, Handle, LwPolyline};
use cadcraft_engine::cmd::{CommandSpec, has_doc};
use cadcraft_engine::{Result, Session};
use cadcraft_geom::{Bounds2, Vec2};
use serde_json::{Value, json};

use crate::catalog;
use crate::commands::bent_profile::flat_length;
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
        PartKind::Profile { catalog_ref } => dim_profile_or_tube(
            s,
            &block_name,
            world_offset,
            "PERFIL",
            catalog_ref.as_deref(),
            &part.material,
        ),
        PartKind::Tube { catalog_ref } => dim_profile_or_tube(
            s,
            &block_name,
            world_offset,
            "TUBO",
            catalog_ref.as_deref(),
            &part.material,
        ),
        PartKind::BentProfile {
            thickness,
            bend_radius,
            k_factor,
        } => dim_bent_profile(
            s,
            &block_name,
            world_offset,
            thickness,
            bend_radius,
            k_factor,
            &part.material,
        ),
    }
}

/// The hole table every PartKind's template uses the same way: one row per hole, datum = the
/// part's own origin corner (`bounds.min`) — the same convention a draftsman would dimension
/// holes by hand from. `None` when there are no holes (nothing to table).
fn hole_table(s: &mut Session, bounds: Bounds2, holes: &[(Vec2, f64)]) -> Result<Option<Value>> {
    if holes.is_empty() {
        return Ok(None);
    }
    let margin = (bounds.width().max(bounds.height()) * 0.15).max(10.0);
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
    Ok(r.get("handle").cloned())
}

/// The world-coordinate vertices of the first open or closed polyline found in `block` — the
/// one entity every smart tool's outline/footprint/folded-strip is drawn as.
fn outline_vertices(block: &Block, world_offset: Vec2) -> Option<Vec<Vec2>> {
    block.entities.iter().find_map(|e| match &e.kind {
        EntityKind::LwPolyline(LwPolyline { vertices, .. }) => {
            Some(vertices.iter().map(|v| v.p + world_offset).collect())
        }
        _ => None,
    })
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

    // The hole table, to the right of the part.
    let hole_table_handle = hole_table(s, bounds, &holes)?;

    // Everything that needs to appear (the part plus the margin dimensions/callout/table need);
    // a generous extra margin stands in for the dimension/table extents this pass doesn't
    // measure precisely yet — the sheet-choice algorithm itself (layout_fit) is fully exact and
    // tested; this is a conservative over-estimate of what it's fed.
    let content = Bounds2::new(
        Vec2::new(bounds.min.x - margin * 2.0, bounds.min.y - margin * 2.0),
        Vec2::new(bounds.max.x + margin * 4.0, bounds.max.y + margin * 2.0),
    );
    let sheet_info = place_on_sheet(s, block_name, content)?;

    Ok(json!({
        "dimensions": dim_handles,
        "holeTable": hole_table_handle,
        "layout": sheet_info["layout"].clone(),
        "sheet": sheet_info["sheet"].clone(),
        "scale": sheet_info["scale"].clone(),
    }))
}

/// Chooses a sheet/scale for `content` (`layout_fit`), creates the destination layout, and
/// points its default viewport at `content` at that scale and centre — the step that physically
/// wires the sheet-choice decision to a real cadcraft viewport (`viewport.set` takes a `"a:b"`
/// scale and a model-space `center` directly, so the decision and the wiring are a single call
/// apart). Shared by every PartKind's dimensioning template, not just Plate's.
fn place_on_sheet(s: &mut Session, block_name: &str, content: Bounds2) -> Result<Value> {
    let sheet = choose_sheet_and_scale(content, 15.0, 200.0)
        .ok_or_else(|| bad("fabdim", "could not choose a sheet size for this drawing"))?;

    let layout_name = format!("FAB_{block_name}");
    let layout_result = s.execute("layout.new", &json!({"name": layout_name}))?;
    let layout_name = layout_result
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&layout_name)
        .to_string();

    if let Some(viewport_handle) = layout_result.get("viewport").and_then(Value::as_str) {
        let center = content.center();
        s.execute(
            "viewport.set",
            &json!({
                "handle": viewport_handle,
                "scale": format!("1:{}", sheet.scale_divisor),
                "center": [center.x, center.y],
            }),
        )?;
    }

    Ok(json!({
        "layout": layout_name,
        "sheet": sheet.sheet_name,
        "scale": format!("1:{}", sheet.scale_divisor),
    }))
}

/// Profile/Tube template: overall length, a catalog cross-section mini-view when `catalog_ref`
/// is known (drawn from the public catalog's own dimensions — never guessed, never fabricated
/// for a free/uncatalogued section), and the hole table.
fn dim_profile_or_tube(
    s: &mut Session,
    block_name: &str,
    world_offset: Vec2,
    label: &str,
    catalog_ref: Option<&str>,
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
            "this part has no outline geometry to dimension",
        ));
    }

    let margin = (bounds.width().max(bounds.height()) * 0.15).max(10.0);
    let mut dim_handles = Vec::new();

    // Overall length, below the elevation.
    let length_at = Vec2::new(bounds.center().x, bounds.min.y - margin);
    let r = s.execute("dimlinear", &json!({"p1": [bounds.min.x, bounds.min.y], "p2": [bounds.max.x, bounds.min.y], "at": [length_at.x, length_at.y]}))?;
    dim_handles.push(r["handle"].clone());

    // Section callout, above the elevation.
    let section_label = catalog_ref.unwrap_or("SEÇÃO LIVRE");
    let callout = if material.is_empty() {
        format!("{label} {section_label}")
    } else {
        format!("{label} {section_label}  —  {material}")
    };
    let callout_height = (bounds.width().max(bounds.height()) * 0.03).max(2.5);
    s.execute("text", &json!({"at": [bounds.min.x, bounds.max.y + margin * 0.3], "height": callout_height, "text": callout}))?;

    let hole_table_handle = hole_table(s, bounds, &holes)?;

    // The cross-section mini-view, below the overall length dimension: a real rectangle drawn
    // from the catalog's own width/height, labelled with the thickness — only when the section is
    // cataloged, never fabricated for "SEÇÃO LIVRE".
    let mut section_bounds = bounds;
    if let Some(dims) = catalog_ref.and_then(catalog::lookup) {
        let section_at = Vec2::new(bounds.min.x, bounds.min.y - margin * 3.0 - dims.height);
        s.execute("pline", &json!({"vertices": [[section_at.x, section_at.y], [section_at.x + dims.width, section_at.y], [section_at.x + dims.width, section_at.y + dims.height], [section_at.x, section_at.y + dims.height]], "closed": true}))?;
        s.execute("text", &json!({"at": [section_at.x, section_at.y - margin * 0.3], "height": callout_height, "text": format!("e = {}", dims.thickness)}))?;
        section_bounds = Bounds2::new(
            section_at,
            Vec2::new(section_at.x + dims.width, section_at.y + dims.height),
        );
    }

    let content = Bounds2::new(
        Vec2::new(
            bounds.min.x.min(section_bounds.min.x) - margin * 2.0,
            section_bounds.min.y.min(bounds.min.y) - margin * 2.0,
        ),
        Vec2::new(
            bounds.max.x.max(section_bounds.max.x) + margin * 4.0,
            bounds.max.y + margin * 2.0,
        ),
    );
    let sheet_info = place_on_sheet(s, block_name, content)?;

    Ok(json!({
        "dimensions": dim_handles,
        "holeTable": hole_table_handle,
        "layout": sheet_info["layout"].clone(),
        "sheet": sheet_info["sheet"].clone(),
        "scale": sheet_info["scale"].clone(),
    }))
}

/// BentProfile template — the highest-risk one (design spec §5). Flange lengths and bend angles
/// are read back from the folded polyline's own geometry (each is just a segment length / the
/// turn angle at a vertex — no extra storage needed); the bend radius and K-factor come from the
/// Part's attributes (`part::PartKind::BentProfile`), since those leave no geometric trace.
/// Dimensions each flange on the folded view, labels each bend's angle, then draws the flat
/// (unfolded) view as a single straight strip of the recomputed flat length with its own overall
/// dimension — the two views the design spec calls for a bent part.
fn dim_bent_profile(
    s: &mut Session,
    block_name: &str,
    world_offset: Vec2,
    thickness: f64,
    bend_radius: f64,
    k_factor: f64,
    material: &str,
) -> Result<Value> {
    let (folded, holes) = {
        let doc = s.doc()?;
        let block = doc
            .blocks
            .get(block_name)
            .ok_or_else(|| bad("fabdim", "the Part's block definition is missing"))?;
        let verts = outline_vertices(block, world_offset).ok_or_else(|| {
            bad(
                "fabdim",
                "this bent profile has no folded outline to dimension",
            )
        })?;
        let (_, holes) = part_geometry(block, world_offset);
        (verts, holes)
    };
    if folded.len() < 3 {
        return Err(bad(
            "fabdim",
            "a bent profile needs at least 2 flanges (3 points) to dimension",
        ));
    }

    let folded_bounds = Bounds2::from_points(folded.iter().copied());
    let margin = (folded_bounds.width().max(folded_bounds.height()) * 0.15).max(10.0);

    // Flange lengths and bend angles, straight from the geometry — see the module doc.
    let flanges: Vec<f64> = folded.windows(2).map(|w| (w[1] - w[0]).len()).collect();
    let headings: Vec<f64> = folded
        .windows(2)
        .map(|w| (w[1] - w[0]).y.atan2((w[1] - w[0]).x))
        .collect();
    let bend_angles: Vec<f64> = headings
        .windows(2)
        .map(|w| (w[1] - w[0]).to_degrees().rem_euclid(360.0))
        .collect();

    let mut dim_handles = Vec::new();
    for (i, seg) in folded.windows(2).enumerate() {
        let (a, b) = (seg[0], seg[1]);
        let mid = Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let dir = (b - a).normalized();
        let perp = Vec2::new(-dir.y, dir.x);
        let at = mid + perp * margin;
        let r = s.execute(
            "dimlinear",
            &json!({"p1": [a.x, a.y], "p2": [b.x, b.y], "at": [at.x, at.y]}),
        )?;
        dim_handles.push(r["handle"].clone());
        let _ = i;
    }
    let callout_height = margin
        .min((folded_bounds.width().max(folded_bounds.height())) * 0.03)
        .max(2.5);
    for (vertex, angle) in folded[1..folded.len() - 1].iter().zip(bend_angles.iter()) {
        s.execute("text", &json!({"at": [vertex.x + callout_height * 0.5, vertex.y + callout_height * 0.5], "height": callout_height, "text": format!("{angle:.0}°")}))?;
    }

    let flat =
        flat_length(&flanges, &bend_angles, bend_radius, thickness, k_factor).map_err(|m| {
            bad(
                "fabdim",
                format!("could not recompute the flat pattern from this part's geometry: {m}"),
            )
        })?;

    // The flat/unfolded view: a single straight strip of the recomputed length, below the folded
    // view, with its own overall dimension and a callout naming it as the development length.
    let flat_at = Vec2::new(folded_bounds.min.x, folded_bounds.min.y - margin * 3.0);
    s.execute(
        "line",
        &json!({"points": [[flat_at.x, flat_at.y], [flat_at.x + flat, flat_at.y]]}),
    )?;
    let r = s.execute("dimlinear", &json!({"p1": [flat_at.x, flat_at.y], "p2": [flat_at.x + flat, flat_at.y], "at": [flat_at.x + flat * 0.5, flat_at.y - margin]}))?;
    dim_handles.push(r["handle"].clone());
    let material_suffix = if material.is_empty() {
        String::new()
    } else {
        format!("  —  {material}")
    };
    s.execute("text", &json!({"at": [flat_at.x, flat_at.y - margin * 1.8], "height": callout_height, "text": format!("DESENVOLVIDO = {flat:.1}  (e = {thickness}, R = {bend_radius}, K = {k_factor}){material_suffix}")}))?;

    let flat_bounds = Bounds2::new(
        Vec2::new(flat_at.x, flat_at.y - margin * 2.5),
        Vec2::new(flat_at.x + flat, flat_at.y),
    );
    let hole_table_handle = hole_table(s, folded_bounds, &holes)?;

    let content_min = Vec2::new(
        folded_bounds.min.x.min(flat_bounds.min.x) - margin * 2.0,
        flat_bounds.min.y - margin * 2.0,
    );
    let content_max = Vec2::new(
        folded_bounds.max.x.max(flat_bounds.max.x) + margin * 4.0,
        folded_bounds.max.y + margin * 2.0,
    );
    let sheet_info = place_on_sheet(s, block_name, Bounds2::new(content_min, content_max))?;

    Ok(json!({
        "dimensions": dim_handles,
        "holeTable": hole_table_handle,
        "flatLength": flat,
        "layout": sheet_info["layout"].clone(),
        "sheet": sheet_info["sheet"].clone(),
        "scale": sheet_info["scale"].clone(),
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
    fn dimensions_a_free_profile() {
        let mut s = session_with_sample();
        let profile = fab_execute(&mut s, "profile", &json!({"length": 500.0, "height": 50.0, "holes": [{"center": [100.0, 25.0], "diameter": 8.0}]}))
            .expect("profile should draw");
        let r = fab_execute(&mut s, "fabdim", &json!({"handle": profile["handle"]}))
            .expect("dimensioning a free profile must succeed");
        assert_eq!(
            r["dimensions"].as_array().expect("dimensions").len(),
            1,
            "just the overall length — no catalog cross-section to dimension"
        );
        assert!(r["holeTable"].is_string());
    }

    #[test]
    fn dimensions_a_catalog_tube_with_its_cross_section_mini_view() {
        let mut s = session_with_sample();
        let tube = fab_execute(
            &mut s,
            "tube",
            &json!({"length": 1500.0, "catalogRef": "TUBO QUAD 40x40x2"}),
        )
        .expect("tube should draw");
        let r = fab_execute(&mut s, "fabdim", &json!({"handle": tube["handle"]}))
            .expect("dimensioning a catalog tube must succeed");
        assert!(r["sheet"].is_string());
    }

    /// The flat-pattern math's own numbers (bend_profile::tests) say a 50+50mm, 90°, 1mm-thick
    /// bracket develops to ≈102.09mm — `fabdim` must recover that same number purely from the
    /// folded geometry it just drew, with no separate storage of flange/bend data.
    #[test]
    fn dimensions_an_l_bracket_and_recovers_the_right_flat_length_from_geometry_alone() {
        let mut s = session_with_sample();
        let bracket = fab_execute(&mut s, "bent-profile", &json!({"flanges": [50.0, 50.0], "bendAngles": [90.0], "thickness": 1.0, "bendRadius": 1.0, "kFactor": 0.33}))
            .expect("bent-profile should draw");
        let created_flat = bracket["flatLength"]
            .as_f64()
            .expect("flatLength from the draw command");

        let r = fab_execute(&mut s, "fabdim", &json!({"handle": bracket["handle"]}))
            .expect("dimensioning a fresh bent profile must succeed");
        let redimensioned_flat = r["flatLength"].as_f64().expect("flatLength from fabdim");
        assert!(
            (redimensioned_flat - created_flat).abs() < 1e-6,
            "fabdim's recomputed flat length ({redimensioned_flat}) must match the one bent-profile itself computed at creation ({created_flat})"
        );
        // 2 flange dims (folded view) + 1 overall dim (flat view).
        assert_eq!(r["dimensions"].as_array().expect("dimensions").len(), 3);
    }
}
