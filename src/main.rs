use anyhow::{Context, Result, bail};
use pixelkit::{
    APP_NAME, VERSION,
    color::{FORMAT_NAMES, Rgb, format_named},
    config::{history_path, settings_path},
    daemon, ui,
};
use std::{env, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("{APP_NAME}: {error:#}");
        let command = env::args().nth(1).unwrap_or_else(|| "settings".into());
        let graphical_command = matches!(
            command.as_str(),
            "settings"
                | "gui"
                | "color-picker"
                | "picker"
                | "screen-ruler"
                | "ruler"
                | "color-editor"
                | "editor"
                | "configure-shortcuts"
        );
        if graphical_command
            && (env::var_os("DISPLAY").is_some() || env::var_os("WAYLAND_DISPLAY").is_some())
        {
            let _ = ui::show_error(format!("{error:#}"));
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let command = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| "settings".into());
    let rest: Vec<_> = arguments.collect();
    match command.as_str() {
        "settings" | "gui" => ui::run_hub(),
        "color-picker" | "picker" => ui::run_picker(image_argument(&rest)?.as_deref()),
        "screen-ruler" | "ruler" => ui::run_ruler(image_argument(&rest)?.as_deref()),
        "color-editor" | "editor" => ui::run_editor(color_argument(&rest)?),
        "daemon" => daemon::run(),
        "configure-shortcuts" => daemon::configure_shortcuts(),
        "formats" => print_formats(&rest),
        "paths" => {
            println!("settings={}", settings_path()?.display());
            println!("history={}", history_path()?.display());
            Ok(())
        }
        "--version" | "-V" | "version" => {
            println!("{APP_NAME} {VERSION}");
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        unknown => {
            print_help();
            bail!("unknown command: {unknown}")
        }
    }
}

fn image_argument(arguments: &[std::ffi::OsString]) -> Result<Option<PathBuf>> {
    if arguments.is_empty() {
        return Ok(None);
    }
    if arguments.len() == 2 && arguments[0] == "--image" {
        return Ok(Some(PathBuf::from(&arguments[1])));
    }
    bail!("expected --image <PNG>")
}

fn color_argument(arguments: &[std::ffi::OsString]) -> Result<Option<Rgb>> {
    if arguments.is_empty() {
        return Ok(None);
    }
    if arguments.len() == 2 && arguments[0] == "--color" {
        let value = arguments[1].to_str().context("color is not valid UTF-8")?;
        return Rgb::parse_hex(value)
            .map(Some)
            .context("expected a 3-, 6-, or 8-digit hexadecimal color");
    }
    bail!("expected --color <HEX>")
}

fn print_formats(arguments: &[std::ffi::OsString]) -> Result<()> {
    if arguments.len() != 1 {
        bail!("usage: pixelkit formats <HEX>");
    }
    let value = arguments[0].to_str().context("color is not valid UTF-8")?;
    let color = Rgb::parse_hex(value).context("expected a 3-, 6-, or 8-digit hexadecimal color")?;
    for name in FORMAT_NAMES {
        println!("{name:8} {}", format_named(color, name));
    }
    Ok(())
}

fn print_help() {
    println!(
        r#"PixelKit {VERSION}

Native Linux color picker and screen ruler.

USAGE:
    pixelkit [settings]
    pixelkit color-picker [--image FILE.png]
    pixelkit color-editor [--color HEX]
    pixelkit screen-ruler [--image FILE.png]
    pixelkit daemon
    pixelkit configure-shortcuts
    pixelkit formats HEX
    pixelkit paths

The --image option runs either overlay against a PNG and is useful for demos,
automated UI tests, and desktops where screen-capture permissions are disabled.
"#
    );
}
