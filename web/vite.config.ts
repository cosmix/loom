/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig(({ mode }) => ({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": import.meta.dirname + "/src" } },
  server: {
    ...(mode === "plan" ? { open: "/plan.html" } : {}),
    proxy: {
      "/api": `http://127.0.0.1:${mode === "plan" ? 7272 : 7373}`,
      "/ws": { target: `ws://127.0.0.1:${mode === "plan" ? 7272 : 7373}`, ws: true },
    },
  },
  build: {
    manifest: true,
    sourcemap: false,
    modulePreload: { polyfill: false },
    rolldownOptions: {
      input: {
        index: import.meta.dirname + "/index.html",
        plan: import.meta.dirname + "/plan.html",
      },
      output: {
        entryFileNames: "assets/[name].js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
        // React is shared by both entries; its own chunk keeps the shared chunk under the
        // 500 kB size warning.
        codeSplitting: {
          groups: [{ name: "react", test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/ }],
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    globals: false,
  },
}));
