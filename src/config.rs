use crate::color::{FORMAT_NAMES, Rgb, default_format};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivationAction {
    Editor,
    #[default]
    Picker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClickAction {
    #[default]
    PickThenEditor,
    PickAndClose,
    Close,
}

impl ClickAction {
    pub const ALL: [Self; 3] = [Self::PickThenEditor, Self::PickAndClose, Self::Close];
    pub const fn label(self) -> &'static str {
        match self {
            Self::PickThenEditor => "Pick color, then open editor",
            Self::PickAndClose => "Pick color and close",
            Self::Close => "Close without picking",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditorView {
    Compact,
    #[default]
    Full,
}

impl EditorView {
    pub const ALL: [Self; 2] = [Self::Compact, Self::Full];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Full => "Full editor",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditorViewSwitchPosition {
    #[default]
    Centered,
    TopLeft,
}

impl EditorViewSwitchPosition {
    pub const ALL: [Self; 2] = [Self::Centered, Self::TopLeft];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Centered => "Keep window centered",
            Self::TopLeft => "Keep top-left corner",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RulerMode {
    #[default]
    Bounds,
    Spacing,
    Horizontal,
    Vertical,
}

impl RulerMode {
    pub const ALL: [Self; 4] = [
        Self::Bounds,
        Self::Spacing,
        Self::Horizontal,
        Self::Vertical,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bounds => "Bounds",
            Self::Spacing => "Spacing",
            Self::Horizontal => "Horizontal spacing",
            Self::Vertical => "Vertical spacing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    #[default]
    Pixels,
    Inches,
    Centimetres,
    Millimetres,
}

impl Unit {
    pub const ALL: [Self; 4] = [
        Self::Pixels,
        Self::Inches,
        Self::Centimetres,
        Self::Millimetres,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pixels => "Pixels",
            Self::Inches => "Inches",
            Self::Centimetres => "Centimetres",
            Self::Millimetres => "Millimetres",
        }
    }
    pub const fn abbreviation(self) -> &'static str {
        match self {
            Self::Pixels => "px",
            Self::Inches => "in",
            Self::Centimetres => "cm",
            Self::Millimetres => "mm",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormatSetting {
    pub name: String,
    pub enabled: bool,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PickerSettings {
    pub shortcut: String,
    pub activation_action: ActivationAction,
    pub copied_format: String,
    pub primary_click: ClickAction,
    pub middle_click: ClickAction,
    pub secondary_click: ClickAction,
    pub change_cursor: bool,
    pub show_color_name: bool,
    pub history_limit: usize,
    pub default_editor_view: EditorView,
    pub editor_view_switch_position: EditorViewSwitchPosition,
    pub single_editor_instance: bool,
    pub formats: Vec<FormatSetting>,
    /// Let the portal show its target selector. This may add an extra prompt,
    /// but is useful on multi-monitor Wayland sessions.
    pub interactive_portal: bool,
}

impl Default for PickerSettings {
    fn default() -> Self {
        Self {
            shortcut: "Super+Shift+C".into(),
            activation_action: ActivationAction::Picker,
            copied_format: "HEX".into(),
            primary_click: ClickAction::PickThenEditor,
            middle_click: ClickAction::PickAndClose,
            secondary_click: ClickAction::Close,
            change_cursor: false,
            show_color_name: false,
            history_limit: 20,
            default_editor_view: EditorView::Compact,
            editor_view_switch_position: EditorViewSwitchPosition::Centered,
            single_editor_instance: true,
            formats: FORMAT_NAMES
                .iter()
                .map(|name| FormatSetting {
                    name: (*name).into(),
                    enabled: matches!(*name, "HEX" | "RGB" | "HSL"),
                    template: default_format(name).into(),
                })
                .collect(),
            interactive_portal: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RulerSettings {
    pub shortcut: String,
    pub default_mode: RulerMode,
    pub continuous_capture: bool,
    pub draw_feet_on_cross: bool,
    pub per_color_channel_edge_detection: bool,
    pub pixel_tolerance: u8,
    /// RGBA in CSS notation. Alpha is honored by the overlay.
    pub cross_color: String,
    pub units: Unit,
    /// Used when monitor physical dimensions are unavailable through a portal.
    pub fallback_dpi: f32,
    pub interactive_portal: bool,
}

impl Default for RulerSettings {
    fn default() -> Self {
        Self {
            shortcut: "Super+Shift+M".into(),
            default_mode: RulerMode::Bounds,
            continuous_capture: false,
            draw_feet_on_cross: true,
            per_color_channel_edge_detection: false,
            pixel_tolerance: 30,
            cross_color: "#FF4500FF".into(),
            units: Unit::Pixels,
            fallback_dpi: 96.0,
            interactive_portal: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub picker: PickerSettings,
    pub ruler: RulerSettings,
}

impl Settings {
    pub fn load() -> Result<Self> {
        let path = settings_path()?;
        if !path.exists() {
            let settings = Self::default();
            settings.save()?;
            return Ok(settings);
        }
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let mut settings: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid settings in {}", path.display()))?;
        settings.normalize();
        Ok(settings)
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_else(|error| {
            eprintln!("PixelKit: {error:#}");
            Self::default()
        })
    }

    pub fn save(&self) -> Result<()> {
        atomic_json_write(&settings_path()?, self)
    }

    pub fn normalize(&mut self) {
        self.picker.history_limit = self.picker.history_limit.clamp(1, 10_000);
        self.ruler.fallback_dpi = self.ruler.fallback_dpi.clamp(20.0, 1000.0);
        for name in FORMAT_NAMES {
            if !self.picker.formats.iter().any(|item| item.name == name) {
                self.picker.formats.push(FormatSetting {
                    name: name.into(),
                    enabled: false,
                    template: default_format(name).into(),
                });
            }
        }
        if !self
            .picker
            .formats
            .iter()
            .any(|item| item.name == self.picker.copied_format)
        {
            self.picker.copied_format = "HEX".into();
        }
    }

    pub fn selected_format(&self) -> &str {
        self.picker
            .formats
            .iter()
            .find(|item| item.name == self.picker.copied_format)
            .map(|item| item.template.as_str())
            .unwrap_or_else(|| default_format("HEX"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct History {
    pub colors: Vec<Rgb>,
}

impl History {
    pub fn load() -> Result<Self> {
        let path = history_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("invalid color history in {}", path.display()))
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    pub fn push(&mut self, color: Rgb, limit: usize) -> Result<()> {
        self.colors.retain(|item| *item != color);
        self.colors.insert(0, color);
        self.colors.truncate(limit.max(1));
        self.save()
    }

    pub fn save(&self) -> Result<()> {
        atomic_json_write(&history_path()?, self)
    }
}

pub fn config_dir() -> Result<PathBuf> {
    xdg_dir("XDG_CONFIG_HOME", ".config").map(|path| path.join("pixelkit"))
}
pub fn data_dir() -> Result<PathBuf> {
    xdg_dir("XDG_DATA_HOME", ".local/share").map(|path| path.join("pixelkit"))
}
pub fn settings_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("settings.json"))
}
pub fn history_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("history.json"))
}

fn xdg_dir(variable: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(value) = env::var_os(variable).filter(|value| !value.is_empty()) {
        return Ok(value.into());
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(fallback))
}

fn atomic_json_write(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(&temporary, bytes)
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_expose_all_power_toys_formats() {
        let settings = Settings::default();
        assert_eq!(settings.picker.formats.len(), FORMAT_NAMES.len());
        assert_eq!(
            settings
                .picker
                .formats
                .iter()
                .filter(|format| format.enabled)
                .count(),
            3
        );
        assert_eq!(settings.ruler.pixel_tolerance, 30);
        assert_eq!(settings.picker.default_editor_view, EditorView::Compact);
        assert_eq!(
            settings.picker.editor_view_switch_position,
            EditorViewSwitchPosition::Centered
        );
        assert!(settings.picker.single_editor_instance);
    }

    #[test]
    fn old_partial_json_receives_defaults() {
        let settings: Settings =
            serde_json::from_str(r#"{"picker":{"copied_format":"RGB"}}"#).unwrap();
        assert_eq!(settings.picker.copied_format, "RGB");
        assert_eq!(settings.picker.default_editor_view, EditorView::Compact);
        assert_eq!(
            settings.picker.editor_view_switch_position,
            EditorViewSwitchPosition::Centered
        );
        assert!(settings.picker.single_editor_instance);
        assert_eq!(settings.ruler.cross_color, "#FF4500FF");
    }

    #[test]
    fn editor_preferences_serialize_stably() {
        let settings: Settings = serde_json::from_str(
            r#"{"picker":{"default_editor_view":"full","editor_view_switch_position":"top_left","single_editor_instance":false}}"#,
        )
        .unwrap();
        assert_eq!(settings.picker.default_editor_view, EditorView::Full);
        assert_eq!(
            settings.picker.editor_view_switch_position,
            EditorViewSwitchPosition::TopLeft
        );
        assert!(!settings.picker.single_editor_instance);

        let json = serde_json::to_value(settings).unwrap();
        assert_eq!(json["picker"]["default_editor_view"], "full");
        assert_eq!(json["picker"]["editor_view_switch_position"], "top_left");
        assert_eq!(json["picker"]["single_editor_instance"], false);
    }

    #[test]
    fn compact_editor_preference_deserializes() {
        let settings: Settings =
            serde_json::from_str(r#"{"picker":{"default_editor_view":"compact"}}"#).unwrap();
        assert_eq!(settings.picker.default_editor_view, EditorView::Compact);
    }
}
