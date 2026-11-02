---
name: loom-javascript
description: Modern JavaScript (ES2022+) and Node.js expertise for idiomatic, production-quality code without a TypeScript build.
triggers:
  - javascript
  - js
  - jsx
  - mjs
  - cjs
  - node
  - nodejs
  - node.js
  - esm
  - es modules
  - commonjs
  - package.json
  - jsdoc
  - ts-check
  - eslint
  - biome
  - vitest
  - jest
  - mocha
  - node:test
  - bun test
  - promise
  - async await
  - prototype pollution
  - event loop
---

# JavaScript Language Expertise

## Overview

Idiomatic ES2022+ JavaScript on Node.js 22+ (or Bun): ES modules, async/await, JSDoc types checked by `tsc`, the built-in `node:` APIs, and a small tool set. Assumes the reader knows the language. The content is decision rules and the traps that survive review: a floating promise, `==` on mixed types, a lost `this`, `JSON.parse` output merged into an object (prototype pollution), a `catch` that swallows the error, a test runner that finds zero files and exits 0. TypeScript syntax is covered by `loom-typescript`.

## Tooling

Use the package manager the repository's lockfile names. For new work prefer Bun: `bun add`, `bun remove`, `bunx`. Do not run `npm` or `npx` commands in this workflow.

| Task | Command |
| --- | --- |
| Install | `bun install` |
| Add a dependency | `bun add zod` (dev: `bun add -d vitest`) |
| Run a script | `bun run build` |
| Run a one-off tool | `bunx eslint .` |
| Run a file | `node src/main.js` or `bun src/main.js` |
| Type-check JSDoc | `bunx tsc --noEmit` |
| Lint | `bunx eslint .` or `bunx biome check .` |
| Format | `bunx biome format --write .` or `bunx prettier --write .` |
| Test, whole suite | `bun test` / `bunx vitest run` / `node --test` |
| Audit dependencies | `bun audit` |

- **Lockfile:** commit `bun.lock`. Install in CI with `bun install --frozen-lockfile`, which fails when `package.json` and the lockfile disagree.
- **Node version:** pin it in `package.json` (`"engines": {"node": ">=22"}`) and in `.nvmrc` or `.tool-versions`. Node 22+ ships `require(esm)`, `fetch`, `node:test`, `--watch` and `--env-file`.
- **Linting:** ESLint 9 uses a flat config (`eslint.config.js`); `.eslintrc*` is the legacy format. Biome is a single binary that lints and formats (`biome.json`) and replaces ESLint plus Prettier when the rule set it ships is enough. Pick one linter per repository.

Floating promises need type information. ESLint's `@typescript-eslint/no-floating-promises` works on JavaScript when `parserOptions.projectService` points at a `jsconfig.json`. Without it, review every call to an async function for an `await` or a `.catch`.

## Modules: ESM and CommonJS

A package is ESM when its `package.json` has `"type": "module"`; otherwise `.js` files are CommonJS. `.mjs` is always ESM and `.cjs` always CommonJS.

```javascript
// ESM
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import config from "./config.json" with { type: "json" };

export async function load(dir) {
  return readFile(join(dir, "data.txt"), "utf8");
}
```

| Concern | ESM | CommonJS |
| --- | --- | --- |
| Syntax | `import` / `export` | `require` / `module.exports` |
| Loading | static, hoisted, async-capable (top-level `await`) | synchronous, at the call |
| `__dirname`, `__filename` | absent; use `import.meta.dirname` and `import.meta.filename` (Node 20.11+) | defined |
| File extensions | required in relative specifiers: `./util.js` | optional |
| JSON | `with { type: "json" }` import attribute | `require("./x.json")` |
| `this` at top level | `undefined` | `module.exports` |

