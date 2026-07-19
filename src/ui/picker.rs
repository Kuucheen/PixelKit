use super::{
    TiledCaptureTexture, color32, configure_style, copy_text, overlay_options, spawn_editor,
    wheel_steps,
};
use crate::{
    APP_NAME,
    capture::{CaptureBackend, CaptureFrame, capture_screen},
    color::{Rgb, format_template},
    config::{ActivationAction, ClickAction, History, STANDARD_PICKER_MAX_ZOOM_LEVEL, Settings},
    measurement::Point,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};
use std::{path::Path, sync::Arc};

fn adjusted_zoom_level(current: i32, steps: i32, maximum: u8) -> i32 {
    current.saturating_add(steps).clamp(0, i32::from(maximum))
}

fn zoom_cell_size(level: i32) -> f32 {
    let level = level.max(0) as f32;
    let standard_max = f32::from(STANDARD_PICKER_MAX_ZOOM_LEVEL);
    if level <= standard_max {
        3.0 + level * 1.7
    } else {
        // Preserve the existing standard levels, then grow more gradually so
        // high custom levels remain usable without pushing the loupe off-screen.
        3.0 + standard_max * 1.7 + (level - standard_max).sqrt() * 1.7
    }
}

pub fn run_picker(image_path: Option<&Path>) -> anyhow::Result<()> {
    let settings = Settings::load_or_default();
    if settings.picker.activation_action == ActivationAction::Editor && image_path.is_none() {
        return super::run_editor(None, None, false);
    }
    let frame = if let Some(path) = image_path {
        CaptureFrame::from_path(path, settings.ruler.fallback_dpi)?
    } else {
        capture_screen(
            settings.picker.interactive_portal,
            settings.ruler.fallback_dpi,
        )?
    };
    let options = overlay_options(&format!("Color Picker — {APP_NAME}"));
    super::map_eframe(eframe::run_native(
        &format!("Color Picker — {APP_NAME}"),
        options,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(PickerApp::new(cc, settings, frame)))
        }),
    ))
}

struct PickerApp {
    settings: Settings,
    frame: Arc<CaptureFrame>,
    texture: TiledCaptureTexture,
    point: Point,
    last_pointer: Option<Pos2>,
    zoom_level: i32,
    wheel_remainder: f32,
    completed: bool,
}

impl PickerApp {
    fn new(cc: &eframe::CreationContext<'_>, settings: Settings, frame: CaptureFrame) -> Self {
        let point = Point {
            x: frame.width / 2,
            y: frame.height / 2,
        };
        let texture = TiledCaptureTexture::load(&cc.egui_ctx, "pixelkit-screen-capture", &frame);
        Self {
            settings,
            frame: Arc::new(frame),
            texture,
            point,
            last_pointer: None,
            zoom_level: 2,
            wheel_remainder: 0.0,
            completed: false,
        }
    }

    fn image_rect(&self, available: Rect) -> Rect {
        let scale = (available.width() / self.frame.width as f32)
            .min(available.height() / self.frame.height as f32);
        let size = Vec2::new(
            self.frame.width as f32 * scale,
            self.frame.height as f32 * scale,
        );
        Rect::from_center_size(available.center(), size)
    }

    fn screen_to_pixel(&self, position: Pos2, rect: Rect) -> Option<Point> {
        if !rect.contains(position) {
            return None;
        }
        let x =
            ((position.x - rect.left()) / rect.width() * self.frame.width as f32).floor() as u32;
        let y =
            ((position.y - rect.top()) / rect.height() * self.frame.height as f32).floor() as u32;
        Some(Point {
            x: x.min(self.frame.width - 1),
            y: y.min(self.frame.height - 1),
        })
    }

    fn selected_color(&self) -> Rgb {
        self.frame
            .pixel_checked(self.point.x, self.point.y)
            .unwrap_or_default()
    }

