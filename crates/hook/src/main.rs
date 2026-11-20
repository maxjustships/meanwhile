//! meanwhile-hook: one-line glue between AI agents and the Meanwhile overlay.
//!
//!   meanwhile-hook start  [--from claude]   agent got a prompt: show something to do
//!   meanwhile-hook done   [--from claude]   answer is ready: pull the user back
//!   meanwhile-hook cancel                   stop waiting, hide quietly
//!   meanwhile-hook toggle                   same as the hotkey
//!   meanwhile-hook status                   is the app running?
//!   meanwhile-hook setup claude|codex [--remove] [--dry-run]
//!
//! Hook commands are silent and always exit 0, so an agent is never blocked or slowed down
//! (the app not running just means nothing happens). Add -v to see what happened.

use meanwhile_core::{self as core, Agent, Config};
use std::time::Duration;

fn usage() -> ! {
    eprintln!(
        "meanwhile-hook {}\n\n\
         Usage:\n  \
         meanwhile-hook start|done|cancel|toggle [--from NAME] [-v]\n  \
         meanwhile-hook status\n  \
         meanwhile-hook setup claude|codex [--remove] [--dry-run]\n\n\
         Environment: MEANWHILE_URL (default http://127.0.0.1:<port from config>), MEANWHILE_TOKEN.\n\
         Config: {}",
        core::VERSION,
        core::config_path().display()
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(|s| s.as_str()) else { usage() };
    let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");
    let from = args
        .iter()
        .position(|a| a == "--from")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_default();

    match cmd {
        "start" | "done" | "cancel" | "toggle" | "status" => {
            let cfg = Config::load();
            let base = std::env::var("MEANWHILE_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("127.0.0.1:{}", cfg.port));
            let token = std::env::var("MEANWHILE_TOKEN")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or(cfg.token.clone());
            let from_safe: String = from.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(32).collect();
            let body = format!("{{\"from\":\"{from_safe}\"}}");
            let res = core::post(&base, &format!("/{cmd}"), &token, &body, Duration::from_millis(800));
            if cmd == "status" {
                match res {
                    Ok(r) if r.status == 200 => {
                        println!("{}", r.body.trim());
                        std::process::exit(0)
                    }
                    Ok(r) => {
                        eprintln!("Meanwhile answered HTTP {}: {}", r.status, r.body.trim());
                        std::process::exit(1)
                    }
                    Err(e) => {
                        eprintln!("Meanwhile is not running ({e})");
                        std::process::exit(1)
                    }
                }
            }
            if verbose {
                match &res {
                    Ok(r) => eprintln!("{cmd}: HTTP {} {}", r.status, r.body.trim()),
                    Err(e) => eprintln!("{cmd}: app not reachable ({e})"),
                }
            }
            std::process::exit(0)
        }
        "setup" => {
            let Some(agent) = args.get(1).and_then(|a| Agent::parse(a)) else { usage() };
            let remove = args.iter().any(|a| a == "--remove" || a == "--uninstall");
            let dry = args.iter().any(|a| a == "--dry-run");
            let exe = std::env::current_exe().unwrap_or_else(|_| "meanwhile-hook".into());
            match core::setup_agent(agent, &exe, remove, dry) {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("{}: {e}", agent.name());
                    std::process::exit(1)
                }
            }
        }
        "version" | "--version" | "-V" => println!("meanwhile-hook {}", core::VERSION),
        _ => usage(),
    }
}
