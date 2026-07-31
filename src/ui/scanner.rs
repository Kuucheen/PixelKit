use super::{
    SettingControlText, TiledCaptureTexture, centered_setting_label, centered_setting_spinner,
    configure_style, copy_text, overlay_options, parse_rgba,
};
use crate::{
    APP_ID, APP_NAME,
    capture::{CaptureFrame, capture_screen},
    code_detection::{DetectedCode, SourceRect, capture_luma, detect_codes_in_luma},
    config::{DEFAULT_SCANNER_HIGHLIGHT_COLOR, Settings},
};
use anyhow::{Context, Result as AnyhowResult};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

pub fn run_scanner(image_path: Option<&Path>) -> anyhow::Result<()> {
    let settings = Settings::load_or_default();
    let frame = if let Some(path) = image_path {
        CaptureFrame::from_path(path, settings.ruler.fallback_dpi)?
    } else {
        capture_screen(
            settings.scanner.interactive_portal,
            settings.ruler.fallback_dpi,
        )?
    };
    let title = format!("QR & Barcode Scanner — {APP_NAME}");
    let mut options = overlay_options(&title);
    options.viewport = options.viewport.with_transparent(true);
    super::map_eframe(eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(ScannerApp::new(cc, settings, frame)))
        }),
    ))
}

struct ScanTask {
    receiver: Receiver<std::result::Result<Vec<DetectedCode>, String>>,
}

struct RefreshTask {
    receiver: Receiver<std::result::Result<CaptureFrame, String>>,
    started: Instant,
}

struct LinkTask {
    receiver: Receiver<std::result::Result<(), String>>,
}

struct ScannerApp {
    settings: Settings,
    frame: CaptureFrame,
    texture: TiledCaptureTexture,
    codes: Vec<DetectedCode>,
    selected: Option<usize>,
    scan_task: Option<ScanTask>,
    refresh_task: Option<RefreshTask>,
    link_task: Option<LinkTask>,
    warning: Option<String>,
    status: Option<(String, Instant)>,
}

