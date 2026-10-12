//! `bent-profile` — Desenhar Perfil Dobrado, and the flat-pattern (bend-allowance) math behind
//! it. This is the highest-risk part of the v1 scope (per the design spec §5): converting a
//! folded sheet-metal part into its true flat development needs a bend-allowance calculation,
//! not just geometry. The formula here is a standard, publicly documented sheet-metal engineering
//! formula (bend allowance = angle × (radius + K·thickness)) — not proprietary to any vendor, and
//! deliberately the most heavily tested module in this crate.
//!
//! Bend-angle convention (must be stated, since it's the one place a wrong assumption would
//! silently mis-dimension every bent part): `bendAngles[i]` is the angle, in degrees (0, 180],
//! by which the direction of travel turns at that bend — 90° turns a straight strip into a
//! classic right-angle L bracket.

use cadcraft_engine::cmd::{CommandSpec, has_doc};
use cadcraft_engine::{Result, Session};
use cadcraft_geom::Vec2;
use serde_json::Value;

use super::shared::build_part;
use crate::jsonutil::{bad, point_or, positive_req, str_or};
use crate::part::{PartData, PartKind};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("bent-profile", "Desenhar Perfil Dobrado", run_bent_profile)
            .enabled(has_doc)
            .params(
                "{flanges: [len,...] (>=2), bendAngles: [deg,...] (len(flanges)-1, each in (0,180]), thickness, bendRadius? (default: thickness), kFactor? (default: 0.33), at?, material?, holes?} -> {handle, kind, flatLength}",
            ),
    ]
}

/// The standard bend-allowance formula: how much flat material a bend of `angle_deg` consumes,
/// given the inside bend `radius`, material `thickness` and `k_factor` (where the neutral bend
/// line sits through the thickness, as a fraction from the inside face — commonly 0.3–0.5).
/// `None` for any non-finite or out-of-range input: never a NaN or negative result.
pub fn bend_allowance(angle_deg: f64, radius: f64, thickness: f64, k_factor: f64) -> Option<f64> {
    if ![angle_deg, radius, thickness, k_factor]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }
    if !(angle_deg > 0.0 && angle_deg <= 180.0)
        || radius < 0.0
        || thickness <= 0.0
        || k_factor < 0.0
    {
        return None;
    }
    Some(angle_deg.to_radians() * (radius + k_factor * thickness))
}

/// The flat (unfolded) length of a bent part: the flanges' lengths plus every bend's allowance.
pub fn flat_length(
    flanges: &[f64],
    bend_angles_deg: &[f64],
    radius: f64,
    thickness: f64,
    k_factor: f64,
) -> std::result::Result<f64, String> {
    if flanges.len() < 2 {
        return Err("a bent profile needs at least 2 flanges".into());
    }
    if bend_angles_deg.len() != flanges.len() - 1 {
        return Err(format!(
            "{} flanges need exactly {} bend angles, got {}",
            flanges.len(),
            flanges.len() - 1,
            bend_angles_deg.len()
        ));
    }
    let mut total = 0.0;
    for (i, &f) in flanges.iter().enumerate() {
        if !(f.is_finite() && f > 0.0) {
            return Err(format!("flange {i} must be a positive finite length"));
        }
        total += f;
    }
    for (i, &a) in bend_angles_deg.iter().enumerate() {
        let ba = bend_allowance(a, radius, thickness, k_factor).ok_or_else(|| format!("bend {i}: invalid angle/radius/thickness/k ({a}, {radius}, {thickness}, {k_factor})"))?;
        total += ba;
    }
    if !total.is_finite() {
        return Err("flat length computation did not come out finite".into());
    }
    Ok(total)
}

/// The folded schematic, in 2D: a strip walking each flange, turning by each bend angle. This is
/// the *folded* view, drawn as fabcraft sees it on the model; the *flat* view (a straight strip of
/// `flat_length`) is generated separately by `fabdim`, not here.
fn folded_outline(at: Vec2, flanges: &[f64], bend_angles_deg: &[f64]) -> Vec<Vec2> {
    let mut pts = vec![at];
    let mut cur = at;
    let mut heading = 0.0_f64;
    for (i, &len) in flanges.iter().enumerate() {
        cur += Vec2::from_angle(heading) * len;
        pts.push(cur);
        if let Some(&a) = bend_angles_deg.get(i) {
            heading += a.to_radians();
        }
    }
    pts
}

fn f64_array(cmd: &str, p: &Value, key: &str) -> Result<Vec<f64>> {
    p.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| bad(cmd, format!("`{key}` must be an array of numbers")))?
        .iter()
        .map(|v| {
            v.as_f64()
                .filter(|x| x.is_finite())
                .ok_or_else(|| bad(cmd, format!("`{key}` must contain only finite numbers")))
        })
        .collect()
}

