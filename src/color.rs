//! Color conversion and formatting compatible with the PowerToys picker formats.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn hex(self) -> String {
        format!("{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn parse_hex(value: &str) -> Option<Self> {
        let value = value.trim().trim_start_matches('#');
        let expanded;
        let value = if value.len() == 3 {
            expanded = value.chars().flat_map(|c| [c, c]).collect::<String>();
            expanded.as_str()
        } else {
            value
        };
        if value.len() != 6 && value.len() != 8 {
            return None;
        }
        Some(Self {
            r: u8::from_str_radix(&value[0..2], 16).ok()?,
            g: u8::from_str_radix(&value[2..4], 16).ok()?,
            b: u8::from_str_radix(&value[4..6], 16).ok()?,
        })
    }

    pub fn hue(self) -> f64 {
        let r = self.r as f64 / 255.0;
        let g = self.g as f64 / 255.0;
        let b = self.b as f64 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        if delta == 0.0 {
            return 0.0;
        }
        let hue = if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
        if hue < 0.0 { hue + 360.0 } else { hue }
    }

    pub fn hsv(self) -> (f64, f64, f64) {
        let min = self.r.min(self.g).min(self.b) as f64 / 255.0;
        let max = self.r.max(self.g).max(self.b) as f64 / 255.0;
        (
            self.hue(),
            if max == 0.0 { 0.0 } else { (max - min) / max },
            max,
        )
    }

    pub fn hsl(self) -> (f64, f64, f64) {
        let min = self.r.min(self.g).min(self.b) as f64 / 255.0;
        let max = self.r.max(self.g).max(self.b) as f64 / 255.0;
        let lightness = (max + min) / 2.0;
        let saturation = if min == max {
            0.0
        } else if lightness <= 0.5 {
            (max - min) / (max + min)
        } else {
            (max - min) / (2.0 - max - min)
        };
        (self.hue(), saturation, lightness)
    }

    pub fn hsi(self) -> (f64, f64, f64) {
        if self == Self::BLACK {
            return (0.0, 0.0, 0.0);
        }
        let r = self.r as f64 / 255.0;
        let g = self.g as f64 / 255.0;
        let b = self.b as f64 / 255.0;
        let intensity = (r + g + b) / 3.0;
        (self.hue(), 1.0 - r.min(g).min(b) / intensity, intensity)
    }

    pub fn hwb(self) -> (f64, f64, f64) {
        let min = self.r.min(self.g).min(self.b) as f64 / 255.0;
        let max = self.r.max(self.g).max(self.b) as f64 / 255.0;
        (self.hue(), min, 1.0 - max)
    }

    pub fn cmyk(self) -> (f64, f64, f64, f64) {
        if self == Self::BLACK {
            return (0.0, 0.0, 0.0, 1.0);
        }
        let r = self.r as f64 / 255.0;
        let g = self.g as f64 / 255.0;
        let b = self.b as f64 / 255.0;
        let k = 1.0 - r.max(g).max(b);
        let d = 1.0 - k;
        ((1.0 - r - k) / d, (1.0 - g - k) / d, (1.0 - b - k) / d, k)
    }

    pub fn xyz(self) -> (f64, f64, f64) {
        let linear = |v: u8| {
            let v = v as f64 / 255.0;
            if v > 0.04045 {
                ((v + 0.055) / 1.055).powf(2.4)
            } else {
                v / 12.92
            }
        };
        let (r, g, b) = (linear(self.r), linear(self.g), linear(self.b));
        (
            r * 0.412_390_799_265_959_5 + g * 0.357_584_339_383_878 + b * 0.180_480_788_401_834_3,
            r * 0.212_639_005_871_510_4 + g * 0.715_168_678_767_755_9 + b * 0.072_192_315_360_733_7,
            r * 0.019_330_818_715_591_9 + g * 0.119_194_779_794_626 + b * 0.950_532_152_249_660_6,
        )
    }

    pub fn lab(self) -> (f64, f64, f64) {
        let (mut x, mut y, mut z) = self.xyz();
        x /= 0.950_455_927_051_671_7;
        y /= 1.0;
        z /= 1.089_057_750_759_878_4;
        let delta: f64 = 6.0 / 29.0;
        let transform = |v: f64| {
            if v > delta.powi(3) {
                v.cbrt()
            } else {
                v / (3.0 * delta.powi(2)) + 16.0 / 116.0
            }
        };
        let (fx, fy, fz) = (transform(x), transform(y), transform(z));
        (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
    }

    pub fn oklab(self) -> (f64, f64, f64) {
        let linear = |v: u8| {
            let v = v as f64 / 255.0;
            if v > 0.04045 {
                ((v + 0.055) / 1.055).powf(2.4)
            } else {
                v / 12.92
            }
        };
        let (r, g, b) = (linear(self.r), linear(self.g), linear(self.b));
        let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
        let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
        let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
        (
            0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
            1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
            0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
        )
    }

    pub fn oklch(self) -> (f64, f64, f64) {
        let (l, a, b) = self.oklab();
        let c = (a * a + b * b).sqrt();
        let h = if (c * 1000.0).round() == 0.0 {
            0.0
        } else {
            (b.atan2(a).to_degrees() + 360.0) % 360.0
        };
        (l, c, h)
    }

    pub fn from_hsv(hue: f64, saturation: f64, value: f64) -> Self {
        let h = hue.rem_euclid(360.0);
        let s = saturation.clamp(0.0, 1.0);
        let v = value.clamp(0.0, 1.0);
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;
        let (r, g, b) = match h {
            h if h < 60.0 => (c, x, 0.0),
            h if h < 120.0 => (x, c, 0.0),
            h if h < 180.0 => (0.0, c, x),
            h if h < 240.0 => (0.0, x, c),
            h if h < 300.0 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Self::new(to_byte(r + m), to_byte(g + m), to_byte(b + m))
    }

    pub fn name(self) -> &'static str {
        let (mut hue, mut sat, mut lum) = self.hsl();
        hue = if hue == 0.0 { 0.0 } else { hue / 360.0 * 255.0 };
        sat *= 255.0;
        lum *= 255.0;
        if lum > 240.0 {
            return "White";
        }
        if lum < 20.0 {
            return "Black";
        }
        if sat <= 20.0 {
            return if lum > 170.0 {
                "Light gray"
            } else if lum > 100.0 {
                "Gray"
            } else {
                "Dark gray"
            };
        }
        const HUE_LIMITS: [[i32; 23]; 5] = [
            [
                8, 0, 0, 44, 0, 0, 0, 63, 0, 0, 122, 0, 134, 0, 0, 0, 0, 166, 176, 241, 0, 256, 0,
            ],
            [
                0, 10, 0, 32, 46, 0, 0, 0, 61, 0, 106, 0, 136, 144, 0, 0, 0, 158, 166, 241, 0, 0,
                256,
            ],
            [
                0, 8, 0, 0, 39, 46, 0, 0, 0, 71, 120, 0, 131, 144, 0, 0, 163, 0, 177, 211, 249, 0,
                256,
            ],
            [
                0, 11, 26, 0, 0, 38, 45, 0, 0, 56, 100, 121, 129, 0, 140, 0, 180, 0, 0, 224, 241,
                0, 256,
            ],
            [
                0, 13, 27, 0, 0, 36, 45, 0, 0, 59, 118, 0, 127, 136, 142, 0, 185, 0, 0, 216, 239,
                0, 256,
            ],
        ];
        const LOW: [i32; 23] = [
            130, 100, 115, 100, 100, 100, 110, 75, 100, 90, 100, 100, 100, 100, 80, 100, 100, 100,
            100, 100, 100, 100, 100,
        ];
        const HIGH: [i32; 23] = [
            170, 170, 170, 155, 170, 170, 170, 170, 170, 115, 170, 170, 170, 170, 170, 170, 170,
            170, 150, 150, 170, 140, 165,
        ];
        const LIGHT: [&str; 23] = [
            "Coral",
            "Rose",
            "Light orange",
            "Tan",
            "Tan",
            "Light yellow",
            "Light yellow",
            "Tan",
            "Light green",
            "Lime",
            "Light green",
            "Light green",
            "Aqua",
            "Sky blue",
            "Light turquoise",
            "Pale blue",
            "Light blue",
            "Ice blue",
            "Periwinkle",
            "Lavender",
            "Pink",
            "Tan",
            "Rose",
        ];
        const MID: [&str; 23] = [
            "Coral",
            "Red",
            "Orange",
            "Brown",
            "Tan",
            "Gold",
            "Yellow",
            "Olive green",
            "Olive green",
            "Green",
            "Green",
            "Bright green",
            "Teal",
            "Aqua",
            "Turquoise",
            "Pale blue",
            "Blue",
            "Blue gray",
            "Indigo",
            "Purple",
            "Pink",
            "Brown",
            "Red",
        ];
        const DARK: [&str; 23] = [
            "Brown",
            "Dark red",
            "Brown",
            "Brown",
            "Brown",
            "Dark yellow",
            "Dark yellow",
            "Brown",
            "Dark green",
            "Dark green",
            "Dark green",
            "Dark green",
            "Dark teal",
            "Dark teal",
            "Dark teal",
            "Dark blue",
            "Dark blue",
            "Blue gray",
            "Indigo",
            "Dark purple",
            "Plum",
            "Brown",
            "Dark red",
        ];
        let level = if sat <= 75.0 {
            0
        } else if sat <= 115.0 {
            1
        } else if sat <= 150.0 {
            2
        } else if sat <= 240.0 {
            3
        } else {
            4
        };
        let index = HUE_LIMITS[level]
            .iter()
            .position(|limit| hue < *limit as f64)
            .unwrap_or(22);
        if lum > HIGH[index] as f64 {
            LIGHT[index]
        } else if lum < LOW[index] as f64 {
            DARK[index]
        } else {
            MID[index]
        }
    }
}

