//! Shared bits for the Meanwhile app and the `meanwhile-hook` CLI:
//! config file, tiny HTTP client, and safe merging of agent hook configs.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 47800;
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+Space";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Activities {
    pub body: bool,
    pub brain: bool,
    pub focus: bool,
}

impl Default for Activities {
    fn default() -> Self {
        Activities { body: true, brain: true, focus: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Port of the local hook endpoint (127.0.0.1).
    pub port: u16,
    /// Shared secret the hook sends as `Authorization: Bearer <token>`.
    pub token: String,
    /// Global shortcut that toggles the overlay, e.g. "Ctrl+Alt+Space".
    pub hotkey: String,
    /// top-left | top-right | bottom-left | bottom-right
    pub corner: String,
    pub activities: Activities,
    /// React to `start` from agent hooks (off = hotkey only).
    pub auto_trigger: bool,
    /// Wait this long after `start` before showing anything, so quick answers don't flash the overlay.
    pub start_delay_seconds: u64,
    /// Give up waiting after this many minutes (an interrupted agent never sends `done`).
    pub max_wait_minutes: u64,
    /// Start with the OS.
    pub autostart: bool,
    /// Extra addresses to listen on besides 127.0.0.1 (e.g. a tailnet IP). Token still required.
    pub extra_listen: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            port: DEFAULT_PORT,
            token: String::new(),
            hotkey: DEFAULT_HOTKEY.into(),
            corner: "bottom-right".into(),
            activities: Activities::default(),
            auto_trigger: true,
            start_delay_seconds: 5,
            max_wait_minutes: 15,
            autostart: true,
            extra_listen: vec![],
        }
    }
}

pub fn config_dir() -> PathBuf {
    if let Ok(d) = std::env::var("MEANWHILE_CONFIG_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "linux") {
        base.join("meanwhile")
    } else {
        base.join("Meanwhile")
    }
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn log_path() -> PathBuf {
    config_dir().join("meanwhile.log")
}

pub fn random_token() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b = t.to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Config {
    /// Read the config, filling defaults. Never fails: a broken file gives defaults (and is not overwritten).
    pub fn load() -> Config {
        match std::fs::read_to_string(config_path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    /// Load, creating the file (with a fresh token) when it does not exist yet.
    pub fn load_or_create() -> Config {
        let path = config_path();
        let exists = path.exists();
        let mut c = Config::load();
        let mut changed = !exists;
        if c.token.is_empty() {
            c.token = random_token();
            changed = true;
        }
        if c.hotkey.trim().is_empty() {
            c.hotkey = DEFAULT_HOTKEY.into();
            changed = true;
        }
        if changed {
            let _ = c.save();
        }
        c
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap() + "\n")?;
        std::fs::rename(&tmp, &path)
    }
}

/// Append a line to the shared log (kept small: truncated at ~256 KB).
pub fn log_line(who: &str, msg: &str) {
    let path = log_path();
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if std::fs::metadata(&path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{ts} [{who}] {msg}");
    }
}

// ---------------------------------------------------------------- HTTP client

#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub body: String,
}

/// Minimal blocking HTTP/1.1 POST with tight timeouts (hooks must never slow an agent down).
pub fn post(base: &str, path: &str, token: &str, body: &str, timeout: Duration) -> Result<Reply, String> {
    let hostport = base
        .trim()
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string();
    if base.trim().starts_with("https://") {
        return Err("https is not supported (the hook endpoint is plain http on a trusted network)".into());
    }
    let addrs: Vec<SocketAddr> = hostport
        .to_socket_addrs()
        .map_err(|e| format!("bad address {hostport}: {e}"))?
        .collect();
    let addr = addrs.first().ok_or("no address")?;
    let mut s = TcpStream::connect_timeout(addr, timeout).map_err(|e| format!("connect {addr}: {e}"))?;
    let _ = s.set_read_timeout(Some(timeout));
    let _ = s.set_write_timeout(Some(timeout));
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {hostport}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| format!("bad reply: {}", text.lines().next().unwrap_or("")))?;
    let body = text.split_once("\r\n\r\n").map(|x| x.1.to_string()).unwrap_or_default();
    Ok(Reply { status, body })
}

// ---------------------------------------------------------------- agent hook setup

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn parse(s: &str) -> Option<Agent> {
        match s.to_ascii_lowercase().as_str() {
            "claude" | "claude-code" | "claudecode" => Some(Agent::Claude),
            "codex" => Some(Agent::Codex),
            _ => None,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }
    /// The file whose `hooks` section we merge into.
    pub fn config_file(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(match self {
            Agent::Claude => std::env::var("CLAUDE_CONFIG_DIR")
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude"))
                .join("settings.json"),
            Agent::Codex => std::env::var("CODEX_HOME")
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"))
                .join("hooks.json"),
        })
    }
}

