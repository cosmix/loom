# W5 — Web: React controls and test kit

**Tier:** codex `gpt-5.6-terra`, `--effort xhigh`

**Files you own (write):**

- `web/src/components/settings-control.tsx`
- `web/src/components/settings-cards.tsx`
- `web/src/components/settings-dialog.tsx`
- `web/src/components/settings-lanes.tsx`
- `web/src/components/settings-lanes-cells.tsx`
- `web/src/test/settings-kit.tsx`
- `web/src/components/settings-cards.test.tsx`
- `web/src/components/settings-dialog.test.tsx`
- `web/src/components/settings-entry.test.tsx`
- `web/src/components/settings-string-field.test.tsx` (new file)
- `web/src/components/settings-number-field.test.tsx` (new file)

**Read-only:** `web/src/api/config.ts`, `web/src/components/settings-model.ts`
(both W4's), `web/src/api/fixtures/config.json` (W3's).

**Touch nothing else.** `settings-model.test.ts` is W4's. You and W4 run at the
same time; the shared TypeScript contract is written out below and in W4's
brief, so neither of you waits on the other. **Do not deviate from it.**

**Do not run `git` at all.** The orchestrator stages and commits.

Every line number in this brief is where the symbol sat at HEAD and is a hint
only. Anchor each edit on the component, function or `interface` name given
beside the number.

## Codex units

You are forwarded as four units, in this order, one forward each. A unit must
be completable from this brief alone inside the wrapper's 540 s deadline; the
next forward starts against the tree the previous one left. An exit 124 means
the unit was too large — re-split the remainder along the file boundaries in the
table rather than re-forwarding the same one.

| Unit | Files written | Steps |
| --- | --- | --- |
| `w5a-control-kit` | `settings-control.tsx`, `web/src/test/settings-kit.tsx` | Task 1's `ControlProps`/`ValueControl`/`BoolSwitch`/`NumberField`/`StringField`; Task 1's `EnumSelect`, `slotView`, `LaneSlot`, `LaneSlotProps`, `ClearButton`, `PairSummary`, `BuiltinValue`; Task 3 |
| `w5b-cards-lanes` | `settings-cards.tsx`, `settings-lanes.tsx`, `settings-lanes-cells.tsx`, `settings-cards.test.tsx` | the three components' type-only changes; `settings-cards.test.tsx`'s retyped assertions; nothing else |
| `w5c-dialog-tests` | `settings-dialog.tsx`, `settings-dialog.test.tsx`, `settings-entry.test.tsx`, `settings-string-field.test.tsx` | Task 2's dialog write-status plumbing and `savedToast`; `settings-dialog.test.tsx` and `settings-entry.test.tsx` retypes; the new `settings-string-field.test.tsx` cases |
| `w5d-number-field` | `settings-number-field.test.tsx` | Task 4's `NumberField commits %s as %s` table; then the slice's one scoped check |

The tasks below stay the detailed reference; the table only says which forward
does which. `w5b` and `w5c` carry four files each because the last three in
every row are type-only retypes rather than new logic.

`w5b`, `w5c` and `w5d` compile against what `w5a` wrote, and your source-graph lookups
answer from the published base layer rather than from your own edits, so
`settings-control.tsx`'s exported surface after `w5a` is:

```ts
// web/src/components/settings-control.tsx, after w5a
export interface ControlProps {
  id: string;
  kind: ConfigKind;
  value: ConfigValue;          // was string
  pending: boolean;
  invalid: boolean;
  label: string;
  describedBy?: string;
  onCommit: (value: ConfigValue) => void;   // was (value: string) => void
}

export function ValueControl(props: ControlProps): ReactElement;

export interface LaneSlotProps {
  entry: ConfigEntry;
  lane: "user" | "project";
  status: WriteStatus;
  controlId: string;
  onWrite: (value: ConfigValue | null) => void;   // was (value: string | null)
}

export function LaneSlot(props: LaneSlotProps): ReactElement;
export function PairSummary({ row }: { row: PairRow }): ReactElement;   // unchanged
export function BuiltinValue({ entry }: { entry: ConfigEntry }): ReactElement;   // unchanged
```

`BoolSwitch`, `NumberField`, `StringField`, `EnumSelect`, `ClearButton` and
`slotView` stay module-private; no later unit imports them.

## Why

The controls are already the most kind-aware surface — `ValueControl`
(`settings-control.tsx:34-43`) dispatches three widgets off the wire `kind`.
What they dispatch on is string plumbing: `BoolSwitch` compares
`value === "true"` (`:52`) and commits `"true"`/`"false"` (`:59`). With native
JSON values that indirection goes away, and a fourth kind arrives.

## The shared TypeScript contract W4 is writing

