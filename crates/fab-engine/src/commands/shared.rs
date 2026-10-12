//! What every smart drawing tool (`plate`, `profile`, `tube`, `bent-profile`) shares: draw the
//! outline and any holes, attach the Part's attributes invisibly, and convert the lot into one
//! block instance (see the design spec §3 for why blocks+attributes). A smart tool's own command
//! function only has to compute the right outline/holes for its kind and call `build_part`.

use cadcraft_engine::{Result, Session};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::jsonutil::{bad, point_value};
use crate::part::PartData;

fn handle_of(cmd: &str, v: Value) -> Result<String> {
    v.get("handle")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| bad(cmd, "expected a `handle` back from a drawing primitive"))
}

/// `outline` needs at least 2 points (3 if `closed`). `p["holes"]`, if present, is
/// `[{center: [x,y], diameter}, ...]` — every hole becomes a circle inside the same block.
/// `closed`: true for a plate/profile footprint (a loop), false for a bent profile's folded
/// schematic (an open strip — closing it would draw a false segment back to the start).
pub fn build_part(
    s: &mut Session,
    cmd: &str,
    part: PartData,
    outline: &[Vec2],
    closed: bool,
    p: &Value,
) -> Result<Value> {
    let min_pts = if closed { 3 } else { 2 };
    if outline.len() < min_pts {
        return Err(bad(
            cmd,
            format!("a part's outline needs at least {min_pts} points"),
        ));
    }
    let vertices: Vec<Value> = outline.iter().map(|v| json!([v.x, v.y])).collect();
    let outline_handle = handle_of(
        cmd,
        s.execute("pline", &json!({"vertices": vertices, "closed": closed}))?,
    )?;
    let mut handles = vec![outline_handle.clone()];

    if let Some(holes) = p.get("holes").and_then(Value::as_array) {
        for hole in holes {
            let center = hole
                .get("center")
                .and_then(point_value)
                .ok_or_else(|| bad(cmd, "each hole needs a `center` [x, y]"))?;
            let diameter = hole
                .get("diameter")
                .and_then(Value::as_f64)
                .filter(|d| d.is_finite() && *d > 0.0)
                .ok_or_else(|| bad(cmd, "each hole needs a positive `diameter`"))?;
            let h = handle_of(
                cmd,
                s.execute(
                    "circle",
                    &json!({"center": [center.x, center.y], "diameter": diameter}),
                )?,
            )?;
            handles.push(h);
        }
    }

    // Invisible attributes: the Part's data rides along without changing what the drawing looks
    // like — using a smart tool never costs the draftsman anything visually over drawing freehand.
    let origin = outline[0];
    for (tag, value) in part.attdef_values() {
        let h = handle_of(cmd, s.execute("attdef", &json!({"tag": tag, "default": value, "at": [origin.x, origin.y], "height": 0.01, "invisible": true}))?)?;
        handles.push(h);
    }

    let block_name = format!("FABPART_{outline_handle}");
    let result = s.execute("block", &json!({"name": block_name, "base": [origin.x, origin.y], "handles": handles, "keep": "convert"}))?;
    let insert_handle = result
        .get("insert")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            bad(
                cmd,
                "block conversion did not produce an insert — nothing was selected?",
            )
        })?;
    Ok(json!({"handle": insert_handle, "kind": part.kind.tag_value()}))
}
