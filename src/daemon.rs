use crate::{APP_ID, capture::is_wayland_session, config::Settings};
use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use std::{
    env, fs,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    process::{Command, Stdio},
};

const PICKER_ID: &str = "color-picker";
const MAGNIFIER_ID: &str = "magnifier";
const RULER_ID: &str = "screen-ruler";

type PortalShortcut = (String, String, String);

pub fn run() -> Result<()> {
    let _lock = DaemonLock::acquire()?;
    let settings = Settings::load_or_default();
    if is_wayland_session() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        runtime.block_on(run_portal(settings))
    } else {
        run_x11(settings)
    }
}

pub fn configure_shortcuts() -> Result<()> {
    if !is_wayland_session() {
        return Err(anyhow!(
            "desktop-managed shortcut configuration is only needed on Wayland"
        ));
    }
    let settings = Settings::load_or_default();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(configure_portal_shortcuts(settings))
}

fn run_x11(settings: Settings) -> Result<()> {
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
    let picker: HotKey = settings
        .picker
        .shortcut
        .parse()
        .context("invalid Color Picker shortcut")?;
    let ruler: HotKey = settings
        .ruler
        .shortcut
        .parse()
        .context("invalid Screen Ruler shortcut")?;
    let magnifier: HotKey = settings
        .magnifier
        .shortcut
        .parse()
        .context("invalid Magnifier shortcut")?;
    if picker.id() == ruler.id() || picker.id() == magnifier.id() || magnifier.id() == ruler.id() {
        return Err(anyhow!(
            "Color Picker, Magnifier, and Screen Ruler shortcuts must all differ"
        ));
    }
    let manager =
        GlobalHotKeyManager::new().context("failed to initialize X11 global shortcuts")?;
    manager
        .register_all(&[picker, magnifier, ruler])
        .context("failed to register an X11 shortcut; another application may already own it")?;
    eprintln!(
        "PixelKit daemon: X11 shortcuts registered ({}, {}, and {})",
        settings.picker.shortcut, settings.magnifier.shortcut, settings.ruler.shortcut
    );
    while let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
        if event.state == HotKeyState::Pressed {
            if event.id == picker.id() {
                spawn_action(PICKER_ID);
            } else if event.id == magnifier.id() {
                spawn_action(MAGNIFIER_ID);
            } else if event.id == ruler.id() {
                spawn_action(RULER_ID);
            }
        }
    }
    Ok(())
}

async fn run_portal(settings: Settings) -> Result<()> {
    let (portal, _session, shortcuts) = bind_portal_shortcuts(&settings).await?;
    eprintln!("PixelKit daemon: portal shortcut actions registered");
    log_portal_shortcuts(&shortcuts);
    if shortcuts.iter().any(|(_, _, trigger)| trigger.is_empty()) {
        eprintln!(
            "PixelKit daemon: one or more shortcuts are unassigned; run `pixelkit configure-shortcuts`"
        );
    }
    let mut activated = portal.receive_activated().await?;
    while let Some(event) = activated.next().await {
        match event.shortcut_id() {
            PICKER_ID => spawn_action(PICKER_ID),
            MAGNIFIER_ID => spawn_action(MAGNIFIER_ID),
            RULER_ID => spawn_action(RULER_ID),
            _ => {}
        }
    }
    Ok(())
}

async fn configure_portal_shortcuts(settings: Settings) -> Result<()> {
    use ashpd::desktop::global_shortcuts::ConfigureShortcutsOptions;

    let (portal, session, shortcuts) = bind_portal_shortcuts(&settings).await?;
    if shortcuts.iter().all(|(_, _, trigger)| !trigger.is_empty()) {
        eprintln!("PixelKit: all portal shortcuts are already assigned");
        log_portal_shortcuts(&shortcuts);
        return Ok(());
    }
    if portal.version() < 2 {
        return Err(anyhow!(
            "this desktop portal cannot open its shortcut configuration; assign PixelKit shortcuts in the desktop's keyboard settings"
        ));
    }
    portal
        .configure_shortcuts(&session, None, ConfigureShortcutsOptions::default())
        .await
        .context("failed to open the desktop shortcut configuration")?;
    eprintln!("PixelKit: opened the desktop shortcut configuration");
    Ok(())
}

