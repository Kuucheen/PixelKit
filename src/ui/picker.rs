use super::{
    TiledCaptureTexture, adjusted_zoom_level, configure_style, copy_text,
    loupe::{
        Loupe, LoupeDetails, LoupePlacement, capture_image_rect, draw_loupe as draw_shared_loupe,
        point_position, screen_to_pixel,
    },
    overlay_options, spawn_editor, wheel_steps,
};
use crate::{
    APP_NAME,
    capture::{CaptureBackend, CaptureFrame, capture_screen},
    color::{Rgb, format_template},
    config::{
        ActivationAction, ClickAction, History, LoupeStyle, STANDARD_PICKER_MAX_ZOOM_LEVEL,
        Settings,
    },
    measurement::Point,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke};
use std::{path::Path, sync::Arc};

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
    let title = format!("Color Picker — {APP_NAME}");
    let options = overlay_options(&title);
    super::map_eframe(eframe::run_native(
        &title,
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
                if let Some(point) = screen_to_pixel(&self.frame, position, image_rect) {
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
        let color = self.selected_color();
        let value = format_template(color, self.settings.selected_format());
        let name = self.settings.picker.show_color_name.then(|| color.name());
        let selected_position = point_position(&self.frame, self.point, image_rect);
        let centered = self.settings.picker.loupe_style == LoupeStyle::Centered;
        let anchor = if centered {
            ctx.input(|input| input.pointer.hover_pos())
                .unwrap_or(selected_position)
        } else {
            selected_position
        };
        draw_shared_loupe(
            ctx,
            Loupe {
                frame: &self.frame,
                point: self.point,
                anchor,
                image_rect,
                zoom_level: self.zoom_level,
                cells: 13,
                placement: if centered {
                    LoupePlacement::Centered
                } else {
                    LoupePlacement::Tooltip
                },
                details: Some(LoupeDetails {
                    value: &value,
                    name,
                }),
                layer_id: "picker-loupe",
            },
        );
    }
}

impl eframe::App for PickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.settings.picker.change_cursor {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::BLACK)).show(ctx, |ui| {
            let available = ui.max_rect();
            let image_rect = capture_image_rect(&self.frame, available);
            self.texture.paint(ui.painter(), image_rect);
            self.handle_input(ctx, image_rect);
            let point_screen = point_position(&self.frame, self.point, image_rect);
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
    use crate::ui::zoom_cell_size;

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
