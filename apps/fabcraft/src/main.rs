//! fabcraft desktop app: the full cadcraft CAD interface (`cadcraft_ui_egui::CadApp`), reused
//! wholesale and unmodified — drawing, dimensions, blocks, hatch, layouts, plotting, all of it —
//! plus a side panel of fabcraft's own fabrication commands.
//!
//! Why a side panel and not new entries in cadcraft's own menus/command line: cadcraft-engine's
//! command registry (`find_command`) and cadcraft-ui-egui's command dispatch
//! (`CadApp::run`/`cmdline`) are both sealed to cadcraft's own commands, with calls to them
//! spread across ~18 files inside the `cadcraft-ui-egui` crate — there is no single point to
//! make them fabcraft-aware without forking that crate, which the whole architecture (a pinned
//! git dependency, zero fork) rules out. `CadApp.session` is a public field, though, so this
//! panel calls `fab_engine::dispatch::execute` on it directly, bypassing `CadApp::run` — a clean
//! extension point that needed no changes to cadcraft-ui-egui at all.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![forbid(unsafe_code)]

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use serde_json::{Value, json};

struct FabApp {
    cad: CadApp,
    last_part: Option<String>,
    status: String,
}

impl FabApp {
    fn new() -> Self {
        let mut session = Session::new();
        session.open_drawing(cadcraft_engine::sample::default_sample(), "fabcraft", None);
        FabApp {
            cad: CadApp::new(
                session,
                Services {
                    pick_open: None,
                    pick_save: None,
                },
            ),
            last_part: None,
            status: String::new(),
        }
    }

    fn run(&mut self, id: &str, params: Value) {
        match fab_engine::dispatch::execute(&mut self.cad.session, id, &params) {
            Ok(v) => {
                if let Some(h) = v.get("handle").and_then(Value::as_str) {
                    self.last_part = Some(h.to_string());
                }
                self.status = format!("{id} -> {v}");
            }
            Err(e) => self.status = format!("{id}: {e}"),
        }
        self.cad.session.touch();
    }
}

impl eframe::App for FabApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.cad.logic(ctx);
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.cad.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::right("fabcraft_panel").resizable(false).show(ui, |ui| {
            ui.heading("fabcraft");
            ui.label("Desenhar peça (demo — parâmetros fixos nesta build):");
            ui.separator();
            if ui.button("Desenhar Chapa 100×100×5, 7 furos").clicked() {
                let holes: Vec<Value> = (0..7).map(|i| json!({"center": [10.0 + i as f64 * 12.0, 50.0], "diameter": 10.0})).collect();
                self.run("plate", json!({"width": 100.0, "height": 100.0, "thickness": 5.0, "material": "A36", "holes": holes}));
            }
            if ui.button("Desenhar Perfil U 100x50x3, 2000mm").clicked() {
                self.run("profile", json!({"length": 2000.0, "catalogRef": "U 100x50x3"}));
            }
            if ui.button("Desenhar Tubo QUAD 40x40x2, 1500mm").clicked() {
                self.run("tube", json!({"length": 1500.0, "catalogRef": "TUBO QUAD 40x40x2"}));
            }
            if ui.button("Desenhar Perfil Dobrado (cantoneira 50+50, 90°)").clicked() {
                self.run("bent-profile", json!({"flanges": [50.0, 50.0], "bendAngles": [90.0], "thickness": 1.5, "material": "Aço galvanizado"}));
            }
            ui.separator();
            ui.add_enabled_ui(self.last_part.is_some(), |ui| {
                if ui.button("Cotar Desenho (última peça)").clicked()
                    && let Some(h) = self.last_part.clone()
                {
                    self.run("fabdim", json!({"handle": h}));
                }
            });
            ui.separator();
            ui.label("Status:");
            ui.label(&self.status);
        });
        self.cad.ui(ui);
    }
    fn on_exit(&mut self) {
        self.cad.shutdown_autosave();
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("fabcraft")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([900.0, 560.0]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "fabcraft",
        options,
        Box::new(|_cc| Ok(Box::new(FabApp::new()))),
    )
}
