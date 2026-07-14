mod editor;
mod hub;
mod picker;
mod ruler;

use crate::{APP_ID, APP_NAME, capture::CaptureFrame, color::Rgb};
use anyhow::{Result, anyhow};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2, ViewportBuilder};
use std::{
    process::{Command, Stdio},
    sync::{Arc, OnceLock},
};

pub use editor::run_editor;
pub use hub::run_hub;
pub use picker::run_picker;
pub use ruler::run_ruler;

pub fn show_error(message: String) -> Result<()> {
    map_eframe(eframe::run_native(
        &format!("Error — {APP_NAME}"),
        native_options([620.0, 300.0]),
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(ErrorApp { message }))
        }),
    ))
}

struct ErrorApp {
    message: String,
}

impl eframe::App for ErrorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("PixelKit could not complete the request");
            ui.add_space(8.0);
            panel_frame().show(ui, |ui| {
                ui.label(&self.message);
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Copy details").clicked() {
                    copy_text(ctx, self.message.clone());
                }
                if ui.button("Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }
}

fn native_options(size: [f32; 2]) -> eframe::NativeOptions {
    native_options_with_min_size(size, [560.0, 420.0])
}

fn native_options_with_min_size(size: [f32; 2], min_size: [f32; 2]) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_app_id(APP_ID)
            .with_title(APP_NAME)
            .with_inner_size(size)
            .with_min_inner_size(min_size)
            .with_icon(app_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    }
}

fn overlay_options(title: &str) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_app_id(APP_ID)
            .with_title(title)
            .with_fullscreen(true)
            .with_decorations(false)
            .with_always_on_top()
            .with_active(true)
            .with_icon(app_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    }
}

fn app_icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    ICON.get_or_init(|| {
        let image = image::load_from_memory(include_bytes!(
            "../../packaging/linux/io.github.Kuucheen.PixelKit.png"
        ))
        .expect("embedded PixelKit icon must be a valid PNG")
        .into_rgba8();
        let (width, height) = image.dimensions();
        Arc::new(egui::IconData {
            rgba: image.into_raw(),
            width,
            height,
        })
    })
    .clone()
}

fn map_eframe(result: eframe::Result) -> Result<()> {
    result.map_err(|error| anyhow!(error.to_string()))
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 9.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(7);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(7);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(7);
    style.visuals.selection.bg_fill = Color32::from_rgb(0, 110, 210);
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    ctx.set_style(style);
}

fn spawn_action(action: &str) -> Result<()> {
    let executable = std::env::current_exe()?;
    Command::new(executable)
        .arg(action)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    Ok(())
}

