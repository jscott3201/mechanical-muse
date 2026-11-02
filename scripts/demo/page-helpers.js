// Injected into the app page: a visible cursor, click ripples, full-window cards, eased scrolling
// and element lookup. Everything sits above the app and ignores pointer events.
(() => {
  if (window.__demo) return;
  const css = document.createElement("style");
  css.textContent = `
    #demo-cursor { position: fixed; left: 0; top: 0; width: 26px; height: 26px; z-index: 2147483647;
      pointer-events: none; transform: translate(-60px, -60px); transition: transform 700ms cubic-bezier(.45,0,.2,1); }
    #demo-cursor svg { filter: drop-shadow(0 1px 2px rgba(0,0,0,.55)); }
    .demo-ripple { position: fixed; z-index: 2147483646; pointer-events: none; width: 40px; height: 40px;
      margin: -20px 0 0 -20px; border-radius: 50%; border: 3px solid rgba(110,110,240,.95);
      animation: demo-ripple 650ms ease-out forwards; }
    @keyframes demo-ripple { from { transform: scale(.3); opacity: 1 } to { transform: scale(1.7); opacity: 0 } }
    #demo-card { position: fixed; inset: 0; z-index: 2147483645; display: flex; align-items: center;
      justify-content: center; opacity: 0; transition: opacity 500ms ease; pointer-events: none;
      font-family: ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; }
    #demo-card.show { opacity: 1; }
    #demo-card .muted { opacity: .72; }
    #demo-card code, #demo-card .mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
    .demo-focus { outline: 3px solid rgba(110,110,240,.95) !important; outline-offset: 4px !important;
      border-radius: 8px; transition: outline-color 300ms; }
  `;
  document.head.appendChild(css);

  const cursor = document.createElement("div");
  cursor.id = "demo-cursor";
  cursor.innerHTML =
    '<svg width="26" height="26" viewBox="0 0 24 24"><path d="M4 2 L4 19.5 L8.6 15.2 L11.7 22 L14.4 20.8 L11.3 14.1 L17.8 13.9 Z" fill="white" stroke="black" stroke-width="1.3" stroke-linejoin="round"/></svg>';
  document.body.appendChild(cursor);

  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const ease = (t) => (t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2);
  const visible = (el) => !!el && el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
  const norm = (s) => (s || "").replace(/\s+/g, " ").trim();

  const demo = {
    pos: { x: -60, y: -60 },

    moveCursor(x, y, ms = 700) {
      cursor.style.transition = `transform ${ms}ms cubic-bezier(.45,0,.2,1)`;
      cursor.style.transform = `translate(${x - 4}px, ${y - 2}px)`;
      demo.pos = { x, y };
      return sleep(ms + 40);
    },

    ripple(x, y) {
      const r = document.createElement("div");
      r.className = "demo-ripple";
      r.style.left = `${x}px`;
      r.style.top = `${y}px`;
      document.body.appendChild(r);
      setTimeout(() => r.remove(), 800);
    },

    /** Eased scroll of the document to `top` (px) over `ms`. */
    scrollTo(top, ms = 900) {
      const el = document.scrollingElement;
      const from = el.scrollTop;
      const to = Math.max(0, Math.min(top, el.scrollHeight - innerHeight));
      if (Math.abs(to - from) < 2) return Promise.resolve();
      return new Promise((resolve) => {
        const t0 = performance.now();
        const step = (now) => {
          const t = Math.min(1, (now - t0) / ms);
          el.scrollTop = from + (to - from) * ease(t);
          t < 1 ? requestAnimationFrame(step) : resolve();
        };
        requestAnimationFrame(step);
      });
    },

    scrollBy(dy, ms = 900) {
      return demo.scrollTo(document.scrollingElement.scrollTop + dy, ms);
    },

    /** Brings `el` into view with its top at `frac` of the viewport, unless already comfortably visible. */
    async reveal(el, { frac = 0.35, ms = 800, force = false } = {}) {
      const r = el.getBoundingClientRect();
      const header = 80;
      if (!force && r.top >= header && r.bottom <= innerHeight - 40) return;
      await demo.scrollTo(document.scrollingElement.scrollTop + r.top - innerHeight * frac, ms);
    },

    /**
     * Finds an element. desc: { text, exact (default true), sel (default clickable things),
     * near (text of a container the element must sit in), label (aria-label prefix of a container),
     * nth (default 0 among matches, -1 for last) }.
     */
    find(desc) {
      const sel = desc.sel || 'button, a, [role="tab"], [role="radio"], [role="switch"], label';
      let scope = document;
      if (desc.label) {
        scope = [...document.querySelectorAll("[aria-label]")].find((e) => (e.getAttribute("aria-label") || "").startsWith(desc.label) && visible(e));
        if (!scope) return null;
      }
      let els = [...scope.querySelectorAll(sel)].filter(visible);
      if (desc.text !== undefined) {
        const exact = desc.exact !== false;
        els = els.filter((e) => (exact ? norm(e.textContent) === desc.text : norm(e.textContent).includes(desc.text)));
        // A partial match also matches every ancestor; keep the innermost elements only.
        if (!exact) els = els.filter((e) => !els.some((o) => o !== e && e.contains(o)));
      }
      if (desc.near) {
        els = els.filter((e) => {
          // The smallest ancestor that holds the near text and this element, at most a few levels up.
          let a = e.parentElement;
          for (let i = 0; i < 5 && a; i++, a = a.parentElement) {
            if (norm(a.textContent).includes(desc.near)) return true;
          }
          return false;
        });
      }
      if (els.length === 0) return null;
      const nth = desc.nth ?? 0;
      return nth < 0 ? els[els.length + nth] : els[nth] ?? null;
    },

    /** Finds, reveals, and returns the element's viewport rect (or null). */
    async locate(desc, opts) {
      const el = demo.find(desc);
      if (!el) return null;
      await demo.reveal(el, opts);
      await sleep(120);
      demo.last = el;
      const r = el.getBoundingClientRect();
      return { x: r.left, y: r.top, w: r.width, h: r.height };
    },

    /** Scrolls so the first element matching `sel` whose text includes `text` sits at `frac` of the viewport. */
    async scrollToText(text, { sel = "h1,h2,h3,h4,p,div,span,section,figcaption,dt,dd,td,th,li,strong", frac = 0.2, ms = 1000, nth = 0 } = {}) {
      const els = [...document.querySelectorAll(sel)].filter((e) => visible(e) && norm(e.textContent).includes(text));
      // Prefer the innermost match.
      const inner = els.filter((e) => ![...e.children].some((c) => norm(c.textContent).includes(text)));
      const el = (inner.length ? inner : els)[nth];
      if (!el) return false;
      await demo.scrollTo(document.scrollingElement.scrollTop + el.getBoundingClientRect().top - innerHeight * frac, ms);
      return true;
    },

    focus(desc, ms = 2200) {
      const el = desc.text ? demo.find({ ...desc, sel: desc.sel || "*" }) : null;
      if (!el) return false;
      el.classList.add("demo-focus");
      setTimeout(() => el.classList.remove("demo-focus"), ms);
      return true;
    },

    /** A full-window card over the app. */
    async card(html, { ms = 500 } = {}) {
      let c = document.getElementById("demo-card");
      const bg = getComputedStyle(document.body).backgroundColor;
      const fg = getComputedStyle(document.body).color;
      if (c && c.classList.contains("show")) {
        c.style.transition = "opacity 250ms ease";
        c.classList.remove("show");
        await sleep(260);
        c.innerHTML = html;
        c.style.transition = `opacity ${ms}ms ease`;
        c.classList.add("show");
        await sleep(ms);
        return;
      }
      if (!c) {
        c = document.createElement("div");
        c.id = "demo-card";
        document.body.appendChild(c);
      }
      c.style.background = bg;
      c.style.color = fg;
      c.innerHTML = html;
      c.style.transition = `opacity ${ms}ms ease`;
      void c.offsetWidth;
      c.classList.add("show");
      await sleep(ms);
    },

    async uncard(ms = 500) {
      const c = document.getElementById("demo-card");
      if (!c) return;
      c.style.transition = `opacity ${ms}ms ease`;
      c.classList.remove("show");
      await sleep(ms);
      c.innerHTML = "";
    },

    text: () => document.body.innerText,
  };
  window.__demo = demo;
})();
