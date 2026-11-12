//! Interfaz gráfica (egui).

use crate::bot::{simplify_moves, Bot, MissingAnchor, Mode};
use crate::model::{Action, Route, Step};
use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;
use std::sync::Arc;

pub struct App {
    bot: Arc<Bot>,
    routes: Vec<String>,
    textures: HashMap<std::path::PathBuf, Option<egui::TextureHandle>>,
    new_text: String,
    new_cmd: String,
    new_wait: u64,
    last_mode: Mode,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, bot: Arc<Bot>) -> Self {
        bot.set_repaint(cc.egui_ctx.clone());
        Self {
            bot,
            routes: Route::list(),
            textures: HashMap::new(),
            new_text: "/home".into(),
            new_cmd: String::new(),
            new_wait: 1000,
            last_mode: Mode::Idle,
        }
    }

    fn anchor_texture(&mut self, ctx: &egui::Context, path: std::path::PathBuf) -> Option<egui::TextureHandle> {
        self.textures
            .entry(path.clone())
            .or_insert_with(|| {
                let img = image::open(&path).ok()?.to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                Some(ctx.load_texture(path.to_string_lossy(), color, Default::default()))
            })
            .clone()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let bot = self.bot.clone();
        let mode = bot.mode();
        if mode != self.last_mode {
            self.routes = Route::list(); // p.ej. tras grabar con F9
            self.textures.clear();
            self.last_mode = mode;
        }

        // ── Barra superior: controles ──
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (txt, col) = match mode {
                    Mode::Idle => ("EN ESPERA", Color32::GRAY),
                    Mode::Recording => ("⏺ GRABANDO", Color32::from_rgb(230, 60, 60)),
                    Mode::Playing => ("▶ REPRODUCIENDO", Color32::from_rgb(60, 200, 90)),
                };
                ui.label(RichText::new(txt).color(col).strong().size(16.0));
                ui.separator();
                let rec_label = if mode == Mode::Recording { "⏹ Detener grabación (F9)" } else { "⏺ Grabar (F9)" };
                if ui.add_enabled(mode != Mode::Playing, egui::Button::new(rec_label)).clicked() {
                    bot.toggle_record(true);
                    self.routes = Route::list();
                }
                let play_label = if mode == Mode::Playing { "⏹ Parar (F10)" } else { "▶ Reproducir (F10)" };
                if ui.add_enabled(mode != Mode::Recording, egui::Button::new(play_label)).clicked() {
                    bot.toggle_play();
                }
                if ui.button(RichText::new("✋ PARADA DE EMERGENCIA (F12)").color(Color32::from_rgb(230, 60, 60))).clicked() {
                    bot.stop_all();
                }
            });
            ui.label(RichText::new(bot.state.lock().unwrap().progress.clone()).italics());
            ui.add_space(4.0);
        });

        // ── Registro inferior ──
        egui::TopBottomPanel::bottom("log").resizable(true).default_height(120.0).show(ctx, |ui| {
            ui.label(RichText::new("Registro").strong());
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for line in bot.state.lock().unwrap().log.iter() {
                    ui.monospace(line);
                }
            });
        });

        // ── Panel izquierdo: rutas + ajustes ──
        egui::SidePanel::left("routes").default_width(260.0).show(ctx, |ui| {
            ui.heading("Rutas");
            ui.horizontal(|ui| {
                ui.label("Nombre al grabar:");
                ui.text_edit_singleline(&mut bot.state.lock().unwrap().record_name);
            });
            if ui.button("⟳ Refrescar lista").clicked() {
                self.routes = Route::list();
            }
            egui::ScrollArea::vertical().id_salt("rlist").max_height(160.0).show(ui, |ui| {
                let current = bot.state.lock().unwrap().route.name.clone();
                for name in self.routes.clone() {
                    if ui.selectable_label(current == name, &name).clicked() && mode == Mode::Idle {
                        match Route::load(&name) {
                            Ok(r) => {
                                let mut st = bot.state.lock().unwrap();
                                st.record_name = r.name.clone();
                                st.route = r;
                                drop(st);
                                bot.log(format!("Ruta '{name}' cargada."));
                            }
                            Err(e) => bot.log(format!("Error cargando '{name}': {e}")),
                        }
                    }
                }
            });

            ui.separator();
            ui.heading("Ajustes");
            let mut st = bot.state.lock().unwrap();
            let s = &mut st.settings;
            ui.add(egui::Slider::new(&mut s.speed, 0.25..=4.0).text("Velocidad ×"));
            ui.add(egui::DragValue::new(&mut s.loops).prefix("Repeticiones: ").suffix("  (0 = ∞)"));
            ui.add(egui::DragValue::new(&mut s.pause_between_loops_ms).prefix("Pausa entre vueltas: ").suffix(" ms"));
            ui.checkbox(&mut s.humanize, "Humanizar (tiempos y posición aleatorios)");
            ui.checkbox(&mut s.record_moves, "Grabar movimientos del ratón");
            ui.separator();
            ui.label(RichText::new("Visión (anclas visuales)").strong());
            ui.checkbox(&mut s.use_anchors, "Verificar pantalla antes de cada clic");
            ui.add(egui::Slider::new(&mut s.threshold, 0.5..=1.0).text("Umbral de similitud"));
            ui.add(egui::Slider::new(&mut s.search_radius, 0..=400).text("Radio de búsqueda px"));
            ui.add(egui::DragValue::new(&mut s.anchor_timeout_ms).prefix("Esperar ancla hasta: ").suffix(" ms"));
            ui.horizontal(|ui| {
                ui.label("Si no aparece:");
                ui.radio_value(&mut s.on_missing, MissingAnchor::ClickAnyway, "clicar igual");
                ui.radio_value(&mut s.on_missing, MissingAnchor::Stop, "detener");
            });
        });

        // ── Centro: editor de pasos ──
        egui::CentralPanel::default().show(ctx, |ui| {
            let editable = mode == Mode::Idle;
            let (name, n, secs) = {
                let st = bot.state.lock().unwrap();
                (st.route.name.clone(), st.route.steps.len(), st.route.duration_ms() as f64 / 1000.0)
            };
            ui.heading(format!("Ruta: {name}  ·  {n} pasos  ·  {secs:.1} s"));

            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("💾 Guardar").clicked() {
                        let r = bot.state.lock().unwrap().route.clone();
                        match r.save() {
                            Ok(()) => bot.log(format!("Ruta '{}' guardada.", r.name)),
                            Err(e) => bot.log(format!("Error: {e}")),
                        }
                        self.routes = Route::list();
                    }
                    if ui.button("✂ Simplificar movimientos").on_hover_text("Quita movimientos de ratón intermedios").clicked() {
                        let mut st = bot.state.lock().unwrap();
                        let before = st.route.steps.len();
                        simplify_moves(&mut st.route.steps);
                        let after = st.route.steps.len();
                        drop(st);
                        bot.log(format!("Simplificada: {before} → {after} pasos (pulsa Guardar)."));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.text_edit_singleline(&mut self.new_text);
                    if ui.button("+ Escribir texto").on_hover_text("Ej: comando de chat del juego").clicked() {
                        push(&bot, Action::TypeText { text: self.new_text.clone() });
                    }
                    if ui.button("+ Enter").clicked() {
                        push(&bot, Action::KeyDown { key: rdev::Key::Return });
                        push(&bot, Action::KeyUp { key: rdev::Key::Return });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_cmd).hint_text("notepad.exe / firefox / steam://…"));
                    if ui.button("+ Ejecutar programa").clicked() && !self.new_cmd.trim().is_empty() {
                        push(&bot, Action::RunCommand { command: self.new_cmd.clone() });
                    }
                    ui.add(egui::DragValue::new(&mut self.new_wait).suffix(" ms"));
                    if ui.button("+ Esperar").clicked() {
                        push(&bot, Action::Wait { ms: self.new_wait });
                    }
                });
            });
            ui.separator();

            let route_name = bot.state.lock().unwrap().route.name.clone();
            let mut delete = None;
            let mut swap = None;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let steps: Vec<Step> = bot.state.lock().unwrap().route.steps.clone();
                let mut changed: Vec<(usize, Step)> = vec![];
                for (i, step) in steps.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(editable, |ui| {
                            let mut s = step.clone();
                            ui.monospace(format!("{:>4}", i + 1));
                            let mut edited = ui.add(egui::DragValue::new(&mut s.delay_ms).suffix(" ms").speed(5)).changed();
                            match &mut s.action {
                                Action::TypeText { text } => {
                                    ui.label("Escribir:");
                                    edited |= ui.text_edit_singleline(text).changed();
                                }
                                Action::RunCommand { command } => {
                                    ui.label("Ejecutar:");
                                    edited |= ui.text_edit_singleline(command).changed();
                                }
                                Action::Wait { ms } => {
                                    ui.label("Esperar");
                                    edited |= ui.add(egui::DragValue::new(ms).suffix(" ms")).changed();
                                }
                                Action::ButtonDown { anchor: Some(file), .. } => {
                                    let path = Route::dir(&route_name).join(file.as_str());
                                    let r = ui.label(step.action.describe());
                                    if let Some(tex) = self.anchor_texture(ctx, path) {
                                        r.on_hover_ui(|ui| {
                                            ui.label("Lo que el bot buscará en pantalla:");
                                            ui.image((tex.id(), egui::vec2(144.0, 144.0)));
                                        });
                                    }
                                }
                                other => {
                                    ui.label(other.describe());
                                }
                            }
                            if edited {
                                changed.push((i, s.clone()));
                            }
                            if ui.small_button("⬆").clicked() && i > 0 {
                                swap = Some(i);
                            }
                            if ui.small_button("🗑").clicked() {
                                delete = Some(i);
                            }
                        });
                    });
                }
                if !changed.is_empty() {
                    let mut st = bot.state.lock().unwrap();
                    for (i, s) in changed {
                        st.route.steps[i] = s;
                    }
                }
            });
            let mut st = bot.state.lock().unwrap();
            if let Some(i) = delete {
                st.route.steps.remove(i);
            }
            if let Some(i) = swap {
                st.route.steps.swap(i, i - 1);
            }
        });
    }
}

fn push(bot: &Bot, action: Action) {
    bot.state.lock().unwrap().route.steps.push(Step { delay_ms: 300, action });
}
