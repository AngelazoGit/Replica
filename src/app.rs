//! Interfaz gráfica (egui) con la estética de Replica.

use crate::bot::{simplify_moves, Bot, MissingAnchor, Mode};
use crate::model::{Action, Route, Step, Target};
use crate::theme::{self, BONE, CORAL, DIM, LINE, PANEL_HI, PHOSPHOR, SODIUM, UV, VOID};
use eframe::egui::{self, Color32, Margin, RichText, Rounding, Stroke};
use std::collections::HashMap;
use std::sync::Arc;

pub struct App {
    bot: Arc<Bot>,
    routes: Vec<String>,
    textures: HashMap<std::path::PathBuf, egui::TextureHandle>,
    new_text: String,
    new_cmd: String,
    new_wait: u64,
    last_mode: Mode,
    windows: Vec<Target>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, bot: Arc<Bot>) -> Self {
        theme::apply(&cc.egui_ctx);
        bot.set_repaint(cc.egui_ctx.clone());
        Self {
            bot,
            routes: Route::list(),
            textures: HashMap::new(),
            new_text: "/home".into(),
            new_cmd: String::new(),
            new_wait: 1000,
            last_mode: Mode::Idle,
            windows: vec![],
        }
    }

    fn anchor_texture(&mut self, ctx: &egui::Context, path: std::path::PathBuf) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.textures.get(&path) {
            return Some(tex.clone());
        }
        // Sin caché si falta: el hilo de anclas puede escribir el PNG un instante después.
        let img = image::open(&path).ok()?.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        let tex = ctx.load_texture(path.to_string_lossy(), color, Default::default());
        self.textures.insert(path, tex.clone());
        Some(tex)
    }
}

/// Etiqueta corta y color para cada tipo de paso.
fn tag(a: &Action) -> (&'static str, Color32) {
    match a {
        Action::MouseMove { .. } => ("MOV ", DIM),
        Action::ButtonDown { .. } | Action::ButtonUp { .. } => ("CLIC", SODIUM),
        Action::Wheel { .. } => ("RUED", SODIUM),
        Action::KeyDown { .. } | Action::KeyUp { .. } => ("TECL", UV),
        Action::TypeText { .. } => ("TXT ", PHOSPHOR),
        Action::RunCommand { .. } => ("EXEC", PHOSPHOR),
        Action::Wait { .. } => ("WAIT", BONE),
    }
}

