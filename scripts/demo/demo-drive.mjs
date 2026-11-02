// Drives the Mechanical Muse demo video in a clean Chrome window while a person narrates. It clicks
// through the real app (recorded model sessions and replays, labeled as such on screen) with a
// visible cursor, shows title, diagram, sheet, results and closing cards between scenes, and prints
// each narration line in the terminal. Usually started by scripts/demo/take.sh (see
// docs/demo-script.md, "Recording with the driver").
//
// Run from the repo root with the app served by `make demo` (keyless: recorded model, replays):
//   node scripts/demo/demo-drive.mjs           # cue mode: the terminal shows each line; Enter plays it
//   node scripts/demo/demo-drive.mjs --auto    # timed mode: the script's timings, no key presses
//   node scripts/demo/demo-drive.mjs --headless --auto --shots <dir>   # dry run, a screenshot per step
// Options: --base URL (http://127.0.0.1:8080), --size 1280x720, --pos 0,25, --theme dark|light,
//   --no-fault (skip the optional fault scene), --from <scene id>, --clip <muse-code.mp4>,
//   --chrome <path to Chrome> (or CHROME=...), --port 9334 (DevTools port), --test-bounds (headless:
//   size the window as a headed run does, to check the viewport).
// The narration below follows docs/demo-script.md (scenes table, "Say" column): change both together.
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";
import { connect, sleep } from "./cdp.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const flag = (name) => args.includes(`--${name}`);
const opt = (name, def) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : def;
};
const BASE = opt("base", "http://127.0.0.1:8080");
const [W, H] = opt("size", "1280x720").split("x").map(Number);
const [PX, PY] = opt("pos", "0,25").split(",").map(Number);
const THEME = opt("theme", "dark");
const AUTO = flag("auto");
const HEADLESS = flag("headless");
const SHOTS = opt("shots", null);
const FROM = opt("from", null);
const PORT = Number(opt("port", "9334"));
const CLIP = opt("clip", null);
const CHROME = opt("chrome", process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
const REPO = process.cwd();
if (!existsSync(join(REPO, "docs/demo-script.md"))) {
  console.error("Run this from the repo root (docs/demo-script.md not found).");
  process.exit(2);
}

// ---------- terminal ----------
const B = "\x1b[1m", D = "\x1b[2m", R = "\x1b[0m", C = "\x1b[36m", Y = "\x1b[33m", G = "\x1b[32m", RED = "\x1b[31m";
const rl = AUTO ? null : createInterface({ input: process.stdin });
const lines = [];
let waiter = null;
rl?.on("line", (l) => (waiter ? (waiter(l), (waiter = null)) : lines.push(l)));
const nextLine = () => (lines.length ? Promise.resolve(lines.shift()) : new Promise((r) => (waiter = r)));
const fmt = (ms) => {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};
const parseAt = (s) => {
  const [m, sec] = s.split(":").map(Number);
  return (m * 60 + sec) * 1000;
};

// ---------- browser ----------
const profile = mkdtempSync(join(tmpdir(), "mm-demo-chrome-"));
const chromeArgs = [
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${profile}`,
  "--no-first-run",
  "--no-default-browser-check",
  "--disable-features=Translate,OptimizationHints,MediaRouter",
  "--disable-session-crashed-bubble",
  "--hide-crash-restore-bubble",
  "--password-store=basic",
  "--autoplay-policy=no-user-gesture-required",
  `--window-position=${PX},${PY}`,
  `--window-size=${W},${H + 40}`,
];
if (HEADLESS) chromeArgs.push("--headless=new");
chromeArgs.push(`--app=${BASE}/#/`);
const chrome = spawn(CHROME, chromeArgs, { stdio: "ignore" });
chrome.on("exit", (code) => {
  if (!finished) {
    console.error(`${RED}Chrome exited (${code}).${R}`);
    process.exit(1);
  }
});
let finished = false;

for (let i = 0; i < 60; i++) {
  try {
    await (await fetch(`http://127.0.0.1:${PORT}/json/version`)).json();
    break;
  } catch {
    await sleep(250);
  }
}
const targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const appTarget = targets.find((t) => t.type === "page" && t.url.startsWith(BASE));
const c = await connect(PORT, "about:blank", appTarget ?? null);
const ev = (expr) => c.evaluate(expr);

await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: THEME }] });
// Exact viewport: grow the window by the difference between outer and inner sizes.
if (HEADLESS && !flag("test-bounds")) {
  await c.send("Emulation.setDeviceMetricsOverride", { width: W, height: H, deviceScaleFactor: 2, mobile: false });
} else {
  const { windowId } = await c.send("Browser.getWindowForTarget");
  const [ow, oh, iw, ih] = await ev("[outerWidth, outerHeight, innerWidth, innerHeight]");
  await c.send("Browser.setWindowBounds", { windowId, bounds: { left: PX, top: PY, width: W + (ow - iw), height: H + (oh - ih) } });
  await sleep(300);
  const [w2, h2] = await ev("[innerWidth, innerHeight]");
  console.log(`${D}window viewport ${w2}x${h2}${w2 === W && h2 === H ? "" : ` (wanted ${W}x${H})`}${R}`);
}
const helpers = readFileSync(join(here, "page-helpers.js"), "utf8");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: `try{localStorage.setItem("mm-theme",${JSON.stringify(THEME)})}catch{}; addEventListener("DOMContentLoaded", () => { ${helpers} });` });
await c.send("Page.navigate", { url: `${BASE}/#/` });
await sleep(1500);
await ev(helpers);

