use super::{
    color32, configure_style, contrasting_text, native_options_with_min_size, panel_frame,
    spawn_action,
};
use crate::{
    APP_NAME,
    color::{Rgb, format_template},
    config::{EditorView, EditorViewSwitchPosition, History, Settings, data_dir},
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use serde_json::json;
use std::{
    collections::BTreeMap,
    env, fs,
    os::unix::net::UnixDatagram,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const REPLACE_EDITOR_NOTIFICATION: &[u8] = b"replace\n";

pub fn run_editor(
    initial: Option<Rgb>,
    view_override: Option<EditorView>,
    record_initial: bool,
) -> anyhow::Result<()> {
    let settings = Settings::load_or_default();
    if record_initial && let Some(color) = initial {
        let mut history = History::load_or_default();
        let _ = history.push(color, settings.picker.history_limit);
    }
    let instance = if settings.picker.single_editor_instance {
        Some(EditorInstance::replace_existing()?)
    } else {
        None
    };
    let view = view_override.unwrap_or(settings.picker.default_editor_view);
    let options = native_options_with_min_size(view.window_size(), view.minimum_window_size());
    super::map_eframe(eframe::run_native(
        &format!("Color Editor — {APP_NAME}"),
        options,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(EditorApp::new(initial, settings, instance, view)))
        }),
    ))
}

struct EditorInstance {
    path: PathBuf,
    socket: UnixDatagram,
}

impl EditorInstance {
    fn replace_existing() -> anyhow::Result<Self> {
        let runtime = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::temp_dir().join(format!("pixelkit-{}", unsafe { libc_getuid() }))
            });
        fs::create_dir_all(&runtime)?;
        let path = runtime.join("pixelkit-editor.sock");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut last_close_request = None;
        loop {
            match UnixDatagram::bind(&path) {
                Ok(socket) => {
                    socket.set_nonblocking(true)?;
                    return Ok(Self { path, socket });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
                Err(error) => return Err(error.into()),
            }

            let should_request_close = last_close_request
                .is_none_or(|sent: Instant| sent.elapsed() >= Duration::from_millis(250));
            if should_request_close {
                let notifier = UnixDatagram::unbound()?;
                match notifier.send_to(REPLACE_EDITOR_NOTIFICATION, &path) {
                    Ok(_) => last_close_request = Some(Instant::now()),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                        ) =>
                    {
                        fs::remove_file(&path)?;
                        last_close_request = None;
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                }
            }

            if Instant::now() >= deadline {
                return Err(anyhow::anyhow!(
                    "the existing color editor did not close within 5 seconds"
                ));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn close_requested(&self) -> bool {
        let mut message = [0_u8; 64];
        loop {
            match self.socket.recv(&mut message) {
                Ok(length) if is_replace_editor_notification(&message[..length]) => return true,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return false,
                Err(error) => {
                    eprintln!("PixelKit: could not receive color editor notification: {error}");
                    return false;
                }
            }
        }
    }
}

impl Drop for EditorInstance {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn is_replace_editor_notification(message: &[u8]) -> bool {
    message == REPLACE_EDITOR_NOTIFICATION
}

// Avoid an additional libc crate for one fallback-only numeric identifier.
unsafe extern "C" {
    fn getuid() -> u32;
}
unsafe fn libc_getuid() -> u32 {
    unsafe { getuid() }
}

pub(super) struct EditorApp {
    settings: Settings,
    history: History,
    selected: Rgb,
    selected_index: Option<usize>,
    hex_input: String,
    view: EditorView,
    instance: Option<EditorInstance>,
    message: Option<(String, Instant)>,
}

impl EditorApp {
    fn new(
        initial: Option<Rgb>,
        settings: Settings,
        instance: Option<EditorInstance>,
        view: EditorView,
    ) -> Self {
        let history = History::load_or_default();
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
            view,
            instance,
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

    fn switch_view(&mut self, ctx: &egui::Context, view: EditorView) {
        let target_size = Vec2::from(view.window_size());
        if self.settings.picker.editor_view_switch_position == EditorViewSwitchPosition::TopLeft {
            self.view = view;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
                view.minimum_window_size().into(),
            ));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target_size));
            return;
        }
        let centered_position = ctx.input(|input| {
            let viewport = input.viewport();
            Some(centered_view_position(
                viewport.outer_rect?,
                viewport.inner_rect?.size(),
                target_size,
            ))
        });
        let Some(centered_position) = centered_position else {
            match reopen_editor(self.selected, view) {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(error) => self.message(format!("Could not switch editor view: {error}")),
            }
            return;
        };
        self.view = view;
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
            view.minimum_window_size().into(),
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target_size));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(centered_position));
    }

    fn launch_picker(&mut self, ctx: &egui::Context) {
        match spawn_action("color-picker") {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(error) => self.message(error.to_string()),
        }
    }

    fn close_if_replaced(&self, ctx: &egui::Context) -> bool {
        if self
            .instance
            .as_ref()
            .is_some_and(EditorInstance::close_requested)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        false
    }

    fn history_panel(&mut self, ui: &mut egui::Ui) {
        let action_frame =
            egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 8));
        let remove_selected = egui::TopBottomPanel::bottom("editor_history_actions")
            .resizable(false)
            .frame(action_frame)
            .show_inside(ui, |ui| {
                ui.vertical_centered(|ui| ui.button("Remove selected"))
                    .inner
            })
            .inner
            .clicked();

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
        if remove_selected && let Some(index) = self.selected_index.take() {
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
                self.launch_picker(ui.ctx());
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
            if ui.button("Compact view").clicked() {
                self.switch_view(ui.ctx(), EditorView::Compact);
            }
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

    fn compact_history(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("History");
            ui.label(
                RichText::new(format!("{} colors", self.history.colors.len()))
                    .small()
                    .weak(),
            );
        });
        let colors = self.history.colors.clone();
        egui::ScrollArea::horizontal()
            .id_salt("compact_history")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, color) in colors.into_iter().enumerate() {
                        let response = ui.add(
                            egui::Button::new("")
                                .min_size(Vec2::splat(28.0))
                                .fill(color32(color))
                                .selected(self.selected_index == Some(index))
                                .corner_radius(14),
                        );
                        if response.clicked() {
                            self.select(color, Some(index));
                        }
                        response.on_hover_text(format!("#{} — {}", color.hex(), color.name()));
                    }
                });
            });
    }

    fn compact_formats(&mut self, ui: &mut egui::Ui) {
        ui.strong("Color formats");
        ui.label(RichText::new("Click a value to copy it.").small().weak());
        ui.add_space(4.0);

        let formats = self
            .settings
            .picker
            .formats
            .iter()
            .filter(|format| format.enabled)
            .map(|format| {
                (
                    format.name.clone(),
                    format_template(self.selected, &format.template),
                )
            })
            .collect::<Vec<_>>();
        if formats.is_empty() {
            ui.label(RichText::new("No formats are enabled in Settings.").weak());
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("compact_formats")
            .show(ui, |ui| {
                for (name, value) in formats {
                    panel_frame().inner_margin(8).show(ui, |ui| {
                        ui.label(RichText::new(name.to_uppercase()).small().strong());
                        let response = ui.add(
                            egui::Label::new(RichText::new(&value).monospace())
                                .sense(egui::Sense::click()),
                        );
                        if response.clicked() {
                            ui.ctx().copy_text(value.clone());
                            self.message(format!("Copied {value}"));
                        }
                    });
                    ui.add_space(4.0);
                }
            });
    }

    fn compact_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Pick another color").clicked() {
                self.launch_picker(ui.ctx());
            }
            if ui.button("Settings").clicked() {
                let _ = spawn_action("settings");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Full editor").clicked() {
                    self.switch_view(ui.ctx(), EditorView::Full);
                }
            });
        });
        ui.separator();
        self.compact_history(ui);
        ui.separator();

        ui.columns(2, |columns| {
            columns[0].vertical_centered(|ui| {
                let side = ui.available_width().min(180.0);
                let card = egui::Frame::new()
                    .fill(color32(self.selected))
                    .corner_radius(12)
                    .stroke(Stroke::new(1.0, Color32::from_white_alpha(45)));
                card.show(ui, |ui| {
                    ui.set_min_size(Vec2::new(side, side));
                });
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("#{}", self.selected.hex()))
                        .monospace()
                        .strong(),
                );
                ui.label(RichText::new(self.selected.name()).weak());
                if ui.button("Copy default format").clicked() {
                    let text = format_template(self.selected, self.settings.selected_format());
                    ui.ctx().copy_text(text.clone());
                    self.message(format!("Copied {text}"));
                }
            });
            self.compact_formats(&mut columns[1]);
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
        if self.close_if_replaced(ctx) {
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        match self.view {
            EditorView::Compact => {
                egui::CentralPanel::default().show(ctx, |ui| self.compact_editor(ui));
            }
            EditorView::Full => {
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
            }
        }
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
        if self.instance.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}