- Built-in modules take the `node:` prefix (`node:fs`, `node:crypto`): it states that the import is built in and cannot be shadowed by an installed package.
- Use named exports. A default export lets every importer choose a different local name, which defeats search and rename.
- Interop: ESM imports CommonJS freely (`module.exports` becomes the default export); named imports from CommonJS work only when Node can statically detect them. CommonJS loads ESM with `require()` on Node 22.12+ when the graph has no top-level `await`; otherwise use `await import("./x.mjs")`.
- Circular imports resolve differently: ESM exports are live bindings and a cycle fails with a `ReferenceError` when a binding is read before its module has run; CommonJS hands back a partially filled `exports` object. Break the cycle by moving the shared piece into a third module.
- A package that ships to others declares an `"exports"` map; it also blocks deep imports of files that are not listed.

```json
{
  "name": "@acme/fmt",
  "type": "module",
  "exports": {
    ".": { "import": "./src/index.js", "require": "./dist/index.cjs" },
    "./package.json": "./package.json"
  }
}
```

## Language Rules

### Values and equality

- `===` and `!==` always. `==` coerces: `0 == ""`, `"1" == 1` and `null == undefined` are all true. The one defensible use is `value == null` to test for `null` or `undefined` together; ESLint's `eqeqeq` option `["error", "always", { null: "ignore" }]` allows it.
- Use `Number.isNaN`, never `x === NaN` (always false) or the coercing global `isNaN`. `Object.is(a, b)` distinguishes `-0` and `+0`.
- `typeof null` is `"object"`. `Array.isArray` identifies arrays. `typeof` an undeclared name is `"undefined"` and does not throw.
- `||` replaces every falsy value (`0`, `""`, `false`); `??` replaces only `null` and `undefined`. Defaults for numbers and strings use `??`, and `??=` assigns when nullish.
- Numbers are IEEE doubles: `0.1 + 0.2 !== 0.3`, integers are exact only up to `Number.MAX_SAFE_INTEGER`. Use `BigInt` (`10n ** 20n`) for ids and money in minor units beyond that; `BigInt` does not mix with `Number` in arithmetic and `JSON.stringify` throws on it.

### Declarations and functions

- `const` by default, `let` when reassigned, never `var` (function scope, hoisted). Loop closures over `let` capture one binding per iteration.
- Arrow functions capture `this` lexically and have no `arguments`; use them for callbacks. Methods that rely on `this` are written as methods or `function`, and a method passed as a callback loses its `this`:

```javascript
class Counter {
  count = 0;
  inc = () => { this.count += 1; };   // field arrow: bound per instance
  dec() { this.count -= 1; }
}

const c = new Counter();
const { dec } = c;
// dec();                 // TypeError: this is undefined in a module
const bound = c.dec.bind(c);
bound();
```

- Parameters: destructure options objects with defaults (`function open({ path, mode = "r" } = {})`) rather than long positional lists. A default value is evaluated per call, so `function f(list = [])` is safe.
- Prefer `Map` and `Set` over plain objects for dynamic keys: no prototype collisions, any key type, `.size`, insertion order. `Object.create(null)` or `Object.hasOwn(obj, key)` for objects used as dictionaries.

### Collections and data

| Need | Use |
| --- | --- |
| Copy an array without mutating | `toSorted()`, `toReversed()`, `toSpliced()`, `with(i, v)` (Node 20+) |
| Last element | `arr.at(-1)` |
| Group by key | `Object.groupBy(items, fn)` or `Map.groupBy` (Node 21+) |
| Deep copy | `structuredClone(value)`; `JSON.parse(JSON.stringify(x))` drops `undefined`, `Date` and `Map` |
| Merge objects | spread `{ ...a, ...b }` (shallow) |
| Unique values | `[...new Set(arr)]` |
| Iterate entries | `for (const [k, v] of Object.entries(o))` |

- `Array.prototype.sort` without a comparator sorts as strings: `[10, 9, 1].sort()` gives `[1, 10, 9]`. Pass `(a, b) => a - b`.
- `for...in` walks inherited enumerable keys; use `for...of` over `Object.keys`, `Object.entries` or the iterable itself. `forEach` ignores `await` inside its callback.

## Async and Errors

An `async` function always returns a promise, and a throw inside it rejects that promise. A promise that rejects with no handler crashes Node (`unhandledRejection` is fatal since Node 15).

```javascript
// Floating promise: the rejection has no handler and the caller never waits
function save(user) {
  db.insert(user);                 // missing await
}

// Correct
async function save(user) {
  await db.insert(user);
}
```

