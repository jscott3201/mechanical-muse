/// <reference types="vitest/config" />
import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The browser talks only to mm-server. In development, /api is proxied to it; the proxy
// passes text/event-stream through unbuffered and uncompressed, so the run stream stays live.
const api = process.env.MM_API ?? "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(import.meta.dirname, "src") } },
  server: { proxy: { "/api": { target: api, changeOrigin: false } } },
  test: { environment: "jsdom", include: ["src/**/*.test.{ts,tsx}"] },
});
