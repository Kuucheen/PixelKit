use super::{configure_style, native_options, panel_frame, rgba_hex_input, spawn_action};
use crate::{
    APP_NAME, VERSION,
    color::{FORMAT_NAMES, Rgb, format_template},
    config::{
        ActivationAction, ClickAction, EditorView, EditorViewSwitchPosition, History,
        MAGNIFIER_GRID_SIZES, MAX_MAGNIFIER_ZOOM_LEVEL, MAX_PICKER_MAX_ZOOM_LEVEL, MagnifierStyle,
        RulerMode, Settings, Unit,
    },
};
use eframe::egui::{self, RichText};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Picker,
    Magnifier,
    Ruler,
    Shortcuts,
    About,
}

pub fn run_hub() -> anyhow::Result<()> {
    let options = native_options([940.0, 700.0]);
    super::map_eframe(eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(HubApp::new()))
        }),
    ))
}

struct HubApp {
    settings: Settings,
    history: History,
    page: Page,
    status: Option<(String, Instant)>,
    dirty: bool,
    last_save: Instant,
}

impl HubApp {
    fn new() -> Self {
        Self {
            settings: Settings::load_or_default(),
            history: History::load_or_default(),
            page: Page::Home,
            status: None,
            dirty: false,
            last_save: Instant::now(),
        }
    }

    fn message(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
    }

    fn launch(&mut self, action: &str) {
        match spawn_action(action) {
            Ok(()) => self.message("Launched"),
            Err(error) => self.message(format!("Could not launch: {error}")),
        }
    }

    fn save_if_needed(&mut self) {
        if self.dirty && self.last_save.elapsed() >= Duration::from_millis(400) {
            self.settings.normalize();
            match self.settings.save() {
                Ok(()) => {
                    self.dirty = false;
                    self.last_save = Instant::now();
                }
                Err(error) => self.message(format!("Could not save settings: {error:#}")),
            }
        }
    }

