import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { checkRules, initialClosure } from "./bundle-check.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const webDir = resolve(scriptDir, "..");
const manifest = {
  "index.html": { file: "assets/index.js", imports: ["shared"], dynamicImports: ["lazy"] },
  shared: { file: "assets/shared.js", src: "src/plan/shared.ts", imports: ["index.html"] },
  lazy: { file: "assets/lazy.js", src: "src/plan/lazy.ts" },
};
const rules = { forbidChunks: ["src/plan/**"], forbidText: ["data-loom-plan"] };

describe("bundle check", () => {
  it("checks static imports, handles cycles, and excludes dynamic imports", () => {
    const chunks = initialClosure(manifest, "index.html");
    expect(chunks.map((chunk) => chunk.file)).toEqual(["assets/index.js", "assets/shared.js"]);
    expect(checkRules("index.html", chunks, rules, () => "")).toEqual([
      "index.html: assets/shared.js matches src/plan/**",
    ]);
    const dynamicOnly = {
      ...manifest,
      "index.html": { file: "assets/index.js", dynamicImports: ["shared"] },
    };
    expect(
      checkRules("index.html", initialClosure(dynamicOnly, "index.html"), rules, () => ""),
    ).toEqual([]);
  });

  it("checks file names and JavaScript text", () => {
    const chunks = [{ file: "src/plan/page.js" }, { file: "assets/style.css" }];
    expect(checkRules("index.html", chunks, rules, () => "data-loom-plan")).toEqual([
      "index.html: src/plan/page.js matches src/plan/**",
      "index.html: src/plan/page.js matches data-loom-plan",
    ]);
  });

  it("runs identically from the web directory and repository root", () => {
    const dist = mkdtempSync(resolve(tmpdir(), "bundle-check-"));
    try {
      mkdirSync(resolve(dist, ".vite"));
      mkdirSync(resolve(dist, "assets"));
      writeFileSync(
        resolve(dist, ".vite/manifest.json"),
        JSON.stringify({
          "index.html": { file: "assets/index.js" },
          "plan.html": { file: "assets/plan.js" },
        }),
      );
      writeFileSync(resolve(dist, "assets/index.js"), "index");
      writeFileSync(resolve(dist, "assets/plan.js"), "plan");
      const run = (cwd) =>
        spawnSync(process.execPath, [resolve(scriptDir, "bundle-check.mjs")], {
          cwd,
          env: { ...process.env, LOOM_BUNDLE_CHECK_DIST: dist },
          encoding: "utf8",
        });
      const web = run(webDir);
      const root = run(resolve(webDir, ".."));
      expect(web.status).toBe(0);
      expect(web.stdout).toBe("bundle-check: 2 entries, 2 chunks, ok\n");
      expect({ status: root.status, stdout: root.stdout, stderr: root.stderr }).toEqual({
        status: web.status,
        stdout: web.stdout,
        stderr: web.stderr,
      });
    } finally {
      rmSync(dist, { recursive: true, force: true });
    }
  });
});
