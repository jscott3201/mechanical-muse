// Captures the authoring panel with Muse Spark's "Explain the fix" open on each repair of the
// recorded chilled-water plant reset session, in both themes (same approach as readme-shots.mjs).
// Usage: start `make demo` and a headless Chrome with --remote-debugging-port=<port>, then
//   node scripts/fix-shots.mjs http://127.0.0.1:8080 <out-dir> <port>
// Writes authoring-fix-{light,dark}.png. Resize to 1600 px wide before committing.
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
const waitFor = async (expr, ms = 60000) => {
  for (let t = 0; t < ms; t += 250) {
    if (await evaluate(expr)) return;
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${expr}`);
};
const clickAll = (text) =>
  evaluate(`(() => { const bs = [...document.querySelectorAll("button")].filter(b => b.textContent.trim() === ${JSON.stringify(text)}); bs.forEach(b => b.click()); return bs.length; })()`);
const hasText = (text) => `document.body.innerText.includes(${JSON.stringify(text)})`;

await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 1100, deviceScaleFactor: 1.5, mobile: false });
for (const theme of ["light", "dark"]) {
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
  await send("Page.navigate", { url: `${base}/#/` });
  await sleep(800);
  await evaluate("(() => { try { localStorage.clear(); } catch {} location.reload(); return true; })()");
  await sleep(1200);
  await evaluate(`location.hash = "#/sequences/chw_plant_reset/spark"`);
  await sleep(1200);
  await evaluate(`(() => { const b = [...document.querySelectorAll("button")].find(b => b.textContent.trim().includes("Recorded run")); b && b.click(); return !!b; })()`);
  await sleep(300);
  await evaluate(`(() => { const b = [...document.querySelectorAll("button")].find(b => b.textContent.trim() === "Draft"); b && b.click(); return !!b; })()`);
  await waitFor(hasText("Verified draft stored"));
  const n = await clickAll("Explain the fix");
  if (n < 2) throw new Error(`expected two repairs to explain, found ${n}`);
  await waitFor(`document.querySelectorAll('section[aria-label^="What changed in attempt"]').length === 2`);
  await sleep(800);
  await evaluate(`(() => { const s = document.querySelector('section[aria-label="What changed in attempt 2"]'); s && s.scrollIntoView(); window.scrollBy(0, -260); return true; })()`);
  await sleep(600);
  const { data } = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${out}/authoring-fix-${theme}.png`, Buffer.from(data, "base64"));
  console.log("wrote authoring-fix", theme);
}
ws.close();
