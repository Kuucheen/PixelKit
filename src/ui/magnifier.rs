use super::{
    TiledCaptureTexture, adjusted_zoom_level, configure_style,
    loupe::{
        Loupe, LoupePlacement, capture_image_rect, draw_loupe as draw_shared_loupe, point_position,
        screen_to_pixel,
    },
    overlay_options, wheel_steps,
};
use crate::{
    APP_NAME,
    capture::{CaptureBackend, CaptureFrame, capture_screen},
    config::{MagnifierStyle, Settings},
    measurement::Point,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect};
use std::{path::Path, sync::Arc};

const FALLBACK_DPI: f32 = 96.0;

pub fn run_magnifier(image_path: Option<&Path>) -> anyhow::Result<()> {
    let settings = Settings::load_or_default();
    let frame = if let Some(path) = image_path {
        CaptureFrame::from_path(path, FALLBACK_DPI)?
    } else {
        capture_screen(settings.magnifier.interactive_portal, FALLBACK_DPI)?
    };
    let title = format!("Magnifier — {APP_NAME}");
    let options = overlay_options(&title);
    super::map_eframe(eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(MagnifierApp::new(cc, settings, frame)))
        }),
    ))
}

struct MagnifierApp {
    settings: Settings,
    frame: Arc<CaptureFrame>,
    texture: TiledCaptureTexture,
    point: Point,
    zoom_level: i32,
    wheel_remainder: f32,
}

impl MagnifierApp {
    fn new(cc: &eframe::CreationContext<'_>, settings: Settings, frame: CaptureFrame) -> Self {
        let point = Point {
            x: frame.width / 2,
            y: frame.height / 2,
        };
        let texture = TiledCaptureTexture::load(&cc.egui_ctx, "pixelkit-magnifier-capture", &frame);
        let zoom_level = i32::from(settings.magnifier.initial_zoom_level);
        Self {
            settings,
            frame: Arc::new(frame),
            texture,
            point,
            zoom_level,
            wheel_remainder: 0.0,
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context, image_rect: Rect) {
        if let Some(position) = ctx.input(|input| input.pointer.hover_pos())
            && let Some(point) = screen_to_pixel(&self.frame, position, image_rect)
        {
            self.point = point;
        }

        let zoom_steps = wheel_steps(ctx, &mut self.wheel_remainder);
        if zoom_steps != 0 {
            self.zoom_level = adjusted_zoom_level(
                self.zoom_level,
                zoom_steps,
                self.settings.magnifier.maximum_zoom_level,
            )
            .max(1);
        }

        let close = ctx.input(|input| {
            input.key_pressed(egui::Key::Escape) || input.key_pressed(egui::Key::Backspace)
        });
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn draw_loupe(&self, ctx: &egui::Context, image_rect: Rect) {
        let anchor = ctx
            .input(|input| input.pointer.hover_pos())
            .unwrap_or_else(|| point_position(&self.frame, self.point, image_rect));
        let tooltip = self.settings.magnifier.style == MagnifierStyle::Tooltip;
        draw_shared_loupe(
            ctx,
            Loupe {
                frame: &self.frame,
                point: self.point,
                anchor,
                image_rect,
                zoom_level: self.zoom_level,
                cells: i32::from(self.settings.magnifier.grid_size),
                placement: if tooltip {
                    LoupePlacement::Tooltip
                } else {
                    LoupePlacement::Centered
                },
                details: None,
                layer_id: "magnifier-loupe",
            },
        );
    }
}

impl eframe::App for MagnifierApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.settings.magnifier.change_cursor {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::BLACK))
            .show(ctx, |ui| {
                let available = ui.max_rect();
                let image_rect = capture_image_rect(&self.frame, available);
                self.texture.paint(ui.painter(), image_rect);
                self.handle_input(ctx, image_rect);

                let source = match self.frame.backend {
                    CaptureBackend::X11 => "direct X11 capture",
                    CaptureBackend::Portal => "Wayland portal snapshot",
                    CaptureBackend::File => "image preview",
                };
                let style = match self.settings.magnifier.style {
                    MagnifierStyle::Centered => "centered on pointer",
                    MagnifierStyle::Tooltip => "tooltip",
                };
                let help = format!(
                    "{style}  •  wheel zooms  •  Esc closes  •  zoom {}/{}  •  {}×{} grid  •  {source}",
                    self.zoom_level,
                    self.settings.magnifier.maximum_zoom_level,
                    self.settings.magnifier.grid_size,
                    self.settings.magnifier.grid_size,
                );
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        Pos2::new(available.left() + 10.0, available.bottom() - 36.0),
                        Pos2::new(available.right() - 10.0, available.bottom() - 8.0),
                    ),
                    7.0,
                    Color32::from_black_alpha(205),
                );
                ui.painter().text(
                    Pos2::new(available.center().x, available.bottom() - 22.0),
                    egui::Align2::CENTER_CENTER,
                    help,
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
                self.draw_loupe(ctx, image_rect);
            });
    }
}
