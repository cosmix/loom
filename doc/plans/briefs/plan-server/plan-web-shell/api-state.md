# plan-web-shell / api-state: spec for codex units W1-W5

Lane: codex (`codex-units.md`, units W1-W5). Each unit owns the files its row names. Read
`doc/plans/briefs/plan-server/common.md` first. Web conventions: `web/src/api/zod.ts` (import `z`
only from `@/api/zod`; `.oxlintrc.json` bans `zod` directly), `web/src/api/schema.ts` (pattern),
`web/src/api/ws.ts` (`connectStatusSocket`: reconnect, watchdog, phases), `web/src/state/atoms.ts`
(jotai).

## schema.ts

One zod schema per Rust wire type of `loom/src/plan_server/wire/` (read the Rust structs; the
fixtures in `web/src/plan/api/fixtures/` are the truth): `serverInfoSchema`, `youSchema`,
`missingPrereqSchema` (`what` enum `tmux | claude | claude_login | sandbox`), `shareLinkSchema`
(`bind` `loopback | remote`), `presenceListSchema`, `authoringViewSchema`, `checklistViewSchema`
(`mark` `done | gap | active | optional`; `current` discriminated on `kind`: `step | ready | ready_all_done`),
`refusedSchema` (and `project_key` on `serverInfoSchema`), `planMessageSchema` (discriminated union on `type`: `hello | presence | authoring | checklist | server_stopping | refused`),
`clientMessageSchema` (`set_name | reading`). Export the inferred types. Use `z.strictObject` so an
unknown field fails, as `deny_unknown_fields` does in Rust. Export each enum's values as a
`const` array (`PREREQ_KINDS`, `BIND_KINDS`, `STEP_MARKS`, `CURRENT_KINDS`, `PLAN_MESSAGE_TYPES`,
`CLIENT_MESSAGE_TYPES`) so the Rust parity test can read them.

`schema.test.ts`: every fixture parses with its schema (`import fixture from "@/plan/api/fixtures/<name>.json"`);
a fixture with one extra field fails; a `hello` with an unknown `type` fails.

`loom/src/plan_server/wire/tests_schema.rs`: mirror `loom/src/commands/status/web/schema_parity.rs`
(`assert_all_in_schema`:138): read `web/src/plan/api/schema.ts` with `include_str!`, and for each
Rust serde enum assert every wire value (serialize each variant) appears in the matching TS const
array. Test names `<enum>_wire_values_are_in_plan_schema`.

## client.ts

`apiGet<T>(path: string, schema: ZodType<T>, signal?): Promise<ApiResult<T>>` and
`apiPost<T>(path, body: unknown | string, schema, opts: { contentType: "json" | "text" })`;
`type ApiResult<T> = { ok: true; value: T } | { ok: false; status: number; error: string; message: string }`.
`credentials: "same-origin"`. A 401 from any call sets `unauthorizedAtom` (in atoms.ts) through a
module-level store passed to `initClient(store)`. Parse failures are `{ ok: false, status: 0, error: "bad_response" }`.
A rejected `fetch` (the server is down: `TypeError: Failed to fetch`) never escapes: both
functions catch it and return `{ ok: false, status: 0, error: "unreachable", message }`.

## socket.ts

`connectPlanSocket(store, deps?: { WebSocketImpl?, setTimeout?, clearTimeout? }) => { send(msg: ClientMessage): boolean; subscribe<K extends PlanMessageType>(type: K, handler: (msg: Extract<PlanMessage, { type: K }>) => void): () => void; dispose(): void }`.
URL `ws(s)://<location.host>/ws`. Before the first connect and after each close, call
`apiGet("/api/server")`; a 401 stops reconnecting (unauthorized); any other failure, `unreachable`
included, schedules the next attempt on the same delay sequence. Reconnect delays 1 s, 2 s, 5 s,
then every 10 s (section 08, "Connection and server errors"); reset after a `hello`. Each frame is
parsed with `planMessageSchema`; a frame that fails is dropped with `console.warn`. Built-in
handlers write `hello`, `presence`, `authoring`, `checklist`, `server_stopping` into atoms;
`connectionAtom` phase `connecting | open | reconnecting | unauthorized`, plus `lastMessageAt` and
`everOpen: boolean` (true once this page reached `open`; the 401 screen reads it). A socket close
sets `reconnecting` at once, before any retry. `send` returns false unless the phase is `open`
(plan 9 rejects its pending commands on that and on the phase change).
Resync (binding; plans 3, 5, 10, 11 use it): the handle also has
`onResync(handler: () => void): () => void`, called on every `hello` AFTER the first one of the
page (a reconnect: deltas may have been missed), after the built-in atoms were reset from that
`hello`; and `connectionEpochAtom: number` is bumped at the same moment. A plan that keeps state
fed by deltas refetches it in its resync handler. Export the handle as `planSocketAtom` (set by
`main.tsx`) so modules can reach `subscribe`, `send` and `onResync`.

## atoms.ts

`serverInfoAtom`, `youAtom`, `checklistAtom`, `presenceAtom`, `authoringAtom`, `connectionAtom`,
`unauthorizedAtom`, `stoppingAtom`, `connectionEpochAtom`, `planSocketAtom`; derived `isHolderAtom`
(holder public id equals `you.public_id`), `canAuthorAtom` (holder, or nobody holds and
`server.read_only` is empty and not stopping) and `authorBlockReasonAtom: string | null` (binding;
the one reason text every write control shows, null when `canAuthorAtom`): "loom plan is in
read-only mode: <first problem's message>", "loom plan is stopping", or "<holder name> holds
authoring".

## Tests

socket.test.ts with a fake WebSocket and fake timers: delays are exactly 1000, 2000, 5000, 10000,
10000 ms; a `hello` resets the sequence; 401 from `/api/server` stops reconnecting and sets
unauthorized; `/api/server` rejecting twice (fetch throws) then answering 200 still runs 1000,
2000, 5000 and connects; `subscribe` handlers receive only their type; `send` returns false while
closed; a close sets `reconnecting` before the first retry; `onResync` does not fire on the first
`hello` and fires once on the `hello` after a reconnect, after the atoms were updated.
client.test.ts: a rejected fetch gives `unreachable` for `apiGet` and `apiPost`.
atoms.test.ts: `canAuthorAtom` and `authorBlockReasonAtom` for holder / other holder / free / free
but read-only / stopping.

## Done when

`bun run --cwd web typecheck`, `lint`, `format:check`, `bun run --cwd web test` pass, and
`cargo test --manifest-path loom/Cargo.toml --lib plan_server::wire::` passes.
