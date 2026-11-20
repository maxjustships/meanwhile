## Meanwhile v0.1.0 ···

First public release. A tiny always-on-top card that gives you something to do for the minute your AI agent is thinking (stretch, water, eye break, breathing, a math sprint, a reaction test, a vocab card, or "what will you check first?"), then says **"Your answer's ready"** the moment it's done.

**Install**

- Windows 10/11 (PowerShell, no admin): `irm https://github.com/maxjustships/meanwhile/releases/latest/download/install.ps1 | iex`
- Linux (WebKitGTK 4.1): `curl -fsSL https://github.com/maxjustships/meanwhile/releases/latest/download/install.sh | sh`
- macOS 11+ (**beta**): same line as Linux.

The installer connects Claude Code and Codex hooks if it finds them (your other hooks are kept, a backup is written). Codex runs new hooks only after you approve them once: `/hooks` in the CLI, or *Review hooks* / *Settings → Hooks* in the desktop app.

**Status**

- Windows: working, tested on Windows 10 22H2: tray, hotkey, autostart, overlay without focus stealing, Claude Code and Codex hooks. Windows 11 not tried.
- Linux: working, smoke-tested on Arch + Hyprland (Wayland). The compositor may ignore the corner and always-on-top; see the README for a window rule.
- macOS: **beta, untested on real hardware.** Built by GitHub Actions (universal, ad-hoc signed), but nobody has run it on a Mac yet.
- Hermes Agent: optional plugin in `integrations/hermes`.

**Unsigned binaries.** Nothing here is code-signed. Windows SmartScreen may warn ("More info → Run anyway"). The installers verify the SHA-256 in `SHA256SUMS` before installing.

**Local only.** No account, no telemetry. The app listens on `127.0.0.1:47800` and needs a token from its config file.

Files: `Meanwhile-win-x64.zip`, `meanwhile-linux.tar.gz`, `meanwhile-macos.tar.gz` (beta), `install.ps1`, `uninstall.ps1`, `install.sh`, `SHA256SUMS`.
