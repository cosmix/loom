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
        // Vendor groups keep every chunk under the 500 kB size warning. Only the lazy editor chunk
        // imports `cm-view` and `cm-lang`, so no entry's initial closure reaches them
        // (scripts/bundle-rules.json, checked by scripts/bundle-check.mjs). A group loads whole
        // for any importer: gather only packages that every importer of the group needs.
        codeSplitting: {
          groups: [
            { name: "react", test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/ },
            { name: "zod", test: /node_modules[\\/]zod[\\/]/ },
            { name: "router", test: /node_modules[\\/]react-router[\\/]/ },
            {
              name: "flow",
              test: /node_modules[\\/](@xyflow[\\/][^\\/]+|@dagrejs[\\/][^\\/]+|d3-[^\\/]+)[\\/]/,
            },
            {
              name: "markdown",
              test: /node_modules[\\/](micromark[^\\/]*|mdast-[^\\/]+|unist-[^\\/]+)[\\/]/,
            },
            {
              name: "cm-view",
              test: /node_modules[\\/](@codemirror[\\/](view|state)|style-mod|w3c-keyname|crelt|@marijn[\\/]find-cluster-break)[\\/]/,
            },
            { name: "cm-lang", test: /node_modules[\\/](@codemirror|@lezer)[\\/]/ },
          ],
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    globals: false,
    // Without this, vitest serves every CSS import as "", `?raw` included.
    css: { include: [/\.css\?raw$/] },
  },
}));