// ---------- primitives ----------
async function mouse(type, x, y) {
  await c.send("Input.dispatchMouseEvent", { type, x, y, button: type === "mouseMoved" ? "none" : "left", clickCount: type === "mouseMoved" ? 0 : 1 });
}
async function moveTo(x, y, ms = 700) {
  await ev(`__demo.moveCursor(${x}, ${y}, ${ms})`);
  await mouse("mouseMoved", x, y);
}
/** Moves the cursor to an element and clicks it for real (pointer events, so React sees a click). */
async function click(desc, { after = 250, frac, move = 520 } = {}) {
  const r = await ev(`__demo.locate(${JSON.stringify(desc)}, ${JSON.stringify(frac ? { frac } : {})})`);
  if (!r) throw new Error(`not found: ${JSON.stringify(desc)}`);
  const x = Math.round(r.x + r.w / 2), y = Math.round(r.y + r.h / 2);
  await moveTo(x, y, move);
  await mouse("mousePressed", x, y);
  await mouse("mouseReleased", x, y);
  await ev(`__demo.ripple(${x}, ${y})`);
  await sleep(after);
}
/** Moves the cursor to rest on an element without clicking (to point at it). */
async function point(desc, opts = {}) {
  const r = await ev(`__demo.locate(${JSON.stringify(desc)}, ${JSON.stringify(opts)})`);
  if (!r) return false;
  await moveTo(Math.round(r.x + Math.min(r.w / 2, 60)), Math.round(r.y + r.h / 2));
  return true;
}
async function park() {
  await moveTo(W - 40, H - 40, 600);
}
async function waitFor(expr, ms = 30000, what = expr) {
  for (let t = 0; t < ms; t += 150) {
    if (await ev(expr)) return;
    await sleep(150);
  }
  throw new Error(`timed out waiting for ${what}`);
}
const has = (text) => `document.body.innerText.includes(${JSON.stringify(text)})`;
const waitText = (text, ms) => waitFor(has(text), ms, JSON.stringify(text));
const scrollToText = (text, o = {}) => ev(`__demo.scrollToText(${JSON.stringify(text)}, ${JSON.stringify(o)})`);
const scrollBy = (dy, ms = 900) => ev(`__demo.scrollBy(${dy}, ${ms})`);
const scrollTop = (ms = 700) => ev(`__demo.scrollTo(0, ${ms})`);
const card = (html) => ev(`__demo.card(${JSON.stringify(html)})`);
const uncard = () => ev(`__demo.uncard()`);
const dataUrl = (path, type) => `data:${type};base64,${readFileSync(join(REPO, path)).toString("base64")}`;

/** Plays a recorded run from the Runs page at a given speed. */
async function replay(name, speed, { home = false } = {}) {
  await click({ text: "Runs", sel: '[role="tab"], button' });
  await sleep(500);
  await click({ text: home ? "Home" : "Office" });
  await sleep(400);
  await click({ text: speed });
  const r = await ev(`(async () => {
    const card = [...document.querySelectorAll("li, div")].filter(e => e.textContent.includes(${JSON.stringify(name)}) && [...e.querySelectorAll("button")].some(b => b.textContent.trim() === "Replay")).sort((a, b) => a.textContent.length - b.textContent.length)[0];
    if (!card) return null;
    card.setAttribute("data-demo-replay", ${JSON.stringify(name)});
    return true;
  })()`);
  if (!r) throw new Error(`replay not listed: ${name}`);
  await click({ text: "Replay", sel: `[data-demo-replay="${name}"] button` });
  await waitFor(`location.hash.startsWith("#/runs/run-")`, 10000, "run view");
  await sleep(300);
  // The list scrolled to the chosen card; the run view sits at the top of the right column.
  await scrollTop(800);
}

