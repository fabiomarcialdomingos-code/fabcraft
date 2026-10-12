//! Parameter forms for the fabcraft side panel — one small struct per smart command, each
//! owning its own egui widgets and knowing how to turn itself into the JSON params
//! `fab_engine::dispatch::execute` expects. Replaces the fixed-demo-parameter buttons the first
//! GUI cut shipped with.

use serde_json::{Value, json};

#[derive(Clone)]
pub struct Hole {
    pub x: f64,
    pub y: f64,
    pub diameter: f64,
}

pub struct PlateForm {
    pub width: f64,
    pub height: f64,
    pub thickness: f64,
    pub material: String,
    pub holes: Vec<Hole>,
}

impl Default for PlateForm {
    fn default() -> Self {
        PlateForm {
            width: 100.0,
            height: 100.0,
            thickness: 5.0,
            material: "A36".into(),
            holes: Vec::new(),
        }
    }
}

impl PlateForm {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Largura");
        ui.add(
            egui::DragValue::new(&mut self.width)
                .range(1.0..=100_000.0)
                .suffix(" mm"),
        );
        ui.label("Altura");
        ui.add(
            egui::DragValue::new(&mut self.height)
                .range(1.0..=100_000.0)
                .suffix(" mm"),
        );
        ui.label("Espessura");
        ui.add(
            egui::DragValue::new(&mut self.thickness)
                .range(0.1..=1000.0)
                .suffix(" mm"),
        );
        ui.label("Material");
        ui.text_edit_singleline(&mut self.material);
        ui.label("Furos (centro X, Y e diâmetro):");
        let mut remove = None;
        for (i, hole) in self.holes.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut hole.x).prefix("x: "));
                ui.add(egui::DragValue::new(&mut hole.y).prefix("y: "));
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut hole.diameter)
                        .range(0.1..=10_000.0)
                        .prefix("Ø "),
                );
                if ui.small_button("remover").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.holes.remove(i);
        }
        if ui.button("+ furo").clicked() {
            let (cx, cy) = (self.width * 0.5, self.height * 0.5);
            self.holes.push(Hole {
                x: cx,
                y: cy,
                diameter: 10.0,
            });
        }
    }

    pub fn params(&self) -> Value {
        let holes: Vec<Value> = self
            .holes
            .iter()
            .map(|h| json!({"center": [h.x, h.y], "diameter": h.diameter}))
            .collect();
        json!({"width": self.width, "height": self.height, "thickness": self.thickness, "material": self.material, "holes": holes})
    }
}

pub struct SectionForm {
    pub length: f64,
    pub use_catalog: bool,
    pub catalog_idx: usize,
    pub free_height: f64,
    pub material: String,
}

impl Default for SectionForm {
    fn default() -> Self {
        SectionForm {
            length: 2000.0,
            use_catalog: true,
            catalog_idx: 0,
            free_height: 50.0,
            material: String::new(),
        }
    }
}

impl SectionForm {
    pub fn ui(&mut self, ui: &mut egui::Ui, id_salt: &str) {
        ui.horizontal(|ui| {
            ui.label("Comprimento");
            ui.add(
                egui::DragValue::new(&mut self.length)
                    .range(1.0..=100_000.0)
                    .suffix(" mm"),
            );
        });
        ui.checkbox(&mut self.use_catalog, "Seção do catálogo");
        let names = fab_engine::catalog::names();
        if self.use_catalog && !names.is_empty() {
            self.catalog_idx = self.catalog_idx.min(names.len() - 1);
            egui::ComboBox::from_id_salt(id_salt)
                .selected_text(names[self.catalog_idx])
                .show_ui(ui, |ui| {
                    for (i, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.catalog_idx, i, *name);
                    }
                });
        } else {
            ui.horizontal(|ui| {
                ui.label("Altura da seção (livre)");
                ui.add(
                    egui::DragValue::new(&mut self.free_height)
                        .range(0.1..=10_000.0)
                        .suffix(" mm"),
                );
            });
        }
        ui.horizontal(|ui| {
            ui.label("Material");
            ui.text_edit_singleline(&mut self.material);
        });
    }

    pub fn params(&self) -> Value {
        let mut p = json!({"length": self.length, "material": self.material});
        if self.use_catalog {
            if let Some(name) = fab_engine::catalog::names().get(self.catalog_idx) {
                p["catalogRef"] = json!(name);
            }
        } else {
            p["height"] = json!(self.free_height);
        }
        p
    }
}

pub struct BentForm {
    pub flanges: String,
    pub bend_angles: String,
    pub thickness: f64,
    pub bend_radius: f64,
    pub k_factor: f64,
    pub material: String,
}

