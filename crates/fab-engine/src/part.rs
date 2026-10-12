//! The Part model: what fabcraft needs to know about a piece of stock to dimension and lay it
//! out automatically. Storage decision (v1): a Part is a cadcraft block instance whose attributes
//! carry these fields — see the design spec, section 3, for why (reuses cadcraft's existing
//! block/attribute machinery and stays readable by any other DXF/DWG-reading CAD). Nothing outside
//! this module reads block attributes directly, so the storage mechanism can change later without
//! touching the dimensioning/table/layout code that consumes `PartData`.

use cadcraft_doc::{EntityKind, Insert};

/// Attribute tags fabcraft writes/reads on a Part's block. Kept short (DXF tag convention).
pub const TAG_KIND: &str = "FABKIND";
pub const TAG_THICKNESS: &str = "FABTHICK";
pub const TAG_MATERIAL: &str = "FABMATERIAL";
pub const TAG_CATALOG: &str = "FABCATALOG";
pub const TAG_BEND_RADIUS: &str = "FABBENDR";
pub const TAG_K_FACTOR: &str = "FABKFACTOR";

#[derive(Clone, Debug, PartialEq)]
pub enum PartKind {
    /// A flat plate: `thickness` in drawing units.
    Plate { thickness: f64 },
    /// A structural profile (beam/angle/channel). `catalog_ref` names a row in the profile
    /// catalog (§ catalog.rs); `None` means the draftsman drew/dimensioned the section freely.
    Profile { catalog_ref: Option<String> },
    /// A round or rectangular hollow section.
    Tube { catalog_ref: Option<String> },
    /// A sheet-metal part with one or more bends; `thickness`, `bend_radius` and `k_factor` are
    /// exactly what `bent_profile::flat_length` needs — persisted here because, unlike flange
    /// lengths and bend angles (recoverable from the folded polyline's own geometry), they leave
    /// no geometric trace to read back later.
    BentProfile {
        thickness: f64,
        bend_radius: f64,
        k_factor: f64,
    },
}