// ---------- cards ----------
const titleCard = `
  <div style="text-align:center;max-width:1000px;padding:0 40px">
    <div style="font-size:68px;font-weight:700;letter-spacing:-0.02em">Mechanical Muse</div>
    <div style="font-size:27px;margin-top:14px" class="muted">A written building control sequence, turned into a verified controller</div>
    <div style="margin-top:44px;font-size:19px" class="muted">Push the Frontier &middot; Muse Spark via the Meta Model API</div>
  </div>`;
const pipelineCard = () => `
  <div style="text-align:center;width:100%;padding:0 48px">
    <div style="font-size:30px;font-weight:650;margin-bottom:18px">Built during the hackathon, and what we call</div>
    <img src="${dataUrl("docs/img/pipeline.svg", "image/svg+xml")}" style="width:100%;max-width:1160px;height:auto">
  </div>`;
const sheetCard = () => `
  <div style="height:100%;display:flex;align-items:center;justify-content:center;padding:24px">
    <img src="${dataUrl("fixtures/sequences/ahu-sat-reset-sample.png", "image/png")}" style="max-height:${H - 48}px;max-width:90%;border-radius:6px;box-shadow:0 10px 40px rgba(0,0,0,.45);background:white">
  </div>`;
const resultsCard = `
  <div style="max-width:1040px;padding:0 48px">
    <div style="font-size:22px" class="muted">Benchmark &middot; 137 open fault-detection rules, rebuilt by Muse Spark from scrubbed tasks</div>
    <div style="display:flex;gap:56px;margin:30px 0 26px">
      <div><div style="font-size:64px;font-weight:700">122&ndash;125</div><div style="font-size:20px" class="muted">rules pass on the first try</div></div>
      <div><div style="font-size:64px;font-weight:700">130&ndash;132</div><div style="font-size:20px" class="muted">after repair</div></div>
      <div><div style="font-size:64px;font-weight:700">0</div><div style="font-size:20px" class="muted">constant baselines</div></div>
    </div>
    <div style="font-size:19px" class="muted">Scored by replaying the library's 1,760 test vectors on the engine &middot; 3 runs &middot; <span class="mono">muse-spark-1.3-contributor</span></div>
  </div>`;
const museCodeCard = () =>
  CLIP
    ? `<video src="${dataUrl(CLIP, "video/mp4")}" autoplay muted playsinline style="max-width:92%;max-height:${H - 60}px;border-radius:8px"></video>`
    : `
  <div style="max-width:1000px;padding:0 48px">
    <div style="font-size:40px;font-weight:700">Built with Muse Code and Claude Code</div>
    <div style="font-size:23px;margin-top:22px" class="muted">Coding agents get the same checks over MCP (<span class="mono">mm-mcp</span>):</div>
    <div class="mono" style="font-size:21px;margin-top:16px;line-height:1.7">list_specs &middot; get_spec &middot; palette &middot; validate_graph<br>emit_cxf &middot; verify_graph &middot; run_ledger &middot; compare_runs</div>
  </div>`;
const closingCard = `
  <div style="text-align:center;max-width:1040px;padding:0 40px">
    <div style="font-size:56px;font-weight:700">Mechanical Muse</div>
    <div style="font-size:26px;margin-top:12px" class="muted">Muse Spark proposes. The engine checks. Only verified logic runs.</div>
    <div class="mono" style="font-size:24px;margin-top:40px;line-height:1.8">github.com/jscott3201/mechanical-muse<br>jscott3201.github.io/mechanical-muse</div>
    <div style="font-size:17px;margin-top:36px" class="muted">Muse Spark via the Meta Model API &middot; open-control-engine, BOPTEST and rusty-bacnet are existing open-source projects we call</div>
  </div>`;