fn run_bent_profile(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "bent-profile";
    let flanges = f64_array(cmd, p, "flanges")?;
    let bend_angles = f64_array(cmd, p, "bendAngles")?;
    let thickness = positive_req(cmd, p, "thickness")?;
    let radius = p
        .get("bendRadius")
        .and_then(Value::as_f64)
        .filter(|r| r.is_finite() && *r >= 0.0)
        .unwrap_or(thickness);
    let k_factor = p
        .get("kFactor")
        .and_then(Value::as_f64)
        .filter(|k| k.is_finite() && *k >= 0.0)
        .unwrap_or(0.33);
    let flat = flat_length(&flanges, &bend_angles, radius, thickness, k_factor)
        .map_err(|m| bad(cmd, m))?;

    let at = point_or(p, "at", Vec2::ZERO);
    let outline = folded_outline(at, &flanges, &bend_angles);
    let material = str_or(p, "material", "").to_string();
    let part = PartData::new(PartKind::BentProfile { thickness }, material);
    let mut result = build_part(s, cmd, part, &outline, false, p)?;
    result["flatLength"] = serde_json::json!(flat);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- bend_allowance: reference values from the standard public formula BA = θ·(R + K·T) ---

    #[test]
    fn ninety_degree_bend_matches_the_published_formula_by_hand() {
        // R=2mm, T=1mm, K=0.33: BA = (π/2)·(2 + 0.33·1) = (π/2)·2.33 ≈ 3.6599
        let ba = bend_allowance(90.0, 2.0, 1.0, 0.33).expect("valid inputs");
        assert!((ba - 3.6599).abs() < 1e-3, "got {ba}");
    }

    #[test]
    fn zero_radius_sharp_bend_still_has_an_allowance_from_k_times_thickness() {
        let ba = bend_allowance(90.0, 0.0, 2.0, 0.5).expect("valid inputs");
        // BA = (π/2)·(0 + 0.5·2) = π/2
        assert!((ba - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn hostile_bend_allowance_inputs_are_rejected_not_panicked() {
        assert_eq!(bend_allowance(f64::NAN, 1.0, 1.0, 0.33), None);
        assert_eq!(bend_allowance(0.0, 1.0, 1.0, 0.33), None); // 0° isn't a bend
        assert_eq!(bend_allowance(181.0, 1.0, 1.0, 0.33), None); // more than a straight fold back
        assert_eq!(bend_allowance(90.0, -1.0, 1.0, 0.33), None); // negative radius
        assert_eq!(bend_allowance(90.0, 1.0, 0.0, 0.33), None); // zero thickness isn't sheet metal
        assert_eq!(bend_allowance(90.0, 1.0, 1.0, f64::INFINITY), None);
    }

    // --- flat_length: a simple L-bracket, hand-checked ---

    #[test]
    fn flat_length_of_a_simple_l_bracket() {
        // Two 50mm flanges, one 90° bend, 1mm thick, R=1mm, K=0.33.
        // BA = (π/2)·(1 + 0.33) = (π/2)·1.33 ≈ 2.0892
        let flat = flat_length(&[50.0, 50.0], &[90.0], 1.0, 1.0, 0.33).expect("valid inputs");
        assert!((flat - (100.0 + 2.0892)).abs() < 1e-3, "got {flat}");
    }

    #[test]
    fn flat_length_rejects_a_flange_bend_count_mismatch() {
        // 3 flanges need exactly 2 bend angles, not 1 — a drafting mistake, not a panic.
        assert!(flat_length(&[10.0, 10.0, 10.0], &[90.0], 1.0, 1.0, 0.33).is_err());
    }

    #[test]
    fn flat_length_rejects_a_non_positive_flange() {
        assert!(flat_length(&[10.0, -5.0], &[90.0], 1.0, 1.0, 0.33).is_err());
        assert!(flat_length(&[10.0, 0.0], &[90.0], 1.0, 1.0, 0.33).is_err());
        assert!(flat_length(&[10.0, f64::NAN], &[90.0], 1.0, 1.0, 0.33).is_err());
    }

    // --- the command end to end ---

    use crate::dispatch::execute as fab_execute;
    use cadcraft_engine::Session;
    use serde_json::json;

    fn session_with_sample() -> Session {
        let mut s = Session::new();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "test", None);
        s
    }

    #[test]
    fn draws_an_l_bracket_and_reports_its_flat_length() {
        let mut s = session_with_sample();
        let r = fab_execute(
            &mut s,
            "bent-profile",
            &json!({"flanges": [50.0, 50.0], "bendAngles": [90.0], "thickness": 1.0}),
        )
        .expect("a valid L bracket should succeed");
        assert_eq!(r["kind"], "BENT");
        let flat = r["flatLength"]
            .as_f64()
            .expect("flatLength must be a number");
        assert!(
            flat > 100.0,
            "flat length {flat} should exceed the sum of flanges (bend allowance adds material)"
        );
    }

    #[test]
    fn rejects_mismatched_flanges_and_bends_without_touching_the_drawing() {
        let mut s = session_with_sample();
        let err = fab_execute(
            &mut s,
            "bent-profile",
            &json!({"flanges": [50.0, 50.0, 50.0], "bendAngles": [90.0], "thickness": 1.0}),
        )
        .unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}
