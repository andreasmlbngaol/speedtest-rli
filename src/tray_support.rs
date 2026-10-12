//! Pure helpers shared by the tray app, kept free of any GUI dependency so they
//! can be unit-tested under a plain `cargo test` with no display and no GTK.
//!
//! The tray binary includes this file with `#[path = "../tray_support.rs"]`.
//! The CLI includes it as `mod tray_support;` only so its tests run; it never
//! calls into it, hence the crate-level `dead_code` allowance.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Menu item ids, matched in the event loop.
pub const MENU_START: &str = "start";
pub const MENU_UPDATE: &str = "update";
pub const MENU_EXIT: &str = "exit";

const REPO: &str = "andreasmlbngaol/speedtest-rli";

/// GitHub release asset name for the CLI, for a given `(os, arch)`.
///
/// Mirrors the names produced by `.github/workflows/release.yml` (no target
/// triple in the name, unlike what `self_update` would expect).
pub fn cli_asset_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("speedtest-rli-linux-x86_64"),
        ("windows", "x86_64") => Some("speedtest-rli-windows-x86_64.exe"),
        ("macos", "aarch64") => Some("speedtest-rli-macos-aarch64"),
        _ => None,
    }
}

/// GitHub release asset name for the tray, for a given `(os, arch)`.
pub fn tray_asset_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("speedtest-rli-tray-linux-x86_64"),
        ("windows", "x86_64") => Some("speedtest-rli-tray-windows-x86_64.exe"),
        ("macos", "aarch64") => Some("speedtest-rli-tray-macos-aarch64"),
        _ => None,
    }
}

/// `(os, arch)` of the running process, in the vocabulary the asset-name
/// helpers expect.
pub fn current_os_arch() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "other"
    };
    (os, arch)
}

/// The `speedtest` binary. Prefer the one installed next to the tray exe;
/// fall back to the bare name (resolved via `PATH`).
pub fn cli_binary_path() -> PathBuf {
    let name = if cfg!(target_os = "windows") {
        "speedtest.exe"
    } else {
        "speedtest"
    };
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let beside = dir.join(name);
        if beside.exists() {
            return beside;
        }
    }
    PathBuf::from(name)
}

/// `true` if `program` is found on `PATH`.
fn on_path(program: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(program).is_file())
}

/// Single-quote a string for a POSIX shell, escaping embedded single quotes.
fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Escape a string for embedding inside an AppleScript double-quoted literal.
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Batch-file contents that run `cmd` in a new console and then wait for a key.
///
/// On Windows the actual command is written to a temp `.bat` and launched,
/// rather than passed as a `cmd /C` argument: Rust escapes embedded `"` as
/// `\"` when building the child command line, and cmd.exe does not understand
/// `\"`, which corrupts any quoted path. A batch file is parsed by cmd itself,
/// so quotes survive verbatim.
///
/// The trailing `exit` matters: `start "" file.bat` runs the batch via
/// `cmd /K`, so without it the console stays open at a prompt after `pause`
/// (you can keep typing instead of the window closing).
pub fn windows_batch(cmd: &str) -> String {
    format!("@echo off\r\n{cmd}\r\npause\r\nexit\r\n")
}

/// Write `windows_batch(cmd)` to a temp `.bat` and return its path. The name is
/// derived from the content so Start and Update never clobber each other.
fn write_batch(cmd: &str) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    cmd.hash(&mut h);
    let path = std::env::temp_dir().join(format!("speedtest-run-{:016x}.bat", h.finish()));
    std::fs::write(&path, windows_batch(cmd)).ok()?;
    Some(path)
}

/// Command to open a terminal window that runs `cli` and then waits for a key,
/// so the window stays open long enough to read the result.
///
/// Returns `(program, args)` ready for `Command::new(program).args(args)`.
/// `None` if no suitable terminal could be found.
pub fn terminal_command(cli: &Path) -> Option<(String, Vec<String>)> {
    let cli = cli.to_string_lossy();
    let cmd = if cfg!(target_os = "windows") {
        format!("\"{cli}\"")
    } else {
        shell_quote(&cli)
    };
    terminal_command_for(&cmd)
}