impl Default for BentForm {
    fn default() -> Self {
        BentForm {
            flanges: "50, 50".into(),
            bend_angles: "90".into(),
            thickness: 1.5,
            bend_radius: 1.5,
            k_factor: 0.33,
            material: String::new(),
        }
    }
}

fn parse_list(s: &str) -> Vec<f64> {
    s.split(',')
        .filter_map(|t| t.trim().parse::<f64>().ok())
        .collect()
}

impl BentForm {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Abas (mm, separadas por vírgula)");
        ui.text_edit_singleline(&mut self.flanges);
        ui.label("Ângulos de dobra (°, separados por vírgula)");
        ui.text_edit_singleline(&mut self.bend_angles);
        ui.label("Espessura");
        ui.add(
            egui::DragValue::new(&mut self.thickness)
                .range(0.1..=1000.0)
                .suffix(" mm"),
        );
        ui.label("Raio de dobra");
        ui.add(
            egui::DragValue::new(&mut self.bend_radius)
                .range(0.0..=1000.0)
                .suffix(" mm"),
        );
        ui.label("K-factor");
        ui.add(
            egui::DragValue::new(&mut self.k_factor)
                .range(0.0..=1.0)
                .speed(0.01),
        );
        ui.label("Material");
        ui.text_edit_singleline(&mut self.material);
        let flanges = parse_list(&self.flanges);
        let bends = parse_list(&self.bend_angles);
        if flanges.len() != bends.len() + 1 {
            ui.colored_label(
                egui::Color32::from_rgb(200, 120, 0),
                format!(
                    "{} abas precisam de exatamente {} ângulo(s) — tem {}",
                    flanges.len(),
                    flanges.len().saturating_sub(1),
                    bends.len()
                ),
            );
        }
    }

    pub fn params(&self) -> Value {
        json!({
            "flanges": parse_list(&self.flanges),
            "bendAngles": parse_list(&self.bend_angles),
            "thickness": self.thickness,
            "bendRadius": self.bend_radius,
            "kFactor": self.k_factor,
            "material": self.material,
        })
    }
}

pub struct ClassifyForm {
    pub kind_idx: usize,
    pub thickness: f64,
    pub material: String,
    pub catalog_ref: String,
    pub bend_radius: f64,
    pub k_factor: f64,
}

pub const CLASSIFY_KINDS: [&str; 4] = ["PLATE", "PROFILE", "TUBE", "BENT"];

impl Default for ClassifyForm {
    fn default() -> Self {
        ClassifyForm {
            kind_idx: 0,
            thickness: 5.0,
            material: String::new(),
            catalog_ref: String::new(),
            bend_radius: 5.0,
            k_factor: 0.33,
        }
    }
}

impl ClassifyForm {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("fabclassify_kind")
            .selected_text(CLASSIFY_KINDS[self.kind_idx])
            .show_ui(ui, |ui| {
                for (i, k) in CLASSIFY_KINDS.iter().enumerate() {
                    ui.selectable_value(&mut self.kind_idx, i, *k);
                }
            });
        let kind = CLASSIFY_KINDS[self.kind_idx];
        if matches!(kind, "PLATE" | "BENT") {
            ui.label("Espessura");
            ui.add(
                egui::DragValue::new(&mut self.thickness)
                    .range(0.1..=1000.0)
                    .suffix(" mm"),
            );
        }
        if kind == "BENT" {
            ui.label("Raio de dobra");
            ui.add(
                egui::DragValue::new(&mut self.bend_radius)
                    .range(0.0..=1000.0)
                    .suffix(" mm"),
            );
            ui.label("K-factor");
            ui.add(
                egui::DragValue::new(&mut self.k_factor)
                    .range(0.0..=1.0)
                    .speed(0.01),
            );
        }
        if matches!(kind, "PROFILE" | "TUBE") {
            ui.label("Catálogo (opcional)");
            ui.text_edit_singleline(&mut self.catalog_ref);
        }
        ui.label("Material");
        ui.text_edit_singleline(&mut self.material);
    }

    pub fn params(&self, handles: Vec<String>) -> Value {
        let mut p = json!({"handles": handles, "kind": CLASSIFY_KINDS[self.kind_idx], "material": self.material});
        let kind = CLASSIFY_KINDS[self.kind_idx];
        if matches!(kind, "PLATE" | "BENT") {
            p["thickness"] = json!(self.thickness);
        }
        if kind == "BENT" {
            p["bendRadius"] = json!(self.bend_radius);
            p["kFactor"] = json!(self.k_factor);
        }
        if matches!(kind, "PROFILE" | "TUBE") && !self.catalog_ref.is_empty() {
            p["catalogRef"] = json!(self.catalog_ref);
        }
        p
    }
}
