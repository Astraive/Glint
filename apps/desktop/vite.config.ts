// Builds the manager UI into dist/, which src-tauri embeds in the binary.
// No dev server is wired to Tauri: the window only ever loads embedded
// assets (no remote content, ADR 0011).

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // The CSP allows only 'self': no inlined data: assets, no inline
    // module-preload polyfill.
    assetsInlineLimit: 0,
    modulePreload: { polyfill: false },
    target: "es2024",
  },
  test: {
    include: ["test/**/*.test.{ts,tsx}"],
    environment: "node",
  },
});