/// Like [`terminal_command`] but takes a whole shell command line, so callers
/// can pass extra arguments (e.g. `speedtest-tray --update`).
pub fn terminal_command_for(cmd: &str) -> Option<(String, Vec<String>)> {
    if cfg!(target_os = "windows") {
        // Launch the temp batch file: `start ""` opens a new console window for
        // it, and the batch's trailing `pause` waits for a key. The only quoted
        // token passed to cmd is the batch path (no embedded quotes → no `\"`).
        let bat = write_batch(cmd)?;
        return Some((
            "cmd".into(),
            vec![
                "/C".into(),
                "start".into(),
                String::new(),
                bat.to_string_lossy().into_owned(),
            ],
        ));
    }

    // Run the command, then block on a keypress. `bash -c` is invoked explicitly
    // so `read -n1` is available regardless of the user's login shell.
    let body = format!(
        "{cmd}; printf '\\nPress any key to close...'; read -n1 -s -r; printf '\\n'"
    );

    if cfg!(target_os = "macos") {
        let inner = format!("bash -c {}", shell_quote(&body));
        let script = format!(
            "tell application \"Terminal\" to do script \"{}\"",
            applescript_escape(&inner)
        );
        return Some((
            "osascript".into(),
            vec![
                "-e".into(),
                script,
                "-e".into(),
                "tell application \"Terminal\" to activate".into(),
            ],
        ));
    }

    // Linux/BSD: pick the first terminal emulator that is actually installed.
    // `gnome-terminal` uses `--`, `kitty` takes the program directly, the rest
    // use `-e`.
    for emu in [
        "x-terminal-emulator",
        "gnome-terminal",
        "konsole",
        "xfce4-terminal",
        "alacritty",
        "kitty",
        "xterm",
    ] {
        if !on_path(emu) {
            continue;
        }
        let args = match emu {
            "gnome-terminal" => vec!["--".into(), "bash".into(), "-c".into(), body.clone()],
            "kitty" => vec!["bash".into(), "-c".into(), body.clone()],
            _ => vec!["-e".into(), "bash".into(), "-c".into(), body.clone()],
        };
        return Some((emu.into(), args));
    }
    None
}

/// Path the tray writes its `--update` output to on Windows. The tray is a
/// GUI-subsystem exe with no console, so `println!` goes nowhere; the batch
/// file that launched it `type`s this file instead.
pub fn update_log_path() -> PathBuf {
    std::env::temp_dir().join("speedtest-update.log")
}

/// The command that re-runs the tray binary in update mode, for a given `os`.
///
/// Pure (takes `os` explicitly) so both the Windows and the Unix shapes can be
/// unit-tested from any host — the Windows branch cannot be compiled here.
pub fn update_command_line_for(os: &str, tray_exe: &Path, log: &Path) -> String {
    let exe = tray_exe.to_string_lossy();
    if os == "windows" {
        // `start /wait` blocks until the GUI-subsystem tray exe exits, then we
        // print the log it wrote (the exe has no console of its own).
        format!(
            "start \"\" /wait \"{exe}\" --update\r\ntype \"{}\"",
            log.display()
        )
    } else {
        format!("{} --update", shell_quote(&exe))
    }
}

/// [`update_command_line_for`] for the running platform.
pub fn update_command_line(tray_exe: &Path) -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unix"
    };
    update_command_line_for(os, tray_exe, &update_log_path())
}

/// Terminal command that re-runs this tray binary with `--update`, so the
/// check-and-install output is visible instead of flashing by in a tooltip.
pub fn update_terminal_command(tray_exe: &Path) -> Option<(String, Vec<String>)> {
    terminal_command_for(&update_command_line(tray_exe))
}

/// Contents of `~/.config/autostart/speedtest-tray.desktop` (Linux).
pub fn autostart_desktop(tray_path: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=speedtest tray\n\
         Comment=Speedtest.net tray launcher\n\
         Exec={}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        tray_path.display()
    )
}