    fn navigation(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading(RichText::new("PixelKit").size(24.0));
        ui.label(RichText::new("Precision tools for Linux").weak());
        ui.add_space(18.0);
        for (page, label) in [
            (Page::Home, "Overview"),
            (Page::Picker, "Color Picker"),
            (Page::Magnifier, "Magnifier"),
            (Page::Ruler, "Screen Ruler"),
            (Page::Shortcuts, "Background shortcuts"),
            (Page::About, "About & compatibility"),
        ] {
            if ui.selectable_label(self.page == page, label).clicked() {
                self.page = page;
            }
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.label(RichText::new(format!("Version {VERSION}")).weak().small());
        });
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        ui.heading("Overview");
        ui.label("Pick exact pixels, magnify details, edit and export colors, or measure UI geometry and same-color spacing.");
        ui.add_space(12.0);
        ui.columns(3, |columns| {
            panel_frame().show(&mut columns[0], |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.heading("Color Picker");
                    ui.label("Magnified sampling, keyboard precision, 16 formats, custom templates, color names, and persistent history.");
                    ui.add_space(12.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), ui.spacing().interact_size.y],
                            egui::Button::new("Pick a color"),
                        )
                        .clicked()
                    {
                        self.launch("color-picker");
                    }
                    if ui
                        .add_sized(
                            [ui.available_width(), ui.spacing().interact_size.y],
                            egui::Button::new("Open color editor"),
                        )
                        .clicked()
                    {
                        self.launch("color-editor");
                    }
                });
            });
            panel_frame().show(&mut columns[1], |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.heading("Magnifier");
                    ui.label("A centered grid or tooltip with independent zoom, size, capture, and shortcut settings.");
                    ui.add_space(12.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), ui.spacing().interact_size.y],
                            egui::Button::new("Magnify the screen"),
                        )
                        .clicked()
                    {
                        self.launch("magnifier");
                    }
                });
            });
            panel_frame().show(&mut columns[2], |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.heading("Screen Ruler");
                    ui.label("Bounds, cross-spacing, horizontal, and vertical modes with tolerance controls and physical units.");
                    ui.add_space(12.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), ui.spacing().interact_size.y],
                            egui::Button::new("Measure the screen"),
                        )
                        .clicked()
                    {
                        self.launch("screen-ruler");
                    }
                });
            });
        });
        ui.add_space(18.0);
        ui.heading("Recent colors");
        if self.history.colors.is_empty() {
            ui.label(RichText::new("Your picked colors will appear here.").weak());
        } else {
            ui.horizontal_wrapped(|ui| {
                for color in self.history.colors.iter().take(12).copied() {
                    let response = ui.add(
                        egui::Button::new(format!("#{}", color.hex())).fill(super::color32(color)),
                    );
                    if response.clicked() {
                        ui.ctx().copy_text(format!("#{}", color.hex()));
                    }
                    response.on_hover_text(format!("{} — click to copy", color.name()));
                }
            });
        }
    }

    fn magnifier(&mut self, ui: &mut egui::Ui) {
        ui.heading("Magnifier");
        ui.label("Configure the standalone screen magnifier.");
        ui.separator();
        egui::Grid::new("magnifier_settings")
            .num_columns(2)
            .spacing([18.0, 12.0])
            .show(ui, |ui| {
                ui.label("Display style");
                egui::ComboBox::from_id_salt("magnifier_style")
                    .selected_text(self.settings.magnifier.style.label())
                    .show_ui(ui, |ui| {
                        for style in MagnifierStyle::ALL {
                            self.dirty |= ui
                                .selectable_value(
                                    &mut self.settings.magnifier.style,
                                    style,
                                    style.label(),
                                )
                                .changed();
                        }
                    });
                ui.end_row();

                ui.label("Starting zoom level");
                let maximum_zoom_level = self.settings.magnifier.maximum_zoom_level;
                self.dirty |= ui
                    .add(
                        egui::DragValue::new(&mut self.settings.magnifier.initial_zoom_level)
                            .range(1..=maximum_zoom_level)
                            .speed(1.0),
                    )
                    .changed();
                ui.end_row();

                ui.label("Maximum zoom level");
                let maximum_changed = ui
                    .add(
                        egui::DragValue::new(&mut self.settings.magnifier.maximum_zoom_level)
                            .range(1..=MAX_MAGNIFIER_ZOOM_LEVEL)
                            .speed(1.0),
                    )
                    .changed();
                if maximum_changed {
                    self.settings.magnifier.initial_zoom_level = self
                        .settings
                        .magnifier
                        .initial_zoom_level
                        .min(self.settings.magnifier.maximum_zoom_level);
                    self.dirty = true;
                }
                ui.end_row();

                ui.label("Grid size");
                egui::ComboBox::from_id_salt("magnifier_grid_size")
                    .selected_text(format!(
                        "{}×{} pixels",
                        self.settings.magnifier.grid_size, self.settings.magnifier.grid_size
                    ))
                    .show_ui(ui, |ui| {
                        for size in MAGNIFIER_GRID_SIZES {
                            self.dirty |= ui
                                .selectable_value(
                                    &mut self.settings.magnifier.grid_size,
                                    size,
                                    format!("{size}×{size} pixels"),
                                )
                                .changed();
                        }
                    });
                ui.end_row();
            });
        self.dirty |= ui
            .checkbox(
                &mut self.settings.magnifier.change_cursor,
                "Use a crosshair cursor",
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.settings.magnifier.interactive_portal,
                "Let the Wayland portal ask which screen/area to capture",
            )
            .changed();
        ui.add_space(18.0);
        if ui.button("Launch Magnifier").clicked() {
            self.launch("magnifier");
        }
    }

    fn picker(&mut self, ui: &mut egui::Ui) {
        ui.heading("Color Picker");
        ui.label("The selected format is copied when you pick a pixel.");
        ui.separator();
        egui::Grid::new("picker_general")
            .num_columns(2)
            .spacing([18.0, 12.0])
            .show(ui, |ui| {
                ui.label("Activation action");
                egui::ComboBox::from_id_salt("activation_action")
                    .selected_text(match self.settings.picker.activation_action {
                        ActivationAction::Picker => "Open picker",
                        ActivationAction::Editor => "Open editor",
                    })
                    .show_ui(ui, |ui| {
                        self.dirty |= ui
                            .selectable_value(
                                &mut self.settings.picker.activation_action,
                                ActivationAction::Picker,
                                "Open picker",
                            )
                            .changed();
                        self.dirty |= ui
                            .selectable_value(
                                &mut self.settings.picker.activation_action,
                                ActivationAction::Editor,
                                "Open editor",
                            )
                            .changed();
                    });
                ui.end_row();
                ui.label("Default editor view");
                egui::ComboBox::from_id_salt("default_editor_view")
                    .selected_text(self.settings.picker.default_editor_view.label())
                    .show_ui(ui, |ui| {
                        for view in EditorView::ALL {
                            self.dirty |= ui
                                .selectable_value(
                                    &mut self.settings.picker.default_editor_view,
                                    view,
                                    view.label(),
                                )
                                .changed();
                        }
                    });
                ui.end_row();
                ui.label("View switch position");
                egui::ComboBox::from_id_salt("editor_view_switch_position")
                    .selected_text(self.settings.picker.editor_view_switch_position.label())
                    .show_ui(ui, |ui| {
                        for position in EditorViewSwitchPosition::ALL {
                            self.dirty |= ui
                                .selectable_value(
                                    &mut self.settings.picker.editor_view_switch_position,
                                    position,
                                    position.label(),
                                )
                                .changed();
                        }
                    });
                ui.end_row();
                ui.label("Copied format");
                egui::ComboBox::from_id_salt("copied_format")
                    .selected_text(&self.settings.picker.copied_format)
                    .show_ui(ui, |ui| {
                        for format in &self.settings.picker.formats {
                            self.dirty |= ui
                                .selectable_value(
                                    &mut self.settings.picker.copied_format,
                                    format.name.clone(),
                                    &format.name,
                                )
                                .changed();
                        }
                    });
                ui.end_row();
                ui.label("Primary click");
                click_combo(
                    ui,
                    "primary",
                    &mut self.settings.picker.primary_click,
                    &mut self.dirty,
                );
                ui.end_row();
                ui.label("Middle click");
                click_combo(
                    ui,
                    "middle",
                    &mut self.settings.picker.middle_click,
                    &mut self.dirty,
                );
                ui.end_row();
                ui.label("Secondary click");
                click_combo(
                    ui,
                    "secondary",
                    &mut self.settings.picker.secondary_click,
                    &mut self.dirty,
                );
                ui.end_row();
                ui.label("History limit");
                self.dirty |= ui
                    .add(
                        egui::DragValue::new(&mut self.settings.picker.history_limit)
                            .range(1..=10_000),
                    )
                    .changed();
                ui.end_row();
                ui.label("Zoom range");
                self.dirty |= ui
                    .checkbox(
                        &mut self.settings.picker.use_standard_zoom_range,
                        "Use standard zoom range",
                    )
                    .on_hover_text("Limits the magnifier to the standard five zoom levels.")
                    .changed();
                ui.end_row();
                ui.label("Maximum zoom level");
                self.dirty |= ui
                    .add_enabled(
                        !self.settings.picker.use_standard_zoom_range,
                        egui::DragValue::new(&mut self.settings.picker.maximum_zoom_level)
                            .range(1..=MAX_PICKER_MAX_ZOOM_LEVEL)
                            .speed(1.0),
                    )
                    .on_disabled_hover_text("Turn off the standard zoom range to customize this.")
                    .changed();
                ui.end_row();
            });
        self.dirty |= ui
            .checkbox(
                &mut self.settings.picker.change_cursor,
                "Use a crosshair cursor",
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.settings.picker.show_color_name,
                "Show the nearest color name",
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.settings.picker.single_editor_instance,
                "Keep only one color editor open",
            )
            .on_hover_text("Opening a new editor closes the existing editor first.")
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.settings.picker.interactive_portal,
                "Let the Wayland portal ask which screen/area to capture",
            )
            .changed();
        ui.add_space(16.0);
        ui.heading("Visible formats and templates");
        ui.label(RichText::new("Tokens include %Re/%Gr/%Bl, %Hu, %Sl, %Na and the full PowerToys token set. Changes preview against #336699.").weak());
        let preview = Rgb::new(51, 102, 153);
        for name in FORMAT_NAMES {
            if let Some(format) = self
                .settings
                .picker
                .formats
                .iter_mut()
                .find(|format| format.name == name)
            {
                ui.horizontal(|ui| {
                    self.dirty |= ui.checkbox(&mut format.enabled, &format.name).changed();
                    self.dirty |= ui
                        .add_sized(
                            [330.0, 24.0],
                            egui::TextEdit::singleline(&mut format.template),
                        )
                        .changed();
                    ui.label(
                        RichText::new(format_template(preview, &format.template))
                            .monospace()
                            .weak(),
                    );
                });
            }
        }
    }

    fn ruler(&mut self, ui: &mut egui::Ui) {
        ui.heading("Screen Ruler");
        ui.label("Configure edge detection, measurement display, and the default toolbar mode.");
        ui.separator();
        egui::Grid::new("ruler_settings")
            .num_columns(2)
            .spacing([18.0, 12.0])
            .show(ui, |ui| {
                ui.label("Default mode");
                egui::ComboBox::from_id_salt("default_mode")
                    .selected_text(self.settings.ruler.default_mode.label())
                    .show_ui(ui, |ui| {
                        for mode in RulerMode::ALL {
                            self.dirty |= ui
                                .selectable_value(
                                    &mut self.settings.ruler.default_mode,
                                    mode,
                                    mode.label(),
                                )
                                .changed();
                        }
                    });
                ui.end_row();
                ui.label("Units");
                egui::ComboBox::from_id_salt("units")
                    .selected_text(self.settings.ruler.units.label())
                    .show_ui(ui, |ui| {
                        for unit in Unit::ALL {
                            self.dirty |= ui
                                .selectable_value(
                                    &mut self.settings.ruler.units,
                                    unit,
                                    unit.label(),
                                )
                                .changed();
                        }
                    });
                ui.end_row();
                ui.label("Edge tolerance");
                ui.horizontal(|ui| {
                    self.dirty |= ui
                        .add_sized(
                            [170.0, ui.spacing().interact_size.y],
                            egui::Slider::new(&mut self.settings.ruler.pixel_tolerance, 0..=255)
                                .show_value(false),
                        )
                        .changed();
                    self.dirty |= ui
                        .add_sized(
                            [58.0, ui.spacing().interact_size.y],
                            egui::DragValue::new(&mut self.settings.ruler.pixel_tolerance)
                                .range(0..=255)
                                .speed(1.0)
                                .max_decimals(0)
                                .update_while_editing(true),
                        )
                        .on_hover_text("Click to type an exact value from 0 to 255")
                        .changed();
                });
                ui.end_row();
                ui.label("Crosshair color");
                self.dirty |= rgba_hex_input(ui, &mut self.settings.ruler.cross_color).changed();
                ui.end_row();
                ui.label("Fallback display DPI");
                self.dirty |= ui
                    .add(
                        egui::DragValue::new(&mut self.settings.ruler.fallback_dpi)
                            .range(20.0..=1000.0)
                            .speed(1.0),
                    )
                    .changed();
                ui.end_row();
            });
        self.dirty |= ui
            .checkbox(
                &mut self.settings.ruler.per_color_channel_edge_detection,
                "Apply tolerance independently to every RGB channel",
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.settings.ruler.draw_feet_on_cross,
                "Draw end caps on spacing lines",
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.settings.ruler.continuous_capture,
                "Continuous capture (X11; Wayland uses manual Recapture)",
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.settings.ruler.interactive_portal,
                "Let the Wayland portal ask which screen/area to capture",
            )
            .changed();
        ui.add_space(18.0);
        if ui.button("Launch Screen Ruler").clicked() {
            self.launch("screen-ruler");
        }
    }

    fn shortcuts(&mut self, ui: &mut egui::Ui) {
        ui.heading("Background shortcuts");
        ui.label("The small background service registers shortcuts through X11 grabs or the freedesktop GlobalShortcuts portal.");
        ui.add_space(12.0);
        egui::Grid::new("shortcuts")
            .num_columns(2)
            .spacing([18.0, 12.0])
            .show(ui, |ui| {
                ui.label("Color Picker");
                self.dirty |= ui
                    .text_edit_singleline(&mut self.settings.picker.shortcut)
                    .changed();
                ui.end_row();
                ui.label("Magnifier");
                self.dirty |= ui
                    .text_edit_singleline(&mut self.settings.magnifier.shortcut)
                    .changed();
                ui.end_row();
                ui.label("Screen Ruler");
                self.dirty |= ui
                    .text_edit_singleline(&mut self.settings.ruler.shortcut)
                    .changed();
                ui.end_row();
            });
        ui.label(RichText::new("Examples: Super+Shift+C, Ctrl+Alt+M. Restart the service after changing these values.").weak());
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if ui.button("Configure desktop bindings").clicked() {
                self.launch("configure-shortcuts");
            }
            if ui.button("Start for this session").clicked() {
                self.launch("daemon");
            }
            if ui.button("Enable systemd user service").clicked() {
                match Command::new("systemctl")
                    .args(["--user", "enable", "--now", "pixelkit.service"])
                    .stdin(Stdio::null())
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        self.message("Background shortcuts enabled")
                    }
                    Ok(output) => self.message(format!(
                        "systemctl failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )),
                    Err(error) => self.message(format!("systemctl is unavailable: {error}")),
                }
            }
            if ui.button("Restart service").clicked() {
                let status = Command::new("systemctl")
                    .args(["--user", "restart", "pixelkit.service"])
                    .status();
                self.message(if status.is_ok_and(|status| status.success()) {
                    "Service restarted"
                } else {
                    "Could not restart service"
                });
            }
        });
        ui.add_space(16.0);
        panel_frame().show(ui, |ui| {
            ui.strong("Wayland permission note");
            ui.label("Your desktop may display a one-time shortcut binding dialog. The compositor owns the final key choice, which avoids unsafe global input hooks.");
        });
    }

    fn about(&mut self, ui: &mut egui::Ui) {
        ui.heading("About & compatibility");
        ui.label(format!("{APP_NAME} {VERSION} is an independent MIT-licensed Linux implementation inspired by PowerToys Color Picker and Screen Ruler."));
        ui.add_space(14.0);
        panel_frame().show(ui, |ui| {
            ui.strong("X11");
            ui.label("Uses direct root-window capture for immediate, full-desktop sampling and native global key grabs.");
            ui.add_space(8.0);
            ui.strong("Wayland and Flatpak");
            ui.label("Uses screenshot and global-shortcut portals. The security model may add a compositor permission/target dialog; PixelKit never bypasses it.");
            ui.add_space(8.0);
            ui.strong("Privacy");
            ui.label("Screenshots are processed locally in memory. PixelKit has no telemetry or network code.");
        });
    }
}

fn click_combo(ui: &mut egui::Ui, id: &str, value: &mut ClickAction, dirty: &mut bool) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            for action in ClickAction::ALL {
                *dirty |= ui.selectable_value(value, action, action.label()).changed();
            }
        });
}

impl eframe::App for HubApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("navigation")
            .resizable(false)
            .exact_width(190.0)
            .show(ctx, |ui| self.navigation(ui));
        if let Some((message, at)) = &self.status
            && at.elapsed() < Duration::from_secs(5)
        {
            egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
                ui.label(message);
            });
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.page {
                    Page::Home => self.home(ui),
                    Page::Picker => self.picker(ui),
                    Page::Magnifier => self.magnifier(ui),
                    Page::Ruler => self.ruler(ui),
                    Page::Shortcuts => self.shortcuts(ui),
                    Page::About => self.about(ui),
                });
        });
        if self.dirty {
            ctx.request_repaint_after(Duration::from_millis(400));
        }
        self.save_if_needed();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.dirty {
            self.settings.normalize();
            if let Err(error) = self.settings.save() {
                eprintln!("PixelKit: could not save settings on exit: {error:#}");
            }
        }
    }
}
