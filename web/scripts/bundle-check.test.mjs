import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { checkRules, initialClosure, runBundleCheck, staleSentinels } from "./bundle-check.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const webDir = resolve(scriptDir, "..");
const rules = JSON.parse(readFileSync(resolve(scriptDir, "bundle-rules.json"), "utf8"));
// A sentinel both entries forbid (CodeMirror's `cm-editor`) belongs only in a lazy chunk.
const indexText = rules["index.html"].forbidText;
const planText = rules["plan.html"].forbidText;
const planOnly = indexText.filter((text) => !planText.includes(text));
const lazyOnly = planText;
const XTERM = "node_modules/@xterm/xterm/lib/xterm.mjs";

// Shaped like web/dist/.vite/manifest.json: only entry and dynamic-entry chunks
// carry `src`; the static chunk both entries import has `file` and nothing to
// glob against, and the terminal library is reachable only through dynamicImports.
const manifest = {
  "_shared.js": { file: "assets/shared.js", name: "shared", dynamicImports: [XTERM] },
  "index.html": {
    file: "assets/index.js",
    name: "index",
    src: "index.html",
    isEntry: true,
    imports: ["_shared.js"],
  },
  "plan.html": {
    file: "assets/plan.js",
    name: "plan",
    src: "plan.html",
    isEntry: true,
    imports: ["_shared.js"],
  },
  [XTERM]: { file: "assets/xterm.js", name: "xterm", src: XTERM, isDynamicEntry: true },
};
// A healthy bundle: plan code only in plan.js, terminal code only in the dynamic
// chunk, and the shared chunk holding nothing but a loader reference to it.
const clean = {
  "assets/shared.js": "react;import(`./xterm.js`)",
  "assets/index.js": "dashboard",
  "assets/plan.js": planOnly.join(" "),
  "assets/xterm.js": lazyOnly.join(" "),
};
const reader = (files) => (file) => {
  if (!(file in files)) throw new Error(`unexpected read of ${file}`);
  return files[file];
};
const sentinels = Object.entries(rules).flatMap(([entry, entryRules]) =>
  entryRules.forbidText.map((text) => [entry, text]),
);
// The shared chunk sits in both closures, so a sentinel both entries forbid is
// reported twice there; that table takes the sentinels unique to one entry.
const ownSentinels = sentinels.filter(
  ([, text]) => sentinels.filter(([, other]) => other === text).length === 1,
);

describe("initial closure", () => {
  it("follows static imports, survives cycles, and skips dynamic imports", () => {
    const files = (key) => initialClosure(manifest, key).map((chunk) => chunk.file);
    expect(files("index.html")).toEqual(["assets/index.js", "assets/shared.js"]);
    expect(files("plan.html")).toEqual(["assets/plan.js", "assets/shared.js"]);
    const shared = { ...manifest["_shared.js"], imports: ["index.html"] };
    expect(initialClosure({ ...manifest, "_shared.js": shared }, "index.html")).toHaveLength(2);
    const plan = { file: "assets/plan.js", dynamicImports: ["_shared.js"] };
    expect(initialClosure({ ...manifest, "plan.html": plan }, "plan.html")).toHaveLength(1);
  });
});

describe("rule checks over a manifest shaped like Vite's", () => {
  it("passes a clean bundle and counts every closure chunk", () => {
    expect(runBundleCheck(manifest, rules, reader(clean))).toEqual({
      violations: [],
      entries: 2,
      chunks: 4,
    });
  });

  it.each(ownSentinels)(
    "%s fails when %s sits in the shared chunk of its closure",
    (entry, text) => {
      const files = { ...clean, "assets/shared.js": `react;${text}` };
      expect(runBundleCheck(manifest, rules, reader(files)).violations).toEqual([
        `${entry}: assets/shared.js matches ${text}`,
      ]);
    },
  );

  it.each(sentinels)("%s fails when %s is merged into its entry chunk", (entry, text) => {
    const file = manifest[entry].file;
    const files = { ...clean, [file]: `${clean[file]};${text}` };
    expect(runBundleCheck(manifest, rules, reader(files)).violations).toEqual([
      `${entry}: ${file} matches ${text}`,
    ]);
  });

  it("flags a forbidden chunk glob on a statically imported dynamic entry", () => {
    const plan = { ...manifest["plan.html"], imports: ["_shared.js", XTERM] };
    const chunks = initialClosure({ ...manifest, "plan.html": plan }, "plan.html");
    expect(checkRules("plan.html", chunks, rules["plan.html"], () => "")).toEqual([
      "plan.html: assets/xterm.js matches node_modules/@xterm/**",
    ]);
  });

  // The editor chunks come from manualChunks groups: no manifest `src`, so only their `file`
  // can name them.
  it.each([
    ["index.html", "assets/cm-view.js"],
    ["index.html", "assets/cm-lang.js"],
    ["plan.html", "assets/cm-view.js"],
    ["plan.html", "assets/cm-lang.js"],
  ])("%s fails when its initial closure holds the editor chunk %s", (entry, file) => {
    const imports = [...manifest[entry].imports, "_editor.js"];
    const withEditor = {
      ...manifest,
      "_editor.js": { file, name: "editor" },
      [entry]: { ...manifest[entry], imports },
    };
    const files = { ...clean, [file]: "editor" };
    expect(runBundleCheck(withEditor, rules, reader(files)).violations).toEqual([
      `${entry}: ${file} matches ${file}`,
    ]);
  });

  it("reads only JavaScript chunks", () => {
    const chunks = [{ file: "assets/index.js" }, { file: "assets/style.css" }];
    const read = reader({ "assets/index.js": "dashboard" });
    expect(checkRules("index.html", chunks, rules["index.html"], read)).toEqual([]);
  });
});