impl ScannerApp {
    fn new(cc: &eframe::CreationContext<'_>, settings: Settings, frame: CaptureFrame) -> Self {
        let texture =
            TiledCaptureTexture::load(&cc.egui_ctx, "pixelkit-code-scanner-capture", &frame);
        let (scan_task, warning) = match start_scan(&frame) {
            Ok(task) => (Some(task), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            settings,
            frame,
            texture,
            codes: Vec::new(),
            selected: None,
            scan_task,
            refresh_task: None,
            link_task: None,
            warning,
            status: None,
        }
    }

    fn image_rect(&self, available: Rect) -> Rect {
        let scale = (available.width() / self.frame.width as f32)
            .min(available.height() / self.frame.height as f32);
        Rect::from_center_size(
            available.center(),
            Vec2::new(
                self.frame.width as f32 * scale,
                self.frame.height as f32 * scale,
            ),
        )
    }

    fn begin_refresh(&mut self, ctx: &egui::Context) {
        if self.refresh_task.is_some() {
            return;
        }
        let interactive = self.settings.scanner.interactive_portal;
        let fallback_dpi = self.settings.ruler.fallback_dpi;
        let (sender, receiver) = mpsc::channel();
        match thread::Builder::new()
            .name("pixelkit-scanner-capture".into())
            .spawn(move || {
                // Submit a transparent overlay frame before asking the
                // compositor for a new screenshot.
                thread::sleep(Duration::from_millis(160));
                let result =
                    capture_screen(interactive, fallback_dpi).map_err(|error| format!("{error:#}"));
                let _ = sender.send(result);
            }) {
            Ok(_) => {
                self.codes.clear();
                self.selected = None;
                self.scan_task = None;
                self.warning = None;
                self.refresh_task = Some(RefreshTask {
                    receiver,
                    started: Instant::now(),
                });
                ctx.request_repaint();
            }
            Err(error) => {
                self.warning = Some(format!("Could not start recapture: {error}"));
            }
        }
    }

    fn poll_refresh(&mut self, ctx: &egui::Context) -> bool {
        const REFRESH_TIMEOUT: Duration = Duration::from_secs(90);

        let mut finished = None;
        if let Some(task) = &self.refresh_task {
            match task.receiver.try_recv() {
                Ok(result) => finished = Some(result),
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err("the capture worker stopped unexpectedly".into()));
                }
                Err(TryRecvError::Empty) if task.started.elapsed() >= REFRESH_TIMEOUT => {
                    finished = Some(Err(
                        "recapture timed out while waiting for the desktop portal".into(),
                    ));
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if let Some(result) = finished {
            self.refresh_task = None;
            match result {
                Ok(frame) if frame.width > 0 && frame.height > 0 => {
                    self.frame = frame;
                    self.texture.update(ctx, &self.frame);
                    match start_scan(&self.frame) {
                        Ok(task) => {
                            self.scan_task = Some(task);
                            self.warning = None;
                        }
                        Err(error) => self.warning = Some(error),
                    }
                }
                Ok(_) => self.warning = Some("Recapture returned an empty image".into()),
                Err(error) => self.warning = Some(format!("Recapture failed: {error}")),
            }
            ctx.request_repaint();
        }

        if self.refresh_task.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
            true
        } else {
            false
        }
    }

    fn poll_scan(&mut self, ctx: &egui::Context) {
        let mut finished = None;
        if let Some(task) = &self.scan_task {
            match task.receiver.try_recv() {
                Ok(result) => finished = Some(result),
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err("the scanner worker stopped unexpectedly".into()));
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(result) = finished {
            self.scan_task = None;
            match result {
                Ok(codes) => {
                    self.codes = codes;
                    self.selected = None;
                    self.warning = None;
                }
                Err(error) => self.warning = Some(format!("Scan failed: {error}")),
            }
            ctx.request_repaint();
        } else if self.scan_task.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn poll_link(&mut self, ctx: &egui::Context) {
        let result = self
            .link_task
            .as_ref()
            .and_then(|task| match task.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => {
                    Some(Err("the link opener stopped unexpectedly".into()))
                }
                Err(TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            self.link_task = None;
            self.status = Some((
                match result {
                    Ok(()) => "Opened link".into(),
                    Err(error) => format!("Could not open link: {error}"),
                },
                Instant::now(),
            ));
            ctx.request_repaint();
        } else if self.link_task.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn begin_open_link(&mut self, value: &str) {
        if self.link_task.is_some() {
            return;
        }
        let value = value.to_owned();
        let (sender, receiver) = mpsc::channel();
        match thread::Builder::new()
            .name("pixelkit-open-code-link".into())
            .spawn(move || {
                let result = open_link(&value).map_err(|error| format!("{error:#}"));
                let _ = sender.send(result);
            }) {
            Ok(_) => {
                self.link_task = Some(LinkTask { receiver });
                self.status = Some(("Opening link…".into(), Instant::now()));
            }
            Err(error) => {
                self.status = Some((
                    format!("Could not start link opener: {error}"),
                    Instant::now(),
                ));
            }
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) -> Rect {
        egui::Area::new("scanner-toolbar".into())
            .anchor(egui::Align2::CENTER_TOP, [0.0, 12.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(10)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if self.scan_task.is_some() {
                                centered_setting_spinner(ui);
                                centered_setting_label(
                                    ui,
                                    "Scanning full-resolution image…",
                                    SettingControlText::Centered,
                                );
                            } else if self.codes.is_empty() {
                                centered_setting_label(
                                    ui,
                                    "No codes found",
                                    SettingControlText::Centered,
                                );
                            } else {
                                let count = self.codes.len();
                                egui::ComboBox::from_id_salt("scanner-results")
                                    .selected_text(format!(
                                        "{count} code{} found",
                                        if count == 1 { "" } else { "s" }
                                    ))
                                    .show_ui(ui, |ui| {
                                        for (index, code) in self.codes.iter().enumerate() {
                                            let label = format!(
                                                "{}. {} — {}",
                                                index + 1,
                                                code.format,
                                                payload_preview(&code.text, 42)
                                            );
                                            if ui
                                                .selectable_label(
                                                    self.selected == Some(index),
                                                    label,
                                                )
                                                .clicked()
                                            {
                                                self.selected = Some(index);
                                            }
                                        }
                                    });
                            }
                            ui.separator();
                            let recapture = ui
                                .add_enabled(
                                    self.refresh_task.is_none() && self.scan_task.is_none(),
                                    egui::Button::new("Recapture"),
                                )
                                .on_hover_text(
                                    "Take a fresh screen snapshot and scan it automatically (R)",
                                );
                            if recapture.clicked() {
                                self.begin_refresh(ctx);
                            }
                            if ui.button("Close").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    })
                    .response
                    .rect
            })
            .inner
    }

    fn handle_input(&mut self, ctx: &egui::Context, toolbar: Rect) {
        let (escape, backspace, refresh, secondary, pointer) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::Backspace),
                input.key_pressed(egui::Key::R),
                input.pointer.button_clicked(egui::PointerButton::Secondary),
                input.pointer.interact_pos(),
            )
        });
        if refresh && self.scan_task.is_none() {
            self.begin_refresh(ctx);
        }
        if (escape
            || backspace
            || (secondary && pointer.is_none_or(|point| !toolbar.contains(point))))
            && self.selected.take().is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn draw_detections(&mut self, ui: &mut egui::Ui, image: Rect) {
        let accent = parse_rgba(&self.settings.scanner.highlight_color).unwrap_or_else(|| {
            parse_rgba(DEFAULT_SCANNER_HIGHLIGHT_COLOR)
                .expect("the default scanner highlight color must be valid RGBA")
        });
        let [accent_red, accent_green, accent_blue, accent_alpha] = accent.to_srgba_unmultiplied();
        for (index, code) in self.codes.iter().enumerate() {
            let Some(source) = code.bounds else {
                continue;
            };
            let mut target = map_source_rect(source, &self.frame, image);
            let minimum = Vec2::splat(28.0);
            if target.width() < minimum.x || target.height() < minimum.y {
                target = Rect::from_center_size(target.center(), target.size().max(minimum));
            }
            let response = ui
                .interact(
                    target,
                    ui.make_persistent_id(("detected-code", index)),
                    Sense::click(),
                )
                .on_hover_text(format!(
                    "{}\n{}",
                    code.format,
                    payload_preview(&code.text, 160)
                ));
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                self.selected = Some(index);
            }

            let active = self.selected == Some(index);
            let interaction_alpha = if active {
                54
            } else if response.hovered() {
                40
            } else {
                28
            };
            ui.painter().rect_filled(
                target,
                7.0,
                Color32::from_rgba_unmultiplied(
                    accent_red,
                    accent_green,
                    accent_blue,
                    scaled_alpha(accent_alpha, interaction_alpha),
                ),
            );
            ui.painter().rect_stroke(
                target,
                7.0,
                Stroke::new(
                    5.0_f32,
                    Color32::from_black_alpha(scaled_alpha(accent_alpha, 190)),
                ),
                StrokeKind::Outside,
            );
            ui.painter().rect_stroke(
                target,
                7.0,
                Stroke::new(if active { 3.5_f32 } else { 2.5_f32 }, accent),
                StrokeKind::Outside,
            );

            let badge = Rect::from_min_size(
                Pos2::new(target.left() - 2.0, target.top() - 2.0),
                Vec2::splat(25.0),
            );
            ui.painter().rect_filled(badge, 6.0, accent);
            ui.painter().text(
                badge.center(),
                egui::Align2::CENTER_CENTER,
                index + 1,
                FontId::proportional(14.0),
                contrasting_badge_text(accent),
            );
        }
    }

