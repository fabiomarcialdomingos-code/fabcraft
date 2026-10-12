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

const CATALOG: &[(&str, ProfileDims)] = &[
    (
        "U 100x50x3",
        ProfileDims {
            width: 50.0,
            height: 100.0,
            thickness: 3.0,
        },
    ),
    (
        "U 150x50x3",
        ProfileDims {
            width: 50.0,
            height: 150.0,
            thickness: 3.0,
        },
    ),
    (
        "U 200x75x3",
        ProfileDims {
            width: 75.0,
            height: 200.0,
            thickness: 3.0,
        },
    ),
    (
        "TUBO QUAD 40x40x2",
        ProfileDims {
            width: 40.0,
            height: 40.0,
            thickness: 2.0,
        },
    ),
    (
        "TUBO QUAD 50x50x2",
        ProfileDims {
            width: 50.0,
            height: 50.0,
            thickness: 2.0,
        },
    ),
    (
        "TUBO RET 60x40x2",
        ProfileDims {
            width: 60.0,
            height: 40.0,
            thickness: 2.0,
        },
    ),
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
}
