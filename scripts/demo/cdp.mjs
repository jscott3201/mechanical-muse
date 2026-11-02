// Minimal Chrome DevTools Protocol client for the demo driver (same approach as
// scripts/readme-shots.mjs; no dependencies: Node's built-in fetch and WebSocket).
export async function connect(port, url = "about:blank", existing = null) {
  const target = existing ?? (await (await fetch(`http://127.0.0.1:${port}/json/new?${url ?? "about:blank"}`, { method: "PUT" })).json());
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener("open", r, { once: true }));
  let id = 0;
  const pending = new Map();
  const listeners = new Map();
  ws.addEventListener("message", (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) {
      pending.get(m.id)(m);
      pending.delete(m.id);
    } else if (m.method && listeners.has(m.method)) {
      for (const f of listeners.get(m.method)) f(m.params);
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const n = ++id;
      pending.set(n, (m) => (m.error ? reject(new Error(`${method}: ${m.error.message}`)) : resolve(m.result)));
      ws.send(JSON.stringify({ id: n, method, params }));
    });
  const on = (method, f) => {
    if (!listeners.has(method)) listeners.set(method, []);
    listeners.get(method).push(f);
  };
  const evaluate = async (expr) => {
    const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(`evaluate: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}`);
    return r.result.value;
  };
  return { ws, send, on, evaluate, targetId: target.id, close: () => ws.close() };
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function waitFor(c, expr, ms = 30000) {
  for (let t = 0; t < ms; t += 200) {
    if (await c.evaluate(expr)) return;
    await sleep(200);
  }
  throw new Error(`timed out waiting for ${expr}`);
}

export const hasText = (text) => `document.body.innerText.includes(${JSON.stringify(text)})`;
