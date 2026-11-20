# W4 — Web: wire schema and settings model

**Tier:** codex `gpt-5.6-terra`, `--effort xhigh`

**Files you own (write):**

- `web/src/api/config.ts`
- `web/src/components/settings-model.ts`
- `web/src/components/settings-model.test.ts`

**Read-only:** `web/src/api/fixtures/config.json` (W3 regenerates it).

**Touch nothing else under `web/`.** W5 owns every `.tsx` component, the test
kit, and the component test files. You and W5 run at the same time; the shared
TypeScript contract is written out below and in W5's brief, so neither of you
waits on the other. **Do not deviate from it** — if you think something is
wrong, implement as specified and say so in your report.

**Do not run `git` at all.** The orchestrator stages and commits.

## Codex unit

You are ONE unit, `w4-web-model`: three files, one forward. It has to be
completable from this brief alone inside the wrapper's 540 s deadline. An exit
124 means the unit was too large — re-split the remainder into smaller
interface-pinned units (`config.ts` first, then `settings-model.ts`, then the
test file) rather than re-forwarding the same one.

## Why

The server now sends native JSON. Today `web/src/api/config.ts:13,19,22-25`
types every value as `z.string()`, and `settings-model.ts` then re-derives the
type the server never sent: `formatValue` (declared at `:242`) tests a value
with `/^\d+$/` at `:244` before calling `Number()`, and `displayValue` compares
`value === "true"` at `:62`. That client-side re-parsing is exactly what becomes
unnecessary.

## The shared TypeScript contract

```ts
/// A config value as the server sends it: native JSON, shaped by the entry's
/// `kind`. A bool key sends a boolean, a number key a number, an enum or a
/// free-text string key sends a string.
export const configValueSchema = z.union([z.boolean(), z.number(), z.string()]);
export type ConfigValue = z.infer<typeof configValueSchema>;   // boolean | number | string

export const configKindSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("bool") }),
  z.object({ type: z.literal("number") }),          // was "u32"
  z.object({ type: z.literal("enum"), variants: z.array(z.string()) }),
  z.object({ type: z.literal("string") }),          // new
]);

export const scopeValueSchema = z.object({
  value: configValueSchema,
  set: z.boolean(),
});

export const configEntrySchema = z.object({
  name: z.string(),
  help: z.string(),
  kind: configKindSchema,
  scopes: z.array(configScopeSchema),
  default: configValueSchema,
  user: scopeValueSchema,
  project: scopeValueSchema.nullable(),
  effective: z.object({
    value: configValueSchema,
    source: z.enum(["project", "user", "default"]),
  }),
});

export interface ConfigWrite {
  scope: ConfigScope;
  name: string;
  /// `null` unsets the key at that scope so it falls back to the tier below.
  value: ConfigValue | null;
}
```

And, from `settings-model.ts`:

```ts
export function valueAt(entry: ConfigEntry, scope: ConfigScope): ConfigValue | null;
export function fallbackFor(entry: ConfigEntry, scope: ConfigScope): { tier: string; value: ConfigValue };
export function displayValue(kind: ConfigKind, value: ConfigValue): string;
export function formatValue(kind: ConfigKind, value: ConfigValue): string;
export interface LaneState { lane: Lane; value: ConfigValue | null; provenance: LaneProvenance; effective: boolean }
export type OnWrite = (scope: ConfigScope, name: string, value: ConfigValue | null) => void;
export type WriteStatus =
  | { phase: "idle" }
  | { phase: "pending"; value: ConfigValue | null }
  | { phase: "saved" }
  | { phase: "error"; message: string };
```

Everything else in both files keeps its current name and signature.

## Task 1 — `web/src/api/config.ts`

Apply the schema above. `createConfigClient` needs no logic change: `write`
already `JSON.stringify`s the body, and a `ConfigValue` serializes natively.
`writeResponseSchema` and `errorResponseSchema` are unchanged.

`ConfigWrite.value` becoming `ConfigValue | null` is not cosmetic. It is `:44`'s
`string | null` today, and W3's server-side `checked` now rejects a JSON string
for a `Bool` or a `Number` key, so a dashboard that keeps sending strings 400s
on `update.check` and on both `Number` keys. The type is the only thing stopping
that.

Export `configValueSchema` and the `ConfigValue` type — W5 imports the type.

## Task 2 — `web/src/components/settings-model.ts`

- `valueAt`, `fallbackFor`, `LaneState.value`, `laneState`: swap `string` for
  `ConfigValue`. The `?? null` handling is unchanged.
- `displayValue`: a bool is now a real boolean.

  ```ts
  /// Booleans read better as words than as `true`/`false`.
  export function displayValue(kind: ConfigKind, value: ConfigValue): string {
    if (kind.type === "bool") return value === true ? "on" : "off";
    return String(value);
  }
  ```