```ts
// web/src/api/config.ts
export const configValueSchema = z.union([z.boolean(), z.number(), z.string()]);
export type ConfigValue = boolean | number | string;

export type ConfigKind =
  | { type: "bool" }
  | { type: "number" }                                   // was "u32"
  | { type: "enum"; variants: string[] }
  | { type: "string" };                                  // new

export interface ConfigEntry {
  name: string; help: string; kind: ConfigKind; scopes: ConfigScope[];
  default: ConfigValue;
  user: { value: ConfigValue; set: boolean };
  project: { value: ConfigValue; set: boolean } | null;
  effective: { value: ConfigValue; source: "project" | "user" | "default" };
}

export interface ConfigWrite { scope: ConfigScope; name: string; value: ConfigValue | null }

// web/src/components/settings-model.ts
export function displayValue(kind: ConfigKind, value: ConfigValue): string;
export function formatValue(kind: ConfigKind, value: ConfigValue): string;
export function valueAt(entry: ConfigEntry, scope: ConfigScope): ConfigValue | null;
export function fallbackFor(entry: ConfigEntry, scope: ConfigScope): { tier: string; value: ConfigValue };
export interface LaneState { lane: Lane; value: ConfigValue | null; provenance: LaneProvenance; effective: boolean }
export type OnWrite = (scope: ConfigScope, name: string, value: ConfigValue | null) => void;
export type WriteStatus =
  | { phase: "idle" }
  | { phase: "pending"; value: ConfigValue | null }
  | { phase: "saved" }
  | { phase: "error"; message: string };
```

## Task 1 — `settings-control.tsx`

`ControlProps`:

```ts
value: ConfigValue;
onCommit: (value: ConfigValue) => void;
```

`ValueControl`'s switch gains a fourth arm. **Write it as `case "string":`** —
the plan's wiring check greps for that literal as proof the new kind is
dispatched rather than merely declared.

```tsx
switch (props.kind.type) {
  case "bool":   return <BoolSwitch {...props} />;
  case "number": return <NumberField {...props} />;
  case "enum":   return <EnumSelect {...props} variants={props.kind.variants} />;
  case "string": return <StringField {...props} />;
}
```

**`BoolSwitch`** — `checked={value === true}`, `aria-checked={value === true}`,
and `onChange={(event) => onCommit(event.target.checked)}`. The string
comparison and the `"true"`/`"false"` literals both go.

**`NumberField`** — keep the whole draft mechanism (`useState<string | null>`,
`onBlur` commit, Enter commits, Escape drops the draft) and keep it a
`type="text"` with `inputMode="numeric"`. The comment at `:64-66` explains why
and stays true: the server's validator owns the rules so its message about
`"abc"` reaches the operator instead of the browser silently refusing
keystrokes. Two changes:

- `const shown = draft ?? String(value);`
- On commit, send a number when the draft is one, and the raw string otherwise
  so the server answers with its own validation message:

  ```tsx
  const commit = () => {
    if (draft === null) return;
    const next = draft.trim();
    setDraft(null);
    if (next === String(value)) return;
    onCommit(asU32(next) ?? next);
  };
  ```

  with a module-private helper beside it:

  ```tsx
  const U32_MAX = 4294967295;
  /** The number Rust's `u32::from_str` reads from `text`, or null when it
   *  rejects it: an optional `+`, then ASCII digits only, at most u32::MAX. */
  function asU32(text: string): number | null {
    if (!/^\+?[0-9]+$/.test(text)) return null;
    const parsed = Number(text);
    return parsed <= U32_MAX ? parsed : null;
  }
  ```

  The rule is the server's own `raw.parse::<u32>()`, spelled out, and nothing
  looser. JavaScript's `Number()` accepts far more than that: `Number("0x10")`
  is `16`, `Number("1e3")` is `1000`, and `Number("1.0000000000000001")` is
  `1`, so any guard built on `Number()` alone (`isInteger`, `isFinite`) would
  commit values the CLI and TUI reject, one of them silently rounded. The regex
  admits exactly the decimal syntax `u32::from_str` accepts (leading `+` and
  leading zeros included), and the bound keeps an overflowing value out of the
  JSON number the server cannot deserialize. Every other draft — `-1`, `1.5`,
  `0x10`, `1e3`, `4294967296`, `abc`, the empty string — is sent as the raw
  string, reaches the server's `checked`, and comes back as the registry's own
  `is not a u32 (expected a non-negative integer)` naming the key. The
  comparison against 4294967295 is exact: every digit string above it parses to
  a double above it.

**`StringField`** (new) — the same draft/commit shape as `NumberField`, but
always commits the raw trimmed string and has no numeric affordance: `type="text"`,
no `inputMode`, `className="settings-ctl"`. Reuse `NumberField`'s ARIA wiring
exactly (`aria-label`, `aria-busy`, `aria-invalid`, `aria-describedby`,
`disabled={pending}`, `data-draft`).