describe("the check cannot pass vacuously", () => {
  it("fails when an entry is missing from the manifest", () => {
    const { "plan.html": _plan, ...withoutPlan } = manifest;
    expect(() => runBundleCheck(withoutPlan, rules, reader(clean))).toThrow(
      /missing chunk: plan.html/,
    );
  });

  it("fails when the rules list no entries or an entry has no sentinel", () => {
    expect(() => runBundleCheck(manifest, {}, reader(clean))).toThrow(/no entries/);
    const noSentinel = { "index.html": { forbidChunks: ["src/plan/**"], forbidText: [] } };
    expect(() => runBundleCheck(manifest, noSentinel, reader(clean))).toThrow(
      /forbidText sentinel/,
    );
  });

  it("fails when an entry's closure holds no JavaScript chunk", () => {
    const styleOnly = { "index.html": { file: "assets/index.css" } };
    expect(() => runBundleCheck(styleOnly, rules, reader(clean))).toThrow(/no JavaScript chunk/);
  });

  it("fails when a sentinel occurs in no chunk", () => {
    const files = { ...clean, "assets/plan.js": planOnly.slice(1).join(" ") };
    expect(staleSentinels(manifest, rules, reader(files))).toEqual([
      `index.html: sentinel ${planOnly[0]} occurs in no chunk`,
    ]);
    expect(runBundleCheck(manifest, rules, reader(files)).violations).toHaveLength(1);
  });
});

describe("bundle-check CLI", () => {
  function writeDist(files) {
    const dist = mkdtempSync(resolve(tmpdir(), "bundle-check-"));
    mkdirSync(resolve(dist, ".vite"));
    mkdirSync(resolve(dist, "assets"));
    writeFileSync(resolve(dist, ".vite/manifest.json"), JSON.stringify(manifest));
    for (const [file, text] of Object.entries(files)) writeFileSync(resolve(dist, file), text);
    return dist;
  }
  const run = (dist, cwd) =>
    spawnSync(process.execPath, [resolve(scriptDir, "bundle-check.mjs")], {
      cwd,
      env: { ...process.env, LOOM_BUNDLE_CHECK_DIST: dist },
      encoding: "utf8",
    });
  const outcome = ({ status, stdout, stderr }) => ({ status, stdout, stderr });

  it("runs identically from the web directory and repository root", () => {
    const dist = writeDist(clean);
    try {
      const web = run(dist, webDir);
      expect(web.status).toBe(0);
      expect(web.stdout).toBe("bundle-check: 2 entries, 4 chunks, ok\n");
      expect(outcome(run(dist, resolve(webDir, "..")))).toEqual(outcome(web));
    } finally {
      rmSync(dist, { recursive: true, force: true });
    }
  });

  it("exits 1 and names the chunk when plan code leaks into the shared chunk", () => {
    const dist = writeDist({ ...clean, "assets/shared.js": `react;${planOnly[1]}` });
    try {
      const result = run(dist, webDir);
      expect(result.status).toBe(1);
      expect(result.stdout).toBe("");
      expect(result.stderr).toBe(`index.html: assets/shared.js matches ${planOnly[1]}\n`);
    } finally {
      rmSync(dist, { recursive: true, force: true });
    }
  });

  it("exits 1 when an entry is missing from the manifest", () => {
    const dist = writeDist(clean);
    writeFileSync(resolve(dist, ".vite/manifest.json"), JSON.stringify({}));
    try {
      const result = run(dist, webDir);
      expect(result.status).toBe(1);
      expect(result.stderr).toContain("manifest is missing chunk: index.html");
    } finally {
      rmSync(dist, { recursive: true, force: true });
    }
  });
});