fn copy_text(ctx: &egui::Context, value: String) {
    // The egui path integrates with the active Wayland/X11 event loop. Arboard
    // is also attempted so clipboard managers can retain the value when a
    // pick-and-close action immediately exits the overlay.
    ctx.copy_text(value.clone());
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(value);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CaptureTileRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

struct CaptureTextureTile {
    region: CaptureTileRegion,
    texture: egui::TextureHandle,
}

/// Renders a capture at its original resolution while respecting the GPU's
/// maximum size for any one texture. High-resolution and multi-monitor
/// captures are split into lossless tiles rather than downscaled.
struct TiledCaptureTexture {
    label: String,
    source_size: [u32; 2],
    tiles: Vec<CaptureTextureTile>,
}

impl TiledCaptureTexture {
    fn load(ctx: &egui::Context, label: impl Into<String>, frame: &CaptureFrame) -> Self {
        let label = label.into();
        let regions = capture_tile_regions(
            frame.width,
            frame.height,
            ctx.input(|input| input.max_texture_side),
        );
        let tiles = regions
            .into_iter()
            .enumerate()
            .map(|(index, region)| CaptureTextureTile {
                texture: ctx.load_texture(
                    format!("{label}-{index}"),
                    frame.egui_image_region(region.x, region.y, region.width, region.height),
                    egui::TextureOptions::NEAREST,
                ),
                region,
            })
            .collect();
        Self {
            label,
            source_size: [frame.width, frame.height],
            tiles,
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &CaptureFrame) {
        let regions = capture_tile_regions(
            frame.width,
            frame.height,
            ctx.input(|input| input.max_texture_side),
        );
        let can_reuse = self.source_size == [frame.width, frame.height]
            && self.tiles.len() == regions.len()
            && self
                .tiles
                .iter()
                .zip(&regions)
                .all(|(tile, region)| tile.region == *region);
        if !can_reuse {
            *self = Self::load(ctx, self.label.clone(), frame);
            return;
        }
        for tile in &mut self.tiles {
            let region = tile.region;
            tile.texture.set(
                frame.egui_image_region(region.x, region.y, region.width, region.height),
                egui::TextureOptions::NEAREST,
            );
        }
    }

    fn paint(&self, painter: &egui::Painter, target: Rect) {
        let source_width = self.source_size[0] as f32;
        let source_height = self.source_size[1] as f32;
        for tile in &self.tiles {
            let region = tile.region;
            let tile_rect = Rect::from_min_max(
                Pos2::new(
                    target.left() + region.x as f32 / source_width * target.width(),
                    target.top() + region.y as f32 / source_height * target.height(),
                ),
                Pos2::new(
                    target.left()
                        + (region.x + region.width) as f32 / source_width * target.width(),
                    target.top()
                        + (region.y + region.height) as f32 / source_height * target.height(),
                ),
            );
            painter.image(
                tile.texture.id(),
                tile_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}

fn capture_tile_regions(
    width: u32,
    height: u32,
    max_texture_side: usize,
) -> Vec<CaptureTileRegion> {
    let side = u32::try_from(max_texture_side).unwrap_or(u32::MAX).max(1);
    let mut regions = Vec::new();
    let mut y = 0;
    while y < height {
        let tile_height = side.min(height - y);
        let mut x = 0;
        while x < width {
            let tile_width = side.min(width - x);
            regions.push(CaptureTileRegion {
                x,
                y,
                width: tile_width,
                height: tile_height,
            });
            x += tile_width;
        }
        y += tile_height;
    }
    regions
}

/// Converts raw wheel events into intentional, device-independent steps.
/// Egui deliberately smooths notched mouse wheels over several frames; that is
/// pleasant for scrolling, but would turn one notch into many zoom changes.
fn wheel_steps(ctx: &egui::Context, smooth_remainder: &mut f32) -> i32 {
    let events = ctx.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel { unit, delta, .. } if delta.y != 0.0 => {
                    Some((*unit, delta.y))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    events
        .into_iter()
        .map(|(unit, delta)| wheel_event_steps(unit, delta, smooth_remainder))
        .sum()
}

fn wheel_event_steps(unit: egui::MouseWheelUnit, delta: f32, smooth_remainder: &mut f32) -> i32 {
    const SMOOTH_EVENT_LIMIT: f32 = 8.0;
    const SMOOTH_STEP_POINTS: f32 = 12.0;

    if !delta.is_finite() || delta == 0.0 {
        return 0;
    }
    if unit != egui::MouseWheelUnit::Point || delta.abs() >= SMOOTH_EVENT_LIMIT {
        *smooth_remainder = 0.0;
        return delta.signum() as i32;
    }
    *smooth_remainder += delta;
    let steps = (*smooth_remainder / SMOOTH_STEP_POINTS).trunc() as i32;
    *smooth_remainder -= steps as f32 * SMOOTH_STEP_POINTS;
    steps
}

fn color32(color: Rgb) -> Color32 {
    Color32::from_rgb(color.r, color.g, color.b)
}

fn contrasting_text(color: Rgb) -> Color32 {
    let luminance =
        0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b);
    if luminance > 145.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn parse_rgba(value: &str) -> Option<Color32> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 && value.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    let a = if value.len() == 8 {
        u8::from_str_radix(&value[6..8], 16).ok()?
    } else {
        255
    };
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_black_alpha(22))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(24)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_tiles_cover_the_full_image_without_scaling() {
        let regions = capture_tile_regions(5, 3, 2);
        assert_eq!(regions.len(), 6);
        assert!(
            regions
                .iter()
                .all(|region| region.width <= 2 && region.height <= 2)
        );
        assert_eq!(
            regions
                .iter()
                .map(|region| region.width * region.height)
                .sum::<u32>(),
            15
        );
        assert_eq!(
            regions.last(),
            Some(&CaptureTileRegion {
                x: 4,
                y: 2,
                width: 1,
                height: 1,
            })
        );
    }

    #[test]
    fn notched_wheel_event_is_exactly_one_step() {
        let mut remainder = 0.0;
        assert_eq!(
            wheel_event_steps(egui::MouseWheelUnit::Point, 120.0, &mut remainder),
            1
        );
        assert_eq!(
            wheel_event_steps(egui::MouseWheelUnit::Line, -3.0, &mut remainder),
            -1
        );
    }

    #[test]
    fn smooth_touchpad_events_accumulate_gradually() {
        let mut remainder = 0.0;
        assert_eq!(
            wheel_event_steps(egui::MouseWheelUnit::Point, 4.0, &mut remainder),
            0
        );
        assert_eq!(
            wheel_event_steps(egui::MouseWheelUnit::Point, 4.0, &mut remainder),
            0
        );
        assert_eq!(
            wheel_event_steps(egui::MouseWheelUnit::Point, 4.0, &mut remainder),
            1
        );
        assert_eq!(remainder, 0.0);
    }
}