- Every call to an async function is awaited, returned, or explicitly detached with a `.catch` that logs. `void fireAndForget().catch(log)` states the intent.
- `await` in a loop serialises. Independent work runs with `Promise.all`; bound the fan-out for large lists (a worker pool or `p-limit`), since 10 000 simultaneous requests exhaust sockets.
- `Promise.all` rejects on the first failure and leaves the others running. `Promise.allSettled` waits for all and reports each outcome; `Promise.any` resolves with the first success; `Promise.race` settles with the first result of either kind.
- `return await` inside `try` is required for the `catch` to see the rejection; outside `try` it is redundant.
- Wrap callback APIs once with `util.promisify`, or use the `node:fs/promises`, `node:timers/promises` and `node:stream/promises` variants.
- Cancellation: pass an `AbortSignal` (`AbortSignal.timeout(5000)`, `AbortSignal.any([a, b])`) to `fetch`, `fs/promises`, `timers/promises` and your own functions, and check `signal.aborted` or `signal.throwIfAborted()` between steps.

```javascript
async function getJson(url, { timeoutMs = 5000 } = {}) {
  const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
  if (!res.ok) {
    throw new Error(`GET ${url} failed: ${res.status}`, { cause: new HttpError(res.status) });
  }
  return res.json();
}

class HttpError extends Error {
  constructor(status) {
    super(`HTTP ${status}`);
    this.name = "HttpError";
    this.status = status;
  }
}
```

- Throw `Error` objects (they carry a stack), never strings. Pass the original through `{ cause }` when wrapping. Subclass `Error` for conditions a caller branches on, and test with `instanceof` or a stable `code` property.
- `fetch` resolves on HTTP 4xx and 5xx: check `res.ok`. It rejects only on network failure and abort.
- An empty `catch {}` is a defect. Handle the error, rethrow, or log it with the reason it is safe to ignore.
- Process level: register `process.on("unhandledRejection", ...)` only to log and exit non-zero; continuing in an unknown state hides the bug. Handle `SIGTERM` to close servers and flush work before `process.exit`.

### Event loop

- A synchronous loop of 200 ms blocks every request on the process. Move CPU work (hashing large input, parsing megabytes of JSON, image work) to `node:worker_threads` or a child process.
- Order inside one turn: synchronous code, then `process.nextTick`, then promise microtasks, then timers and IO callbacks. `queueMicrotask` and a recursive `nextTick` can starve IO.
- `*Sync` filesystem and crypto calls are acceptable at start-up and in CLIs and a defect in a request handler.
- Streams: use `pipeline` from `node:stream/promises` so errors propagate and handles close; `readable.pipe()` leaks the destination on error. `for await (const chunk of stream)` respects backpressure.

## Node.js APIs

| Need | API |
| --- | --- |
| Files | `node:fs/promises` (`readFile`, `writeFile`, `mkdir({ recursive: true })`, `rm({ recursive: true, force: true })`) |
| Paths | `node:path` (`join`, `resolve`, `relative`), `node:url` (`fileURLToPath`) |
| HTTP client | global `fetch`; server: `node:http` or a framework |
| Hashing, random | `node:crypto` (`randomUUID`, `randomBytes`, `createHash`, `timingSafeEqual`) |
| Child processes | `node:child_process` (`execFile`, `spawn`) |
| CLI arguments | `node:util` `parseArgs` |
| Config | `node --env-file=.env app.js`; read `process.env.NAME` once at start-up and validate |
| Test | `node:test` with `node:assert/strict` |
| Timing | `performance.now()`, `node:timers/promises` `setTimeout` |
| Concurrency | `node:worker_threads`, `AsyncLocalStorage` for request context |

```javascript
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const run = promisify(execFile);

// An argument array, no shell: user input cannot add commands.
const { stdout } = await run("git", ["log", "-1", "--format=%H", ref], { timeout: 10_000 });
```

## Types Without TypeScript

JSDoc plus `// @ts-check` gives the editor and `tsc` the same checking as `.ts` files with no build step. Enable it for the whole project in `jsconfig.json` or `tsconfig.json` and run `bunx tsc --noEmit` in CI.