fn to_byte(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub const FORMAT_NAMES: [&str; 16] = [
    "HEX", "RGB", "HSL", "HSV", "CMYK", "HSB", "HSI", "HWB", "NCol", "CIEXYZ", "CIELAB", "Oklab",
    "Oklch", "VEC4", "Decimal", "HEX Int",
];

pub fn default_format(name: &str) -> &'static str {
    match name {
        "HEX" => "%Rex%Grx%Blx",
        "RGB" => "rgb(%Re, %Gr, %Bl)",
        "HSL" => "hsl(%Hu, %Sl%, %Ll%)",
        "HSV" => "hsv(%Hu, %Sb%, %Va%)",
        "CMYK" => "cmyk(%Cy%, %Ma%, %Ye%, %Bk%)",
        "HSB" => "hsb(%Hu, %Sb%, %Br%)",
        "HSI" => "hsi(%Hu, %Si%, %In%)",
        "HWB" => "hwb(%Hu, %Wh%, %Bn%)",
        "NCol" => "%Hn, %Wh%, %Bn%",
        "CIEXYZ" => "XYZ(%Xv, %Yv, %Zv)",
        "CIELAB" => "CIELab(%Lc, %Ca, %Cb)",
        "Oklab" => "oklab(%Lo, %Oa, %Ob)",
        "Oklch" => "oklch(%Lo, %Oc, %Oh)",
        "VEC4" => "(%Reff, %Grff, %Blff, 1f)",
        "Decimal" => "%Dv",
        "HEX Int" => "0xFF%ReX%GrX%BlX",
        _ => "",
    }
}