    fn handle_input(&mut self, ctx: &egui::Context, image_rect: Rect) {
        let pointer = ctx.input(|input| input.pointer.hover_pos());
        if let Some(position) = pointer {
            let moved = self
                .last_pointer
                .is_none_or(|last| last.distance(position) > 0.25);
            if moved {
                if let Some(point) = self.screen_to_pixel(position, image_rect) {
                    self.point = point;
                }
                self.last_pointer = Some(position);
            }
        }
        let (
            left,
            right,
            up,
            down,
            enter,
            space,
            escape,
            backspace,
            primary,
            middle,
            secondary,
            shift,
        ) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowRight),
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Space),
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::Backspace),
                input.pointer.button_clicked(egui::PointerButton::Primary),
                input.pointer.button_clicked(egui::PointerButton::Middle),
                input.pointer.button_clicked(egui::PointerButton::Secondary),
                input.modifiers.shift,
            )
        });
        let step = if shift { 10 } else { 1 };
        if left {
            self.point.x = self.point.x.saturating_sub(step);
        }
        if right {
            self.point.x = (self.point.x + step).min(self.frame.width - 1);
        }
        if up {
            self.point.y = self.point.y.saturating_sub(step);
        }
        if down {
            self.point.y = (self.point.y + step).min(self.frame.height - 1);
        }
        let zoom_steps = wheel_steps(ctx, &mut self.wheel_remainder);
        if zoom_steps != 0 {
            let maximum = if self.settings.picker.use_standard_zoom_range {
                STANDARD_PICKER_MAX_ZOOM_LEVEL
            } else {
                self.settings.picker.maximum_zoom_level
            };
            self.zoom_level = adjusted_zoom_level(self.zoom_level, zoom_steps, maximum);
        }
        if escape || backspace {
            self.finish(ctx, ClickAction::Close);
        } else if enter || space || primary {
            self.finish(ctx, self.settings.picker.primary_click);
        } else if middle {
            self.finish(ctx, self.settings.picker.middle_click);
        } else if secondary {
            self.finish(ctx, self.settings.picker.secondary_click);
        }
    }

    fn finish(&mut self, ctx: &egui::Context, action: ClickAction) {
        if self.completed {
            return;
        }
        self.completed = true;
        if action != ClickAction::Close {
            let color = self.selected_color();
            let text = format_template(color, self.settings.selected_format());
            copy_text(ctx, text);
            let mut history = History::load_or_default();
            if let Err(error) = history.push(color, self.settings.picker.history_limit) {
                eprintln!("PixelKit: failed to save color history: {error:#}");
            }
            if action == ClickAction::PickThenEditor
                && let Err(error) = spawn_editor(color)
            {
                eprintln!("PixelKit: failed to open editor: {error:#}");
            }
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn draw_loupe(&self, ctx: &egui::Context, image_rect: Rect) {
        if self.zoom_level == 0 {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            "picker-loupe".into(),
        ));
        let cells = 13_i32;
        let cell = zoom_cell_size(self.zoom_level);
        let grid_size = cells as f32 * cell;
        let color = self.selected_color();
        let value = format_template(color, self.settings.selected_format());
        let value_font = FontId::monospace(15.0);
        let value_width = painter
            .layout_no_wrap(value.clone(), value_font.clone(), Color32::WHITE)
            .size()
            .x;
        let name = self.settings.picker.show_color_name.then(|| color.name());
        let name_font = FontId::proportional(13.0);
        let name_width = name.as_ref().map_or(0.0, |name| {
            painter
                .layout_no_wrap(name.to_string(), name_font.clone(), Color32::LIGHT_GRAY)
                .size()
                .x
        });
        let available_width = (image_rect.width() - 30.0).max(grid_size);
        let content_width = grid_size
            .max(value_width + 12.0)
            .max(name_width + 12.0)
            .min(available_width);
        let footer_height =
            10.0 + 18.0 + 7.0 + 18.0 + if name.is_some() { 20.0 } else { 0.0 } + 4.0;
        let total_height = grid_size + footer_height;
        let pointer = Pos2::new(
            image_rect.left()
                + (self.point.x as f32 + 0.5) / self.frame.width as f32 * image_rect.width(),
            image_rect.top()
                + (self.point.y as f32 + 0.5) / self.frame.height as f32 * image_rect.height(),
        );
        let mut origin = pointer + Vec2::new(24.0, 24.0);
        if origin.x + content_width + 12.0 > image_rect.right() {
            origin.x = pointer.x - content_width - 24.0;
        }
        if origin.y + total_height + 12.0 > image_rect.bottom() {
            origin.y = pointer.y - total_height - 24.0;
        }
        let minimum = image_rect.min + Vec2::splat(8.0);
        let maximum = Pos2::new(
            (image_rect.right() - content_width - 8.0).max(minimum.x),
            (image_rect.bottom() - total_height - 8.0).max(minimum.y),
        );
        origin.x = origin.x.clamp(minimum.x, maximum.x);
        origin.y = origin.y.clamp(minimum.y, maximum.y);
        let box_rect = Rect::from_min_size(
            origin - Vec2::splat(7.0),
            Vec2::new(content_width + 14.0, total_height + 14.0),
        );
        painter.rect_filled(box_rect, 9.0, Color32::from_black_alpha(225));
        painter.rect_stroke(
            box_rect,
            9.0,
            Stroke::new(1.0, Color32::from_white_alpha(80)),
            StrokeKind::Outside,
        );
        let grid_origin = origin + Vec2::new((content_width - grid_size) * 0.5, 0.0);
        let radius = cells / 2;
        for gy in 0..cells {
            for gx in 0..cells {
                let px = (self.point.x as i64 + i64::from(gx - radius))
                    .clamp(0, i64::from(self.frame.width - 1)) as u32;
                let py = (self.point.y as i64 + i64::from(gy - radius))
                    .clamp(0, i64::from(self.frame.height - 1)) as u32;
                let rect = Rect::from_min_size(
                    grid_origin + Vec2::new(gx as f32 * cell, gy as f32 * cell),
                    Vec2::splat(cell + 0.25),
                );
                painter.rect_filled(
                    rect,
                    0.0,
                    color32(self.frame.pixel_checked(px, py).unwrap_or_default()),
                );
            }
        }
        let center = Rect::from_min_size(
            grid_origin + Vec2::new(radius as f32 * cell, radius as f32 * cell),
            Vec2::splat(cell),
        );
        painter.rect_stroke(
            center.expand(1.0),
            0.0,
            Stroke::new(2.0, Color32::WHITE),
            StrokeKind::Outside,
        );
        painter.rect_stroke(
            center.expand(3.0),
            0.0,
            Stroke::new(1.0, Color32::BLACK),
            StrokeKind::Outside,
        );
        let swatch = Rect::from_min_size(
            Pos2::new(grid_origin.x, origin.y + grid_size + 10.0),
            Vec2::new(grid_size, 18.0),
        );
        painter.rect_filled(swatch, 5.0, color32(color));
        painter.rect_stroke(
            swatch,
            5.0,
            Stroke::new(1.0, Color32::from_black_alpha(130)),
            StrokeKind::Inside,
        );
        painter.rect_stroke(
            swatch,
            5.0,
            Stroke::new(1.0, Color32::from_white_alpha(105)),
            StrokeKind::Outside,
        );
        let value_top = swatch.bottom() + 7.0;
        painter.text(
            Pos2::new(origin.x + content_width * 0.5, value_top),
            egui::Align2::CENTER_TOP,
            value,
            value_font,
            Color32::WHITE,
        );
        if let Some(name) = name {
            painter.text(
                Pos2::new(origin.x + content_width * 0.5, value_top + 20.0),
                egui::Align2::CENTER_TOP,
                name,
                name_font,
                Color32::LIGHT_GRAY,
            );
        }
    }
}

