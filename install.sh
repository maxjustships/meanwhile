#!/bin/sh
# Meanwhile installer for Linux and macOS (beta). No root needed: installs into ~/.local.
#   curl -fsSL https://github.com/maxjustships/meanwhile/releases/latest/download/install.sh | sh
#   curl -fsSL https://github.com/maxjustships/meanwhile/releases/latest/download/install.sh | sh -s -- --uninstall
#
# Optional environment variables:
#   MEANWHILE_VERSION=v0.1.0   install a specific release (default: latest)
#   MEANWHILE_PREFIX=~/.local  install prefix
#   MEANWHILE_NO_HOOKS=1       do not touch Claude Code / Codex hook files
#   MEANWHILE_NO_LAUNCH=1      do not start the app after installing
set -eu

REPO=maxjustships/meanwhile
PREFIX=${MEANWHILE_PREFIX:-$HOME/.local}
if [ -n "${MEANWHILE_BASE_URL:-}" ]; then
  BASE=$MEANWHILE_BASE_URL
elif [ -n "${MEANWHILE_VERSION:-}" ]; then
  BASE=https://github.com/$REPO/releases/download/$MEANWHILE_VERSION
else
  BASE=https://github.com/$REPO/releases/latest/download
fi

OS=$(uname -s)
case "$OS" in
  Linux)  ASSET=meanwhile-linux.tar.gz ;;
  Darwin) ASSET=meanwhile-macos.tar.gz ;;
  *) echo "This installer is for Linux and macOS (Windows: see the README)." >&2; exit 1 ;;
esac

APPS=$HOME/.local/share/applications
ICONS=$HOME/.local/share/icons/hicolor/128x128/apps

if [ "${1:-}" = "--uninstall" ] || [ "${1:-}" = "uninstall" ]; then
  if [ -x "$PREFIX/bin/meanwhile-hook" ]; then
    "$PREFIX/bin/meanwhile-hook" setup claude --remove || true
    "$PREFIX/bin/meanwhile-hook" setup codex --remove || true
  fi
  pkill -x meanwhile 2>/dev/null || true
  rm -f "$PREFIX/bin/meanwhile" "$PREFIX/bin/meanwhile-hook" "$APPS/meanwhile.desktop" "$ICONS/meanwhile.png" \
        "$HOME/.config/autostart/Meanwhile.desktop" "$HOME/Library/LaunchAgents/Meanwhile.plist"
  echo "Meanwhile is uninstalled. Settings were kept."
  exit 0
fi

need() { command -v "$1" >/dev/null 2>&1 || { echo "Missing tool: $1" >&2; exit 1; }; }
need curl
need tar

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

echo "Downloading Meanwhile..."
curl -fsSL "$BASE/$ASSET" -o "$TMP/$ASSET"
curl -fsSL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS"
WANT=$(grep " \*\{0,1\}$ASSET\$" "$TMP/SHA256SUMS" | awk '{print $1}' | head -n 1)
[ -n "$WANT" ] || { echo "SHA256SUMS has no entry for $ASSET" >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then GOT=$(sha256sum "$TMP/$ASSET" | awk '{print $1}'); else GOT=$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}'); fi
[ "$WANT" = "$GOT" ] || { echo "Checksum mismatch (expected $WANT, got $GOT). Nothing was installed." >&2; exit 1; }

tar -xzf "$TMP/$ASSET" -C "$TMP"
pkill -x meanwhile 2>/dev/null || true
mkdir -p "$PREFIX/bin"
install -m 755 "$TMP/meanwhile" "$PREFIX/bin/meanwhile"
install -m 755 "$TMP/meanwhile-hook" "$PREFIX/bin/meanwhile-hook"

if [ "$OS" = "Linux" ]; then
  mkdir -p "$APPS" "$ICONS"
  sed "s|@BIN@|$PREFIX/bin/meanwhile|g" "$TMP/meanwhile.desktop" > "$APPS/meanwhile.desktop"
  install -m 644 "$TMP/meanwhile.png" "$ICONS/meanwhile.png"
  command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" 2>/dev/null || true
else
  command -v xattr >/dev/null 2>&1 && xattr -d com.apple.quarantine "$PREFIX/bin/meanwhile" "$PREFIX/bin/meanwhile-hook" 2>/dev/null || true
fi

HOOKS=""
if [ "${MEANWHILE_NO_HOOKS:-}" != "1" ]; then
  if [ -d "${CLAUDE_CONFIG_DIR:-$HOME/.claude}" ]; then "$PREFIX/bin/meanwhile-hook" setup claude && HOOKS="$HOOKS claude"; fi
  if [ -d "${CODEX_HOME:-$HOME/.codex}" ]; then "$PREFIX/bin/meanwhile-hook" setup codex && HOOKS="$HOOKS codex"; fi
fi

if [ "${MEANWHILE_NO_LAUNCH:-}" != "1" ]; then
  if [ "$OS" = "Darwin" ] || [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    nohup "$PREFIX/bin/meanwhile" >/dev/null 2>&1 &
  fi
fi

echo
echo "Meanwhile installed: $PREFIX/bin/meanwhile (+ meanwhile-hook)"
[ "$OS" = "Darwin" ] && echo "macOS build is a BETA and has not been tested on real hardware."
[ -n "$HOOKS" ] && echo "Hooks connected:$HOOKS" || echo "No agent hooks connected (run: meanwhile-hook setup claude)"
case "$HOOKS" in *codex*) echo "Codex runs new hooks after one approval: /hooks in the CLI (press t), or Review hooks in the desktop app." ;; esac
echo "Hotkey: Ctrl+Alt+Space. Tray icon: three dots."
case ":$PATH:" in *":$PREFIX/bin:"*) ;; *) echo "Add $PREFIX/bin to your PATH to run meanwhile-hook from a terminal." ;; esac
echo "Uninstall:  curl -fsSL https://github.com/$REPO/releases/latest/download/install.sh | sh -s -- --uninstall"