pub fn format_named(color: Rgb, name: &str) -> String {
    format_template(color, default_format(name))
}

/// Applies PowerToys-compatible `%XX` format tokens, including optional byte
/// format suffixes (`b`, `h`, `H`, `x`, `X`, `f`, `F`). `%Na` inserts the
/// localized-independent English color name.
pub fn format_template(color: Rgb, template: &str) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::with_capacity(template.len() + 16);
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' || i + 2 >= chars.len() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let token = [chars[i + 1], chars[i + 2]].iter().collect::<String>();
        if token == "Na" {
            out.push_str(color.name());
            i += 3;
            continue;
        }
        let default = default_token_format(&token);
        if default.is_none() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let suffix = chars
            .get(i + 3)
            .copied()
            .filter(|c| "bhHxXfFpis".contains(*c));
        let format = suffix.unwrap_or(default.unwrap());
        out.push_str(&format_token(color, &token, format));
        i += if suffix.is_some() { 4 } else { 3 };
    }
    out
}

fn default_token_format(token: &str) -> Option<char> {
    match token {
        "Re" | "Gr" | "Bl" | "Al" => Some('b'),
        "Cy" | "Ma" | "Ye" | "Bk" | "Si" | "Sl" | "Sb" | "Br" | "In" | "Ll" | "Va" | "Wh"
        | "Bn" | "Lc" | "Ca" | "Cb" | "Lo" | "Oa" | "Ob" | "Oc" | "Oh" => Some('p'),
        "Hu" | "Hn" | "Xv" | "Yv" | "Zv" | "Dr" | "Dv" => Some('i'),
        _ => None,
    }
}

