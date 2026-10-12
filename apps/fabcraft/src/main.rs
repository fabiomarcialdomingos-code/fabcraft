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

mod forms;

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use serde_json::{Value, json};

struct FabApp {
    cad: CadApp,
    last_part: Option<String>,
    status: String,
    plate: forms::PlateForm,
    profile: forms::SectionForm,
    tube: forms::SectionForm,
    bent: forms::BentForm,
    classify: forms::ClassifyForm,
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
            plate: forms::PlateForm::default(),
            profile: forms::SectionForm::default(),
            tube: forms::SectionForm::default(),
            bent: forms::BentForm::default(),
            classify: forms::ClassifyForm::default(),
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
        egui::Panel::right("fabcraft_panel")
            .resizable(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("fabcraft");
                    ui.add_enabled_ui(self.last_part.is_some(), |ui| {
                        if ui.button("Cotar Desenho (última peça)").clicked()
                            && let Some(h) = self.last_part.clone()
                        {
                            self.run("fabdim", json!({"handle": h}));
                        }
                    });
                    ui.separator();

                    egui::CollapsingHeader::new("Desenhar Chapa")
                        .default_open(true)
                        .show(ui, |ui| {
                            self.plate.ui(ui);
                            if ui.button("Desenhar").clicked() {
                                let params = self.plate.params();
                                self.run("plate", params);
                            }
                        });

                    egui::CollapsingHeader::new("Desenhar Perfil").show(ui, |ui| {
                        self.profile.ui(ui, "fab_profile_catalog");
                        if ui.button("Desenhar").clicked() {
                            let params = self.profile.params();
                            self.run("profile", params);
                        }
                    });

                    egui::CollapsingHeader::new("Desenhar Tubo").show(ui, |ui| {
                        self.tube.ui(ui, "fab_tube_catalog");
                        if ui.button("Desenhar").clicked() {
                            let params = self.tube.params();
                            self.run("tube", params);
                        }
                    });

                    egui::CollapsingHeader::new("Desenhar Perfil Dobrado").show(ui, |ui| {
                        self.bent.ui(ui);
                        if ui.button("Desenhar").clicked() {
                            let params = self.bent.params();
                            self.run("bent-profile", params);
                        }
                    });

                    egui::CollapsingHeader::new("Classificar seleção").show(ui, |ui| {
                        let n = self.cad.session.selection().len();
                        ui.label(format!("{n} objeto(s) selecionado(s) no desenho"));
                        self.classify.ui(ui);
                        ui.add_enabled_ui(n > 0, |ui| {
                            if ui.button("Classificar").clicked() {
                                let handles: Vec<String> = self
                                    .cad
                                    .session
                                    .selection()
                                    .iter()
                                    .map(|h| h.hex())
                                    .collect();
                                let params = self.classify.params(handles);
                                self.run("fabclassify", params);
                            }
                        });
                    });

                    ui.separator();
                    ui.label("Status:");
                    ui.label(&self.status);
                });
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
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([760.0, 480.0])
            .with_position([10.0, 10.0]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "fabcraft",
        options,
        Box::new(|_cc| Ok(Box::new(FabApp::new()))),
    )
}