/// Marker used to recognise our own handlers (idempotent install / clean removal).
pub const MARKER: &str = "meanwhile-hook";

/// Shell-safe command line for a hook entry. Forward slashes work in bash (Claude Code on Windows)
/// and in cmd.exe (Codex on Windows).
/// Unquoted on purpose: some agents (Codex on
/// Windows) pass it to `cmd /C` with escaped quotes, which breaks a quoted path. A path with spaces
/// becomes its 8.3 short form on Windows; elsewhere (or if short names are off) it gets quoted.
pub fn hook_command(hook_exe: &Path, args: &str) -> String {
    let p = hook_exe.to_string_lossy().replace('\\', "/");
    if !p.contains(' ') {
        return format!("{p} {args}");
    }
    #[cfg(windows)]
    if let Some(short) = short_path(hook_exe) {
        let s = short.replace('\\', "/");
        if !s.contains(' ') {
            return format!("{s} {args}");
        }
    }
    format!("\"{p}\" {args}")
}

#[cfg(windows)]
fn short_path(p: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetShortPathNameW(long: *const u16, short: *mut u16, len: u32) -> u32;
    }
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut buf = vec![0u16; 1024];
    let n = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..n]))
}

fn is_ours(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(|c| c.as_str())
        .map(|c| c.contains(MARKER))
        .unwrap_or(false)
}

/// Remove our handlers from one event array. Groups that only contained our handlers are dropped;
/// everything else is left exactly as it was (same order, same indices).
fn strip_ours(arr: &mut Vec<Value>) -> usize {
    let mut removed = 0;
    arr.retain_mut(|group| {
        let Some(hs) = group.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
            return true;
        };
        let before = hs.len();
        hs.retain(|h| !is_ours(h));
        let n = before - hs.len();
        removed += n;
        !(n > 0 && hs.is_empty())
    });
    removed
}

/// Merge (or remove) Meanwhile hooks into an agent config document. Pure function, easy to test.
/// Returns the new document and a human summary.
pub fn merge_hooks(doc: &str, agent: Agent, hook_exe: &Path, remove: bool) -> Result<(String, String), String> {
    let mut root: Value = if doc.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(doc).map_err(|e| format!("not valid JSON, left untouched: {e}"))?
    };
    let obj = root.as_object_mut().ok_or("top level is not a JSON object, left untouched")?;
    if !obj.contains_key("hooks") {
        if remove {
            return Ok((doc.to_string(), "nothing to remove".into()));
        }
        obj.insert("hooks".into(), json!({}));
    }
    let hooks = obj
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .ok_or("\"hooks\" is not an object, left untouched")?;

    // Claude Code also tells us when a turn dies on an API error (StopFailure) and when the
    // session ends (SessionEnd): both cancel a pending wait so the overlay doesn't hang around.
    let events: &[(&str, &str)] = match agent {
        Agent::Claude => &[("UserPromptSubmit", "start"), ("Stop", "done"), ("StopFailure", "cancel"), ("SessionEnd", "cancel")],
        Agent::Codex => &[("UserPromptSubmit", "start"), ("Stop", "done")],
    };
    const ALL_EVENTS: &[&str] = &["UserPromptSubmit", "Stop", "StopFailure", "SessionEnd"];
    let mut removed = 0;
    for ev in ALL_EVENTS {
        if let Some(arr) = hooks.get_mut(*ev).and_then(|a| a.as_array_mut()) {
            removed += strip_ours(arr);
        }
    }
    // drop event keys we emptied completely
    for ev in ALL_EVENTS {
        if hooks.get(*ev).and_then(|a| a.as_array()).map(|a| a.is_empty()).unwrap_or(false) {
            hooks.remove(*ev);
        }
    }
    if remove {
        if hooks.is_empty() {
            obj.remove("hooks");
        }
        let out = serde_json::to_string_pretty(&root).unwrap() + "\n";
        return Ok((out, format!("removed {removed} Meanwhile hook(s)")));
    }
    for (ev, verb) in events {
        let entry = hooks.entry(ev.to_string()).or_insert_with(|| json!([]));
        let arr = entry.as_array_mut().ok_or(format!("hooks.{ev} is not an array, left untouched"))?;
        arr.push(json!({
            "hooks": [{
                "type": "command",
                "command": hook_command(hook_exe, &format!("{verb} --from {}", agent.name())),
                "timeout": 5
            }]
        }));
    }
    let out = serde_json::to_string_pretty(&root).unwrap() + "\n";
    let what = if removed > 0 { "updated" } else { "added" };
    let extra = if agent == Agent::Claude { ", StopFailure/SessionEnd -> cancel" } else { "" };
    Ok((out, format!("{what} UserPromptSubmit -> start and Stop -> done{extra}")))
}

