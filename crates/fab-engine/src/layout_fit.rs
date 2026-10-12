//! The sheet auto-layout algorithm (design spec §6, "estilo hero"): given everything that has to
//! appear on a fabrication drawing (views, dimensions, hole table, callouts), pick the smallest
//! sheet size and the largest (most legible) standard engineering scale that still fits it — not
//! a fixed A4, not a fixed scale ratio. This module is the decision only: a pure function over a
//! bounding box, with no drawing side effects, so it's cheap to test exhaustively.

use cadcraft_geom::Bounds2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetSize {
    pub name: &'static str,
    pub width_mm: f64,
    pub height_mm: f64,
}

/// ISO 216 series, smallest first. A real shop may want a different list (e.g. ANSI) — this is
/// configuration, not a hardcoded assumption baked into the algorithm below.
pub const SHEETS: &[SheetSize] = &[
    SheetSize {
        name: "A4",
        width_mm: 210.0,
        height_mm: 297.0,
    },
    SheetSize {
        name: "A3",
        width_mm: 297.0,
        height_mm: 420.0,
    },
    SheetSize {
        name: "A2",
        width_mm: 420.0,
        height_mm: 594.0,
    },
    SheetSize {
        name: "A1",
        width_mm: 594.0,
        height_mm: 841.0,
    },
];

/// Standard drawing-scale divisors (1:N), smallest (most zoomed in, most legible) first.
pub const STANDARD_SCALES: &[f64] = &[1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChosenSheet {
    pub sheet_name: &'static str,
    /// Paper size actually used, after orientation (may be `width_mm`/`height_mm` swapped).
    pub paper_width_mm: f64,
    pub paper_height_mm: f64,
    /// The drawing is at 1 : `scale_divisor` (model units assumed to be mm).
    pub scale_divisor: f64,
}

/// Picks the smallest sheet, and on it the smallest (clearest) standard scale, that fits
/// `content` inside `margin_mm` of every edge. `max_scale_divisor` is the legibility floor: a
/// scale coarser than this is rejected in favour of a bigger sheet. Tries both orientations of
/// each sheet. Falls back to the largest sheet at the coarsest standard scale rather than
/// returning nothing, if even that combination doesn't fit — degrade gracefully, never refuse to
/// produce a sheet at all. `None` only for genuinely unusable input (empty/non-finite content).
pub fn choose_sheet_and_scale(
    content: Bounds2,
    margin_mm: f64,
    max_scale_divisor: f64,
) -> Option<ChosenSheet> {
    if content.is_empty() {
        return None;
    }
    let content_w = content.width();
    let content_h = content.height();
    if !(content_w.is_finite()
        && content_h.is_finite()
        && margin_mm.is_finite()
        && margin_mm >= 0.0)
    {
        return None;
    }

    for sheet in SHEETS {
        for (pw, ph) in [
            (sheet.width_mm, sheet.height_mm),
            (sheet.height_mm, sheet.width_mm),
        ] {
            let usable_w = pw - 2.0 * margin_mm;
            let usable_h = ph - 2.0 * margin_mm;
            if usable_w <= 0.0 || usable_h <= 0.0 {
                continue;
            }
            // The scale divisor needed so the content's bounding box fits the usable area: a
            // content_w×content_h box drawn at 1:d occupies (content_w/d)×(content_h/d) on paper.
            let required =
                required_divisor(content_w, usable_w).max(required_divisor(content_h, usable_h));
            if let Some(&divisor) = STANDARD_SCALES.iter().find(|&&d| d >= required)
                && divisor <= max_scale_divisor
            {
                return Some(ChosenSheet {
                    sheet_name: sheet.name,
                    paper_width_mm: pw,
                    paper_height_mm: ph,
                    scale_divisor: divisor,
                });
            }
        }
    }
    // Nothing fit within the legibility floor: use the biggest sheet at the coarsest scale we
    // have, which is still a usable (if cramped) sheet, rather than producing no sheet at all.
    let last_sheet = SHEETS.last()?;
    let last_scale = *STANDARD_SCALES.last()?;
    Some(ChosenSheet {
        sheet_name: last_sheet.name,
        paper_width_mm: last_sheet.width_mm,
        paper_height_mm: last_sheet.height_mm,
        scale_divisor: last_scale,
    })
}

fn required_divisor(content_len: f64, usable_len: f64) -> f64 {
    if content_len <= 0.0 {
        return 1.0;
    }
    content_len / usable_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_geom::Vec2;

    fn content(w: f64, h: f64) -> Bounds2 {
        Bounds2::new(Vec2::ZERO, Vec2::new(w, h))
    }

    #[test]
    fn a_small_plate_fits_on_a4_at_1to1() {
        // The design spec's own example: a 100x100mm plate.
        let chosen = choose_sheet_and_scale(content(100.0, 100.0), 20.0, 50.0).expect("should fit");
        assert_eq!(chosen.sheet_name, "A4");
        assert_eq!(chosen.scale_divisor, 1.0);
    }

    #[test]
    fn a_long_profile_steps_up_to_a_bigger_sheet_or_scale() {
        // A 3-metre profile does not fit A4 at 1:1 — it must land on a bigger sheet, or a coarser
        // scale, or both, but it must still produce *something* sane (fits within the sheet).
        let chosen = choose_sheet_and_scale(content(3000.0, 150.0), 20.0, 200.0)
            .expect("should still produce a sheet");
        let usable_w = chosen.paper_width_mm - 40.0;
        assert!(
            3000.0 / chosen.scale_divisor <= usable_w + 1e-6,
            "content must fit the chosen sheet at the chosen scale"
        );
    }

    #[test]
    fn never_picks_a_sheet_smaller_than_necessary() {
        // A4 at 1:1 easily fits a 50x50 part — must not jump straight to A1.
        let chosen = choose_sheet_and_scale(content(50.0, 50.0), 10.0, 100.0).expect("should fit");
        assert_eq!(chosen.sheet_name, "A4");
    }

    #[test]
    fn degenerates_gracefully_on_hostile_input() {
        assert_eq!(choose_sheet_and_scale(Bounds2::EMPTY, 10.0, 50.0), None);
        // `Vec2::min`/`max` (inside `Bounds2::new`) follow IEEE 754 and quietly discard a NaN
        // operand, so a NaN *input coordinate* never survives into `Bounds2`. A genuinely
        // non-finite bounding box (the thing this guard exists for) has to be built directly.
        let infinite = Bounds2::new(Vec2::ZERO, Vec2::new(f64::INFINITY, 10.0));
        assert_eq!(choose_sheet_and_scale(infinite, 10.0, 50.0), None);
        assert_eq!(
            choose_sheet_and_scale(content(10.0, 10.0), f64::NAN, 50.0),
            None
        );
        assert_eq!(
            choose_sheet_and_scale(content(10.0, 10.0), -5.0, 50.0),
            None
        );
    }

    #[test]
    fn an_enormous_part_still_returns_a_usable_sheet_not_none() {
        // Degrades to the biggest sheet / coarsest scale rather than giving up.
        let chosen = choose_sheet_and_scale(content(1.0e9, 1.0e9), 10.0, 1.0)
            .expect("must still return something");
        assert_eq!(chosen.sheet_name, "A1");
    }
}
