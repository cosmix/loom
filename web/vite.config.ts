/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": import.meta.dirname + "/src" } },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:7373",
      "/ws": { target: "ws://127.0.0.1:7373", ws: true },
    },
  },
  build: {
    sourcemap: false,
    modulePreload: { polyfill: false },
    rolldownOptions: {
      output: {
        entryFileNames: "assets/[name].js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    globals: false,
  },
});