impl EditorView {
    const fn window_size(self) -> [f32; 2] {
        match self {
            Self::Compact => [540.0, 480.0],
            Self::Full => [980.0, 720.0],
        }
    }

    const fn minimum_window_size(self) -> [f32; 2] {
        match self {
            Self::Compact => [440.0, 380.0],
            Self::Full => [560.0, 420.0],
        }
    }
}

fn centered_view_position(
    outer_rect: egui::Rect,
    current_inner_size: Vec2,
    target_inner_size: Vec2,
) -> egui::Pos2 {
    outer_rect.min + (current_inner_size - target_inner_size) / 2.0
}

fn reopen_editor(color: Rgb, view: EditorView) -> anyhow::Result<()> {
    let executable = env::current_exe()?;
    let color = color.hex();
    let view = match view {
        EditorView::Compact => "compact",
        EditorView::Full => "full",
    };
    Command::new(executable)
        .args(["color-editor", "--selection", &color, "--view", view])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_notification_is_recognized() {
        assert!(is_replace_editor_notification(REPLACE_EDITOR_NOTIFICATION));
    }

    #[test]
    fn unrelated_editor_notification_is_ignored() {
        assert!(!is_replace_editor_notification(b"color 336699\n"));
    }

    #[test]
    fn switching_editor_views_preserves_the_window_center() {
        let full_outer =
            egui::Rect::from_min_size(egui::pos2(100.0, 80.0), egui::vec2(1_000.0, 760.0));
        let full_inner = Vec2::new(980.0, 720.0);
        let compact_inner = Vec2::new(540.0, 480.0);

        let compact_position = centered_view_position(full_outer, full_inner, compact_inner);
        assert_eq!(compact_position, egui::pos2(320.0, 200.0));

        let compact_outer = egui::Rect::from_min_size(
            compact_position,
            compact_inner + (full_outer.size() - full_inner),
        );
        let restored_position = centered_view_position(compact_outer, compact_inner, full_inner);
        assert_eq!(restored_position, full_outer.min);
    }
}
