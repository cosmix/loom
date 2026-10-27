import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));

export function initialClosure(manifest, entry) {
  const seen = new Set();
  const visit = (key) => {
    if (seen.has(key)) return;
    const chunk = manifest[key];
    if (!chunk) throw new Error(`manifest is missing chunk: ${key}`);
    seen.add(key);
    for (const imported of chunk.imports ?? []) visit(imported);
  };
  visit(entry);
  return [...seen].map((key) => manifest[key]);
}

function globRegex(glob) {
  let pattern = "^";
  for (let i = 0; i < glob.length; i++) {
    const char = glob[i];
    if (char === "*" && glob[i + 1] === "*") {
      if (glob[i + 2] === "/") {
        pattern += "(?:.*/)?";
        i += 2;
      } else {
        pattern += ".*";
        i++;
      }
    } else if (char === "*") pattern += "[^/]*";
    else if (char === "?") pattern += "[^/]";
    else pattern += char.replace(/[\\^$.*+?()[\]{}|]/g, "\\$&");
  }
  return new RegExp(`${pattern}$`);
}

const SCRIPT = /\.m?js$/;

// Vite writes `src` only for entry and dynamic-entry chunks. A static chunk
// shared by two entries (for example `_help-shortcuts.js`) has just `file`, so
// `forbidChunks` cannot see code that was merged into one. The `forbidText`
// sentinels are the guard for that code: strings that survive minification and
// occur only in the code an entry must not carry.
export function checkRules(entry, chunks, rules, readChunk) {
  if ((rules.forbidText ?? []).length === 0) {
    throw new Error(`${entry}: needs a forbidText sentinel (forbidChunks misses shared chunks)`);
  }
  const scripts = chunks.filter((chunk) => SCRIPT.test(chunk.file));
  if (scripts.length === 0) throw new Error(`${entry}: no JavaScript chunk in its closure`);
  const violations = [];
  for (const chunk of chunks) {
    for (const rule of rules.forbidChunks ?? []) {
      const pattern = globRegex(rule);
      if (pattern.test(chunk.file) || (chunk.src && pattern.test(chunk.src))) {
        violations.push(`${entry}: ${chunk.file} matches ${rule}`);
      }
    }
  }
  for (const chunk of scripts) {
    const text = readChunk(chunk.file);
    for (const rule of rules.forbidText) {
      if (text.includes(rule)) violations.push(`${entry}: ${chunk.file} matches ${rule}`);
    }
  }
  return violations;
}

// A sentinel found in no chunk guards nothing: the string was renamed or the
// code was removed, and the check would pass for any bundle.
export function staleSentinels(manifest, rules, readChunk) {
  const texts = Object.values(manifest)
    .filter((chunk) => SCRIPT.test(chunk.file))
    .map((chunk) => readChunk(chunk.file));
  const stale = [];
  for (const [entry, entryRules] of Object.entries(rules)) {
    for (const rule of entryRules.forbidText ?? []) {
      if (!texts.some((text) => text.includes(rule))) {
        stale.push(`${entry}: sentinel ${rule} occurs in no chunk`);
      }
    }
  }
  return stale;
}

export function runBundleCheck(manifest, rules, readChunk) {
  const entries = Object.entries(rules);
  if (entries.length === 0) throw new Error("bundle-rules.json lists no entries");
  const violations = [];
  let chunkCount = 0;
  for (const [entry, entryRules] of entries) {
    const chunks = initialClosure(manifest, entry);
    chunkCount += chunks.length;
    violations.push(...checkRules(entry, chunks, entryRules, readChunk));
  }
  violations.push(...staleSentinels(manifest, rules, readChunk));
  return { violations, entries: entries.length, chunks: chunkCount };
}

function main() {
  const dist = process.env.LOOM_BUNDLE_CHECK_DIST
    ? resolve(process.env.LOOM_BUNDLE_CHECK_DIST)
    : resolve(scriptDir, "../dist");
  const manifest = JSON.parse(readFileSync(resolve(dist, ".vite/manifest.json"), "utf8"));
  const rules = JSON.parse(readFileSync(resolve(scriptDir, "bundle-rules.json"), "utf8"));
  const { violations, entries, chunks } = runBundleCheck(manifest, rules, (file) =>
    readFileSync(resolve(dist, file), "utf8"),
  );
  if (violations.length) {
    for (const violation of violations) console.error(violation);
    process.exitCode = 1;
  } else console.log(`bundle-check: ${entries} entries, ${chunks} chunks, ok`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    main();
  } catch (error) {
    console.error(`bundle-check: ${error.message}`);
    process.exitCode = 1;
  }
}
