#!/bin/sh
# speedtest-rli installer for Linux and macOS.
# Usage: curl -fsSL https://github.com/andreasmlbngaol/speedtest-rli/releases/latest/download/install.sh | sh
set -eu

REPO="andreasmlbngaol/speedtest-rli"
BIN="speedtest-rli"
BASE="https://github.com/$REPO/releases/latest/download"

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Linux)
    case "$arch" in
      x86_64|amd64) asset="speedtest-rli-linux-x86_64" ;;
      *) echo "No prebuilt binary for Linux $arch. Build from source: https://github.com/$REPO" >&2; exit 1 ;;
    esac
    ;;
  Darwin)
    case "$arch" in
      arm64|aarch64) asset="speedtest-rli-macos-aarch64" ;;
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

echo "Installed: $dir/$BIN"

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
