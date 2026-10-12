//! System-tray launcher for `speedtest`.
//!
//! Built only with `--features tray` (see Cargo.toml). It has no window: it
//! shows a tray icon with a Start / Update / Exit menu. Start opens a terminal
//! running the `speedtest` CLI; Update opens a terminal that downloads the
//! latest GitHub release and swaps in both the CLI and this tray binary; Exit
//! quits. Both terminals wait for a keypress before closing.
//!
//! Passing `--update` runs the update in the foreground and exits, which is how
//! the Update menu item drives the visible terminal.
//!
//! This binary deliberately contains no speedtest logic — it only orchestrates.
//!
//! Platform notes:
//! - Linux uses tray-icon's `ksni` backend, which runs its own D-Bus worker
//!   thread, so no GUI event loop (and no GTK) is needed — the tray binary
//!   stays a self-contained static build.
//! - Windows needs a win32 message pump and macOS a main-thread run loop, so
//!   those targets drive the tray with `tao`.

// No console window flash on Windows.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[path = "../tray_support.rs"]
mod tray_support;

use std::process::Command;

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const REPO: &str = "andreasmlbngaol/speedtest-rli";
const ICON_SIZE: u32 = 32;

/// Build the tray icon and its three-item menu.
fn build_tray() -> TrayIcon {
    let menu = Menu::new();
    let start = MenuItem::with_id(tray_support::MENU_START, "Start", true, None);
    let update = MenuItem::with_id(tray_support::MENU_UPDATE, "Update", true, None);
    let exit = MenuItem::with_id(tray_support::MENU_EXIT, "Exit", true, None);
    menu.append_items(&[&start, &update, &exit]).ok();

    #[allow(unused_mut)]
    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("speedtest")
        .with_icon(make_icon());
    #[cfg(target_os = "macos")]
    {
        builder = builder.with_icon_as_template(true);
    }
    builder.build().expect("failed to create tray icon")
}