// ---------- scenes (docs/demo-script.md) ----------
const SCENES = [
  {
    id: "open", at: "0:00", title: "Opening",
    beats: [
      { ms: 8000, say: "Mechanical Muse uses Muse Spark, through the Meta Model API, to turn a written building control sequence into a verified controller.",
        run: async () => { await park(); await card(titleCard); } },
      { ms: 9000, say: "Everything shown was built during this hackathon; the engine, BOPTEST and the BACnet stack are existing open-source projects we call.",
        run: async () => { await card(pipelineCard()); } },
    ],
  },
  {
    id: "sheet", at: "0:17", title: "The sheet",
    beats: [
      { ms: 9000, say: "Sequences arrive as sheets like this. Each building's are programmed by hand and rarely tested against the sheet before they run.",
        run: async () => { await card(sheetCard()); } },
    ],
  },
  {
    id: "read", at: "0:26", title: "Read and accept (revision A)",
    beats: [
      { ms: 5500, say: "In a recorded session, Spark reads the photo into a precise task,",
        run: async () => {
          await ev(`location.hash = "#/workbench"`);
          await sleep(300);
          await uncard();
          await click({ text: "Use the sample sequence sheet" });
          await sleep(500);
          await click({ text: "Read with Muse Spark" });
          await waitText("Accept 7 scenarios", 20000);
          await sleep(300);
          await scrollToText("Task for Muse Spark", { frac: 0.12, ms: 1100 });
        } },
      { ms: 6000, say: "lists what the sheet leaves open instead of guessing, and proposes test scenarios.",
        run: async () => {
          await scrollToText("The document leaves", { frac: 0.12, ms: 1000 });
          await sleep(1600);
          await scrollToText("Proposed test scenarios", { frac: 0.1, ms: 1300 });
          await sleep(600);
          await scrollBy(420, 2200);
        } },
      { ms: 5000, say: "The engineer accepts them; only then do they become the gate.",
        run: async () => {
          await click({ text: "Accept 7 scenarios and create the sequence" }, { frac: 0.6, after: 900 });
        } },
    ],
  },
  {
    id: "draftA", at: "0:42", title: "Draft and bind (revision A)",
    beats: [
      { ms: 6000, say: "Each draft is replayed on the engine: seven of seven, with a content id.",
        run: async () => {
          await scrollTop(400);
          await click({ text: "Muse Spark", sel: '[role="tab"]' });
          await click({ text: "Recorded session" }, { after: 200 });
          await click({ text: "Draft" });
          await waitText("Verified draft stored", 20000);
          await point({ text: "Passes 7 of 7 scenarios", exact: false, sel: "div, span, p, strong" }, { frac: 0.45 });
        } },
      { ms: 7000, say: "Spark proposes which building points to use; the server checks units and limits, and the engineer confirms.",
        run: async () => {
          await scrollTop(500);
          await click({ text: "Building", sel: '[role="tab"]' });
          await click({ text: "Propose a binding with Muse Spark" });
          await waitText("Confirm this binding", 20000);
          await sleep(400);
          await scrollBy(260, 1400);
          await sleep(700);
          await click({ text: "Confirm this binding" }, { frac: 0.6, after: 900 });
        } },
    ],
  },
  {
    id: "runA", at: "0:55", title: "Revision A on the building (replay)",
    beats: [
      { ms: 12000, say: "In a recorded run on BOPTEST's office, it ran safely and did nothing: the sheet's humidity limits keep its override on. We found that in simulation, not in a building.",
        run: async () => {
          await scrollTop(400);
          await replay("day143-morning-your-sequence-rev-a", "4×");
          await scrollToText("Your sequence on the building", { frac: 0.1, ms: 1200 });
          await park();
        } },
    ],
  },
  {
    id: "revB", at: "1:07", title: "Revision B: red, then green",
    beats: [
      { ms: 5000, say: "The engineer revises the sheet; same gate.",
        run: async () => {
          await scrollTop(400);
          await click({ text: "Bring your own", sel: '[role="tab"], button' });
          await sleep(500);
          if (await ev(`!!__demo.find({ text: "Read another document" })`)) await click({ text: "Read another document" });
          else if (await ev(`!!__demo.find({ text: "Start over" })`)) await click({ text: "Start over" });
          await sleep(400);
          await click({ text: "Revision B of the sample" });
          await sleep(400);
          await click({ text: "Read with Muse Spark" });
          await waitText("Accept 6 scenarios", 20000);
          await sleep(600);
          await click({ text: "Accept 6 scenarios and create the sequence" }, { frac: 0.6, after: 900 });
        } },
      { ms: 7000, say: "Spark's first draft is red: it sends sixty-five Fahrenheit, eighteen Celsius, inside the start-up hold.",
        run: async () => {
          await scrollTop(400);
          await click({ text: "Muse Spark", sel: '[role="tab"]' });
          await click({ text: "Recorded session" }, { after: 200 });
          await click({ text: "Draft" });
          await waitText("Loads, but passes 4 of 6", 20000);
          await scrollToText("Attempt 1 of 2", { frac: 0.25, ms: 900 });
          await point({ text: "startupHoldThenColdReset", exact: false, sel: "div, span, p, code, li" }, { frac: 0.4 });
        } },
      { ms: 5000, say: "The failing scenarios go back, and the second draft passes six of six.",
        run: async () => {
          await waitText("Verified draft stored", 20000);
          await point({ text: "Passes 6 of 6 scenarios", exact: false, sel: "div, span, p, strong" }, { frac: 0.45 });
        } },
      { ms: 5000, say: "Explain the fix says what changed, in plain words.",
        run: async () => {
          await click({ text: "Explain the fix" });
          await waitFor(`!!document.querySelector('section[aria-label^="What changed in attempt"]')`, 20000, "fix explanation");
          await sleep(300);
          await ev(`(async () => { const s = document.querySelector('section[aria-label^="What changed in attempt"]'); await __demo.reveal(s, { frac: 0.18, force: true, ms: 1000 }); })()`);
          await park();
        } },
      { ms: 12000, say: "Then Spark tries to break its own verified draft with four new edge cases; the engine replays them, and all four pass.",
        run: async () => {
          await scrollTop(500);
          await click({ text: "Block graph", sel: '[role="tab"]' });
          await sleep(500);
          await click({ text: "Try to break it", sel: 'section[aria-label="Try to break it"] button' }, { frac: 0.3 });
          await waitFor(`(() => { const s = document.querySelector('section[aria-label="Try to break it"]'); return s && !s.innerText.includes("looking for edges") && /pass/i.test(s.innerText); })()`, 30000, "attack result");
          await sleep(300);
          await ev(`(async () => { const s = document.querySelector('section[aria-label="Try to break it"]'); await __demo.reveal(s, { frac: 0.12, force: true, ms: 1000 }); })()`);
          await park();
        } },
    ],
  },
  {
    id: "runB", at: "1:41", title: "Revision B on the building (replay)",
    beats: [
      { ms: 8000, say: "Revision B's recorded run on the same spring morning: thirty-seven percent less energy than BOPTEST's own controller, with less discomfort.",
        run: async () => {
          await scrollTop(400);
          await replay("day143-morning-your-sequence-rev-b", "4×");
          await scrollToText("Your sequence on the building", { frac: 0.1, ms: 1200 });
          await park();
        } },
      { ms: 7000, say: "On days it wasn't tuned on, a quarter to a half less in spring and fall, nothing in summer.",
        run: async () => {
          await waitText("240/240", 30000);
          await scrollToText("KPIs", { sel: "h2,h3,h4,div", frac: 0.08, ms: 1200 });
        } },
    ],
  },
  {
    id: "home", at: "1:56", title: "Second building: the home (replay)",
    beats: [
      { ms: 10000, say: "Same gate, second building, recorded: a heat pump house on spot electricity prices. The sequence preheats before the priciest hours and coasts through them.",
        run: async () => {
          await scrollTop(400);
          await replay("home-day108-sequence", "10×", { home: true });
          await park();
        } },
      { ms: 8000, say: "Over two simulated weeks, power in those hours drops twenty-six percent and the bill eight percent, with less discomfort.",
        run: async () => {
          const t = Date.now();
          await waitText("Power in peak-price hours", 90000);
          if (Date.now() - t > 500) console.log(`  (waited ${((Date.now() - t) / 1000).toFixed(1)} s for the peak-price card)`);
          await scrollToText("Power in peak-price hours", { frac: 0.12, ms: 1200 });
        } },
      { ms: 8000, say: "Utilities add thousands of homes together; the fleet line is an illustration, and we show the rebound too.",
        run: async () => {
          await scrollToText("Fleet view", { frac: 0.35, ms: 1000 });
        } },
    ],
  },
  {
    id: "faults", at: "2:22", title: "Faults (optional)", optional: true,
    beats: [
      { ms: 6000, say: "In recorded runs, a blank sensor is refused, and control goes back to BOPTEST.",
        run: async () => {
          await scrollTop(400);
          await replay("day186-midnight-fault-demo", "4×");
          await waitText("60/60", 30000);
          await sleep(300);
          await point({ text: "Fail-safe active for step", exact: false, sel: "p, div, span" }, { frac: 0.3 });
        } },
      { ms: 8000, say: "A sensor reading four kelvin, seven Fahrenheit, high is worse, because it's believed: two hours of it cost about twelve dollars and forty-four kilograms of carbon.",
        run: async () => {
          await scrollTop(400);
          await replay("day185-noon-fault-sensor-bias", "10×");
          await park();
          await waitText("300/300", 30000);
          await sleep(400);
          await scrollToText("Energy & emissions", { sel: "h2,h3,h4,div", frac: 0.1, ms: 1200 });
        } },
    ],
  },
  {
    id: "close", at: "2:36", title: "Benchmark, Muse Code, close",
    beats: [
      { ms: 7000, say: "On a hundred thirty-seven fault rules, Spark's drafts pass the library's test vectors on at least a hundred thirty after repair, never seeing the tests.",
        run: async () => { await park(); await card(resultsCard); } },
      { ms: 6000, say: "Muse Code helped build this, and coding agents get the same checks over MCP.",
        run: async () => { await card(museCodeCard()); } },
      { ms: 7000, say: "Muse Spark proposes, the engine checks, and only verified logic runs. Links below.",
        run: async () => { await card(closingCard); } },
    ],
  },
];