fn format_token(color: Rgb, token: &str, format: char) -> String {
    let byte = match token {
        "Re" => Some(color.r),
        "Gr" => Some(color.g),
        "Bl" => Some(color.b),
        "Al" => Some(255),
        _ => None,
    };
    if let Some(value) = byte {
        return match format {
            'h' => format!("{:x}", value / 16),
            'H' => format!("{:X}", value / 16),
            'x' => format!("{value:02x}"),
            'X' => format!("{value:02X}"),
            'f' => compact(value as f64 / 255.0, 2, true),
            'F' => compact(value as f64 / 255.0, 2, false),
            _ => value.to_string(),
        };
    }
    let percent = |v: f64| compact((v * 100.0).round(), 0, true);
    match token {
        "Cy" => percent(color.cmyk().0),
        "Ma" => percent(color.cmyk().1),
        "Ye" => percent(color.cmyk().2),
        "Bk" => percent(color.cmyk().3),
        "Hu" => compact(color.hue().round(), 0, true),
        "Hn" => natural_hue(color),
        "Sb" => percent(color.hsv().1),
        "Br" | "Va" => percent(color.hsv().2),
        "Si" => percent(color.hsi().1),
        "In" => percent(color.hsi().2),
        "Sl" => percent(color.hsl().1),
        "Ll" => percent(color.hsl().2),
        "Wh" => percent(color.hwb().1),
        "Bn" => percent(color.hwb().2),
        "Lc" => percent_value(color.lab().0, format, 2),
        "Ca" => percent_value(color.lab().1, format, 2),
        "Cb" => percent_value(color.lab().2, format, 2),
        "Lo" => compact(color.oklab().0, 2, true),
        "Oa" => compact(color.oklab().1, 2, true),
        "Ob" => compact(color.oklab().2, 2, true),
        "Oc" => compact(color.oklch().1, 2, true),
        "Oh" => compact(color.oklch().2, 2, true),
        "Xv" => compact(color.xyz().0 * 100.0, 4, true),
        "Yv" => compact(color.xyz().1 * 100.0, 4, true),
        "Zv" => compact(color.xyz().2 * 100.0, 4, true),
        "Dr" => ((color.r as u32) * 65_536 + (color.g as u32) * 256 + color.b as u32).to_string(),
        "Dv" => (color.r as u32 + (color.g as u32) * 256 + (color.b as u32) * 65_536).to_string(),
        _ => String::new(),
    }
}

fn compact(value: f64, decimals: usize, leading_zero: bool) -> String {
    let mut value = if value.abs() < 0.5 * 10f64.powi(-(decimals as i32)) {
        0.0
    } else {
        value
    };
    if decimals == 0 {
        value = value.round();
    }
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if !leading_zero {
        if let Some(rest) = text.strip_prefix("0.") {
            return format!(".{rest}");
        }
        if let Some(rest) = text.strip_prefix("-0.") {
            return format!("-.{rest}");
        }
    }
    text
}

fn percent_value(value: f64, format: char, decimals: usize) -> String {
    if format == 'i' {
        compact(value.round(), 0, true)
    } else {
        compact(value, decimals, true)
    }
}

fn natural_hue(color: Rgb) -> String {
    let hue = color.hue();
    let (letter, value) = if hue < 60.0 {
        ('R', hue / 0.6)
    } else if hue < 120.0 {
        ('Y', (hue - 60.0) / 0.6)
    } else if hue < 180.0 {
        ('G', (hue - 120.0) / 0.6)
    } else if hue < 240.0 {
        ('C', (hue - 180.0) / 0.6)
    } else if hue < 300.0 {
        ('B', (hue - 240.0) / 0.6)
    } else {
        ('M', (hue - 300.0) / 0.6)
    };
    let mut out = String::new();
    let _ = write!(out, "{letter}{}", value.round() as i32);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_toys_red_formats_match() {
        let red = Rgb::new(255, 0, 0);
        let cases = [
            ("HEX", "ff0000"),
            ("RGB", "rgb(255, 0, 0)"),
            ("HSL", "hsl(0, 100%, 50%)"),
            ("HSV", "hsv(0, 100%, 100%)"),
            ("CMYK", "cmyk(0%, 100%, 100%, 0%)"),
            ("HSI", "hsi(0, 100%, 33%)"),
            ("VEC4", "(1f, 0f, 0f, 1f)"),
            ("Decimal", "255"),
            ("HEX Int", "0xFFFF0000"),
        ];
        for (name, expected) in cases {
            assert_eq!(format_named(red, name), expected, "{name}");
        }
    }

    #[test]
    fn black_and_white_color_spaces() {
        assert_eq!(format_named(Rgb::BLACK, "CIELAB"), "CIELab(0, 0, 0)");
        assert_eq!(format_named(Rgb::BLACK, "Oklch"), "oklch(0, 0, 0)");
        let (l, a, b) = Rgb::WHITE.lab();
        assert!((l - 100.0).abs() < 0.01 && a.abs() < 0.01 && b.abs() < 0.01);
    }

    #[test]
    fn parses_short_and_long_hex() {
        assert_eq!(Rgb::parse_hex("#f80"), Some(Rgb::new(255, 136, 0)));
        assert_eq!(Rgb::parse_hex("336699"), Some(Rgb::new(51, 102, 153)));
        assert_eq!(Rgb::parse_hex("nope"), None);
    }

    #[test]
    fn custom_tokens_and_name_work() {
        let text = format_template(Rgb::new(255, 0, 0), "#%ReX%GrX%BlX — %Na");
        assert_eq!(text, "#FF0000 — Red");
    }
}