- `formatValue`: the regex guard goes away — a number key's value IS a number.

  ```ts
  export function formatValue(kind: ConfigKind, value: ConfigValue): string {
    if (kind.type === "bool") return displayValue(kind, value);
    if (kind.type === "number" && typeof value === "number") {
      return value.toLocaleString("en-US");
    }
    return String(value);
  }
  ```

  Keep the `typeof value === "number"` guard: it is no longer re-deriving a
  type, it is narrowing the union so `toLocaleString` type-checks, and it keeps
  the display honest if a server ever sends a mismatched shape.

- `rowHaystack` — **this one is easy to miss and the compiler will catch only
  some of it.** It pushes values into a `string[]` and joins them. Every value
  pushed from an entry is now a `ConfigValue`, so wrap each in `String(...)`:
  `entry.default`, `entry.user.value`, `entry.effective.value`, and
  `entry.project.value`. The `displayValue(...)` calls beside them already
  return strings and need no wrapping. Search is by lowercased text, so
  `String(800000)` giving `"800000"` is the behaviour you want — an operator
  typing `800000` still finds the key.

- `OnWrite` and `WriteStatus` per the contract above.

`sectionOf`, `fieldOf`, `groupBySection`, `provenanceAt`, `effectiveLane`,
`laneScope`, `sectionRows`, `rowEntries`, `rowLabel`, `filterSections`,
`statusKey`, `statusFor`, `LANES`, `SECTION_CAPTIONS`, `ROW_CAPTIONS` are all
unchanged apart from types flowing through.

## Task 3 — `settings-model.test.ts`

The existing file is 213 lines and is driven by the REAL fixture, not by inline
data: `:4` imports `@/api/fixtures/config.json` and `:19` runs
`configResponseSchema.parse(fixtureJson)` at MODULE scope, so the `sectionRows`
(`:21-96`), `laneState`/`effectiveLane` (`:98-130`) and `filterSections`
(`:132-177`) blocks all read `fixture.entries`. The only inline entry in the
file is `lonely` (`:82-91`).

That makes W3's regenerated fixture a hard prerequisite: the moment you drop the
`u32` arm from `configKindSchema`, a fixture still carrying `"u32"` fails that
module-scope `parse` and the whole file dies at import, before a single case
runs. Before you start, run:

```text
jq '[.entries[].kind.type] | unique' web/src/api/fixtures/config.json
```

If that still prints `u32`, W3's work is not on disk: STOP and report it. Do NOT
keep a `u32` arm in `configKindSchema` to make the parse succeed.

Two existing assertions change:

- `:124` asserts `expect(entry.project?.value).toBe("900000")` on the fixture's
  `context.ceiling_tokens` (the entry picked up at `:99`); it becomes
  `toBe(900000)`. The three other assertions in that block (`:107`, `:116`,
  `:123`) compare against the entry's own fields and need no change.
- The `formatValue` describe block (`:200-212`) is written against the old
  shapes and must be REPLACED, not extended:

  ```ts
  describe("formatValue", () => {
    it("groups a number with commas", () => {
      expect(formatValue({ type: "number" }, 800000)).toBe("800,000");
    });
    it("leaves a mismatched non-number unchanged", () => {
      expect(formatValue({ type: "number" }, "abc")).toBe("abc");
    });
    it("maps bool to on/off", () => {
      expect(formatValue({ type: "bool" }, true)).toBe("on");
      expect(formatValue({ type: "bool" }, false)).toBe("off");
    });
  });
  ```

  Only `{type:"u32"}` is compiler-caught there; the string values `"800000"` and
  `"true"` are not, and leaving them turns two passing assertions into one
  failure and one pass for the wrong reason.

Then add:

- `displayValue` for a real boolean `true`/`false` → `"on"`/`"off"`. The
  replacement block above covers `formatValue`'s bool arm and the `en-US`
  grouping a number key must keep producing; this case covers `displayValue`
  directly.
- `formatValue` for an enum → the variant string unchanged.
- `formatValue` for a `{type:"string"}` entry → the text unchanged. No registry
  key uses that kind yet, so build a synthetic entry; without this the new kind
  is typed but never executed.
- `filterSections` matches on a numeric value: an entry whose value is the
  number `800000` is found by the query `"800000"`. This is the `rowHaystack`
  `String(...)` fix — before it, the join produced `[object Object]`-class
  breakage or dropped the value, and search silently stopped matching numbers.

## Acceptance for your slice

Run at most ONE scoped check, once:

```text
bun run --cwd web test -- settings-model
```

Do not run `typecheck`, `lint`, `format:check`, `build`, or the Rust gate — the
main agent does all of that. Note that a full `typecheck` will fail until W5's
components land, which is expected and not yours to fix.

## Report

Files changed, the final exported contract if it differs in any way from the
spec above (W5 is compiling against the spec, so any deviation must be reported
loudly), anything unresolved.