/// Contents of `~/Library/LaunchAgents/<label>.plist` (macOS).
pub fn launchagent_plist(tray_path: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key><string>{REPO}.tray</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array><string>{}</string></array>\n\
         \t<key>RunAtLoad</key><true/>\n\
         </dict>\n\
         </plist>\n",
        tray_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_cover_the_three_supported_platforms() {
        assert_eq!(
            cli_asset_name("linux", "x86_64"),
            Some("speedtest-rli-linux-x86_64")
        );
        assert_eq!(
            cli_asset_name("windows", "x86_64"),
            Some("speedtest-rli-windows-x86_64.exe")
        );
        assert_eq!(
            cli_asset_name("macos", "aarch64"),
            Some("speedtest-rli-macos-aarch64")
        );
        assert_eq!(
            tray_asset_name("linux", "x86_64"),
            Some("speedtest-rli-tray-linux-x86_64")
        );
        assert_eq!(
            tray_asset_name("windows", "x86_64"),
            Some("speedtest-rli-tray-windows-x86_64.exe")
        );
        assert_eq!(
            tray_asset_name("macos", "aarch64"),
            Some("speedtest-rli-tray-macos-aarch64")
        );
    }

    #[test]
    fn unsupported_platforms_have_no_asset() {
        assert_eq!(cli_asset_name("linux", "aarch64"), None);
        assert_eq!(cli_asset_name("freebsd", "x86_64"), None);
        assert_eq!(tray_asset_name("windows", "aarch64"), None);
        assert_eq!(tray_asset_name("macos", "x86_64"), None);
    }

    #[test]
    fn menu_ids_are_distinct() {
        assert_ne!(MENU_START, MENU_UPDATE);
        assert_ne!(MENU_UPDATE, MENU_EXIT);
        assert_ne!(MENU_START, MENU_EXIT);
    }

    #[test]
    fn current_platform_is_supported() {
        let (os, arch) = current_os_arch();
        // This test host is one of the three; both binaries must resolve.
        assert!(cli_asset_name(os, arch).is_some(), "os={os} arch={arch}");
        assert!(tray_asset_name(os, arch).is_some(), "os={os} arch={arch}");
    }

    #[test]
    fn windows_terminal_command_has_title_placeholder() {
        if !cfg!(target_os = "windows") {
            return;
        }
        let (prog, args) = terminal_command(Path::new("C:\\x\\speedtest.exe")).unwrap();
        assert_eq!(prog, "cmd");
        assert_eq!(args[0], "/C");
        assert_eq!(args[1], "start");
        assert_eq!(args[2], ""); // empty title
        // Launches a temp batch file (no embedded quotes to be mangled).
        assert!(args[3].ends_with(".bat"), "arg={}", args[3]);
        assert!(!args[3].contains('"'), "no quotes in bat path: {}", args[3]);
    }

    #[test]
    fn windows_batch_runs_the_command_then_pauses() {
        let bat = windows_batch("\"C:\\x\\speedtest.exe\"");
        assert!(bat.starts_with("@echo off"));
        assert!(bat.contains("\"C:\\x\\speedtest.exe\""));
        assert!(bat.contains("\r\npause"));
    }

    #[test]
    fn windows_batch_exits_after_pause() {
        // `start "" file.bat` runs the batch via `cmd /K`, which would otherwise
        // leave the console open at a prompt after `pause`.
        let bat = windows_batch("echo hi");
        assert!(bat.contains("\r\npause\r\nexit\r\n"), "bat={bat:?}");
    }

    #[test]
    fn macos_terminal_command_uses_osascript() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let (prog, args) = terminal_command(Path::new("/usr/local/bin/speedtest")).unwrap();
        assert_eq!(prog, "osascript");
        assert!(args[0] == "-e");
        assert!(args[1].contains("Terminal"));
    }

    #[test]
    fn linux_terminal_args_match_emulator_conventions() {
        if cfg!(target_os = "windows") || cfg!(target_os = "macos") {
            return;
        }
        // Only meaningful if a terminal is installed on this host.
        let Some((prog, args)) = terminal_command(Path::new("speedtest")) else {
            return;
        };
        // Every Linux branch runs `bash -c "<cli>; ... read -n1 ..."`.
        let body_idx = match prog.as_str() {
            "gnome-terminal" => {
                assert_eq!(args[0], "--");
                1
            }
            "kitty" => 0,
            _ => {
                assert_eq!(args[0], "-e");
                1
            }
        };
        assert_eq!(&args[body_idx..body_idx + 2], ["bash", "-c"]);
        let body = &args[body_idx + 2];
        assert!(body.contains("speedtest"));
        assert!(body.contains("read -n1"));
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn update_command_line_targets_this_exe_with_the_update_flag() {
        // Unix shape.
        let unix = update_command_line_for(
            "unix",
            Path::new("/x/speedtest-tray"),
            Path::new("/tmp/speedtest-update.log"),
        );
        assert!(unix.contains("'/x/speedtest-tray' --update"), "unix={unix}");

        // Windows shape (testable from any host since `os` is a parameter).
        let win = update_command_line_for(
            "windows",
            Path::new("C:\\x\\speedtest-tray.exe"),
            Path::new("C:\\Temp\\speedtest-update.log"),
        );
        assert!(win.contains("start \"\" /wait"), "win={win}");
        assert!(win.contains("\"C:\\x\\speedtest-tray.exe\" --update"), "win={win}");
        assert!(win.contains("type \"C:\\Temp\\speedtest-update.log\""), "win={win}");
        assert!(win.contains("\r\n"), "win should use CRLF: {win}");
        // The bug this fixes: no escaped `\"` may reach cmd.exe.
        assert!(!win.contains("\\\""), "win must not contain escaped quotes: {win}");
    }

    #[test]
    fn autostart_files_name_the_tray_path() {
        let p = Path::new("/home/u/.local/bin/speedtest-tray");
        assert!(autostart_desktop(p).contains("/home/u/.local/bin/speedtest-tray"));
        assert!(autostart_desktop(p).contains("Terminal=false"));
        let plist = launchagent_plist(p);
        assert!(plist.contains("/home/u/.local/bin/speedtest-tray"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
    }
}
