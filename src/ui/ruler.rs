use super::{
    TiledCaptureTexture, configure_style, copy_text, overlay_options, parse_rgba, wheel_steps,
};
use crate::{
    APP_NAME,
    capture::{CaptureBackend, CaptureFrame, capture_screen},
    config::{RulerMode, Settings},
    measurement::{MeasureRect, Point, detect_edges},
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

pub fn run_ruler(image_path: Option<&Path>) -> anyhow::Result<()> {
    let settings = Settings::load_or_default();
    let frame = if let Some(path) = image_path {
        CaptureFrame::from_path(path, settings.ruler.fallback_dpi)?
    } else {
        capture_screen(
            settings.ruler.interactive_portal,
            settings.ruler.fallback_dpi,
        )?
    };
    let mut options = overlay_options(&format!("Screen Ruler — {APP_NAME}"));
    options.viewport = options.viewport.with_transparent(true);
    super::map_eframe(eframe::run_native(
        &format!("Screen Ruler — {APP_NAME}"),
        options,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(RulerApp::new(cc, settings, frame)))
        }),
    ))
}

#[derive(Clone, Copy)]
struct MeasurementRecord {
    rect: MeasureRect,
    anchor: Point,
    mode: RulerMode,
}

struct RefreshTask {
    receiver: Receiver<Result<CaptureFrame, String>>,
    started: Instant,
}

struct RulerApp {
    settings: Settings,
    frame: CaptureFrame,
    texture: TiledCaptureTexture,
    point: Point,
    last_pointer: Option<Pos2>,
    mode: RulerMode,
    drag_start: Option<Point>,
    retained: Vec<MeasurementRecord>,
    tolerance: u8,
    wheel_remainder: f32,
    last_capture: Instant,
    last_capture_point: Point,
    refresh_task: Option<RefreshTask>,
    capture_warning: Option<String>,
}

