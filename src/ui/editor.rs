use super::{
    color32, configure_style, contrasting_text, native_options, panel_frame, spawn_action,
};
use crate::{
    APP_NAME,
    color::{Rgb, format_template},
    config::{History, Settings, data_dir},
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub fn run_editor(initial: Option<Rgb>) -> anyhow::Result<()> {
    let options = native_options([980.0, 720.0]);
    super::map_eframe(eframe::run_native(
        &format!("Color Editor — {APP_NAME}"),
        options,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(EditorApp::new(initial)))
        }),
    ))
}

pub(super) struct EditorApp {
    settings: Settings,
    history: History,
    selected: Rgb,
    selected_index: Option<usize>,
    hex_input: String,
    message: Option<(String, Instant)>,
}

impl EditorApp {
    pub(super) fn new(initial: Option<Rgb>) -> Self {
        let settings = Settings::load_or_default();
        let mut history = History::load_or_default();
        if let Some(color) = initial {
            let _ = history.push(color, settings.picker.history_limit);
        }
        let selected = initial
            .or_else(|| history.colors.first().copied())
            .unwrap_or(Rgb::new(51, 102, 153));
        let selected_index = history.colors.iter().position(|color| *color == selected);
        Self {
            settings,
            history,
            selected,
            selected_index,
            hex_input: selected.hex(),
            message: None,
        }
    }

    fn select(&mut self, color: Rgb, index: Option<usize>) {
        self.selected = color;
        self.selected_index = index;
        self.hex_input = color.hex();
    }

