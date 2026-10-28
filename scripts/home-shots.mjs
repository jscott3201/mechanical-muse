// Captures the home run view for the README and the docs site, in both themes, over the DevTools
// protocol (same approach as readme-shots.mjs).
// Usage: start `make demo` and a headless Chrome with --remote-debugging-port=<port>, then
//   node scripts/home-shots.mjs http://127.0.0.1:8080 <out-dir> <port>
// It replays fixtures/replays/home-day108-sequence (the recorded home baseline of the same
// window sits beside it) and writes home-run-{light,dark}.png (the top of the run view) and
// home-charts-{light,dark}.png (the charts). Resize to 1600 px wide before committing.
import { writeFileSync } from "node:fs";

const [base, out, port] = process.argv.slice(2);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const run = await (
  await fetch(`${base}/api/runs/replay`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ record: "home-day108-sequence", speed: 10 }),
  })
).json();
for (let i = 0; i < 240; i++) {
  const info = await (await fetch(`${base}/api/runs/${run.id}`)).json();
  if (info.state !== "starting" && info.state !== "running") break;
  await sleep(500);
}

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
const waitFor = async (expr, ms = 30000) => {
  for (let t = 0; t < ms; t += 250) {
    if (await evaluate(expr)) return;
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${expr}`);
};
const hasText = (text) => `document.body.innerText.includes(${JSON.stringify(text)})`;

await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 1000, deviceScaleFactor: 1.5, mobile: false });

for (const theme of ["light", "dark"]) {
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
  await send("Page.navigate", { url: `${base}/#/` });
  await sleep(800);
  await evaluate("(() => { try { localStorage.clear(); } catch {} location.reload(); return true; })()");
  await sleep(1200);
  await evaluate(`location.hash = ${JSON.stringify(`#/runs/${run.id}`)}`);
  // Show the home start form beside the home run.
  await evaluate(`(() => { const b = [...document.querySelectorAll("button")].find(b => b.textContent.trim() === "Home"); b && b.click(); return !!b; })()`);
  await waitFor(hasText("Power in peak-price hours"));
  await waitFor(hasText("Fleet view"));
  await sleep(1500);
  // The run view sits beside the forms: bring its top under the header.
  await evaluate(`(() => { const h = [...document.querySelectorAll("section")].find(s => s.innerText.includes("Power in peak-price hours")); h && h.scrollIntoView(); window.scrollBy(0, -80); return true; })()`);
  await sleep(600);
  let { data } = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${out}/home-run-${theme}.png`, Buffer.from(data, "base64"));
  await evaluate(`(() => { const h = [...document.querySelectorAll("h3, div")].find(d => d.textContent.trim().startsWith("Room temperature and comfort band")); h && h.scrollIntoView(); window.scrollBy(0, -100); return true; })()`);
  await sleep(800);
  ({ data } = await send("Page.captureScreenshot", { format: "png" }));
  writeFileSync(`${out}/home-charts-${theme}.png`, Buffer.from(data, "base64"));
  console.log("wrote home shots", theme);
}
ws.close();