impl eframe::App for PickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.settings.picker.change_cursor {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::BLACK)).show(ctx, |ui| {
            let available = ui.max_rect();
            let image_rect = self.image_rect(available);
            self.texture.paint(ui.painter(), image_rect);
            self.handle_input(ctx, image_rect);
            let point_screen = Pos2::new(
                image_rect.left() + (self.point.x as f32 + 0.5) / self.frame.width as f32 * image_rect.width(),
                image_rect.top() + (self.point.y as f32 + 0.5) / self.frame.height as f32 * image_rect.height(),
            );
            ui.painter().circle_stroke(point_screen, 6.0, Stroke::new(1.5, Color32::WHITE));
            ui.painter().circle_stroke(point_screen, 8.0, Stroke::new(1.0, Color32::BLACK));
            let source = match self.frame.backend { CaptureBackend::X11 => "direct X11 capture", CaptureBackend::Portal => "Wayland portal snapshot", CaptureBackend::File => "image preview" };
            let help = format!("Click / Enter to pick  •  arrows move 1 px  •  Shift+arrows move 10 px  •  wheel zooms  •  Esc closes  •  {}×{}  •  {source}", self.frame.width, self.frame.height);
            ui.painter().rect_filled(Rect::from_min_max(Pos2::new(available.left() + 10.0, available.bottom() - 36.0), Pos2::new(available.right() - 10.0, available.bottom() - 8.0)), 7.0, Color32::from_black_alpha(205));
            ui.painter().text(Pos2::new(available.center().x, available.bottom() - 22.0), egui::Align2::CENTER_CENTER, help, FontId::proportional(13.0), Color32::WHITE);
            self.draw_loupe(ctx, image_rect);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_level_respects_standard_and_custom_limits() {
        assert_eq!(adjusted_zoom_level(5, 1, 5), 5);
        assert_eq!(adjusted_zoom_level(5, 1, 12), 6);
        assert_eq!(adjusted_zoom_level(11, 10, 12), 12);
        assert_eq!(adjusted_zoom_level(0, -1, 12), 0);
        assert_eq!(adjusted_zoom_level(254, 10, 255), 255);
    }

    #[test]
    fn zoom_level_adjustment_cannot_overflow() {
        assert_eq!(adjusted_zoom_level(i32::MAX, 1, 16), 16);
        assert_eq!(adjusted_zoom_level(i32::MIN, -1, 16), 0);
    }

    #[test]
    fn custom_zoom_sizes_preserve_standard_levels_and_remain_usable() {
        assert_eq!(zoom_cell_size(0), 3.0);
        assert_eq!(zoom_cell_size(5), 11.5);
        assert!(zoom_cell_size(6) > zoom_cell_size(5));
        assert!(zoom_cell_size(255) > zoom_cell_size(64));
        assert!(zoom_cell_size(255) < 40.0);
    }
}
