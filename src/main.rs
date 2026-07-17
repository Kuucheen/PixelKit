use anyhow::{Context, Result, bail};
use pixelkit::{
    APP_NAME, VERSION,
    color::{FORMAT_NAMES, Rgb, format_named},
    config::{EditorView, history_path, settings_path},
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
        "color-editor" | "editor" => {
            let arguments = editor_arguments(&rest)?;
            ui::run_editor(arguments.initial, arguments.view, arguments.record_initial)
        }
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

#[derive(Debug, Default, PartialEq, Eq)]
struct EditorArguments {
    initial: Option<Rgb>,
    view: Option<EditorView>,
    record_initial: bool,
}

fn editor_arguments(arguments: &[std::ffi::OsString]) -> Result<EditorArguments> {
    let mut parsed = EditorArguments::default();
    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index]
            .to_str()
            .context("editor option is not valid UTF-8")?;
        let value = arguments
            .get(index + 1)
            .context("editor option is missing a value")?
            .to_str()
            .context("editor option value is not valid UTF-8")?;
        match flag {
            "--color" | "--selection" => {
                if parsed.initial.is_some() {
                    bail!("editor color was specified more than once");
                }
                parsed.initial = Some(
                    Rgb::parse_hex(value)
                        .context("expected a 3-, 6-, or 8-digit hexadecimal color")?,
                );
                parsed.record_initial = flag == "--color";
            }
            "--view" => {
                if parsed.view.is_some() {
                    bail!("editor view was specified more than once");
                }
                parsed.view = Some(match value {
                    "compact" => EditorView::Compact,
                    "full" => EditorView::Full,
                    _ => bail!("expected editor view to be compact or full"),
                });
            }
            _ => bail!("unknown editor option: {flag}"),
        }
        index += 2;
    }
    Ok(parsed)
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
    pixelkit color-editor [--color HEX] [--view compact|full]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn picked_editor_color_is_recorded() {
        let arguments = [OsString::from("--color"), OsString::from("a201ff")];
        let parsed = editor_arguments(&arguments).unwrap();
        assert_eq!(parsed.initial, Some(Rgb::new(162, 1, 255)));
        assert!(parsed.record_initial);
        assert_eq!(parsed.view, None);
    }

    #[test]
    fn editor_view_handoff_preserves_selection_without_recording_it() {
        let arguments = [
            OsString::from("--selection"),
            OsString::from("a201ff"),
            OsString::from("--view"),
            OsString::from("compact"),
        ];
        let parsed = editor_arguments(&arguments).unwrap();
        assert_eq!(parsed.initial, Some(Rgb::new(162, 1, 255)));
        assert!(!parsed.record_initial);
        assert_eq!(parsed.view, Some(EditorView::Compact));
    }
}