impl PartKind {
    pub fn tag_value(&self) -> &'static str {
        match self {
            PartKind::Plate { .. } => "PLATE",
            PartKind::Profile { .. } => "PROFILE",
            PartKind::Tube { .. } => "TUBE",
            PartKind::BentProfile { .. } => "BENT",
        }
    }

    /// Builds a `PartKind` from the same tag strings `tag_value`/the attributes use. Public (at
    /// crate scope) so `fabclassify` can turn already-drawn freehand geometry into a Part from
    /// the same single set of fields the smart tools fill in automatically — see the design
    /// spec §3's "single question" classification path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_tag(
        tag: &str,
        thickness: f64,
        catalog_ref: Option<String>,
        bend_radius: f64,
        k_factor: f64,
    ) -> Option<PartKind> {
        match tag {
            "PLATE" => Some(PartKind::Plate { thickness }),
            "PROFILE" => Some(PartKind::Profile { catalog_ref }),
            "TUBE" => Some(PartKind::Tube { catalog_ref }),
            "BENT" => Some(PartKind::BentProfile {
                thickness,
                bend_radius,
                k_factor,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartData {
    pub kind: PartKind,
    pub material: String,
}

impl PartData {
    pub fn new(kind: PartKind, material: impl Into<String>) -> Self {
        PartData {
            kind,
            material: material.into(),
        }
    }

    /// The attribute tag/value pairs a smart command should attach (as ATTDEF defaults) when it
    /// builds a Part's block. Order doesn't matter; values are plain text, readable by any CAD.
    pub fn attdef_values(&self) -> Vec<(&'static str, String)> {
        let mut v = vec![
            (TAG_KIND, self.kind.tag_value().to_string()),
            (TAG_MATERIAL, self.material.clone()),
        ];
        match &self.kind {
            PartKind::Plate { thickness } => {
                v.push((TAG_THICKNESS, format!("{thickness}")));
            }
            PartKind::BentProfile {
                thickness,
                bend_radius,
                k_factor,
            } => {
                v.push((TAG_THICKNESS, format!("{thickness}")));
                v.push((TAG_BEND_RADIUS, format!("{bend_radius}")));
                v.push((TAG_K_FACTOR, format!("{k_factor}")));
            }
            PartKind::Profile { catalog_ref } | PartKind::Tube { catalog_ref } => {
                v.push((TAG_CATALOG, catalog_ref.clone().unwrap_or_default()));
            }
        }
        v
    }

    /// Read `PartData` back from an already-placed block reference (Insert). `None` means this
    /// Insert isn't a fabcraft Part — e.g. plain freehand geometry, or an ordinary cadcraft block.
    pub fn from_insert(insert: &Insert) -> Option<PartData> {
        let get = |tag: &str| {
            insert
                .attribs
                .iter()
                .find(|a| a.tag == tag)
                .map(|a| a.text.value.clone())
        };
        let kind_tag = get(TAG_KIND)?;
        let parse = |tag: &str, default: f64| {
            get(tag)
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(default)
        };
        let thickness = parse(TAG_THICKNESS, 0.0);
        let bend_radius = parse(TAG_BEND_RADIUS, thickness);
        let k_factor = parse(TAG_K_FACTOR, 0.33);
        let catalog_ref = get(TAG_CATALOG).filter(|s| !s.is_empty());
        let kind = PartKind::from_tag(&kind_tag, thickness, catalog_ref, bend_radius, k_factor)?;
        let material = get(TAG_MATERIAL).unwrap_or_default();
        Some(PartData { kind, material })
    }

    /// `from_insert`, given any entity: `None` for anything that isn't an Insert, or isn't a Part.
    pub fn from_entity(kind: &EntityKind) -> Option<PartData> {
        match kind {
            EntityKind::Insert(ins) => PartData::from_insert(ins),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{Attrib, HAlign, Text, VAlign};
    use cadcraft_geom::Vec3;

    fn attrib(tag: &str, value: &str) -> Attrib {
        Attrib {
            tag: tag.into(),
            text: Text {
                insert: Vec3::ZERO,
                align_pt: None,
                height: 2.5,
                value: value.into(),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: HAlign::Left,
                valign: VAlign::Baseline,
            },
            invisible: true,
            constant: false,
            prompt: String::new(),
            props: Default::default(),
        }
    }

    fn insert_with(attribs: Vec<Attrib>) -> Insert {
        Insert {
            block: "FABPART_1".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs,
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }
    }

    #[test]
    fn round_trips_a_plate() {
        let part = PartData::new(PartKind::Plate { thickness: 5.0 }, "A36");
        let attribs: Vec<Attrib> = part
            .attdef_values()
            .into_iter()
            .map(|(tag, val)| attrib(tag, &val))
            .collect();
        let back = PartData::from_insert(&insert_with(attribs));
        assert_eq!(back, Some(part));
    }

    #[test]
    fn round_trips_a_bent_profile() {
        let part = PartData::new(
            PartKind::BentProfile {
                thickness: 1.5,
                bend_radius: 2.0,
                k_factor: 0.4,
            },
            "Aço galvanizado",
        );
        let attribs: Vec<Attrib> = part
            .attdef_values()
            .into_iter()
            .map(|(tag, val)| attrib(tag, &val))
            .collect();
        let back = PartData::from_insert(&insert_with(attribs));
        assert_eq!(back, Some(part));
    }

    #[test]
    fn a_plain_block_is_not_a_part() {
        // No FABKIND attribute at all — an ordinary block, not something fabcraft created.
        assert_eq!(
            PartData::from_insert(&insert_with(vec![attrib("OTHER", "x")])),
            None
        );
    }

    #[test]
    fn an_unknown_kind_tag_is_rejected_not_guessed() {
        // Hostile/corrupted data (e.g. hand-edited DXF): never panics, never invents a kind.
        assert_eq!(
            PartData::from_insert(&insert_with(vec![attrib(TAG_KIND, "NOT_A_REAL_KIND")])),
            None
        );
    }
}
