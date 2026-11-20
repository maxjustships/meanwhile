// Meanwhile activities. Each one is short (<= 60 s) and never open-ended.
// run(view, ctx) builds the UI inside `view` and returns an optional cleanup function.
// ctx: begin() starts the clock (for click-to-start games), finish() ends early,
//      score(text), summary(text), note(text), best(key, value, higherIsBetter), every(ms, fn), after(ms, fn)
(function () {
  const rnd = (a, b) => a + Math.floor(Math.random() * (b - a + 1));
  const pick = (arr) => arr[Math.floor(Math.random() * arr.length)];
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

  function figure(cls) {
    return `<svg class="fig ${cls}" viewBox="0 0 120 140" width="112" height="132" aria-hidden="true">
      <line class="mat" x1="18" y1="134" x2="102" y2="134"/>
      <line x1="60" y1="94" x2="46" y2="130"/><line x1="60" y1="94" x2="74" y2="130"/>
      <g class="upper">
        <line x1="60" y1="48" x2="60" y2="94"/>
        <g class="headg"><circle class="head" cx="60" cy="30" r="12"/></g>
        <line class="arm-l" x1="60" y1="52" x2="42" y2="82"/>
        <line class="arm-r" x1="60" y1="52" x2="78" y2="82"/>
      </g>
    </svg>`;
  }

  const STRETCHES = [
    ["neck", "Neck tilt", "Slowly tilt your head to the left, then to the right.", "Let your shoulders drop while you do it."],
    ["reach", "Reach up", "Sit tall and reach both arms overhead. Hold, then lower slowly.", "Breathe out as the arms come down."],
    ["shrug", "Shoulder rolls", "Lift your shoulders to your ears, roll them back and down.", "Slow circles. Ten of them."],
    ["side", "Side bend", "Arms up, lean gently to one side, then the other.", "Feel the stretch along your ribs."],
    ["twist", "Seated twist", "Sit tall and turn your upper body left, then right.", "Hold each side for a breath."],
  ];

  const A = [];

  A.push({
    id: "stretch", cat: "body", title: "Stretch", seconds: 30,
    run(view, ctx) {
      const [cls, title, line, sub] = pick(STRETCHES);
      ctx.title(title);
      view.innerHTML = `<div class="art">${figure(cls)}</div>
        <div class="txt"><div class="big">${line}</div><div class="sub">${sub}</div></div>`;
      ctx.summary("Loosened up.");
    },
  });

  A.push({
    id: "water", cat: "body", title: "Hydrate", seconds: 20,
    run(view, ctx) {
      const glass = "M14 10 L66 10 L60 100 Q60 104 56 104 L24 104 Q20 104 20 100 Z";
      view.innerHTML = `<div class="art"><svg viewBox="0 0 80 110" width="92" height="126" aria-hidden="true">
          <defs><clipPath id="glass"><path d="${glass}"/></clipPath></defs>
          <g clip-path="url(#glass)">
            <rect class="glass-water" x="0" y="22" width="80" height="84" style="--dur:${this.seconds}s"/>
            <g><circle class="bubble" cx="32" cy="98" r="2.5"/><circle class="bubble" cx="46" cy="98" r="2"/><circle class="bubble" cx="39" cy="98" r="1.8"/></g>
          </g>
          <path d="${glass}" fill="none" stroke="#fff" stroke-opacity=".75" stroke-width="3" stroke-linejoin="round"/>
        </svg></div>
        <div class="txt"><div class="big">Drink a glass of water.</div>
        <div class="sub">Glass is empty? Perfect excuse to stand up and refill it.</div></div>`;
      ctx.summary("Hydrated.");
    },
  });

  A.push({
    id: "eyes", cat: "body", title: "20-20-20 eye break", seconds: 20,
    run(view, ctx) {
      view.innerHTML = `<div class="art" style="width:100px"><div style="text-align:center">
          <svg viewBox="0 0 64 40" width="70" height="44" aria-hidden="true"><g class="eye-lid">
            <path d="M4 20 Q32 -4 60 20 Q32 44 4 20 Z" fill="none" stroke="#fff" stroke-opacity=".8" stroke-width="3"/>
            <circle cx="32" cy="20" r="8" fill="var(--acc)"/><circle cx="34.5" cy="17.5" r="2.4" fill="#fff"/></g></svg>
          <div class="count" id="eyecount">20</div></div></div>
        <div class="txt"><div class="big" id="eyetxt">Look at something about 6 m (20 ft) away.</div>
        <div class="sub" id="eyesub">Out the window is ideal. Let your eyes relax.</div></div>`;
      const c = view.querySelector("#eyecount");
      ctx.every(250, (t) => {
        const left = Math.max(0, Math.ceil(this.seconds - t / 1000));
        c.textContent = left;
        if (left <= 3) {
          view.querySelector("#eyetxt").textContent = "Now blink slowly a few times.";
          view.querySelector("#eyesub").textContent = "Your screen will still be here.";
        }
      });
      ctx.summary("Eyes rested.");
    },
  });

  A.push({
    id: "breathe", cat: "body", title: "4 slow breaths", seconds: 40,
    run(view, ctx) {
      view.innerHTML = `<div class="art"><div class="breath"><div class="ring"></div><div class="orb" id="orb"></div><div class="label" id="blabel"></div></div></div>
        <div class="txt"><div class="big" id="btxt">Breathe in through your nose…</div><div class="sub" id="bsub">Breath 1 of 4 · in for 4, out for 6</div></div>`;
      const orb = view.querySelector("#orb"), lab = view.querySelector("#blabel");
      const txt = view.querySelector("#btxt"), sub = view.querySelector("#bsub");
      let n = 0;
      const cycle = () => {
        if (n >= 4) return;
        n++;
        sub.textContent = `Breath ${n} of 4 · in for 4, out for 6`;
        txt.textContent = "Breathe in through your nose…";
        lab.textContent = "in";
        orb.style.transitionDuration = "4s";
        requestAnimationFrame(() => (orb.style.transform = "scale(1)"));
        ctx.after(4000, () => {
          txt.textContent = "…and slowly out through your mouth.";
          lab.textContent = "out";
          orb.style.transitionDuration = "6s";
          orb.style.transform = "scale(0.45)";
          ctx.after(6000, cycle);
        });
      };
      ctx.after(60, cycle);
      ctx.summary("Calmer already.");
    },
  });

  function startScreen(view, ctx, glyph, line, onGo) {
    view.innerHTML = `<div class="view col" style="position:absolute">
      <div class="glyph">${glyph}</div><div class="sub" style="font-size:13px">${line}</div>
      <div class="row" style="margin-top:4px"><button class="btn primary lg" id="go">Start</button></div></div>`;
    const go = view.querySelector("#go");
    go.addEventListener("click", () => {
      ctx.begin();
      onGo();
    });
  }

  A.push({
    id: "math", cat: "brain", title: "Mental math sprint", seconds: 45, needsStart: true,
    run(view, ctx) {
      startScreen(view, ctx, "+−×", "45 seconds. Type the answer, it moves on by itself.", () => {
        view.innerHTML = `<div class="view col" style="position:absolute"><div class="q" id="q"></div>
          <input class="ans" id="ans" inputmode="numeric" autocomplete="off" spellcheck="false"></div>`;
        const q = view.querySelector("#q"), ans = view.querySelector("#ans");
        let answer = 0, solved = 0;
        const next = () => {
          const k = rnd(0, 9);
          let a, b, op;
          if (k < 4) { a = rnd(12, 89); b = rnd(11, 79); op = "+"; answer = a + b; }
          else if (k < 7) { a = rnd(30, 99); b = rnd(11, a - 5); op = "−"; answer = a - b; }
          else if (k < 9) { a = rnd(3, 12); b = rnd(3, 12); op = "×"; answer = a * b; }
          else { a = rnd(12, 25); b = rnd(3, 6); op = "×"; answer = a * b; }
          q.textContent = `${a} ${op} ${b}`;
          ans.value = "";
        };
        ans.addEventListener("input", () => {
          const v = ans.value.replace(/[^0-9-]/g, "");
          if (v !== ans.value) ans.value = v;
          if (v !== "" && Number(v) === answer) {
            solved++;
            ctx.score(`${solved} solved`);
            q.classList.remove("ok"); void q.offsetWidth; q.classList.add("ok");
            next();
          }
        });
        ans.addEventListener("keydown", (e) => {
          if (e.key === "Enter" && ans.value !== "") {
            ans.classList.remove("shake"); void ans.offsetWidth; ans.classList.add("shake");
            ans.value = "";
          }
        });
        next();
        ans.focus();
        ctx.score("0 solved");
        ctx.onEnd(() => {
          const best = ctx.best("math", solved, true);
          ctx.summary(`${solved} solved · best ${best}`);
        });
      });
    },
  });

  A.push({
    id: "reaction", cat: "brain", title: "Reaction test", seconds: 30, needsStart: true,
    run(view, ctx) {
      startScreen(view, ctx, `<svg viewBox="0 0 24 24" width="28" height="28"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2.4"/><circle cx="12" cy="12" r="3.5" fill="currentColor"/></svg>`, "Click the targets before they vanish. 30 seconds.", () => {
        view.innerHTML = `<div class="arena" id="arena"></div>`;
        const arena = view.querySelector("#arena");
        let hits = 0, misses = 0, times = [], life = 1150, timer = null, alive = true;
        const spawn = () => {
          if (!alive) return;
          const r = arena.getBoundingClientRect();
          const t = document.createElement("button");
          t.className = "target";
          t.style.setProperty("--life", life + "ms");
          t.style.left = rnd(24, Math.max(25, r.width - 24)) + "px";
          t.style.top = rnd(22, Math.max(23, r.height - 22)) + "px";
          const born = performance.now();
          t.addEventListener("mousedown", (e) => {
            e.stopPropagation();
            clearTimeout(timer);
            const ms = Math.round(performance.now() - born);
            hits++; times.push(ms);
            life = Math.max(650, life - 25);
            const p = document.createElement("div");
            p.className = "pop"; p.textContent = ms + " ms";
            p.style.left = t.style.left; p.style.top = t.style.top;
            arena.appendChild(p); setTimeout(() => p.remove(), 600);
            t.remove();
            ctx.score(`${hits} hits`);
            ctx.after(rnd(120, 420), spawn);
          });
          arena.appendChild(t);
          timer = setTimeout(() => { t.remove(); misses++; spawn(); }, life);
        };
        arena.addEventListener("mousedown", () => { misses++; });
        ctx.score("0 hits");
        ctx.after(250, spawn);
        ctx.onEnd(() => {
          alive = false; clearTimeout(timer);
          const avg = times.length ? Math.round(times.reduce((a, b) => a + b, 0) / times.length) : 0;
          const best = ctx.best("reaction", hits, true);
          ctx.summary(`${hits} hits · ${misses} misses${avg ? ` · avg ${avg} ms` : ""} · best ${best}`);
        });
        return () => { alive = false; clearTimeout(timer); };
      });
    },
  });

  A.push({
    id: "vocab", cat: "brain", title: "Word of the moment", seconds: 45,
    run(view, ctx) {
      const words = window.MW_WORDS || [];
      let known = 0, seen = 0;
      const show = () => {
        const [w, pos, def, ex] = pick(words);
        seen++;
        view.innerHTML = `<div class="view col flash" style="position:absolute" id="fc">
          <div><span class="word">${esc(w)}</span><span class="pos">${esc(pos)}</span></div>
          <div class="hint" id="hint">Do you know it? Click to check.</div>
          <div class="def" id="def" style="display:none">${esc(def)}</div>
          <div class="ex" id="ex" style="display:none">“${esc(ex)}”</div>
          <div class="row" id="btns" style="display:none;margin-top:4px">
            <button class="btn" id="no">New to me</button><button class="btn primary" id="yes">Knew it</button></div></div>`;
        const fc = view.querySelector("#fc");
        fc.addEventListener("click", () => {
          view.querySelector("#hint").style.display = "none";
          for (const id of ["def", "ex"]) view.querySelector("#" + id).style.display = "";
          view.querySelector("#btns").style.display = "";
        }, { once: true });
        view.querySelector("#yes").addEventListener("click", (e) => { e.stopPropagation(); known++; ctx.score(`${known}/${seen}`); show(); });
        view.querySelector("#no").addEventListener("click", (e) => { e.stopPropagation(); ctx.score(`${known}/${seen}`); show(); });
      };
      show();
      ctx.onEnd(() => ctx.summary(seen > 1 ? `${known} of ${seen - 1} known` : "One new word."));
    },
  });

  A.push({
    id: "focus", cat: "focus", title: "Before the answer lands", seconds: 30, onlyWaiting: true,
    run(view, ctx) {
      view.innerHTML = `<div class="view col" style="position:absolute;align-items:stretch;text-align:left">
        <div class="big">What will you check first in the answer?</div>
        <input class="note" id="note" maxlength="140" placeholder="e.g. does it handle an empty list?" autocomplete="off">
        <div class="sub" id="nsub">Press Enter. I'll show it to you when the answer is ready.</div></div>`;
      const inp = view.querySelector("#note");
      inp.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && inp.value.trim()) {
          ctx.note(inp.value.trim());
          inp.disabled = true;
          view.querySelector("#nsub").textContent = "Got it. Now take a breather.";
          ctx.summary("Noted.");
          ctx.after(1400, () => ctx.finish());
        }
      });
      inp.addEventListener("mousedown", () => inp.focus());
      ctx.summary("");
    },
  });

  window.MW_ACTIVITIES = A;
})();