```json
{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "noEmit": true,
    "strict": true,
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "target": "ES2022"
  },
  "include": ["src", "test"]
}
```

```javascript
// @ts-check

/**
 * @typedef {object} User
 * @property {string} id
 * @property {string} email
 * @property {"admin" | "member"} [role]
 */

/**
 * @param {readonly User[]} users
 * @param {string} email
 * @returns {User | undefined}
 */
export function findByEmail(users, email) {
  return users.find((u) => u.email === email);
}

/** @template T @param {T[]} items @returns {T | undefined} */
export const first = (items) => items[0];

/** @type {Map<string, User>} */
const cache = new Map();

/** @param {unknown} value @returns {value is User} */
export function isUser(value) {
  return typeof value === "object" && value !== null && "id" in value && "email" in value;
}
```

- Types shared between files go in a `types.d.ts` or a JSDoc `@typedef` exported from one module and imported with `/** @import { User } from "./types.js" */` (TypeScript 5.5+).
- Check external input at runtime (zod, valibot, or a hand-written guard); JSDoc types vanish at run time and `JSON.parse` returns `any`.
- A cast is `/** @type {Foo} */ (value)` with the parentheses; use it rarely, with a comment saying why the compiler cannot know.

## Module and API Design

- One module, one responsibility; export the smallest surface. Keep side effects out of import time: a module that opens a connection on import cannot be tested or reused. Export a `createX(options)` factory and let `main.js` wire dependencies.
- Pass dependencies (clock, `fetch`, logger, database) as parameters or constructor options so tests substitute them without monkey-patching.
- Validate at the boundary (HTTP bodies, CLI arguments, environment, files) and trust the validated shape inside.

## Common Pitfalls

```javascript
// 1. Prototype pollution: merging untrusted JSON into an object
function merge(target, source) {
  for (const key of Object.keys(source)) {
    if (key === "__proto__" || key === "constructor" || key === "prototype") continue;
    if (typeof source[key] === "object" && source[key] !== null) {
      target[key] = merge(Object.hasOwn(target, key) ? target[key] : {}, source[key]);
    } else {
      target[key] = source[key];
    }
  }
  return target;
}
// Without the key check, JSON.parse('{"__proto__":{"admin":true}}') in a naive deep merge sets
// Object.prototype.admin. Prefer Map, Object.create(null), or a vetted library.

// 2. Truthiness on numbers
const retries = 0;
const n = retries || 3;    // 3: the explicit 0 is lost
const m = retries ?? 3;    // 0

// 3. Async in forEach
items.forEach(async (item) => { await save(item); });   // returns before any save finishes
await Promise.all(items.map((item) => save(item)));     // waits
```

- Regular expressions with nested quantifiers (`(a+)+$`) backtrack exponentially on hostile input (ReDoS); bound the input length or use a linear-time engine.
- Template strings built into HTML, SQL or shell commands are injection sinks: use an escaping template library, parameterised queries, and `execFile` with an argument array.
- Do not compare secrets with `===`; use `crypto.timingSafeEqual` on equal-length buffers. Tokens come from `crypto.randomBytes` or `randomUUID`, never `Math.random`.
- `package.json` scripts run at install time for dependencies: review new packages, pin versions through the lockfile, and run `bun audit`.

## Testing

| Runner | Fits | Single test |
| --- | --- | --- |
| `node:test` | zero dependencies, Node-only code | `node --test --test-name-pattern='name' file.test.js` |
| `bun test` | Bun projects, fast, Jest-style API | `bun test file.test.js -t 'name'` |
| Vitest | Vite or ESM-first projects, watch mode, coverage | `bunx vitest run file.test.js -t 'name'` |
| Jest | existing Jest suites | `bunx jest file.test.js -t 'name'` |

```javascript
// math.test.js (node:test; the same shape runs under bun test and vitest with their imports)
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { add } from "./math.js";

describe("math", () => {
  it("adds two numbers", () => {
    assert.equal(add(2, 3), 5);
  });

  it("rejects non-numbers", () => {
    assert.throws(() => add("2", 3), { name: "TypeError" });
  });
});
```