async fn bind_portal_shortcuts(
    settings: &Settings,
) -> Result<(
    ashpd::desktop::global_shortcuts::GlobalShortcuts,
    ashpd::desktop::Session<ashpd::desktop::global_shortcuts::GlobalShortcuts>,
    Vec<PortalShortcut>,
)> {
    use ashpd::desktop::{
        CreateSessionOptions,
        global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut},
    };

    let app_id = APP_ID
        .parse()
        .context("PixelKit has an invalid application ID")?;
    ashpd::register_host_app(app_id)
        .await
        .context("failed to register PixelKit's native portal application ID")?;
    let portal = GlobalShortcuts::new()
        .await
        .context("the desktop does not provide the GlobalShortcuts portal")?;
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await?;
    let picker_trigger = portal_trigger(&settings.picker.shortcut)?;
    let magnifier_trigger = portal_trigger(&settings.magnifier.shortcut)?;
    let ruler_trigger = portal_trigger(&settings.ruler.shortcut)?;
    let shortcuts = [
        NewShortcut::new(PICKER_ID, "Open PixelKit Color Picker")
            .preferred_trigger(Some(picker_trigger.as_str())),
        NewShortcut::new(MAGNIFIER_ID, "Open PixelKit Magnifier")
            .preferred_trigger(Some(magnifier_trigger.as_str())),
        NewShortcut::new(RULER_ID, "Open PixelKit Screen Ruler")
            .preferred_trigger(Some(ruler_trigger.as_str())),
    ];
    let response = portal
        .bind_shortcuts(&session, &shortcuts, None, BindShortcutsOptions::default())
        .await?
        .response()?;
    if response.shortcuts().is_empty() {
        return Err(anyhow!(
            "the desktop did not grant any PixelKit global shortcut"
        ));
    }
    let shortcuts = response
        .shortcuts()
        .iter()
        .map(|shortcut| {
            (
                shortcut.id().to_owned(),
                shortcut.description().to_owned(),
                shortcut.trigger_description().to_owned(),
            )
        })
        .collect();
    Ok((portal, session, shortcuts))
}

fn log_portal_shortcuts(shortcuts: &[PortalShortcut]) {
    for (_, description, trigger) in shortcuts {
        let trigger = if trigger.is_empty() {
            "unassigned"
        } else {
            trigger
        };
        eprintln!("  {description}: {trigger}");
    }
}

/// Converts user-friendly/global-hotkey notation to the freedesktop Shortcuts
/// specification (`CTRL+SHIFT+c`, `LOGO+SHIFT+m`, ...).
fn portal_trigger(shortcut: &str) -> Result<String> {
    let tokens: Vec<&str> = shortcut
        .split('+')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return Err(anyhow!("shortcut is empty"));
    }
    let mut output = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        let upper = token.to_ascii_uppercase();
        let value = if index + 1 == tokens.len() {
            match upper.as_str() {
                "ENTER" => "Return".to_owned(),
                "SPACE" => "space".to_owned(),
                "ESC" | "ESCAPE" => "Escape".to_owned(),
                value if value.len() == 1 => value.to_ascii_lowercase(),
                value if value.starts_with("KEY") && value.len() == 4 => {
                    value[3..].to_ascii_lowercase()
                }
                value if value.starts_with("DIGIT") && value.len() == 6 => value[5..].to_owned(),
                _ => (*token).to_owned(),
            }
        } else {
            match upper.as_str() {
                "CTRL" | "CONTROL" => "CTRL".to_owned(),
                "ALT" | "OPTION" => "ALT".to_owned(),
                "SHIFT" => "SHIFT".to_owned(),
                "SUPER" | "META" | "LOGO" | "WIN" => "LOGO".to_owned(),
                _ => return Err(anyhow!("unsupported shortcut modifier: {token}")),
            }
        };
        output.push(value);
    }
    Ok(output.join("+"))
}

fn spawn_action(action: &str) {
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("PixelKit daemon: cannot locate executable: {error}");
            return;
        }
    };
    if let Err(error) = Command::new(executable)
        .arg(action)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
    {
        eprintln!("PixelKit daemon: failed to launch {action}: {error}");
    }
}

struct DaemonLock {
    path: PathBuf,
    _listener: UnixListener,
}

impl DaemonLock {
    fn acquire() -> Result<Self> {
        let runtime = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::temp_dir().join(format!("pixelkit-{}", unsafe { libc_getuid() }))
            });
        fs::create_dir_all(&runtime)?;
        let path = runtime.join("pixelkit-daemon.sock");
        let listener = match UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                if UnixStream::connect(&path).is_ok() {
                    return Err(anyhow!("PixelKit daemon is already running"));
                }
                fs::remove_file(&path).context("failed to remove stale daemon socket")?;
                UnixListener::bind(&path)?
            }
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            _listener: listener,
        })
    }
}

// Avoid an additional libc crate for one fallback-only numeric identifier.
unsafe extern "C" {
    fn getuid() -> u32;
}
unsafe fn libc_getuid() -> u32 {
    unsafe { getuid() }
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_follow_xdg_trigger_syntax() {
        assert_eq!(portal_trigger("Super+Shift+C").unwrap(), "LOGO+SHIFT+c");
        assert_eq!(
            portal_trigger("Ctrl + Alt + Enter").unwrap(),
            "CTRL+ALT+Return"
        );
        assert!(portal_trigger("Hyper+C").is_err());
    }

    #[test]
    fn portal_application_id_is_valid() {
        assert!(APP_ID.parse::<ashpd::AppID>().is_ok());
    }
}