fn panel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::none().fill(fill).inner_margin(Margin::symmetric(12.0, 8.0)).stroke(Stroke::new(1.0_f32, LINE))
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let bot = self.bot.clone();
        let mode = bot.mode();
        if mode != self.last_mode {
            self.routes = Route::list(); // p.ej. tras grabar con F9
            self.textures.clear();
            self.windows.clear(); // se vuelve a listar al abrir el selector
            self.last_mode = mode;
        }
        let t = ctx.input(|i| i.time);

        // ── Cabecera: logo + estado + controles ──
        egui::TopBottomPanel::top("controls").frame(panel_frame(VOID)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                theme::glitch_title(ui, "REPLICA", 26.0);
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new("// motor de réplica de rutas").small().color(DIM));
                    ui.label(RichText::new("v0.1 · F9 grabar · F10 reproducir · F12 cortar").small().color(DIM));
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Chip de estado, parpadea al grabar.
                    let (txt, col) = match mode {
                        Mode::Idle => ("STANDBY", DIM),
                        Mode::Recording => ("● REC", CORAL),
                        Mode::Playing => ("▶ RUN", PHOSPHOR),
                    };
                    let alpha = if mode == Mode::Recording { 0.55 + 0.45 * (t * 4.0).sin().abs() as f32 } else { 1.0 };
                    egui::Frame::none()
                        .stroke(Stroke::new(1.0_f32, col.gamma_multiply(alpha)))
                        .fill(col.gamma_multiply(0.08))
                        .inner_margin(Margin::symmetric(12.0, 5.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new(txt).monospace().strong().size(15.0).color(col.gamma_multiply(alpha)));
                        });
                    if mode != Mode::Idle {
                        ctx.request_repaint_after(std::time::Duration::from_millis(50));
                    }
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let rec_label = if mode == Mode::Recording { "⏹ DETENER GRABACIÓN  F9" } else { "⏺ GRABAR  F9" };
                if ui.add_enabled(mode != Mode::Playing, theme::neon_button(rec_label, CORAL)).clicked() {
                    bot.toggle_record(true);
                    self.routes = Route::list();
                }
                let play_label = if mode == Mode::Playing { "⏹ PARAR  F10" } else { "▶ REPRODUCIR  F10" };
                if ui.add_enabled(mode != Mode::Recording, theme::neon_button(play_label, PHOSPHOR)).clicked() {
                    bot.toggle_play();
                }
                let stop = egui::Button::new(RichText::new("✋ CORTE DE EMERGENCIA  F12").color(VOID).strong()).fill(CORAL);
                if ui.add(stop).clicked() {
                    bot.stop_all();
                }
                let progress = bot.state.lock().unwrap().progress.clone();
                if !progress.is_empty() {
                    ui.label(RichText::new(format!("» {progress}")).monospace().color(SODIUM));
                }
            });
            ui.add_space(2.0);
        });

        // ── Terminal inferior ──
        egui::TopBottomPanel::bottom("log").frame(panel_frame(VOID)).resizable(true).default_height(130.0).show(ctx, |ui| {
            theme::section(ui, "TERMINAL", UV);
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                for line in bot.state.lock().unwrap().log.iter() {
                    let low = line.to_lowercase();
                    let col = if low.contains("error") || low.contains("no se pudo") || line.starts_with('⏹') {
                        CORAL
                    } else if line.starts_with('✔') || line.starts_with('▶') {
                        PHOSPHOR
                    } else if line.starts_with('●') || line.starts_with('■') || low.contains("ancla") {
                        SODIUM
                    } else {
                        BONE.gamma_multiply(0.75)
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(">").monospace().color(UV));
                        ui.label(RichText::new(line).monospace().color(col));
                    });
                }
            });
        });

        // ── Panel izquierdo: rutas + ajustes ──
        egui::SidePanel::left("routes").frame(panel_frame(theme::PANEL)).default_width(280.0).show(ctx, |ui| {
            theme::section(ui, "RUTAS", SODIUM);
            ui.horizontal(|ui| {
                ui.label(RichText::new("nombre").small().color(DIM));
                ui.add(egui::TextEdit::singleline(&mut bot.state.lock().unwrap().record_name).text_color(SODIUM));
            });
            egui::ScrollArea::vertical().id_salt("rlist").max_height(150.0).show(ui, |ui| {
                let current = bot.state.lock().unwrap().route.name.clone();
                if self.routes.is_empty() {
                    ui.label(RichText::new("sin rutas · pulsa F9 para grabar").small().color(DIM));
                }
                for name in self.routes.clone() {
                    let sel = current == name;
                    let text = RichText::new(format!("{} {name}", if sel { "▸" } else { " " }))
                        .monospace()
                        .color(if sel { SODIUM } else { BONE });
                    if ui.selectable_label(sel, text).clicked() && mode == Mode::Idle {
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
            if ui.small_button("⟳ refrescar").clicked() {
                self.routes = Route::list();
            }

            theme::section(ui, "REPRODUCCIÓN", PHOSPHOR);
            let mut st = bot.state.lock().unwrap();
            let s = &mut st.settings;
            ui.add(egui::Slider::new(&mut s.speed, 0.25..=4.0).text("velocidad ×"));
            ui.add(egui::DragValue::new(&mut s.loops).prefix("repeticiones ").suffix("  (0 = ∞)"));
            ui.add(egui::DragValue::new(&mut s.pause_between_loops_ms).prefix("pausa entre vueltas ").suffix(" ms"));
            ui.checkbox(&mut s.humanize, "humanizar tiempos y posición");
            ui.checkbox(&mut s.record_moves, "grabar movimientos del ratón");

            theme::section(ui, "VISIÓN", UV);
            ui.checkbox(&mut s.use_anchors, "verificar pantalla antes de clicar");
            ui.add(egui::Slider::new(&mut s.threshold, 0.5..=1.0).text("similitud"));
            ui.add(egui::Slider::new(&mut s.search_radius, 0..=400).text("radio px"));
            ui.add(egui::DragValue::new(&mut s.anchor_timeout_ms).prefix("esperar ancla ").suffix(" ms"));
            ui.horizontal(|ui| {
                ui.label(RichText::new("si no aparece").small().color(DIM));
                ui.radio_value(&mut s.on_missing, MissingAnchor::ClickAnyway, "clicar");
                ui.radio_value(&mut s.on_missing, MissingAnchor::Stop, "detener");
            });
        });

        // ── Centro: editor de pasos ──
        egui::CentralPanel::default().frame(egui::Frame::none().inner_margin(Margin::same(16.0))).show(ctx, |ui| {
            theme::hud_background(ui, ui.max_rect().expand(16.0));
            let editable = mode == Mode::Idle;
            let (name, n, secs) = {
                let st = bot.state.lock().unwrap();
                (st.route.name.clone(), st.route.steps.len(), st.route.duration_ms() as f64 / 1000.0)
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new("RUTA").small().color(DIM));
                ui.label(RichText::new(name.to_uppercase()).heading().color(SODIUM));
                ui.label(RichText::new(format!("{n} pasos · {secs:.1} s")).monospace().color(DIM));
            });
            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("VENTANA").small().color(DIM));
                    let target = bot.state.lock().unwrap().route.target.clone();
                    let shown = target.as_ref().map_or("pantalla completa (coordenadas absolutas)".into(), Target::label);
                    let mut pick: Option<Option<Target>> = None;
                    egui::ComboBox::from_id_salt("target")
                        .selected_text(RichText::new(shown).color(UV))
                        .width(420.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(false, "⟳ actualizar lista").clicked() || self.windows.is_empty() {
                                self.windows = crate::vision::list_windows();
                            }
                            if ui.selectable_label(target.is_none(), "pantalla completa").clicked() {
                                pick = Some(None);
                            }
                            for w in &self.windows {
                                if ui.selectable_label(target.as_ref() == Some(w), w.label()).clicked() {
                                    pick = Some(Some(w.clone()));
                                }
                            }
                        })
                        .response
                        .on_hover_text(
                            "Al reproducir, las coordenadas se adaptan a la posición y tamaño actuales de esta \
                             ventana. Si la cambias, hazlo sin haberla movido desde que grabaste.",
                        );
                    if let Some(t) = pick {
                        bot.state.lock().unwrap().route.target = t;
                        bot.log("Ventana objetivo cambiada (pulsa guardar).");
                    }
                });
            });
            ui.add_space(4.0);

            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.add(theme::neon_button("💾 guardar", SODIUM)).clicked() {
                        let r = bot.state.lock().unwrap().route.clone();
                        match r.save() {
                            Ok(()) => bot.log(format!("Ruta '{}' guardada.", r.name)),
                            Err(e) => bot.log(format!("Error: {e}")),
                        }
                        self.routes = Route::list();
                    }
                    if ui
                        .add(theme::neon_button("✂ simplificar movimientos", UV))
                        .on_hover_text("Quita movimientos de ratón intermedios")
                        .clicked()
                    {
                        let mut st = bot.state.lock().unwrap();
                        let before = st.route.steps.len();
                        simplify_moves(&mut st.route.steps);
                        let after = st.route.steps.len();
                        drop(st);
                        bot.log(format!("Simplificada: {before} → {after} pasos (pulsa guardar)."));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_text).desired_width(180.0).text_color(PHOSPHOR));
                    if ui.button("+ escribir texto").on_hover_text("Ej: comando de chat del juego").clicked() {
                        push(&bot, Action::TypeText { text: self.new_text.clone() });
                    }
                    if ui.button("+ enter").clicked() {
                        push(&bot, Action::KeyDown { key: rdev::Key::Return });
                        push(&bot, Action::KeyUp { key: rdev::Key::Return });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_cmd)
                            .desired_width(180.0)
                            .text_color(PHOSPHOR)
                            .hint_text("notepad.exe / steam://…"),
                    );
                    if ui.button("+ ejecutar programa").clicked() && !self.new_cmd.trim().is_empty() {
                        push(&bot, Action::RunCommand { command: self.new_cmd.clone() });
                    }
                    ui.add(egui::DragValue::new(&mut self.new_wait).suffix(" ms"));
                    if ui.button("+ esperar").clicked() {
                        push(&bot, Action::Wait { ms: self.new_wait });
                    }
                });
            });
            ui.add_space(6.0);
            theme::section(ui, "SECUENCIA", SODIUM);

            let route_name = name;
            let mut delete = None;
            let mut swap = None;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let steps: Vec<Step> = bot.state.lock().unwrap().route.steps.clone();
                if steps.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("[ sin datos ]").monospace().size(16.0).color(DIM));
                        ui.label(RichText::new("pulsa F9, juega, y pulsa F9 otra vez").small().color(DIM));
                    });
                }
                let mut changed: Vec<(usize, Step)> = vec![];
                for (i, step) in steps.iter().enumerate() {
                    let (tag_txt, tag_col) = tag(&step.action);
                    let fill = if i % 2 == 0 { PANEL_HI.gamma_multiply(0.6) } else { Color32::TRANSPARENT };
                    egui::Frame::none().fill(fill).inner_margin(Margin::symmetric(6.0, 1.0)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Marca lateral del color del tipo de paso.
                            let (r, _) = ui.allocate_exact_size(egui::vec2(2.0, 18.0), egui::Sense::hover());
                            ui.painter().rect_filled(r, Rounding::ZERO, tag_col);
                            ui.add_enabled_ui(editable, |ui| {
                                let mut s = step.clone();
                                ui.label(RichText::new(format!("{:03}", i + 1)).monospace().color(DIM));
                                ui.label(RichText::new(tag_txt).monospace().strong().color(tag_col));
                                let mut edited =
                                    ui.add(egui::DragValue::new(&mut s.delay_ms).suffix(" ms").speed(5)).changed();
                                match &mut s.action {
                                    Action::TypeText { text } => {
                                        edited |= ui.add(egui::TextEdit::singleline(text).text_color(PHOSPHOR)).changed();
                                    }
                                    Action::RunCommand { command } => {
                                        edited |= ui.add(egui::TextEdit::singleline(command).text_color(PHOSPHOR)).changed();
                                    }
                                    Action::Wait { ms } => {
                                        edited |= ui.add(egui::DragValue::new(ms).prefix("esperar ").suffix(" ms")).changed();
                                    }
                                    Action::ButtonDown { anchor: Some(file), .. } => {
                                        let path = Route::dir(&route_name).join(file.as_str());
                                        let r = ui.label(RichText::new(step.action.describe()).color(BONE));
                                        if let Some(tex) = self.anchor_texture(ctx, path) {
                                            r.on_hover_ui(|ui| {
                                                ui.label(RichText::new("OBJETIVO VISUAL").small().color(UV));
                                                let resp = ui.image((tex.id(), egui::vec2(144.0, 144.0)));
                                                theme::corners(ui, resp.rect.expand(3.0), SODIUM, 10.0);
                                            });
                                        }
                                    }
                                    other => {
                                        let col = if matches!(other, Action::MouseMove { .. }) { DIM } else { BONE };
                                        ui.label(RichText::new(other.describe()).color(col));
                                    }
                                }
                                if edited {
                                    changed.push((i, s.clone()));
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button(RichText::new("🗑").color(CORAL)).clicked() {
                                        delete = Some(i);
                                    }
                                    if ui.small_button("⬆").clicked() && i > 0 {
                                        swap = Some(i);
                                    }
                                });
                            });
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