/// A white ring + centre dot, drawn in code so no icon asset or image-decoding
/// dependency is needed. A dark 1px halo is added around the shape so the icon
/// stays visible on light panels too (GNOME/Zorin ship a light panel by default).
fn make_icon() -> Icon {
    let n = ICON_SIZE as i32;
    let r = n as f64 / 2.0;
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    let mut shape = vec![false; (n * n) as usize];
    for y in 0..n {
        for x in 0..n {
            let dx = x as f64 + 0.5 - r;
            let dy = y as f64 + 0.5 - r;
            let dist = (dx * dx + dy * dy).sqrt();
            // Ring between 0.55r and 0.9r, plus a filled centre dot.
            let in_ring = dist > r * 0.55 && dist < r * 0.9;
            let in_dot = dist < r * 0.3;
            if in_ring || in_dot {
                shape[(y * n + x) as usize] = true;
            }
        }
    }
    for y in 0..n {
        for x in 0..n {
            let idx = (y * n + x) as usize;
            let i = idx * 4;
            if shape[idx] {
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            } else if neighbours_shape(&shape, n, x, y) {
                rgba[i..i + 4].copy_from_slice(&[32, 32, 32, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).expect("valid icon")
}

/// True if any 4-neighbour of (x, y) is part of the drawn shape.
fn neighbours_shape(shape: &[bool], n: i32, x: i32, y: i32) -> bool {
    [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        (0..n).contains(&nx) && (0..n).contains(&ny) && shape[(ny * n + nx) as usize]
    })
}

/// Open a terminal window running the CLI.
fn start_speedtest() {
    let cli = tray_support::cli_binary_path();
    let Some((program, args)) = tray_support::terminal_command(&cli) else {
        return;
    };
    let _ = Command::new(program).args(args).spawn();
}

/// Open a terminal window that runs this binary with `--update`, so the
/// check/download output is visible and the window waits for a key at the end.
/// Returns `false` if no terminal could be launched.
fn update_in_terminal() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some((program, args)) = tray_support::update_terminal_command(&exe) else {
        return false;
    };
    Command::new(program).args(args).spawn().is_ok()
}

/// Handle a clicked menu item. Returns `true` if the app should exit.
fn handle_menu(id: &str, _tray: &TrayIcon) -> bool {
    match id {
        tray_support::MENU_START => start_speedtest(),
        tray_support::MENU_UPDATE => {
            if update_in_terminal() {
                // On unix the running tray can be replaced in place, so keep it.
                // On Windows the exe is locked while running: exit now and let
                // the updater start a fresh tray when it finishes, so we don't
                // end up with the old and new tray both showing.
                return cfg!(target_os = "windows");
            }
        }
        tray_support::MENU_EXIT => return true,
        _ => {}
    }
    false
}

// ---------------------------------------------------------------------------
// Platform event loops
// ---------------------------------------------------------------------------

/// Print progress. On Windows this process is a GUI-subsystem exe with no
/// console, so its stdout is invisible; also append to the log file that the
/// launching batch file prints after this process exits.
fn log(msg: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(tray_support::update_log_path())
        {
            let _ = writeln!(f, "{msg}");
        }
    }
    println!("{msg}");
}

/// The tray path this updater must replace and relaunch.
///
/// On Windows the updater runs from a copy in `%TEMP%` (a running exe is
/// locked), so the real tray path is passed as an argument. Elsewhere the
/// updater is the tray itself, so its own path is the target.
fn update_target() -> Option<std::path::PathBuf> {
    let mut args = std::env::args().skip_while(|a| a != "--update");
    args.next(); // consume "--update"
    args.next()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

/// Run `--update` in the foreground, print the result, and exit.
///
/// On Windows the tray that launched us has already exited (the running exe
/// must be replaced), so start exactly one fresh tray here — otherwise clicking
/// Update would leave the old tray and the new one both running. On unix the
/// running tray replaced itself in place and is still alive, so we do not.
fn run_update_mode() {
    let Some(target) = update_target() else {
        return;
    };
    #[cfg(target_os = "windows")]
    let _ = std::fs::write(tray_support::update_log_path(), "");
    let msg = match do_update(&target) {
        Ok(None) => "Already up to date".to_string(),
        Ok(Some(v)) => format!("Updated to v{v}"),
        Err(e) => format!("Update failed: {e}"),
    };
    log(&msg);
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new(&target).spawn();
    }
}

/// Linux/BSD: the ksni backend owns its D-Bus thread, so poll the menu channel.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn main() {
    // `--update` runs the update in this (foreground) process and exits, so the
    // tray can pop a terminal showing progress. Normal launch starts the tray.
    if std::env::args().any(|a| a == "--update") {
        run_update_mode();
        return;
    }
    let tray = build_tray();
    loop {
        if let Ok(event) = MenuEvent::receiver().try_recv()
            && handle_menu(event.id().0.as_str(), &tray)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Windows/macOS: run a real event loop, woken by forwarded menu events.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn main() {
    use tao::event::{Event, StartCause};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tray_icon::TrayIconEvent;

    if std::env::args().any(|a| a == "--update") {
        run_update_mode();
        return;
    }

    enum UserEvent {
        Menu(MenuEvent),
        Wake,
    }

    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();
    let event_loop = builder.build();
    let proxy = event_loop.create_proxy();

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));
    let tray_proxy = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |_event| {
        let _ = tray_proxy.send_event(UserEvent::Wake);
    }));

    let mut tray: Option<TrayIcon> = None;
    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            // Create the icon once the loop is running (macOS: main thread).
            Event::NewEvents(StartCause::Init) => tray = Some(build_tray()),
            Event::UserEvent(UserEvent::Menu(event)) => {
                let exit = tray
                    .as_ref()
                    .is_some_and(|t| handle_menu(event.id().0.as_str(), t));
                if exit {
                    // Drop the icon *now*, before the loop ends. `tao`'s
                    // `EventLoop::run` never returns — it calls `process::exit`,
                    // which skips destructors — so a `TrayIcon` still alive at
                    // exit is never removed. On Windows that leaves a ghost icon
                    // in the notification area (and the updater's fresh tray
                    // makes it look like two); on macOS a stale menu-bar item.
                    tray = None;
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Check GitHub for a newer release and, if there is one, replace both the CLI
/// and the tray binary at `tray_exe`.
fn do_update(tray_exe: &std::path::Path) -> Result<Option<String>, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("speedtest-tray/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    log("Checking for updates...");
    let body = client
        .get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| format!("cannot reach GitHub: {e}"))?;
    let release: Release =
        serde_json::from_str(&body).map_err(|e| format!("bad release JSON: {e}"))?;

    let latest = release.tag_name.trim_start_matches('v');
    if latest == env!("CARGO_PKG_VERSION") {
        return Ok(None);
    }
    log(&format!(
        "Updating {} -> v{latest}...",
        env!("CARGO_PKG_VERSION")
    ));

    let (os, arch) = tray_support::current_os_arch();

    // Replace the CLI first (it is not running), then the tray binary.
    if let Some(name) = tray_support::cli_asset_name(os, arch)
        && let Some(asset) = release.assets.iter().find(|a| a.name == name)
    {
        log(&format!("Downloading {name}..."));
        install_asset(&client, asset, &tray_support::cli_binary_beside(tray_exe))?;
    }

    if let Some(name) = tray_support::tray_asset_name(os, arch)
        && let Some(asset) = release.assets.iter().find(|a| a.name == name)
    {
        log(&format!("Downloading {name}..."));
        install_asset(&client, asset, tray_exe)?;
    }

    Ok(Some(latest.to_string()))
}

/// Download `asset` and put it at `target`.
///
/// On Windows the running exe cannot be overwritten, so it is moved aside
/// (`<target>.old`) first; the caller restarts the tray afterwards.
fn install_asset(
    client: &reqwest::blocking::Client,
    asset: &Asset,
    target: &std::path::Path,
) -> Result<(), String> {
    let tmp = tmp_beside(target);
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.bytes())
        .map_err(|e| format!("download failed: {e}"))?;
    std::fs::write(&tmp, &bytes).map_err(|e| format!("write failed: {e}"))?;
    make_executable(&tmp);

    // Unix: rename over a running binary is fine.
    #[cfg(not(windows))]
    {
        std::fs::rename(&tmp, target).map_err(|e| format!("replace failed: {e}"))?;
    }
    // Windows: the running exe is locked, so move it aside and drop the new one in.
    #[cfg(windows)]
    {
        let old = target.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(target, &old).map_err(|e| format!("replace failed: {e}"))?;
        std::fs::rename(&tmp, target).map_err(|e| format!("replace failed: {e}"))?;
    }
    Ok(())
}

fn tmp_beside(target: &std::path::Path) -> std::path::PathBuf {
    let mut name = target
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(".new");
    target.with_file_name(name)
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) {}