    fn details(&mut self, ctx: &egui::Context) {
        let Some(index) = self.selected.filter(|index| *index < self.codes.len()) else {
            return;
        };
        let code = self.codes[index].clone();
        let mut open = true;
        egui::Window::new(format!("Detected {} #{}", code.format, index + 1))
            .id("scanner-code-details".into())
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(true)
            .default_width(500.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(RichText::new(&code.format).strong());
                ui.label(
                    RichText::new(if web_link(&code.text).is_some() {
                        "Web link"
                    } else {
                        "Text content"
                    })
                    .weak(),
                );
                ui.add_space(6.0);
                let mut text = code.text.clone();
                ui.add_sized(
                    [ui.available_width(), 120.0],
                    egui::TextEdit::multiline(&mut text)
                        .font(egui::TextStyle::Monospace)
                        .interactive(false),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Copy text").clicked() {
                        copy_text(ctx, code.text.clone());
                        self.status = Some(("Copied code text".into(), Instant::now()));
                    }
                    if let Some(link) = web_link(&code.text) {
                        let open_link = ui
                            .add_enabled(self.link_task.is_none(), egui::Button::new("Open link"))
                            .on_hover_text("Open this HTTP(S) address through the desktop portal");
                        if open_link.clicked() {
                            self.begin_open_link(link);
                        }
                    }
                    if ui.button("Close details").clicked() {
                        self.selected = None;
                    }
                });
                if web_link(&code.text).is_some() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Review the displayed destination before opening it.")
                            .small()
                            .weak(),
                    );
                }
            });
        if !open {
            self.selected = None;
        }
    }

    fn bottom_message(&self, ctx: &egui::Context) {
        let message = if let Some(warning) = &self.warning {
            Some((warning.as_str(), Color32::LIGHT_RED))
        } else if self.scan_task.is_none() && self.codes.is_empty() {
            Some((
                "No QR codes or barcodes found — change the screen and press R to recapture",
                Color32::WHITE,
            ))
        } else {
            None
        };
        if let Some((message, color)) = message {
            egui::Area::new("scanner-message".into())
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -48.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.colored_label(color, message);
                    });
                });
        }

        egui::Area::new("scanner-help".into())
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(
                        "Click a highlighted code to inspect it  •  R recaptures  •  Esc closes",
                    );
                });
            });

        if let Some((status, at)) = &self.status
            && at.elapsed() < Duration::from_secs(5)
        {
            egui::Area::new("scanner-status".into())
                .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -12.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        if self.link_task.is_some() {
                            ui.spinner();
                        }
                        ui.label(status);
                    });
                });
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}

