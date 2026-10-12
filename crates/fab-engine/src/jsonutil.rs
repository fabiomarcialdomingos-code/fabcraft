//! Tiny JSON-param helpers for fab-engine's own commands. cadcraft-engine's equivalents
//! (`cmd::bad`, `cmd::f64_req`, ...) are `pub(crate)` to that crate and not reachable from here,
//! so these are fabcraft's own copies of the same idea: refuse hostile/missing input with a clear
//! `EngineError`, never a panic, never a silent default where one isn't explicitly allowed.

use cadcraft_engine::{EngineError, Result};
use cadcraft_geom::Vec2;
use serde_json::Value;

pub fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams {
        cmd: cmd.into(),
        msg: msg.into(),
    }
}

pub fn f64_req(cmd: &str, p: &Value, key: &str) -> Result<f64> {
    p.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| bad(cmd, format!("`{key}` (number) is required")))
}

pub fn f64_or(p: &Value, key: &str, default: f64) -> f64 {
    p.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

pub fn str_or<'a>(p: &'a Value, key: &str, default: &'a str) -> &'a str {
    p.get(key).and_then(Value::as_str).unwrap_or(default)
}

pub fn point_value(v: &Value) -> Option<Vec2> {
    let a = v.as_array()?;
    if a.len() != 2 {
        return None;
    }
    let x = a[0].as_f64()?;
    let y = a[1].as_f64()?;
    if x.is_finite() && y.is_finite() {
        Some(Vec2::new(x, y))
    } else {
        None
    }
}

pub fn point_or(p: &Value, key: &str, default: Vec2) -> Vec2 {
    p.get(key).and_then(point_value).unwrap_or(default)
}

pub fn point_req(cmd: &str, p: &Value, key: &str) -> Result<Vec2> {
    p.get(key)
        .and_then(point_value)
        .ok_or_else(|| bad(cmd, format!("`{key}` ([x, y]) is required")))
}

/// A positive size (a thickness, a diameter...): refused if present but not a positive finite
/// number, never clamped or made absolute (a bad value must never silently become a different,
/// "safe" one).
pub fn positive_req(cmd: &str, p: &Value, key: &str) -> Result<f64> {
    match p.get(key).and_then(Value::as_f64) {
        Some(v) if v.is_finite() && v > 0.0 => Ok(v),
        _ => Err(bad(cmd, format!("`{key}` must be a positive number"))),
    }
}