impl RulerApp {
    fn new(cc: &eframe::CreationContext<'_>, settings: Settings, frame: CaptureFrame) -> Self {
        let point = Point {
            x: frame.width / 2,
            y: frame.height / 2,
        };
        let texture = TiledCaptureTexture::load(&cc.egui_ctx, "pixelkit-ruler-capture", &frame);
        Self {
            point,
            mode: settings.ruler.default_mode,
            tolerance: settings.ruler.pixel_tolerance,
            settings,
            frame,
            texture,
            last_pointer: None,
            drag_start: None,
            retained: Vec::new(),
            last_capture: Instant::now(),
            last_capture_point: point,
            wheel_remainder: 0.0,
            refresh_task: None,
            capture_warning: None,
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

    fn screen_to_pixel(&self, position: Pos2, rect: Rect) -> Option<Point> {
        if !rect.contains(position) {
            return None;
        }
        Some(Point {
            x: (((position.x - rect.left()) / rect.width() * self.frame.width as f32).floor()
                as u32)
                .min(self.frame.width - 1),
            y: (((position.y - rect.top()) / rect.height() * self.frame.height as f32).floor()
                as u32)
                .min(self.frame.height - 1),
        })
    }

    fn set_mode(&mut self, mode: RulerMode) {
        if self.mode != mode {
            self.mode = mode;
            self.drag_start = None;
            self.retained.clear();
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) -> Rect {
        let result = egui::Area::new("ruler-toolbar".into())
            .anchor(egui::Align2::CENTER_TOP, [0.0, 12.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(10)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (index, mode) in [
                                (1, RulerMode::Bounds),
                                (2, RulerMode::Spacing),
                                (3, RulerMode::Horizontal),
                                (4, RulerMode::Vertical),
                            ] {
                                let response = mode_button(ui, mode, self.mode == mode)
                                    .on_hover_text(format!("{} · Ctrl+{index}", mode_hint(mode)));
                                if response.clicked() {
                                    self.set_mode(mode);
                                }
                            }
                            ui.separator();
                            ui.label("Tolerance");
                            ui.add_sized(
                                [52.0, 30.0],
                                egui::DragValue::new(&mut self.tolerance)
                                    .range(0..=255)
                                    .speed(1.0)
                                    .max_decimals(0)
                                    .update_while_editing(true),
                            )
                            .on_hover_text("Click to type an exact value from 0 to 255");
                            if ui.small_button("−").clicked() {
                                self.tolerance = self.tolerance.saturating_sub(15);
                            }
                            if ui.small_button("+").clicked() {
                                self.tolerance = self.tolerance.saturating_add(15);
                            }
                            let recapture = ui
                                .add_enabled(
                                    self.refresh_task.is_none(),
                                    egui::Button::new("Recapture"),
                                )
                                .on_hover_text(
                                    "Take a fresh screen snapshot after the content underneath has changed (R)",
                                );
                            if recapture.clicked() {
                                self.begin_refresh(ctx);
                            }
                            if ui.small_button("Close").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    })
                    .response
                    .rect
            });
        result.inner
    }

    fn begin_refresh(&mut self, ctx: &egui::Context) {
        if self.refresh_task.is_some() {
            return;
        }
        let interactive = self.settings.ruler.interactive_portal;
        let fallback_dpi = self.settings.ruler.fallback_dpi;
        let (sender, receiver) = mpsc::channel();
        match thread::Builder::new()
            .name("pixelkit-screen-capture".into())
            .spawn(move || {
                // Give the event loop time to submit one fully transparent
                // frame so PixelKit is not included in its own screenshot.
                thread::sleep(Duration::from_millis(160));
                let result =
                    capture_screen(interactive, fallback_dpi).map_err(|error| format!("{error:#}"));
                let _ = sender.send(result);
            }) {
            Ok(_) => {
                self.capture_warning = None;
                self.refresh_task = Some(RefreshTask {
                    receiver,
                    started: Instant::now(),
                });
                ctx.request_repaint();
            }
            Err(error) => {
                self.capture_warning = Some(format!("Could not start recapture: {error}"));
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
            self.last_capture = Instant::now();
            match result {
                Ok(frame) if frame.width > 0 && frame.height > 0 => {
                    self.frame = frame;
                    self.texture.update(ctx, &self.frame);
                    self.point.x = self.point.x.min(self.frame.width - 1);
                    self.point.y = self.point.y.min(self.frame.height - 1);
                    self.drag_start = None;
                    self.retained.clear();
                    self.last_capture_point = self.point;
                    self.capture_warning = None;
                }
                Ok(_) => self.capture_warning = Some("Recapture returned an empty image".into()),
                Err(error) => {
                    self.capture_warning = Some(format!("Recapture failed: {error}"));
                }
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

    fn continuous_capture_tick(&mut self, ctx: &egui::Context) {
        if !self.settings.ruler.continuous_capture
            || self.frame.backend != CaptureBackend::X11
            || self.refresh_task.is_some()
        {
            return;
        }
        let capture_interval = if self.point == self.last_capture_point {
            Duration::from_millis(1_250)
        } else {
            Duration::from_millis(180)
        };
        if self.last_capture.elapsed() >= capture_interval {
            self.begin_refresh(ctx);
        } else {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }

    fn current_record(&self) -> Option<MeasurementRecord> {
        match self.mode {
            RulerMode::Bounds => self.drag_start.map(|start| MeasurementRecord {
                rect: MeasureRect::from_points(start, self.point),
                anchor: self.point,
                mode: self.mode,
            }),
            RulerMode::Spacing | RulerMode::Horizontal | RulerMode::Vertical => {
                Some(MeasurementRecord {
                    rect: detect_edges(
                        &self.frame,
                        self.point,
                        self.settings.ruler.per_color_channel_edge_detection,
                        self.tolerance,
                    ),
                    anchor: self.point,
                    mode: self.mode,
                })
            }
        }
    }

    fn copy_records(&self, ctx: &egui::Context, current: Option<MeasurementRecord>) {
        let mut records = self.retained.clone();
        if let Some(record) = current {
            records.push(record);
        }
        let text = records
            .iter()
            .map(|record| {
                let (width, height) = axes(record.mode);
                record
                    .rect
                    .clipboard_text(width, height, self.settings.ruler.units, self.frame.dpi)
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            copy_text(ctx, text);
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context, image_rect: Rect, toolbar_rect: Rect) {
        let pointer = ctx.input(|input| input.pointer.hover_pos());
        if let Some(position) = pointer
            && self
                .last_pointer
                .is_none_or(|last| last.distance(position) > 0.25)
        {
            if !toolbar_rect.contains(position)
                && let Some(point) = self.screen_to_pixel(position, image_rect)
            {
                self.point = point;
            }
            self.last_pointer = Some(position);
        }
        let (ctrl, shift, keys, escape, refresh, pressed, released, secondary) =
            ctx.input(|input| {
                (
                    input.modifiers.ctrl,
                    input.modifiers.shift,
                    [
                        input.key_pressed(egui::Key::Num1),
                        input.key_pressed(egui::Key::Num2),
                        input.key_pressed(egui::Key::Num3),
                        input.key_pressed(egui::Key::Num4),
                    ],
                    input.key_pressed(egui::Key::Escape),
                    input.key_pressed(egui::Key::R),
                    input.pointer.button_pressed(egui::PointerButton::Primary),
                    input.pointer.button_released(egui::PointerButton::Primary),
                    input.pointer.button_clicked(egui::PointerButton::Secondary),
                )
            });
        if ctrl {
            if keys[0] {
                self.set_mode(RulerMode::Bounds);
            } else if keys[1] {
                self.set_mode(RulerMode::Spacing);
            } else if keys[2] {
                self.set_mode(RulerMode::Horizontal);
            } else if keys[3] {
                self.set_mode(RulerMode::Vertical);
            }
        }
        if refresh {
            self.begin_refresh(ctx);
        }
        let tolerance_steps = wheel_steps(ctx, &mut self.wheel_remainder);
        let tolerance_adjustment = tolerance_steps
            .unsigned_abs()
            .saturating_mul(15)
            .min(u32::from(u8::MAX)) as u8;
        if tolerance_steps > 0 {
            self.tolerance = self.tolerance.saturating_add(tolerance_adjustment);
        } else if tolerance_steps < 0 {
            self.tolerance = self.tolerance.saturating_sub(tolerance_adjustment);
        }
        let over_toolbar = pointer.is_some_and(|position| toolbar_rect.contains(position));
        if escape {
            if self.mode == RulerMode::Bounds {
                self.copy_records(ctx, self.current_record());
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if secondary && !over_toolbar {
            if self.mode == RulerMode::Bounds {
                if self.drag_start.take().is_none() {
                    if self.retained.is_empty() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        self.retained.clear();
                    }
                }
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        if self.mode == RulerMode::Bounds {
            if pressed && !over_toolbar {
                self.drag_start = Some(self.point);
            }
            if released && self.drag_start.is_some() && !over_toolbar {
                let record = self.current_record();
                self.copy_records(ctx, record);
                if shift && let Some(record) = record {
                    self.retained.push(record);
                }
                self.drag_start = None;
            }
        } else if released && !over_toolbar {
            let record = self.current_record();
            self.copy_records(ctx, record);
            if shift && let Some(record) = record {
                self.retained.push(record);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn draw_record(&self, painter: &egui::Painter, image: Rect, record: MeasurementRecord) {
        let color =
            parse_rgba(&self.settings.ruler.cross_color).unwrap_or(Color32::from_rgb(255, 69, 0));
        let stroke = Stroke::new(1.5, color);
        let map_x = |x: f32| image.left() + x / self.frame.width as f32 * image.width();
        let map_y = |y: f32| image.top() + y / self.frame.height as f32 * image.height();
        let left = map_x(record.rect.left as f32);
        let right = map_x((record.rect.right + 1) as f32);
        let top = map_y(record.rect.top as f32);
        let bottom = map_y((record.rect.bottom + 1) as f32);
        let anchor = Pos2::new(
            map_x(record.anchor.x as f32 + 0.5),
            map_y(record.anchor.y as f32 + 0.5),
        );
        match record.mode {
            RulerMode::Bounds => {
                painter.rect_stroke(
                    Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, bottom)),
                    0.0,
                    stroke,
                    StrokeKind::Inside,
                );
            }
            RulerMode::Spacing | RulerMode::Horizontal | RulerMode::Vertical => {
                let (horizontal, vertical) = axes(record.mode);
                if horizontal {
                    painter.line_segment(
                        [Pos2::new(left, anchor.y), Pos2::new(right, anchor.y)],
                        stroke,
                    );
                    if self.settings.ruler.draw_feet_on_cross {
                        painter.line_segment(
                            [
                                Pos2::new(left, anchor.y - 4.0),
                                Pos2::new(left, anchor.y + 4.0),
                            ],
                            stroke,
                        );
                        painter.line_segment(
                            [
                                Pos2::new(right, anchor.y - 4.0),
                                Pos2::new(right, anchor.y + 4.0),
                            ],
                            stroke,
                        );
                    }
                }
                if vertical {
                    painter.line_segment(
                        [Pos2::new(anchor.x, top), Pos2::new(anchor.x, bottom)],
                        stroke,
                    );
                    if self.settings.ruler.draw_feet_on_cross {
                        painter.line_segment(
                            [
                                Pos2::new(anchor.x - 4.0, top),
                                Pos2::new(anchor.x + 4.0, top),
                            ],
                            stroke,
                        );
                        painter.line_segment(
                            [
                                Pos2::new(anchor.x - 4.0, bottom),
                                Pos2::new(anchor.x + 4.0, bottom),
                            ],
                            stroke,
                        );
                    }
                }
            }
        }
        let (show_width, show_height) = axes(record.mode);
        let text = record.rect.display_text(
            show_width,
            show_height,
            self.settings.ruler.units,
            self.frame.dpi,
        );
        let lines = text.lines().count() as f32;
        let label_size = Vec2::new(160.0, 12.0 + lines * 18.0);
        let mut label = Rect::from_center_size(anchor + Vec2::new(0.0, -34.0), label_size);
        label = label.translate(Vec2::new(
            (image.left() - label.left()).max(0.0) - (label.right() - image.right()).max(0.0),
            (image.top() - label.top()).max(0.0) - (label.bottom() - image.bottom()).max(0.0),
        ));
        painter.rect_filled(label, 5.0, Color32::from_black_alpha(205));
        painter.rect_stroke(
            label,
            5.0,
            Stroke::new(1.0, Color32::from_white_alpha(55)),
            StrokeKind::Inside,
        );
        painter.text(
            label.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::monospace(13.0),
            Color32::WHITE,
        );
    }

    fn draw_measurements(&self, ctx: &egui::Context, image_rect: Rect) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            "ruler-measurements".into(),
        ));
        for record in self.retained.iter().copied() {
            self.draw_record(&painter, image_rect, record);
        }
        if let Some(record) = self.current_record() {
            self.draw_record(&painter, image_rect, record);
        }
    }
}

impl eframe::App for RulerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.poll_refresh(ctx) {
            ctx.set_cursor_icon(egui::CursorIcon::Progress);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                .show(ctx, |_| {});
            return;
        }
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        self.continuous_capture_tick(ctx);
        let live_transparent =
            self.settings.ruler.continuous_capture && self.frame.backend == CaptureBackend::X11;
        let image_rect = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let rect = self.image_rect(ui.max_rect());
                if !live_transparent {
                    self.texture.paint(ui.painter(), rect);
                }
                rect
            })
            .inner;
        let toolbar_rect = self.toolbar(ctx);
        self.handle_input(ctx, image_rect, toolbar_rect);
        self.draw_measurements(ctx, image_rect);
        if self.frame.backend == CaptureBackend::Portal {
            egui::Area::new("wayland-continuous-note".into())
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0])
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(
                            "Wayland snapshot — use Recapture (R) after content underneath changes",
                        );
                    });
                });
        }
        if let Some(warning) = &self.capture_warning {
            egui::Area::new("capture-warning".into())
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -48.0])
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.colored_label(Color32::LIGHT_RED, warning);
                    });
                });
        }
    }
}

