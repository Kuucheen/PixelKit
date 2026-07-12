//! Shared PixelKit color, capture, configuration, and measurement logic.

pub mod capture;
pub mod color;
pub mod config;
pub mod daemon;
pub mod measurement;
pub mod ui;

pub const APP_ID: &str = "io.github.Kuucheen.PixelKit";
pub const APP_NAME: &str = "PixelKit";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