/// Apply `merge_hooks` to the agent's real config file, with a timestamped backup next to it.
pub fn setup_agent(agent: Agent, hook_exe: &Path, remove: bool, dry_run: bool) -> Result<String, String> {
    let path = agent.config_file().ok_or("no home directory")?;
    let old = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let old_clean = old.trim_start_matches('\u{feff}');
    let (new, summary) = merge_hooks(old_clean, agent, hook_exe, remove)?;
    if dry_run {
        return Ok(format!("{} (dry run) {}:\n{}", agent.name(), path.display(), new));
    }
    if new == old_clean {
        return Ok(format!("{}: {} ({}), no change", agent.name(), summary, path.display()));
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    if !old.is_empty() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let bak = path.with_file_name(format!(
            "{}.bak-meanwhile-{ts}",
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&bak, &old).map_err(|e| format!("backup failed, nothing changed: {e}"))?;
    }
    let tmp = path.with_extension("json.meanwhile-tmp");
    std::fs::write(&tmp, &new).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    let mut msg = format!("{}: {} in {}", agent.name(), summary, path.display());
    if agent == Agent::Codex && !remove {
        msg.push_str(
            "\nCodex runs new hooks only after you trust them: CLI `/hooks` (press t), or in the desktop app \
             \"Review hooks\" in the composer / Settings > Hooks.",
        );
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODEX: &str = r#"{
  "description": "x",
  "hooks": {
    "SessionStart": [ { "matcher": "startup", "hooks": [ { "command": "py a.py hook", "type": "command", "timeout": 3 } ] } ],
    "Stop": [ { "hooks": [ { "command": "py a.py hook", "type": "command", "timeout": 3 } ] } ]
  }
}"#;

    #[test]
    fn merge_keeps_existing_and_appends() {
        let exe = Path::new("C:\\Users\\Lap\\AppData\\Local\\Meanwhile\\meanwhile-hook.exe");
        let (out, _) = merge_hooks(CODEX, Agent::Codex, exe, false).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["description"], "x");
        assert_eq!(v["hooks"]["SessionStart"][0]["hooks"][0]["command"], "py a.py hook");
        // existing Stop group stays at index 0, ours is appended
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "py a.py hook");
        assert_eq!(
            v["hooks"]["Stop"][1]["hooks"][0]["command"],
            "C:/Users/Lap/AppData/Local/Meanwhile/meanwhile-hook.exe done --from codex"
        );
        assert!(v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("start --from codex"));
        // key order preserved: description first
        assert!(out.find("description").unwrap() < out.find("hooks").unwrap());
    }

    #[test]
    fn merge_is_idempotent_and_removable() {
        let exe = Path::new("/home/u/.local/bin/meanwhile-hook");
        let (a, _) = merge_hooks(CODEX, Agent::Codex, exe, false).unwrap();
        let (b, _) = merge_hooks(&a, Agent::Codex, exe, false).unwrap();
        assert_eq!(a, b);
        let (c, s) = merge_hooks(&b, Agent::Codex, exe, true).unwrap();
        assert!(s.contains("removed 2"));
        let orig: Value = serde_json::from_str(CODEX).unwrap();
        let back: Value = serde_json::from_str(&c).unwrap();
        assert_eq!(orig, back);
    }

    #[test]
    fn merge_into_empty_and_settings_without_hooks() {
        let exe = Path::new("/x/meanwhile-hook");
        let (out, _) = merge_hooks("", Agent::Claude, exe, false).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap().contains("done --from claude"));
        let (out2, _) = merge_hooks(r#"{"effortLevel":"high"}"#, Agent::Claude, exe, false).unwrap();
        let v2: Value = serde_json::from_str(&out2).unwrap();
        assert_eq!(v2["effortLevel"], "high");
    }

    #[test]
    fn broken_json_is_refused() {
        assert!(merge_hooks("{ nope", Agent::Claude, Path::new("/x"), false).is_err());
        assert!(merge_hooks("[1]", Agent::Claude, Path::new("/x"), false).is_err());
    }

    #[test]
    fn mixed_group_keeps_foreign_handler() {
        let doc = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other"},{"type":"command","command":"/x/meanwhile-hook done"}]}]}}"#;
        let (out, _) = merge_hooks(doc, Agent::Claude, Path::new("/x/meanwhile-hook"), true).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hooks"]["Stop"][0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "other");
    }

    #[test]
    fn config_defaults_fill_missing_keys() {
        let c: Config = serde_json::from_str(r#"{"port": 5000}"#).unwrap();
        assert_eq!(c.port, 5000);
        assert_eq!(c.hotkey, DEFAULT_HOTKEY);
        assert!(c.activities.brain);
    }
}
