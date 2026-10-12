//! `profile` and `tube` — Desenhar Perfil / Desenhar Tubo. v1 draws a 2D elevation footprint
//! (length × the section's catalog height, or a free height) with holes along it; the actual
//! cross-section shape comes from `fab_engine::catalog` when `catalogRef` is given. "Perfil
//! livre" is `catalogRef` simply omitted — nothing about this command forces the catalog.

use cadcraft_engine::cmd::{CommandSpec, has_doc};
use cadcraft_engine::{Result, Session};
use cadcraft_geom::Vec2;
use serde_json::Value;

use super::shared::build_part;
use crate::catalog;
use crate::jsonutil::{bad, point_or, positive_req, str_or};
use crate::part::{PartData, PartKind};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("profile", "Desenhar Perfil", run_profile)
            .enabled(has_doc)
            .params("{length, catalogRef? (a name from the profile catalog) | height, at?, material?, holes?} -> {handle, kind}"),
        CommandSpec::new("tube", "Desenhar Tubo", run_tube)
            .enabled(has_doc)
            .params("{length, catalogRef? | height, at?, material?, holes?} -> {handle, kind}"),
    ]
}

/// The elevation footprint: length along X from `at`, height either from the catalog section or
/// given directly ("Perfil livre").
fn footprint(cmd: &str, p: &Value) -> Result<(Vec<Vec2>, Option<String>)> {
    let length = positive_req(cmd, p, "length")?;
    let at = point_or(p, "at", Vec2::ZERO);
    let catalog_ref = p
        .get("catalogRef")
        .and_then(Value::as_str)
        .map(str::to_string);
    let height = match &catalog_ref {
        Some(name) => {
            catalog::lookup(name)
                .ok_or_else(|| {
                    bad(
                        cmd,
                        format!("`catalogRef` \"{name}\" is not in the profile catalog"),
                    )
                })?
                .height
        }
        None => positive_req(cmd, p, "height")?,
    };
    let outline = vec![
        at,
        Vec2::new(at.x + length, at.y),
        Vec2::new(at.x + length, at.y + height),
        Vec2::new(at.x, at.y + height),
    ];
    Ok((outline, catalog_ref))
}

fn run_profile(s: &mut Session, p: &Value) -> Result<Value> {
    let (outline, catalog_ref) = footprint("profile", p)?;
    let material = str_or(p, "material", "").to_string();
    build_part(
        s,
        "profile",
        PartData::new(PartKind::Profile { catalog_ref }, material),
        &outline,
        true,
        p,
    )
}

fn run_tube(s: &mut Session, p: &Value) -> Result<Value> {
    let (outline, catalog_ref) = footprint("tube", p)?;
    let material = str_or(p, "material", "").to_string();
    build_part(
        s,
        "tube",
        PartData::new(PartKind::Tube { catalog_ref }, material),
        &outline,
        true,
        p,
    )
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
    fn draws_a_catalog_profile() {
        let mut s = session_with_sample();
        let r = fab_execute(
            &mut s,
            "profile",
            &json!({"length": 2000.0, "catalogRef": "U 100x50x3"}),
        )
        .expect("known catalog ref should work");
        assert_eq!(r["kind"], "PROFILE");
    }

    #[test]
    fn draws_a_free_profile() {
        let mut s = session_with_sample();
        let r = fab_execute(
            &mut s,
            "profile",
            &json!({"length": 2000.0, "height": 42.0}),
        )
        .expect("free profile should work without a catalog ref");
        assert_eq!(r["kind"], "PROFILE");
    }

    #[test]
    fn rejects_an_unknown_catalog_ref() {
        let mut s = session_with_sample();
        let err = fab_execute(
            &mut s,
            "profile",
            &json!({"length": 2000.0, "catalogRef": "NOT A REAL PROFILE"}),
        )
        .unwrap_err();
        assert!(err.to_string().contains("catalog"));
    }

    #[test]
    fn draws_a_tube() {
        let mut s = session_with_sample();
        let r = fab_execute(
            &mut s,
            "tube",
            &json!({"length": 1500.0, "catalogRef": "TUBO QUAD 40x40x2"}),
        )
        .expect("tube with catalog ref should work");
        assert_eq!(r["kind"], "TUBE");
    }
}