If `NumberField` and `StringField` end up near-identical apart from the commit
rule and the `inputMode`, factor the shared body into one internal
`DraftField({ ...props, inputMode, commitValue })` rather than duplicating it —
but only if it genuinely collapses; do not build an abstraction for two callers
that still need separate bodies.

**`EnumSelect`** — `value` is a `ConfigValue` typed as the union, so narrow once
at the top: `const current = String(value);` and use `current` for the
`variants.includes(...)` check, the `value=` prop and the fallback option. The
"a value the registry no longer lists still needs an option" comment and
behaviour stay.

**`PairSummary`, `slotView`, `LaneSlot`, `BuiltinValue`** — these pass values to
`formatValue`, which now takes a `ConfigValue`, so they mostly just re-type.
Two specifics:

- `slotView`'s `shown` is `ConfigValue | null`, and `LaneSlot` currently renders
  `value={view.shown ?? ""}`. Keep `?? ""` — `??` is nullish-only, so a `false`
  or a `0` passes through untouched, an empty string is a valid `ConfigValue`,
  and the unavailable branch never reaches the control anyway.
- `ClearButton`'s title is built from `formatValue(entry.kind, fallback.value)`,
  unchanged.
- `LaneSlotProps.onWrite` (`:163`) is `(value: string | null) => void` today and
  becomes `(value: ConfigValue | null) => void`. Compiler-caught, one line, but
  it is the seam every write flows through.

## Task 2 — the other four components

`settings-cards.tsx`, `settings-dialog.tsx`, `settings-lanes.tsx` and
`settings-lanes-cells.tsx` are layout and grouping; they pass `ConfigEntry`
through and do not branch on `kind.type`. Expect type-only changes: anywhere a
`string` value is declared, held in state, or passed to `onWrite`, it becomes
`ConfigValue`. `settings-dialog.tsx` calls `formatValue` once (around `:209`,
for toast text) — that keeps working.

Check `settings-dialog.tsx`'s write-status plumbing specifically: `WriteStatus`
carries `value: ConfigValue | null` now, and the optimistic value it stores
while a write is in flight must not be stringified on the way in. The three
declarations to retype there and in the cards are all `string | null` today:
`write` (`settings-dialog.tsx:182`), `savedToast` (`:206`) and
`SettingsTableProps.onWrite` (`settings-cards.tsx:27`).

## Task 3 — `web/src/test/settings-kit.tsx`

This is the shared harness that `settings-cards.test.tsx`,
`settings-dialog.test.tsx` and `settings-entry.test.tsx` all build on:
`entry()` (`:14-26`), `snapshot()` (`:117`), `applyWrite()` (`:123`),
`fakeClient()` (`:150`), `renderAt()` (`:167`) and `row()` (`:182`) are
exported; `SNAPSHOT` (`:33-113`) is a module-private `const` that callers reach
only through `snapshot()`'s deep copy. `SNAPSHOT` is hand-written in the old
all-string shape.

- Convert every value: `update.check` → `default: true`, `user: { value: true, set: false }`,
  `effective: { value: true, source: "default" }`. `update.check_interval_hours`
  → `kind: { type: "number" }` and numeric values. `terminal.backend` and every
  `pressure.*`/`models.*` entry keep string values but their `kind` is
  unchanged.
- **Add one synthetic entry whose kind is written as the literal
  `type: "string"`** so `StringField` has coverage; a gate greps this file for
  that exact text. No registry key uses that kind, so this fixture is the only
  thing that will ever exercise it. Give it a plausible shape — a user-only key
  with a free-text default — put it in a SECTION OF ITS OWN (`notes.title`, say)
  and keep it last. A section of its own leaves every scoped count assertion
  alone; adding it to `update` risks `settings-dialog.test.tsx:102`, which
  asserts exactly one "User-only keys" badge `within` that section. There are no
  index-based assertions to shift in the three test files — the `sections[0]`
  style assertions live in W4's `settings-model.test.ts`, which reads the real
  fixture rather than this kit.
- `entry()` (`:14-26`) keeps its `"x"` string defaults for `default`, `user` and
  `effective`, and those typecheck under a `ConfigValue` whatever kind the caller
  passes — so `entry({ name, kind: { type: "bool" } })` silently produces a bool
  entry whose value is `"x"`, rendered as "off", with no compiler error. A caller
  passing a `bool` or `number` kind must pass matching values. One such caller
  exists: `settings-dialog.test.tsx:205-215` builds `models.stray_budget` as
  `{ type: "u32" }` with four `"1000"` strings; it becomes `{ type: "number" }`
  with `1000`. Only the kind is compiler-caught.
