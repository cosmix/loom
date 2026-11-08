# W3: web dashboard daemon line shows the busy operation

Read `doc/plans/briefs/scheduler-busy-status/busy-tick/common.md` in full first, especially
"Wording", "Rules every surface follows" and "Web payload". You run in parallel with W2; the
payload shape in common.md is fixed, so you do not wait for it. W1 finished before you start:
`crate::orchestrator::busy::BusyOp` exists.

## Files you own

- `web/src/api/busy-schema.ts` (new)
- `web/src/api/schema.ts`
- `web/src/lib/format.ts`
- `web/src/lib/format.test.ts`
- `web/src/components/header-lines.tsx`
- `loom/src/commands/status/web/schema_parity.rs`

Do not edit the frozen contract file `web/src/lib/daemon-line-busy.test.ts`; make it pass.
`web/src/api/fixtures/snapshot.json` is W2's: it gains `"busy": null` there, and your schema
default parses it either way.

## Tasks

1. **`busy-schema.ts` and `schema.ts`.** `schema.ts` is at 392 lines, so the new schema lives in
   its own file.
   - In `busy-schema.ts`:
     `export const busySchema = z.object({ op: z.enum(["brief-generation", "merge-verification", "provisioning"]), what: z.string(), stage_id: z.string(), started_at: z.string(), ceiling_secs: z.number().int(), global_ceiling_secs: z.number().int() }).strict();`
     and `export type Busy = z.infer<typeof busySchema>;`.
   - In `schema.ts`: import it, re-export `busySchema` and `Busy`, and add
     `busy: busySchema.nullable().default(null)` to `snapshotSchema` (which is `.strict()`).
     zod 4.5.4: the parse input may omit `busy` (hand-built fixtures and `HELP_SNAPSHOT` keep
     parsing), the output type is `Busy | null`. A TypeScript object literal typed as the
     snapshot output must now carry `busy`; rg for such literals and fix any you find in your
     files (none were found outside spreads of an existing snapshot).
2. **`format.ts` `daemonLine`.**
   - New signature:
     `daemonLine(daemon: DaemonState, tickAgeSecs: number | null, staleSecs: number | null, busy: Busy | null, nowMs: number)`.
   - Arm order, exactly as common.md "Rules every surface follows", web bullet:
     1. `staleSecs` set (today's arm);
     2. `process-only` (today's arm);
     3. `not-running` (today's arm);
     4. `tickAgeSecs !== null && tickAgeSecs >= 60`: today's arm,
        `{ text: "loop stalled " + formatElapsed(tickAgeSecs), tone: "warning" }`, busy or not;
     5. `busy` set and `elapsed >= busy.global_ceiling_secs`:
        `{ text: "loop stalled " + formatElapsed(elapsed), tone: "warning" }`;
     6. `unreachable` (today's arm, unchanged);
     7. `busy` set: `{ text: "daemon running", detail: "busy " + busy.what + " " + formatElapsed(elapsed), tone }`,
        `tone` `"warning"` when `elapsed >= busy.ceiling_secs`, else `"completed"`;
     8. today's tick arm.
   - `elapsed = Math.max(0, Math.round((nowMs - Date.parse(busy.started_at)) / 1000))`; an
     unparseable `started_at` (`NaN`) counts as elapsed 0.
3. **`header-lines.tsx` `DaemonLine`.** Pass `snapshot.busy` and the `now` it already gets from
   `useNow()` (the stage's wiring greps `snapshot.busy` in this file, so pass it as a real
   argument, not in a comment).
4. **`format.test.ts`.**
   - Update the existing `daemonLine` calls to the new signature (`null, Date.now()` for the
     two new arguments) without changing what they assert. The table test's
     `expect(daemonLine(...))` line changes; the orchestrator files that integrity event.
   - Add tests: `daemonLine("not-running", 2, null, busy, now)` returns "daemon stopped" and
     `"process-only"` its own arm (a stale busy line after a killed daemon never shows); a
     stale tick while busy prints the tick age, not the operation's elapsed time;
     `"unreachable"` with busy keeps "socket unreachable".
   - Add a test that the WebBusy JSON from common.md's example, placed in the fixture
     snapshot, round-trips through `snapshotSchema`, and that a snapshot without `busy` parses
     to `busy: null`. W2's pinned output is identical by construction; the orchestrator checks
     the two against each other after both workers return.
5. **`schema_parity.rs`** (Rust, `#[cfg(test)]` module under `commands/status/web/`).
   - Add `include_str!` of `web/src/api/busy-schema.ts` beside the existing `schema.ts` one
     (copy its relative-path form).
   - Add `all_busy_ops() -> Vec<BusyOp>`, opening with an exhaustive `match` over a placeholder
     (no wildcard arm), like the other `all_*` lists in the file.
   - Add a test asserting that the busy schema text contains `"<op.as_str()>"` for every op.
     `BusyOp` is not `Serialize`, so do not reuse `assert_all_in_schema`; write the loop.
6. **Format.** `cd web && node_modules/.bin/oxfmt` over the files you changed.

## Done when

These pass, from `web/`:

- `node_modules/.bin/tsc -b --noEmit`
- `node_modules/.bin/oxlint --deny-warnings`
- `node_modules/.bin/oxfmt --check`
- `node_modules/.bin/vitest run src/lib src/api src/components src/state src/routes`

and, from the repository root,
`cargo test --manifest-path loom/Cargo.toml --lib commands::status::web::schema_parity`.

Use the `loom-typescript`, `loom-react` and `loom-rust` skills. Report the files changed and
anything you could not settle.
