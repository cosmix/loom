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

export function checkRules(entry, chunks, rules, readChunk) {
  const violations = [];
  for (const chunk of chunks) {
    for (const rule of rules.forbidChunks ?? []) {
      const pattern = globRegex(rule);
      if (pattern.test(chunk.file) || (chunk.src && pattern.test(chunk.src))) {
        violations.push(`${entry}: ${chunk.file} matches ${rule}`);
      }
    }
    if (!/\.m?js$/.test(chunk.file)) continue;
    const text = readChunk(chunk.file);
    for (const rule of rules.forbidText ?? []) {
      if (text.includes(rule)) violations.push(`${entry}: ${chunk.file} matches ${rule}`);
    }
  }
  return violations;
}

function main() {
  const dist = process.env.LOOM_BUNDLE_CHECK_DIST
    ? resolve(process.env.LOOM_BUNDLE_CHECK_DIST)
    : resolve(scriptDir, "../dist");
  const manifest = JSON.parse(readFileSync(resolve(dist, ".vite/manifest.json"), "utf8"));
  const rules = JSON.parse(readFileSync(resolve(scriptDir, "bundle-rules.json"), "utf8"));
  const violations = [];
  let count = 0;
  for (const [entry, entryRules] of Object.entries(rules)) {
    const chunks = initialClosure(manifest, entry);
    count += chunks.length;
    violations.push(
      ...checkRules(entry, chunks, entryRules, (file) => readFileSync(resolve(dist, file), "utf8")),
    );
  }
  if (violations.length) {
    for (const violation of violations) console.error(violation);
    process.exitCode = 1;
  } else console.log(`bundle-check: ${Object.keys(rules).length} entries, ${count} chunks, ok`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    main();
  } catch (error) {
    console.error(`bundle-check: ${error.message}`);
    process.exitCode = 1;
  }
}