    fn message(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), Instant::now()));
    }

    fn history_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("History");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Clear").clicked() {
                    self.history.colors.clear();
                    self.selected_index = None;
                    if let Err(error) = self.history.save() {
                        self.message(error.to_string());
                    }
                }
            });
        });
        ui.label(RichText::new(format!("{} saved colors", self.history.colors.len())).weak());
        ui.separator();
        let colors = self.history.colors.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (index, color) in colors.into_iter().enumerate() {
                let selected = self.selected_index == Some(index);
                let text = RichText::new(format!("#{}  {}", color.hex(), color.name()))
                    .color(contrasting_text(color));
                let response = ui.add_sized(
                    [ui.available_width(), 38.0],
                    egui::Button::new(text)
                        .fill(color32(color))
                        .selected(selected),
                );
                if response.clicked() {
                    self.select(color, Some(index));
                }
            }
        });
        ui.separator();
        if ui.button("Remove selected").clicked()
            && let Some(index) = self.selected_index.take()
        {
            self.history.colors.remove(index);
            if let Some(color) = self
                .history
                .colors
                .get(index.min(self.history.colors.len().saturating_sub(1)))
                .copied()
            {
                let new_index = self.history.colors.iter().position(|item| *item == color);
                self.select(color, new_index);
            }
            if let Err(error) = self.history.save() {
                self.message(error.to_string());
            }
        }
    }

    fn color_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("Color Editor");
        ui.horizontal(|ui| {
            if ui.button("Pick another color").clicked() {
                match spawn_action("color-picker") {
                    Ok(()) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(error) => self.message(error.to_string()),
                }
            }
            if ui.button("Settings").clicked() {
                let _ = spawn_action("settings");
            }
            ui.menu_button("Export history", |ui| {
                if ui.button("JSON grouped by color").clicked() {
                    self.export(false, false);
                    ui.close_menu();
                }
                if ui.button("JSON grouped by format").clicked() {
                    self.export(true, false);
                    ui.close_menu();
                }
                if ui.button("Text grouped by color").clicked() {
                    self.export(false, true);
                    ui.close_menu();
                }
                if ui.button("Text grouped by format").clicked() {
                    self.export(true, true);
                    ui.close_menu();
                }
            });
        });
        ui.add_space(10.0);
        let card = egui::Frame::new()
            .fill(color32(self.selected))
            .corner_radius(12)
            .inner_margin(18)
            .stroke(Stroke::new(1.0, Color32::from_white_alpha(45)));
        card.show(ui, |ui| {
            ui.set_min_height(105.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(format!("#{}", self.selected.hex()))
                        .size(28.0)
                        .strong()
                        .color(contrasting_text(self.selected)),
                );
                ui.label(
                    RichText::new(self.selected.name()).color(contrasting_text(self.selected)),
                );
            });
        });
        ui.add_space(12.0);

        let old = self.selected;
        let mut r = self.selected.r;
        let mut g = self.selected.g;
        let mut b = self.selected.b;
        let mut rgb_changed = false;
        egui::Grid::new("editor-rgb")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Red");
                rgb_changed |= ui
                    .add(egui::Slider::new(&mut r, 0..=255).show_value(true))
                    .changed();
                ui.end_row();
                ui.label("Green");
                rgb_changed |= ui
                    .add(egui::Slider::new(&mut g, 0..=255).show_value(true))
                    .changed();
                ui.end_row();
                ui.label("Blue");
                rgb_changed |= ui
                    .add(egui::Slider::new(&mut b, 0..=255).show_value(true))
                    .changed();
                ui.end_row();
            });
        if rgb_changed {
            self.selected = Rgb::new(r, g, b);
            self.hex_input = self.selected.hex();
            self.selected_index = None;
        }

        let (mut h, mut s, mut v) = self.selected.hsv();
        let mut hsv_changed = false;
        egui::Grid::new("editor-hsv")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Hue");
                hsv_changed |= ui
                    .add(egui::Slider::new(&mut h, 0.0..=360.0).show_value(true))
                    .changed();
                ui.end_row();
                ui.label("Saturation");
                hsv_changed |= ui
                    .add(egui::Slider::new(&mut s, 0.0..=1.0).show_value(true))
                    .changed();
                ui.end_row();
                ui.label("Value");
                hsv_changed |= ui
                    .add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(true))
                    .changed();
                ui.end_row();
            });
        if hsv_changed {
            self.selected = Rgb::from_hsv(h, s, v);
            self.hex_input = self.selected.hex();
            self.selected_index = None;
        }

        ui.horizontal(|ui| {
            ui.label("HEX");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.hex_input)
                    .desired_width(150.0)
                    .font(egui::TextStyle::Monospace),
            );
            if response.changed()
                && let Some(color) = Rgb::parse_hex(&self.hex_input)
            {
                self.selected = color;
                self.selected_index = None;
            }
            if Rgb::parse_hex(&self.hex_input).is_none() {
                ui.colored_label(Color32::LIGHT_RED, "Use 3 or 6 hexadecimal digits");
            }
        });

        if self.selected != old {
            ui.ctx().request_repaint();
        }
        ui.horizontal(|ui| {
            if ui.button("Save adjusted color to history").clicked() {
                match self
                    .history
                    .push(self.selected, self.settings.picker.history_limit)
                {
                    Ok(()) => {
                        self.selected_index = Some(0);
                        self.message("Color saved");
                    }
                    Err(error) => self.message(error.to_string()),
                }
            }
            if ui.button("Copy default format").clicked() {
                let text = format_template(self.selected, self.settings.selected_format());
                ui.ctx().copy_text(text.clone());
                self.message(format!("Copied {text}"));
            }
        });
        ui.add_space(14.0);
        ui.strong("Similar colors");
        ui.horizontal(|ui| {
            for color in variations(self.selected) {
                let response = ui.add_sized(
                    Vec2::new(62.0, 38.0),
                    egui::Button::new("").fill(color32(color)),
                );
                if response.clicked() {
                    self.select(color, None);
                }
                response.on_hover_text(format!("#{}", color.hex()));
            }
        });
    }

    fn formats(&mut self, ui: &mut egui::Ui) {
        ui.heading("Color formats");
        ui.label(RichText::new("Click a value to copy it.").weak());
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for format in self
                .settings
                .picker
                .formats
                .iter()
                .filter(|format| format.enabled)
            {
                let value = format_template(self.selected, &format.template);
                panel_frame().inner_margin(10).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format.name.to_uppercase());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(value.clone());
                            }
                        });
                    });
                    let response = ui.add(
                        egui::Label::new(RichText::new(&value).monospace())
                            .sense(egui::Sense::click()),
                    );
                    if response.clicked() {
                        ui.ctx().copy_text(value);
                    }
                });
                ui.add_space(5.0);
            }
        });
    }

    fn export(&mut self, group_by_format: bool, text: bool) {
        match export_history(&self.history, &self.settings, group_by_format, text) {
            Ok(path) => self.message(format!("Exported to {}", path.display())),
            Err(error) => self.message(format!("Export failed: {error:#}")),
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        egui::SidePanel::left("history")
            .default_width(245.0)
            .min_width(190.0)
            .show(ctx, |ui| self.history_panel(ui));
        egui::SidePanel::right("formats")
            .default_width(285.0)
            .min_width(230.0)
            .show(ctx, |ui| self.formats(ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.color_editor(ui))
        });
        if let Some((message, instant)) = &self.message
            && instant.elapsed() < Duration::from_secs(5)
        {
            egui::Area::new("editor_message".into())
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(message);
                    });
                });
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}