impl eframe::App for ScannerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_link(ctx);
        if self.poll_refresh(ctx) {
            ctx.set_cursor_icon(egui::CursorIcon::Progress);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                .show(ctx, |_| {});
            return;
        }
        self.poll_scan(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::BLACK))
            .show(ctx, |ui| {
                let available = ui.max_rect();
                let image = self.image_rect(available);
                self.texture.paint(ui.painter(), image);
                ui.painter()
                    .rect_filled(available, 0.0, Color32::from_black_alpha(34));
                self.draw_detections(ui, image);
            });
        let toolbar = self.toolbar(ctx);
        self.handle_input(ctx, toolbar);
        self.details(ctx);
        self.bottom_message(ctx);
    }
}

fn start_scan(frame: &CaptureFrame) -> std::result::Result<ScanTask, String> {
    let luma = capture_luma(frame).map_err(|error| format!("{error:#}"))?;
    let width = frame.width;
    let height = frame.height;
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("pixelkit-code-detection".into())
        .spawn(move || {
            let result =
                detect_codes_in_luma(luma, width, height).map_err(|error| format!("{error:#}"));
            let _ = sender.send(result);
        })
        .map_err(|error| format!("Could not start scanner: {error}"))?;
    Ok(ScanTask { receiver })
}

fn map_source_rect(source: SourceRect, frame: &CaptureFrame, image: Rect) -> Rect {
    let map_x = |x: f32| image.left() + x / frame.width as f32 * image.width();
    let map_y = |y: f32| image.top() + y / frame.height as f32 * image.height();
    Rect::from_min_max(
        Pos2::new(map_x(source.left), map_y(source.top)),
        Pos2::new(map_x(source.right), map_y(source.bottom)),
    )
}

fn scaled_alpha(alpha: u8, maximum: u8) -> u8 {
    ((u16::from(alpha) * u16::from(maximum) + 127) / 255) as u8
}

fn contrasting_badge_text(color: Color32) -> Color32 {
    let [red, green, blue, alpha] = color.to_srgba_unmultiplied();
    let luminance = 0.2126 * f32::from(red) + 0.7152 * f32::from(green) + 0.0722 * f32::from(blue);
    if luminance > 145.0 {
        Color32::from_rgba_unmultiplied(0, 0, 0, alpha)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
    }
}

fn payload_preview(value: &str, limit: usize) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let mut characters = normalized.chars();
    let preview = characters.by_ref().take(limit).collect::<String>();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn web_link(value: &str) -> Option<&str> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    ((lower.starts_with("https://") || lower.starts_with("http://"))
        && !value.chars().any(char::is_control))
    .then_some(value)
}

fn open_link(value: &str) -> AnyhowResult<()> {
    let uri = ashpd::Uri::parse(value).context("the decoded link is not a valid URI")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to initialize the desktop link opener")?;
    runtime.block_on(async {
        let connection = ashpd::zbus::Connection::session()
            .await
            .context("failed to connect to the desktop portal")?;
        if let Ok(app_id) = APP_ID.parse() {
            let _ = ashpd::register_host_app_with_connection(connection.clone(), app_id).await;
        }
        ashpd::desktop::open_uri::OpenFileRequest::default()
            .connection(Some(connection))
            .send_uri(&uri)
            .await
            .context("the desktop portal could not open the link")?
            .response()
            .context("the desktop rejected the link request")?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_links_receive_an_open_action() {
        assert_eq!(
            web_link(" https://example.com/path "),
            Some("https://example.com/path")
        );
        assert_eq!(web_link("HTTP://example.com"), Some("HTTP://example.com"));
        assert_eq!(web_link("file:///etc/passwd"), None);
        assert_eq!(web_link("javascript:alert(1)"), None);
        assert_eq!(web_link("ordinary text"), None);
    }

    #[test]
    fn previews_are_character_safe_and_single_line() {
        assert_eq!(payload_preview("a\nb", 10), "a b");
        assert_eq!(payload_preview("QR✓code", 3), "QR✓…");
    }

    #[test]
    fn configured_opacity_scales_every_detection_layer() {
        assert_eq!(scaled_alpha(255, 190), 190);
        assert_eq!(scaled_alpha(128, 190), 95);
        assert_eq!(scaled_alpha(0, 190), 0);
    }

    #[test]
    fn badge_text_contrasts_with_the_configured_color() {
        assert_eq!(contrasting_badge_text(Color32::WHITE), Color32::BLACK);
        assert_eq!(contrasting_badge_text(Color32::BLACK), Color32::WHITE);
        assert_eq!(
            contrasting_badge_text(Color32::from_rgba_unmultiplied(255, 255, 255, 64)),
            Color32::from_black_alpha(64)
        );
    }
}
