// Drives headless Chrome over the DevTools protocol to capture README screenshots.
// Usage: start `make demo` and a headless Chrome with --remote-debugging-port=<port>, then
//   node scripts/readme-shots.mjs http://127.0.0.1:8080 <out-dir> <port>
// It writes workbench, authoring (and overview, graph) shots in both themes. The run view
// (run-*.png) is a finished replay of day143-morning-your-sequence-rev-b, captured from its
// #/runs/<id> link. Images are resized to 1600 px wide before they go in docs/img/screens.
import { writeFileSync } from "node:fs";

const [base, out, port] = process.argv.slice(2);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: "PUT" })).json();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0;
const pending = new Map();
ws.addEventListener("message", (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
  }
});
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const n = ++id;
    pending.set(n, (m) => (m.error ? reject(new Error(`${method}: ${m.error.message}`)) : resolve(m.result)));
    ws.send(JSON.stringify({ id: n, method, params }));
  });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result.value;
const waitFor = async (expr, ms = 20000) => {
  for (let t = 0; t < ms; t += 250) {
    if (await evaluate(expr)) return;
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${expr}`);
};
const clickText = (text, sel = "button") =>
  evaluate(`(() => { const el = [...document.querySelectorAll(${JSON.stringify(sel)})].find(b => b.textContent.trim().includes(${JSON.stringify(text)})); if (!el) return false; el.click(); return true; })()`);
const hasText = (text) => `document.body.innerText.includes(${JSON.stringify(text)})`;

await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 1000, deviceScaleFactor: 1.5, mobile: false });

async function shot(name, theme, steps) {
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
  await send("Page.navigate", { url: `${base}/#/` });
  await sleep(800);
  // The app remembers a chosen theme; clear it so the media query decides.
  await evaluate("(() => { try { localStorage.clear(); } catch {} location.reload(); return true; })()");
  await sleep(1200);
  const keepScroll = await steps();
  if (!keepScroll) await evaluate("window.scrollTo(0, 0)");
  await sleep(600);
  const { data } = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${out}/${name}-${theme}.png`, Buffer.from(data, "base64"));
  console.log("wrote", name, theme);
}

const go = async (hash) => {
  await evaluate(`location.hash = ${JSON.stringify(hash)}`);
  await sleep(1200);
};

for (const theme of ["light", "dark"]) {
  await shot("overview", theme, async () => {
    await go("#/");
    await waitFor(hasText("Recorded results on BOPTEST"));
  });
  await shot("workbench", theme, async () => {
    await go("#/workbench");
    await clickText("Revision B of the sample");
    await sleep(800);
    await clickText("Read with Muse Spark");
    await waitFor(hasText("The document leaves"));
    await sleep(800);
  });
  await shot("authoring", theme, async () => {
    await go("#/sequences/chw_plant_reset/spark");
    await clickText("Recorded run");
    await sleep(300);
    await evaluate(`(() => { const b = [...document.querySelectorAll("button")].find(b => b.textContent.trim() === "Draft"); b && b.click(); return !!b; })()`);
    await waitFor(hasText("Verified draft stored"), 60000);
    await sleep(800);
    // Bring the attempts into view under the header.
    await evaluate(`(() => { const h = [...document.querySelectorAll("div")].find(d => d.textContent.trim().startsWith("Draft with Muse Spark")); h && h.scrollIntoView(); window.scrollBy(0, -90); return true; })()`);
    return true;
  });
  await shot("graph", theme, async () => {
    await go("#/sequences/chw_plant_reset/graph");
    await waitFor("document.querySelector('.react-flow__node') !== null");
    await sleep(1500);
  });
}
ws.close();
