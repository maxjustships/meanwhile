// Overlay controller: picks activities, runs the clock, shows "your answer's ready".
(function () {
  const TAURI = window.__TAURI__;
  const invoke = TAURI ? TAURI.core.invoke : async () => null;
  const listen = TAURI ? TAURI.event.listen : async () => () => {};
  const $ = (id) => document.getElementById(id);
  const card = $("card"), stage = $("stage"), bar = $("bar"), statusEl = $("status"), pulse = $("pulse");
  const ACTS = window.MW_ACTIVITIES;
  const AGENTS = { claude: "Claude", codex: "Codex", hermes: "Hermes", gemini: "Gemini", cursor: "Cursor" };
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
  const mmss = (ms) => { const s = Math.max(0, Math.floor(ms / 1000)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`; };

  const S = {
    cfg: { hotkey: "Ctrl+Alt+Space", activities: { body: true, brain: true, focus: true } },
    visible: false, mode: "manual", from: "", waiting: false, waitStart: 0, note: "",
    phase: "idle", cur: null, recent: [], betweenUntil: 0, betweenStart: 0, readyUntil: 0, readyStart: 0, hover: false,
  };
  const agentName = () => AGENTS[(S.from || "").toLowerCase()] || "Your AI";

  // ------------------------------------------------------------ activity lifecycle
  function makeCtx(cur) {
    const timers = cur.timers;
    return {
      begin() { if (!cur.begun) { cur.begun = true; cur.startedAt = performance.now(); } },
      finish() { if (S.cur === cur) endActivity(true); },
      title(t) { $("title").textContent = t; },
      score(t) { $("score").textContent = t; },
      summary(t) { cur.summary = t; },
      note(t) { S.note = t; },
      onEnd(fn) { cur.onEnd.push(fn); },
      best(key, val, higher) {
        const k = "mw.best." + key, old = Number(localStorage.getItem(k) || (higher ? 0 : 1e9));
        const b = higher ? Math.max(old, val) : Math.min(old, val);
        localStorage.setItem(k, String(b));
        return b;
      },
      every(ms, fn) { const t0 = performance.now(); timers.push(setInterval(() => fn(performance.now() - t0), ms)); },
      after(ms, fn) { timers.push(setTimeout(() => { if (S.cur === cur) fn(); }, ms)); },
    };
  }

  function pickActivity() {
    const on = S.cfg.activities || {};
    let pool = ACTS.filter((a) => on[a.cat] !== false && (!a.onlyWaiting || (S.waiting && !S.note && !S.focusAsked)));
    if (!pool.length) pool = ACTS.filter((a) => a.cat === "body");
    // the focus prompt is most useful early in a wait
    const focus = pool.find((a) => a.id === "focus");
    if (focus && Math.random() < 0.45) return focus;
    const fresh = pool.filter((a) => !S.recent.includes(a.id));
    const list = fresh.length ? fresh : pool;
    return list[Math.floor(Math.random() * list.length)];
  }

  function teardown() {
    const cur = S.cur;
    if (!cur) return;
    S.cur = null;
    for (const fn of cur.onEnd) { try { fn(); } catch (e) { console.error(e); } }
    for (const t of cur.timers) { clearTimeout(t); clearInterval(t); }
    if (cur.cleanup) { try { cur.cleanup(); } catch (e) { console.error(e); } }
  }

  function startActivity(act) {
    teardown();
    act = act || pickActivity();
    if (act.id === "focus") S.focusAsked = true;
    S.recent = [act.id, ...S.recent.filter((x) => x !== act.id)].slice(0, 3);
    S.phase = "activity";
    card.dataset.cat = act.cat;
    $("cat").textContent = act.cat;
    $("title").textContent = act.title;
    $("score").textContent = "";
    $("skip").style.visibility = "";
    stage.innerHTML = "";
    const view = document.createElement("div");
    view.className = "view";
    stage.appendChild(view);
    const cur = { act, timers: [], onEnd: [], begun: !act.needsStart, startedAt: performance.now(), shownAt: performance.now(), summary: "" };
    S.cur = cur;
    try { cur.cleanup = act.run.call(act, view, makeCtx(cur)); } catch (e) { console.error(e); }
  }

  function endActivity(natural) {
    const cur = S.cur;
    if (!cur) return;
    teardown();
    if (!natural) return startActivity();
    showBetween(cur.summary);
  }

  function showBetween(summary) {
    S.phase = "between";
    $("score").textContent = "";
    stage.innerHTML = "";
    const v = document.createElement("div");
    v.className = "view col";
    if (S.waiting) {
      v.innerHTML = `<div class="big">${esc(summary || "Nice.")}</div><div class="sub">Still thinking. Here's another one…</div>`;
      S.betweenStart = performance.now(); S.betweenUntil = S.betweenStart + 1800;
    } else {
      v.innerHTML = `<div class="big">${esc(summary || "Nice.")}</div><div class="sub">That's your minute.</div>
        <div class="row" style="margin-top:6px"><button class="btn" id="bclose">Close</button><button class="btn primary" id="bmore">One more</button></div>`;
      S.betweenStart = performance.now(); S.betweenUntil = S.betweenStart + 10000;
    }
    stage.appendChild(v);
    const more = v.querySelector("#bmore"), close = v.querySelector("#bclose");
    if (more) more.onclick = () => startActivity();
    if (close) close.onclick = () => dismiss();
  }

  function showReady(waitedMs) {
    teardown();
    S.phase = "ready";
    card.dataset.cat = "ready";
    $("cat").textContent = "ready";
    $("title").textContent = "";
    $("score").textContent = "";
    $("skip").style.visibility = "hidden";
    const took = waitedMs > 0 ? ` after ${mmss(waitedMs)}` : "";
    stage.innerHTML = `<div class="view" style="cursor:pointer" id="rv">
      <svg class="check" viewBox="0 0 64 64" aria-hidden="true"><circle cx="32" cy="32" r="28"/><path d="M20 33 l8 8 l16 -18"/></svg>
      <div class="txt"><div class="ready-title">Your answer's ready</div>
      <div class="sub">${esc(agentName())} finished${took}. Back you go.</div>
      ${S.note ? `<div class="quote"><b>You wanted to check</b>${esc(S.note)}</div>` : ""}</div></div>`;
    $("rv").onclick = () => dismiss();
    card.classList.remove("ready-pulse"); void card.offsetWidth; card.classList.add("ready-pulse");
    S.readyStart = performance.now();
    S.readyUntil = S.readyStart + (S.note ? 6000 : 2400);
    S.waiting = false;
    S.note = "";
    S.focusAsked = false;
  }

  function dismiss() {
    invoke("dismiss");
    if (!TAURI) hideAnim();
  }

  function hideAnim() {
    S.visible = false;
    card.classList.remove("in");
    card.classList.add("out");
    setTimeout(() => { if (!S.visible) { teardown(); S.phase = "idle"; stage.innerHTML = ""; } }, 220);
  }

  function showAnim() {
    S.visible = true;
    card.classList.remove("out");
    card.classList.remove("in");
    void card.offsetWidth;
    requestAnimationFrame(() => card.classList.add("in"));
  }

  // ------------------------------------------------------------ clock: status line + progress bar
  function tick() {
    if (!S.visible) return;
    const now = performance.now();
    let frac = 0;
    if (S.phase === "activity" && S.cur) {
      const c = S.cur;
      if (!c.begun) {
        frac = 0;
        if (now - c.shownAt > 20000) return startActivity(); // nobody clicked Start: move on
      } else {
        const total = c.act.seconds * 1000, el = now - c.startedAt;
        frac = Math.max(0, 1 - el / total);
        if (el >= total) return endActivity(true);
      }
    } else if (S.phase === "between") {
      frac = Math.max(0, (S.betweenUntil - now) / (S.betweenUntil - S.betweenStart));
      if (now >= S.betweenUntil) return S.waiting ? startActivity() : dismiss();
    } else if (S.phase === "ready") {
      if (S.hover) S.readyUntil += 250;
      frac = Math.max(0, (S.readyUntil - now) / (S.readyUntil - S.readyStart));
      if (now >= S.readyUntil) { S.phase = "closing"; return dismiss(); }
    }
    bar.style.transform = `scaleX(${frac.toFixed(4)})`;
    if (S.phase === "ready") {
      statusEl.textContent = "Click to close";
      pulse.className = "pulse";
    } else if (S.waiting) {
      statusEl.textContent = `${agentName()} is thinking · ${mmss(Date.now() - S.waitStart)}`;
      pulse.className = "pulse live";
    } else {
      statusEl.textContent = `Quick break · ${S.cfg.hotkey || "Esc"} to close`;
      pulse.className = "pulse";
    }
  }
  setInterval(tick, 250);

  // ------------------------------------------------------------ wiring
  $("close").onclick = () => dismiss();
  $("skip").onclick = () => startActivity();
  card.addEventListener("mouseenter", () => (S.hover = true));
  card.addEventListener("mouseleave", () => (S.hover = false));
  window.addEventListener("keydown", (e) => { if (e.key === "Escape") dismiss(); });

  function onShow(p) {
    if (p.config) S.cfg = p.config;
    const fresh = p.fresh || !S.visible;
    if (p.mode === "auto" && !S.waiting && !p.doneOnly) {
      S.waiting = true;
      S.from = p.from || "";
      S.waitStart = Date.now() - (p.waitedMs || 0);
      if (fresh) { S.note = ""; S.focusAsked = false; }
    } else if (p.mode === "auto") {
      S.from = p.from || S.from;
    }
    if (fresh) {
      showAnim();
      if (!p.doneOnly) startActivity();
    }
    tick();
  }

  listen("mw:show", (e) => onShow(e.payload));
  listen("mw:done", (e) => {
    const waited = S.waiting ? Date.now() - S.waitStart : (e.payload && e.payload.waitedMs) || 0;
    if (e.payload && e.payload.from) S.from = e.payload.from;
    showReady(waited);
    tick();
  });
  listen("mw:cancel", () => { S.waiting = false; S.note = ""; });
  listen("mw:hide", () => hideAnim());
  listen("mw:config", (e) => { S.cfg = e.payload; });

  invoke("get_state").then((st) => { if (st && st.config) S.cfg = st.config; }).catch(() => {});
  invoke("overlay_ready").catch(() => {});

  // ------------------------------------------------------------ browser preview (no Tauri): overlay.html?demo=math&mode=auto&from=claude
  if (!TAURI) {
    const q = new URLSearchParams(location.search);
    document.body.style.background = q.get("bg") || "#3b4252";
    const id = q.get("demo");
    onShow({ mode: q.get("mode") || "manual", from: q.get("from") || "claude", fresh: true, doneOnly: id === "ready", waitedMs: 47000 });
    if (id === "ready") { S.note = q.get("note") || ""; S.waiting = true; S.waitStart = Date.now() - 84000; showReady(84000); S.readyUntil = Infinity; }
    else if (id) startActivity(ACTS.find((a) => a.id === id));
    window.MW = { S, startActivity, showReady };
  }
})();
