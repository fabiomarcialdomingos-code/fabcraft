//! A small table of standard structural-profile dimensions. Sourced from public engineering
//! references (manufacturer dimension tables, standard shape designations) — never copied from
//! any commercial CAD's bundled library (see the design spec §8). `profile`/`tube` always accept
//! a free/uncatalogued section too ("Perfil livre") — this catalog exists to save time, not to
//! gate what can be drawn. Meant to grow; this starting set is illustrative, not complete.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileDims {
    /// Overall width across the section (mm or drawing units — whatever the drawing uses).
    pub width: f64,
    /// Overall height/depth of the section.
    pub height: f64,
    /// Wall or flange thickness.
    pub thickness: f64,
}

macro_rules! dims {
    ($w:expr, $h:expr, $t:expr) => {
        ProfileDims {
            width: $w,
            height: $h,
            thickness: $t,
        }
    };
}

const CATALOG: &[(&str, ProfileDims)] = &[
    // U (channel) sections.
    ("U 75x40x2.25", dims!(40.0, 75.0, 2.25)),
    ("U 100x50x3", dims!(50.0, 100.0, 3.0)),
    ("U 127x64x3.5", dims!(64.0, 127.0, 3.5)),
    ("U 150x50x3", dims!(50.0, 150.0, 3.0)),
    ("U 152x76x4", dims!(76.0, 152.0, 4.0)),
    ("U 200x75x3", dims!(75.0, 200.0, 3.0)),
    ("U 254x76x4.5", dims!(76.0, 254.0, 4.5)),
    // I/W (wide-flange beam) sections — `width` is the overall section depth here (the elevation
    // footprint's thin dimension), `thickness` the flange thickness; the web/flange-width detail
    // isn't modelled in this illustrative v1 catalog.
    ("W 150x13", dims!(100.0, 150.0, 7.0)),
    ("W 200x15", dims!(100.0, 200.0, 6.5)),
    ("W 250x18", dims!(101.0, 251.0, 6.9)),
    // L (equal-leg angle) sections.
    ("L 50x50x5", dims!(50.0, 50.0, 5.0)),
    ("L 76x76x6", dims!(76.0, 76.0, 6.0)),
    ("L 102x102x8", dims!(102.0, 102.0, 8.0)),
    // Square and rectangular hollow sections (tube).
    ("TUBO QUAD 20x20x1.5", dims!(20.0, 20.0, 1.5)),
    ("TUBO QUAD 30x30x2", dims!(30.0, 30.0, 2.0)),
    ("TUBO QUAD 40x40x2", dims!(40.0, 40.0, 2.0)),
    ("TUBO QUAD 50x50x2", dims!(50.0, 50.0, 2.0)),
    ("TUBO QUAD 50x50x3", dims!(50.0, 50.0, 3.0)),
    ("TUBO QUAD 60x60x3", dims!(60.0, 60.0, 3.0)),
    ("TUBO QUAD 80x80x3", dims!(80.0, 80.0, 3.0)),
    ("TUBO RET 40x20x1.5", dims!(40.0, 20.0, 1.5)),
    ("TUBO RET 60x40x2", dims!(60.0, 40.0, 2.0)),
    ("TUBO RET 80x40x3", dims!(80.0, 40.0, 3.0)),
    ("TUBO RET 100x50x3", dims!(100.0, 50.0, 3.0)),
];

pub fn lookup(name: &str) -> Option<ProfileDims> {
    CATALOG
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, d)| *d)
}

pub fn names() -> Vec<&'static str> {
    CATALOG.iter().map(|(n, _)| *n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_case_insensitively() {
        assert_eq!(lookup("u 100x50x3"), lookup("U 100x50x3"));
        assert!(lookup("U 100x50x3").is_some());
    }

    #[test]
    fn unknown_name_is_none_not_a_panic() {
        assert_eq!(lookup("NOT A REAL PROFILE"), None);
        assert_eq!(lookup(""), None);
    }

    #[test]
    fn covers_channel_beam_angle_and_tube_families() {
        for name in [
            "U 150x50x3",
            "W 200x15",
            "L 76x76x6",
            "TUBO QUAD 50x50x2",
            "TUBO RET 60x40x2",
        ] {
            assert!(lookup(name).is_some(), "{name} should be in the catalog");
        }
        assert!(names().len() >= 20);
    }
}