// ---------- run ----------
const health = await (await fetch(`${BASE}/api/health`)).json();
const specs = await (await fetch(`${BASE}/api/specs`)).json();
const specCount = Array.isArray(specs) ? specs.length : (specs.specs?.length ?? "?");
console.log(`${D}server: engine ${health.engine_rev?.slice(0, 7)}, recorded model ${health.recorded_model}, BOPTEST ${health.boptest}, sequences ${specCount}${R}`);
if (specCount !== 5) console.log(`${Y}Warning: the server already has your own sequences (fresh state shows 5). Restart it with a fresh MM_RUNS_DIR for a clean take.${R}`);

let scenes = SCENES.filter((s) => !(s.optional && flag("no-fault")));
if (FROM) scenes = scenes.slice(Math.max(0, scenes.findIndex((s) => s.id === FROM)));
if (SHOTS) mkdirSync(SHOTS, { recursive: true });

// A blank card first, so recording can start on a clean frame.
await card("");
await park();
if (!AUTO) {
  console.log(`\n${B}Ready.${R} Start the recording, then press ${B}Enter${R} for each line. ${D}(q + Enter quits)${R}`);
}

let t0 = null;
const log = [];
let shot = 0;
for (const scene of scenes) {
  for (const [i, beat] of scene.beats.entries()) {
    const head = `${C}${B}[${scene.at} ${scene.title}${scene.beats.length > 1 ? ` · ${i + 1}/${scene.beats.length}` : ""}]${R}`;
    if (!AUTO) {
      console.log(`\n${head}  ${D}target ${scene.at}${t0 ? `, now ${fmt(Date.now() - t0)}` : ""}${R}`);
      console.log(`  ${D}Enter, then say:${R}  ${B}${beat.say}${R}`);
      const l = await nextLine();
      if (l.trim() === "q") break;
    } else {
      console.log(`\n${head}\n  ${beat.say}`);
    }
    if (t0 === null) t0 = Date.now();
    const start = Date.now();
    try {
      await beat.run();
    } catch (e) {
      console.log(`  ${RED}step failed: ${e.message}${R}`);
    }
    const took = Date.now() - start;
    log.push({ scene: scene.id, beat: i + 1, took });
    if (!AUTO) console.log(`  ${G}✓ on screen${R} ${D}(${(took / 1000).toFixed(1)} s)${R}`);
    if (AUTO && took < beat.ms) await sleep(beat.ms - took);
    if (SHOTS) {
      const { data } = await c.send("Page.captureScreenshot", { format: "png" });
      writeFileSync(join(SHOTS, `${String(++shot).padStart(2, "0")}-${scene.id}-${i + 1}.png`), Buffer.from(data, "base64"));
    }
  }
}
const total = t0 ? Date.now() - t0 : 0;
console.log(`\n${B}Done${R} in ${fmt(total)} (the guide asks for about 3:00).`);
console.log(log.map((l) => `${l.scene}.${l.beat} ${(l.took / 1000).toFixed(1)}s`).join("  "));
if (!AUTO) {
  console.log(`${D}Stop the recording, then press Enter to close the window.${R}`);
  await nextLine();
}
finished = true;
c.close();
chrome.kill();
rl?.close();
process.exit(0);
