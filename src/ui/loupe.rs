use super::{color32, zoom_cell_size};
use crate::{capture::CaptureFrame, measurement::Point};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoupePlacement {
    Centered,
    Tooltip,
}

pub(super) struct LoupeDetails<'a> {
    pub value: &'a str,
    pub name: Option<&'a str>,
}

pub(super) struct Loupe<'a> {
    pub frame: &'a CaptureFrame,
    pub point: Point,
    pub anchor: Pos2,
    pub image_rect: Rect,
    pub zoom_level: i32,
    pub cells: i32,
    pub placement: LoupePlacement,
    pub details: Option<LoupeDetails<'a>>,
    pub layer_id: &'static str,
}

pub(super) fn capture_image_rect(frame: &CaptureFrame, available: Rect) -> Rect {
    let scale =
        (available.width() / frame.width as f32).min(available.height() / frame.height as f32);
    let size = Vec2::new(frame.width as f32 * scale, frame.height as f32 * scale);
    Rect::from_center_size(available.center(), size)
}

pub(super) fn screen_to_pixel(
    frame: &CaptureFrame,
    position: Pos2,
    image_rect: Rect,
) -> Option<Point> {
    if !image_rect.contains(position) {
        return None;
    }
    let x =
        ((position.x - image_rect.left()) / image_rect.width() * frame.width as f32).floor() as u32;
    let y = ((position.y - image_rect.top()) / image_rect.height() * frame.height as f32).floor()
        as u32;
    Some(Point {
        x: x.min(frame.width - 1),
        y: y.min(frame.height - 1),
    })
}

pub(super) fn point_position(frame: &CaptureFrame, point: Point, image_rect: Rect) -> Pos2 {
    Pos2::new(
        image_rect.left() + (point.x as f32 + 0.5) / frame.width as f32 * image_rect.width(),
        image_rect.top() + (point.y as f32 + 0.5) / frame.height as f32 * image_rect.height(),
    )
}

fn loupe_origin(
    anchor: Pos2,
    image_rect: Rect,
    content_width: f32,
    total_height: f32,
    grid_size: f32,
    placement: LoupePlacement,
) -> Pos2 {
    if placement == LoupePlacement::Centered {
        return Pos2::new(anchor.x - content_width * 0.5, anchor.y - grid_size * 0.5);
    }

    let mut origin = anchor + Vec2::new(24.0, 24.0);
    if origin.x + content_width + 12.0 > image_rect.right() {
        origin.x = anchor.x - content_width - 24.0;
    }
    if origin.y + total_height + 12.0 > image_rect.bottom() {
        origin.y = anchor.y - total_height - 24.0;
    }
    let minimum = image_rect.min + Vec2::splat(8.0);
    let maximum = Pos2::new(
        (image_rect.right() - content_width - 8.0).max(minimum.x),
        (image_rect.bottom() - total_height - 8.0).max(minimum.y),
    );
    origin.x = origin.x.clamp(minimum.x, maximum.x);
    origin.y = origin.y.clamp(minimum.y, maximum.y);
    origin
}

pub(super) fn draw_loupe(ctx: &egui::Context, loupe: Loupe<'_>) {
    if loupe.zoom_level == 0 {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        loupe.layer_id.into(),
    ));
    let cell = zoom_cell_size(loupe.zoom_level);
    let grid_size = loupe.cells as f32 * cell;
    let color = loupe
        .frame
        .pixel_checked(loupe.point.x, loupe.point.y)
        .unwrap_or_default();
    let value_font = FontId::monospace(15.0);
    let name_font = FontId::proportional(13.0);
    let (value_width, name_width, footer_height) =
        loupe.details.as_ref().map_or((0.0, 0.0, 0.0), |details| {
            let value_width = painter
                .layout_no_wrap(details.value.to_owned(), value_font.clone(), Color32::WHITE)
                .size()
                .x;
            let name_width = details.name.map_or(0.0, |name| {
                painter
                    .layout_no_wrap(name.to_owned(), name_font.clone(), Color32::LIGHT_GRAY)
                    .size()
                    .x
            });
            let footer_height =
                10.0 + 18.0 + 7.0 + 18.0 + if details.name.is_some() { 20.0 } else { 0.0 } + 4.0;
            (value_width, name_width, footer_height)
        });
    let available_width = (loupe.image_rect.width() - 30.0).max(grid_size);
    let content_width = grid_size
        .max(value_width + 12.0)
        .max(name_width + 12.0)
        .min(available_width);
    let total_height = grid_size + footer_height;
    let origin = loupe_origin(
        loupe.anchor,
        loupe.image_rect,
        content_width,
        total_height,
        grid_size,
        loupe.placement,
    );
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
    let radius = loupe.cells / 2;
    for gy in 0..loupe.cells {
        for gx in 0..loupe.cells {
            let px = (loupe.point.x as i64 + i64::from(gx - radius))
                .clamp(0, i64::from(loupe.frame.width - 1)) as u32;
            let py = (loupe.point.y as i64 + i64::from(gy - radius))
                .clamp(0, i64::from(loupe.frame.height - 1)) as u32;
            let rect = Rect::from_min_size(
                grid_origin + Vec2::new(gx as f32 * cell, gy as f32 * cell),
                Vec2::splat(cell + 0.25),
            );
            painter.rect_filled(
                rect,
                0.0,
                color32(loupe.frame.pixel_checked(px, py).unwrap_or_default()),
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

    let Some(details) = loupe.details else {
        return;
    };
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
        details.value,
        value_font,
        Color32::WHITE,
    );
    if let Some(name) = details.name {
        painter.text(
            Pos2::new(origin.x + content_width * 0.5, value_top + 20.0),
            egui::Align2::CENTER_TOP,
            name,
            name_font,
            Color32::LIGHT_GRAY,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_loupe_stays_centered_at_screen_edges() {
        let pointer = Pos2::new(4.0, 6.0);
        let origin = loupe_origin(
            pointer,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0)),
            130.0,
            130.0,
            130.0,
            LoupePlacement::Centered,
        );
        assert_eq!(origin, Pos2::new(-61.0, -59.0));
        assert_eq!(origin + Vec2::splat(65.0), pointer);
    }

    #[test]
    fn tooltip_flips_away_from_the_bottom_right_edge() {
        let image_rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        let origin = loupe_origin(
            Pos2::new(490.0, 390.0),
            image_rect,
            130.0,
            200.0,
            130.0,
            LoupePlacement::Tooltip,
        );
        assert_eq!(origin, Pos2::new(336.0, 166.0));
    }
}
