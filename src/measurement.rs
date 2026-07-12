use crate::{color::Rgb, config::Unit};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureRect {
    /// Inclusive pixel coordinates, matching PowerToys Screen Ruler.
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl MeasureRect {
    pub fn from_points(a: Point, b: Point) -> Self {
        Self {
            left: a.x.min(b.x),
            top: a.y.min(b.y),
            right: a.x.max(b.x),
            bottom: a.y.max(b.y),
        }
    }

    pub const fn width_px(self) -> u32 {
        self.right.saturating_sub(self.left) + 1
    }
    pub const fn height_px(self) -> u32 {
        self.bottom.saturating_sub(self.top) + 1
    }

    pub fn width(self, unit: Unit, dpi: f32) -> f32 {
        convert(self.width_px() as f32, unit, dpi)
    }
    pub fn height(self, unit: Unit, dpi: f32) -> f32 {
        convert(self.height_px() as f32, unit, dpi)
    }

    pub fn display_text(self, show_width: bool, show_height: bool, unit: Unit, dpi: f32) -> String {
        let pixels = dimensions(
            self.width_px() as f32,
            self.height_px() as f32,
            show_width,
            show_height,
        );
        let mut text = format!("{} px", pixels);
        if unit != Unit::Pixels {
            text.push_str(&format!(
                "\n({} {})",
                dimensions(
                    self.width(unit, dpi),
                    self.height(unit, dpi),
                    show_width,
                    show_height
                ),
                unit.abbreviation()
            ));
        }
        text
    }

    pub fn clipboard_text(
        self,
        show_width: bool,
        show_height: bool,
        unit: Unit,
        dpi: f32,
    ) -> String {
        let values = dimensions(
            self.width(unit, dpi),
            self.height(unit, dpi),
            show_width,
            show_height,
        );
        if unit == Unit::Pixels {
            values
        } else {
            format!("{} {}", values, unit.abbreviation())
        }
    }
}

fn dimensions(width: f32, height: f32, show_width: bool, show_height: bool) -> String {
    match (show_width, show_height) {
        (true, true) => format!("{} × {}", compact(width), compact(height)),
        (true, false) => compact(width),
        (false, true) => compact(height),
        (false, false) => String::new(),
    }
}

fn compact(value: f32) -> String {
    if (value - value.round()).abs() < 0.000_05 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn convert(pixels: f32, unit: Unit, dpi: f32) -> f32 {
    let dpi = dpi.max(1.0);
    match unit {
        Unit::Pixels => pixels,
        Unit::Inches => pixels / dpi,
        Unit::Centimetres => pixels / dpi * 2.54,
        Unit::Millimetres => pixels / dpi * 25.4,
    }
}

pub trait PixelSource {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn pixel(&self, x: u32, y: u32) -> Rgb;
}

/// Finds the contiguous region around `center` whose pixels remain within the
/// configured distance of the starting pixel. It intentionally compares each
/// candidate to the start pixel, matching PowerToys rather than accumulating
/// gradual color drift.
pub fn detect_edges(
    source: &impl PixelSource,
    center: Point,
    per_channel: bool,
    tolerance: u8,
) -> MeasureRect {
    if source.width() == 0 || source.height() == 0 {
        return MeasureRect::default();
    }
    let center = Point {
        x: center.x.min(source.width() - 1),
        y: center.y.min(source.height() - 1),
    };
    let start = source.pixel(center.x, center.y);
    let mut left = center.x;
    while left > 0
        && pixels_close(
            start,
            source.pixel(left - 1, center.y),
            per_channel,
            tolerance,
        )
    {
        left -= 1;
    }
    let mut right = center.x;
    while right + 1 < source.width()
        && pixels_close(
            start,
            source.pixel(right + 1, center.y),
            per_channel,
            tolerance,
        )
    {
        right += 1;
    }
    let mut top = center.y;
    while top > 0
        && pixels_close(
            start,
            source.pixel(center.x, top - 1),
            per_channel,
            tolerance,
        )
    {
        top -= 1;
    }
    let mut bottom = center.y;
    while bottom + 1 < source.height()
        && pixels_close(
            start,
            source.pixel(center.x, bottom + 1),
            per_channel,
            tolerance,
        )
    {
        bottom += 1;
    }
    MeasureRect {
        left,
        top,
        right,
        bottom,
    }
}

pub fn pixels_close(a: Rgb, b: Rgb, per_channel: bool, tolerance: u8) -> bool {
    let differences = [a.r.abs_diff(b.r), a.g.abs_diff(b.g), a.b.abs_diff(b.b)];
    if per_channel {
        differences
            .into_iter()
            .all(|difference| difference <= tolerance)
    } else {
        differences.into_iter().map(u16::from).sum::<u16>() <= u16::from(tolerance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Image {
        width: u32,
        pixels: Vec<Rgb>,
    }
    impl PixelSource for Image {
        fn width(&self) -> u32 {
            self.width
        }
        fn height(&self) -> u32 {
            self.pixels.len() as u32 / self.width
        }
        fn pixel(&self, x: u32, y: u32) -> Rgb {
            self.pixels[(y * self.width + x) as usize]
        }
    }

    #[test]
    fn inclusive_bounds_match_screen_ruler() {
        let rect = MeasureRect::from_points(Point { x: 10, y: 20 }, Point { x: 19, y: 24 });
        assert_eq!(rect.width_px(), 10);
        assert_eq!(rect.height_px(), 5);
        assert_eq!(
            rect.clipboard_text(true, true, Unit::Pixels, 96.0),
            "10 × 5"
        );
    }

    #[test]
    fn edge_detection_stops_on_changed_pixel() {
        let dark = Rgb::new(10, 10, 10);
        let light = Rgb::new(200, 200, 200);
        let image = Image {
            width: 5,
            pixels: vec![
                light, light, light, light, light, light, dark, dark, dark, light, light, dark,
                dark, dark, light,
            ],
        };
        let found = detect_edges(&image, Point { x: 2, y: 1 }, false, 30);
        assert_eq!(
            found,
            MeasureRect {
                left: 1,
                top: 1,
                right: 3,
                bottom: 2
            }
        );
    }

    #[test]
    fn distance_modes_differ() {
        let a = Rgb::new(100, 100, 100);
        let b = Rgb::new(110, 110, 110);
        assert!(pixels_close(a, b, true, 10));
        assert!(!pixels_close(a, b, false, 10));
    }

    #[test]
    fn physical_units_use_dpi() {
        let rect = MeasureRect {
            left: 0,
            top: 0,
            right: 95,
            bottom: 95,
        };
        assert!((rect.width(Unit::Inches, 96.0) - 1.0).abs() < f32::EPSILON);
        assert!((rect.width(Unit::Millimetres, 96.0) - 25.4).abs() < 0.001);
    }
}