fn mode_button(ui: &mut egui::Ui, mode: RulerMode, selected: bool) -> egui::Response {
    let response = ui.add(
        egui::Button::new(format!("      {}", mode.label()))
            .selected(selected)
            .corner_radius(7)
            .min_size(Vec2::new(0.0, 30.0)),
    );
    let stroke = ui
        .style()
        .interact_selectable(&response, selected)
        .fg_stroke;
    let icon = Rect::from_center_size(
        Pos2::new(response.rect.left() + 17.0, response.rect.center().y),
        Vec2::splat(15.0),
    );
    paint_mode_icon(ui.painter(), icon, mode, stroke);
    response
}

fn paint_mode_icon(painter: &egui::Painter, rect: Rect, mode: RulerMode, stroke: Stroke) {
    let center = rect.center();
    match mode {
        RulerMode::Bounds => {
            painter.rect_stroke(rect.shrink(1.5), 2.0, stroke, StrokeKind::Inside);
        }
        RulerMode::Spacing => {
            painter.line_segment(
                [
                    Pos2::new(rect.left(), center.y),
                    Pos2::new(rect.right(), center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, rect.top()),
                    Pos2::new(center.x, rect.bottom()),
                ],
                stroke,
            );
            for x in [rect.left(), rect.right()] {
                painter.line_segment(
                    [Pos2::new(x, center.y - 2.5), Pos2::new(x, center.y + 2.5)],
                    stroke,
                );
            }
            for y in [rect.top(), rect.bottom()] {
                painter.line_segment(
                    [Pos2::new(center.x - 2.5, y), Pos2::new(center.x + 2.5, y)],
                    stroke,
                );
            }
            painter.circle_filled(center, 1.75, stroke.color);
        }
        RulerMode::Horizontal => {
            painter.line_segment(
                [
                    Pos2::new(rect.left(), center.y),
                    Pos2::new(rect.right(), center.y),
                ],
                stroke,
            );
            for x in [rect.left(), rect.right()] {
                painter.line_segment(
                    [Pos2::new(x, center.y - 3.5), Pos2::new(x, center.y + 3.5)],
                    stroke,
                );
            }
        }
        RulerMode::Vertical => {
            painter.line_segment(
                [
                    Pos2::new(center.x, rect.top()),
                    Pos2::new(center.x, rect.bottom()),
                ],
                stroke,
            );
            for y in [rect.top(), rect.bottom()] {
                painter.line_segment(
                    [Pos2::new(center.x - 3.5, y), Pos2::new(center.x + 3.5, y)],
                    stroke,
                );
            }
        }
    }
}

fn mode_hint(mode: RulerMode) -> &'static str {
    match mode {
        RulerMode::Bounds => "Drag to measure a rectangle",
        RulerMode::Spacing => "Measure matching-color space in both directions",
        RulerMode::Horizontal => "Measure matching-color horizontal space",
        RulerMode::Vertical => "Measure matching-color vertical space",
    }
}

fn axes(mode: RulerMode) -> (bool, bool) {
    match mode {
        RulerMode::Horizontal => (true, false),
        RulerMode::Vertical => (false, true),
        RulerMode::Bounds | RulerMode::Spacing => (true, true),
    }
}