fn variations(color: Rgb) -> [Rgb; 4] {
    let (h, s, v) = color.hsv();
    let high = if 1.0 - v < 0.15 { 1.0 } else { 0.0 };
    let low = if v - 0.3 < 0.0 { 1.0 } else { 0.0 };
    [
        Rgb::from_hsv((h + high * 8.0).min(360.0), s, (v + 0.3).min(1.0)),
        Rgb::from_hsv((h + high * 4.0).min(360.0), s, (v + 0.15).min(1.0)),
        Rgb::from_hsv((h - low * 4.0).max(0.0), s, (v - 0.2).max(0.0)),
        Rgb::from_hsv((h - low * 8.0).max(0.0), s, (v - 0.3).max(0.0)),
    ]
}

fn export_history(
    history: &History,
    settings: &Settings,
    group_by_format: bool,
    text: bool,
) -> anyhow::Result<PathBuf> {
    let directory = directories::UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(PathBuf::from))
        .unwrap_or(data_dir()?);
    fs::create_dir_all(&directory)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let extension = if text { "txt" } else { "json" };
    let grouping = if group_by_format {
        "by-format"
    } else {
        "by-color"
    };
    let path = directory.join(format!(
        "pixelkit-colors-{grouping}-{timestamp}.{extension}"
    ));
    let mut colors = Vec::new();
    for color in &history.colors {
        let mut formats = BTreeMap::new();
        for format in settings
            .picker
            .formats
            .iter()
            .filter(|format| format.enabled)
        {
            formats.insert(
                format.name.clone(),
                format_template(*color, &format.template),
            );
        }
        colors.push(
            json!({"hex": format!("#{}", color.hex()), "name": color.name(), "formats": formats}),
        );
    }
    if text {
        let mut output = String::new();
        if group_by_format {
            for format in settings
                .picker
                .formats
                .iter()
                .filter(|format| format.enabled)
            {
                output.push_str(&format!("{}\n", format.name));
                for color in &history.colors {
                    output.push_str(&format!(
                        "#{};{}\n",
                        color.hex(),
                        format_template(*color, &format.template)
                    ));
                }
                output.push('\n');
            }
        } else {
            for color in &history.colors {
                output.push_str(&format!("#{};{}\n", color.hex(), color.name()));
                for format in settings
                    .picker
                    .formats
                    .iter()
                    .filter(|format| format.enabled)
                {
                    output.push_str(&format!(
                        "{};{}\n",
                        format.name,
                        format_template(*color, &format.template)
                    ));
                }
                output.push('\n');
            }
        }
        fs::write(&path, output)?;
    } else if group_by_format {
        let mut formats = BTreeMap::new();
        for format in settings
            .picker
            .formats
            .iter()
            .filter(|format| format.enabled)
        {
            let values = history
                .colors
                .iter()
                .map(|color| {
                    json!({
                        "hex": format!("#{}", color.hex()),
                        "value": format_template(*color, &format.template),
                    })
                })
                .collect::<Vec<_>>();
            formats.insert(format.name.clone(), values);
        }
        let root = json!({"application": APP_NAME, "version": env!("CARGO_PKG_VERSION"), "formats": formats});
        fs::write(&path, serde_json::to_vec_pretty(&root)?)?;
    } else {
        let root = json!({"application": APP_NAME, "version": env!("CARGO_PKG_VERSION"), "colors": colors});
        fs::write(&path, serde_json::to_vec_pretty(&root)?)?;
    }
    Ok(path)
}