- `applyWrite()` (`:123-148`) already assigns `write.value` straight through at
  `:129-131` and `:134-136` and needs no edit beyond the types flowing through.
  There is no coercion there to hunt for.
- The header comment at `:10` lists a `settings-writes` suite that does not
  exist; the kit's only consumers are the three test files named above. Fix the
  comment while you are in the file.

## Task 4 — component tests

`settings-dialog.test.tsx` is 380 lines against a 400-line ceiling. Put new
cases in a new `web/src/components/settings-string-field.test.tsx` rather than
growing it. That file must contain the literal `StringField`; a gate greps for
it, because it is the only thing that proves the new control renders rather than
merely being dispatched.

Existing cases need their expected CONFIG values re-typed (`"true"` → `true`,
`"24"` → `24`), and only those. Three specifics:

- `settings-cards.test.tsx:92` already asserts the committed bool value —
  `expect(writes).toEqual([{ scope: "user", name: "update.check", value: "false" }])`.
  Retype it to `value: false` rather than adding a duplicate case elsewhere.
  Leaving it is a failing test; deleting it loses the coverage.
- Assertions about DOM attributes and DOM input values stay quoted strings:
  `data-effective` is set from `view.effective || undefined`
  (`settings-control.tsx:229`) and carries no config value, `getAttribute`
  always returns a string, and a DOM input's `value` is always a string. Leave
  `settings-cards.test.tsx:64,68` and `settings-dialog.test.tsx:76,85,147,170,176`
  exactly as they are — `toHaveProperty("value", "800000")` keeps passing under
  `const shown = draft ?? String(value);`.
- `settings-dialog.test.tsx:153` holds a copy of the server's wording,
  `'context.ceiling_tokens: "abc" is not a u32 (expected a non-negative integer)'`,
  fed to `fakeClient(data, failWith)` and asserted back. It stays byte-identical:
  the server's `Number` arm keeps that exact phrase after the rename, because it
  describes the accepted range rather than the Rust variant name.

`settings-entry.test.tsx` is 65 lines and has no string-value assertions at all,
so expect it to need type-only changes or none.

Beyond the retypes, add:

- `BoolSwitch` renders checked for `true` and unchecked for `false`, and
  toggling commits the boolean `true`/`false`, not the strings. Assert on the
  value handed to the write callback — a test asserting only that a write
  happened passes either way and is the failure mode this case exists to catch.
- In the new `web/src/components/settings-number-field.test.tsx`, one
  `it.each` table named `NumberField commits %s as %s`. `NumberField` is
  module-private, so render the exported `ValueControl` with
  `kind: { type: "number" }`, `value: 24` and `onCommit: vi.fn()`; type each
  input into the textbox, press Enter, and asserting the exact value
  (type included, `toStrictEqual`) handed to the write callback — the value
  that becomes `ConfigWrite.value` in the request:

  | Input | Committed |
  | --- | --- |
  | `"900000"` | `900000` |
  | `"0"` | `0` |
  | `"4294967295"` | `4294967295` |
  | `"+5"` | `5` |
  | `"00042"` | `42` |
  | `"4294967296"` | `"4294967296"` |
  | `"0x10"` | `"0x10"` |
  | `"1e3"` | `"1e3"` |
  | `"1.0000000000000001"` | `"1.0000000000000001"` |
  | `"1.5"` | `"1.5"` |
  | `"-1"` | `"-1"` |
  | `"abc"` | `"abc"` |

  The file must contain the literal `NumberField commits`; a gate greps for it.
- In `settings-string-field.test.tsx`: `StringField` renders its value,
  commits the trimmed text (assert the exact string handed to the write
  callback, e.g. `"  my project "` commits `"my project"`), and reverts on
  Escape. The commit case is named `StringField commits the trimmed text`; a
  gate greps for that literal.
- `EnumSelect` still renders an unlisted current value as an extra option.

## Acceptance for your slice

Run at most ONE scoped check, once, and only in the LAST unit (`w5d-number-field`)
— the filters name `settings-string-field` and `settings-number-field`, which
do not exist on disk until `w5c` and `w5d` write them, and vitest exits 1 when a filter matches no file:

```text
bun run --cwd web test -- settings-cards settings-entry settings-string-field settings-number-field
```

The filters are path substrings. Do not use `settings-control`: there is no
`settings-control.test.tsx` and that filter matches nothing.

Do not run `typecheck`, `lint`, `format:check`, `build`, or the Rust gate. A
full `typecheck` will fail until W4's files land, which is expected and not
yours to fix. **Do not run `bun run --cwd web build`** — `web/dist` is the
orchestrator's, and rebuilding it from here would collide with the other
worker's output.

## Report

Files changed, whether `NumberField`/`StringField` collapsed into a shared
internal component or stayed separate and why, the synthetic string entry you
added to `SNAPSHOT`, anything unresolved.
