#!/bin/sh
# speedtest installer for Linux and macOS.
# Usage: curl -fsSL https://github.com/andreasmlbngaol/speedtest-rli/releases/latest/download/install.sh | sh
set -eu

REPO="andreasmlbngaol/speedtest-rli"
BIN="speedtest"
ALIAS="speedtest-rli"
BASE="https://github.com/$REPO/releases/latest/download"

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Linux)
    case "$arch" in
      x86_64|amd64) asset="speedtest-rli-linux-x86_64"; tray_asset="speedtest-rli-tray-linux-x86_64" ;;
      *) echo "No prebuilt binary for Linux $arch. Build from source: https://github.com/$REPO" >&2; exit 1 ;;
    esac
    ;;
  Darwin)
    case "$arch" in
      arm64|aarch64) asset="speedtest-rli-macos-aarch64"; tray_asset="speedtest-rli-tray-macos-aarch64" ;;
      *) echo "No prebuilt binary for macOS $arch. Build from source: https://github.com/$REPO" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $os. Build from source: https://github.com/$REPO" >&2
    exit 1
    ;;
esac

dir="$HOME/.local/bin"
mkdir -p "$dir"

echo "Downloading $asset..."
curl -fsSL "$BASE/$asset" -o "$dir/$BIN"
chmod +x "$dir/$BIN"
# Keep the old name working too, in case scripts call speedtest-rli.
ln -sf "$BIN" "$dir/$ALIAS"

echo "Installed: $dir/$BIN"

# Optional system-tray launcher (skipped if the download fails).
if curl -fsSL "$BASE/$tray_asset" -o "$dir/speedtest-tray"; then
  chmod +x "$dir/speedtest-tray"
  echo "Installed tray: $dir/speedtest-tray"
  case "$os" in
    Linux)
      autostart_dir="$HOME/.config/autostart"
      mkdir -p "$autostart_dir"
      cat > "$autostart_dir/speedtest-tray.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=speedtest tray
Comment=Speedtest.net tray launcher
Exec=$dir/speedtest-tray
Terminal=false
X-GNOME-Autostart-enabled=true
EOF
      echo "Autostart enabled (remove $autostart_dir/speedtest-tray.desktop to disable)."
      ;;
    Darwin)
      agents="$HOME/Library/LaunchAgents"
      mkdir -p "$agents"
      cat > "$agents/com.andreasmlbngaol.speedtest-tray.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key><string>com.andreasmlbngaol.speedtest-tray</string>
	<key>ProgramArguments</key>
	<array><string>$dir/speedtest-tray</string></array>
	<key>RunAtLoad</key><true/>
</dict>
</plist>
EOF
      launchctl unload "$agents/com.andreasmlbngaol.speedtest-tray.plist" 2>/dev/null || true
      launchctl load "$agents/com.andreasmlbngaol.speedtest-tray.plist" 2>/dev/null || true
      echo "Autostart enabled (LaunchAgent in $agents)."
      ;;
  esac
  # Start it right away too, so the tray shows up without waiting for a re-login.
  # setsid/nohup detach it from this shell (which is often a curl | sh pipe).
  if command -v setsid >/dev/null 2>&1; then
    setsid "$dir/speedtest-tray" >/dev/null 2>&1 < /dev/null &
  else
    nohup "$dir/speedtest-tray" >/dev/null 2>&1 < /dev/null &
  fi
  echo "Tray started."
else
  echo "Note: tray binary not available for this platform; CLI installed on its own."
fi

case ":$PATH:" in
  *":$dir:"*)
    echo "Run it with: $BIN"
    ;;
  *)
    echo
    echo "NOTE: $dir is not on your PATH yet. Add it by running:"
    echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.profile && . ~/.profile"
    echo "Then run: $BIN"
    ;;
esac