- A test asserts behaviour, one reason to fail per test, names that state the behaviour. Async tests `await` or return the promise; an un-awaited assertion passes after the test ends.
- Assert rejections with `await assert.rejects(fn(), { name: "HttpError" })`, never a `try`/`catch` that passes when nothing throws.
- Fake time with the runner's timers (`mock.timers` in `node:test`, `vi.useFakeTimers()`, `jest.useFakeTimers()`), and restore them in `afterEach`.

## Loom Test Runner Adapter

**Adapter selection.** `loom project detect` prints one adapter per package. A directory with `package.json` is kind `javascript` (kind `typescript` when `tsconfig.json` exists or `typescript` is a dependency; both kinds use the same rule). The runner is the first match of:

1. a `vitest`, `jest` or `mocha` entry in `dependencies` or `devDependencies`, checked in that order, so a package declaring both `vitest` and `jest` gets `vitest`;
2. `bun-test`, when `bun.lock` or `bun.lockb` exists in the package directory or `scripts.test` starts with `bun test`;
3. `node-test`, when `scripts.test` contains `node --test`.

A package with none of these has no adapter, and the contract's optional `runner:` field must name one. `node:test` projects therefore need `"test": "node --test"` in `package.json`.

```text
web  kinds=javascript  runner=vitest  skills=loom-javascript
```

**Single-test command**, run with the package directory as cwd; `{file}` is the contract `file`, `{test}` the contract `test` with regex metacharacters escaped:

```bash
# vitest   (runner prefix is bunx when bun.lock or bun.lockb exists in the package directory, else npx)
bunx vitest run {file} -t '{test}'
# jest     (same prefix rule)
bunx jest {file} -t '{test}'
# mocha    (same prefix rule)
bunx mocha {file} --grep '{test}'
# bun-test
bun test {file} -t '{test}'
# node-test
node --test --test-name-pattern='^{test}$' {file}
```

Facts from `loom/src/testrun/adapters/` (`vitest.rs`, `jest.rs`, `mocha.rs`, `bun_test.rs`, `node_test.rs`, `package_runner` in `command.rs`): loom picks `npx` over `bunx` when the package directory has no Bun lockfile, so an npm-managed package runs `npx` through loom's own command even though this skill does not recommend it. The `-t`, `--grep` and `--test-name-pattern` values are regular expressions matched as substrings, except `node-test`, where loom anchors the name with `^` and `$`.

**The `test` field** is the full test name as the runner joins it. Vitest separates nested suite and test titles with ` > ` (`math > adds two numbers`); Jest, Mocha, Bun, and Node use spaces (`math adds two numbers`). Keep names to letters, digits, spaces and `_`; a `'` or a `$` in a title is quoted by loom but makes the name hard to match by hand.

**Writing contract tests:**

- A path counts as a JavaScript test file when it matches `*.test.*` or `*.spec.*` with extension `js`, `jsx`, `ts`, `tsx`, `mjs` or `cjs`, or lies under `__tests__/` (`loom/src/testrun/languages.rs`, `javascript` profile). A test in `test/foo.js` matches no pattern.
- The profile recognises `it(` and `test(` as declarations and `expect(` or `assert.` as assertions. A contract test whose only assertions go through a custom helper matches neither pattern.
- A stage worktree is a fresh checkout with no `node_modules`. A package that declares dependencies needs a `loom.provision` entry in a version 2 plan whose command is a hardened install form that plan validation accepts (`bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null`, plus the `.npmrc` guard it wraps around that); the validation message for an uncovered package prints the exact command to paste.
- Run the command by hand once from the package directory before freezing, and confirm the output names the test; a name that matches nothing is not a pass.

```yaml
contracts:
  - id: rejects-empty-email
    file: src/users.test.js
    test: users rejects an empty email
    scenario: calls createUser with an empty email string
    rejects: a createUser that stores the user and returns an id without validating the email
```

**Build failures:** plain JavaScript has no compile step, so a contract test importing a function that does not exist yet fails at run time with `SyntaxError: ... does not provide an export named` or `Cannot find module`. The runner reports a failing file; that is red.
