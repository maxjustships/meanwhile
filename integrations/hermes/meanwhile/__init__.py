"""Meanwhile for Hermes: start/done signals for the Meanwhile overlay.

pre_llm_call (once per turn, before the agent loop)  -> meanwhile-hook start --from hermes
post_llm_call (turn finished)                        -> meanwhile-hook done  --from hermes

The desktop running Meanwhile is usually not the machine running Hermes, so the default
transport is SSH: `ssh <host> "<hook exe>" start --from hermes`. Plain HTTP to the app's
endpoint works too (needs `extra_listen` + a firewall rule on the desktop).

Configuration (environment variables, e.g. in ~/.hermes/.env):
  MEANWHILE_SSH_HOST      ssh host alias of the desktop (default: asus-laptop)
  MEANWHILE_SSH_CONFIG    ssh config file (default: ~/.config/superkompek/ssh.conf if it exists)
  MEANWHILE_HOOK_PATH     hook exe on the desktop (default: C:/Users/Lap/AppData/Local/Meanwhile/meanwhile-hook.exe)
  MEANWHILE_URL           use HTTP instead of SSH, e.g. http://100.88.53.36:47800
  MEANWHILE_TOKEN         token for MEANWHILE_URL (from the desktop's config.json)
  MEANWHILE_PLATFORMS     comma list of Hermes platforms that trigger it (default: telegram)

Everything runs in a background process with short timeouts; failures are ignored.
"""
from __future__ import annotations

import json
import logging
import os
import subprocess
import threading
import urllib.request
from typing import Any

logger = logging.getLogger("plugins.meanwhile")

DEFAULT_HOOK = "C:/Users/Lap/AppData/Local/Meanwhile/meanwhile-hook.exe"


def _platforms() -> set[str]:
    raw = os.environ.get("MEANWHILE_PLATFORMS", "telegram")
    return {p.strip().lower() for p in raw.split(",") if p.strip()}


def _wanted(platform: Any) -> bool:
    return str(platform or "").lower() in _platforms()


def _ssh_cmd(verb: str) -> list[str]:
    host = os.environ.get("MEANWHILE_SSH_HOST", "asus-laptop")
    hook = os.environ.get("MEANWHILE_HOOK_PATH", DEFAULT_HOOK)
    cmd = ["ssh"]
    cfg = os.environ.get("MEANWHILE_SSH_CONFIG") or os.path.expanduser("~/.config/superkompek/ssh.conf")
    if os.path.exists(cfg):
        cmd += ["-F", cfg]
    cmd += ["-o", "BatchMode=yes", "-o", "ConnectTimeout=4", "-o", "LogLevel=ERROR", host, f'"{hook}" {verb} --from hermes']
    return cmd


def _http(verb: str) -> None:
    url = os.environ["MEANWHILE_URL"].rstrip("/") + "/" + verb
    req = urllib.request.Request(
        url,
        data=json.dumps({"from": "hermes"}).encode(),
        headers={"Authorization": "Bearer " + os.environ.get("MEANWHILE_TOKEN", ""), "Content-Type": "application/json"},
        method="POST",
    )
    try:
        urllib.request.urlopen(req, timeout=2).read()
    except Exception as exc:  # the desktop may be asleep; that's fine
        logger.debug("meanwhile %s via http failed: %s", verb, exc)


def _send(verb: str) -> None:
    if os.environ.get("MEANWHILE_URL"):
        threading.Thread(target=_http, args=(verb,), daemon=True).start()
        return
    try:
        subprocess.Popen(
            _ssh_cmd(verb),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
    except Exception as exc:
        logger.debug("meanwhile %s via ssh failed: %s", verb, exc)


_active: set[str] = set()
_lock = threading.Lock()


def pre_llm_call(*, platform: str | None = None, parent_session_id: str | None = None, session_id: str | None = None, **kwargs: Any) -> None:
    # subagents (parent_session_id set) share the parent's wait; only the main turn counts
    if _wanted(platform) and not parent_session_id:
        with _lock:
            _active.add(str(session_id or ""))
        _send("start")
    return None


def post_llm_call(*, platform: str | None = None, session_id: str | None = None, **kwargs: Any) -> None:
    with _lock:
        key = str(session_id or "")
        mine = key in _active
        _active.discard(key)
    if mine and _wanted(platform):
        _send("done")


def register(ctx) -> None:
    ctx.register_hook("pre_llm_call", pre_llm_call)
    ctx.register_hook("post_llm_call", post_llm_call)
