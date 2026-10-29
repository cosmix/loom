# chat-web W1: model store, commands, API clients, drafts, attachment formatter

Dispatch: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 600000 ms. The orchestrator partitions this territory into ordered file-sized forwards before dispatch to fit the 540 s deadline; workers never delegate. All test/proof commands below are for the orchestrator; workers run compile-only checks, no git, and never write under `.loom/`.

Lane note: codex units own `api/schema-chat.ts` (C1), `api/schema-session-brief.ts` (C2),
`chat/storage.ts` (C3), `chat/visible-controls.ts` (C4), `review/api.ts` (C5), `chat/drafts.ts`
(C9) and `chat/prefs.ts` (C10), each with its test; the sections below on them are the reference
their briefs build on. C1 to C5 are in the tree when you start; C6 to C10 run beside you.

Working dir `web/`. You own `src/plan/chat/{model.ts, model.test.ts, commands.ts,
format-attachments.ts, format-attachments.test.ts}`. Everyone else starts from your exports: report their
names and signatures when `bun run typecheck` passes. The frozen contracts are
`src/plan/chat/chat.contracts.test.tsx`.

## model.ts

`schema-chat.ts`: zod for plan 7's chat model and frames, field for field from
`doc/plans/briefs/feat-planner-07-session-bridge/common.md` "The chat model" and
`loom/src/plan_server/wire/chat.rs` (`ChatFrame` tagged `frame`, `ChatDelta` tagged `op`, `Item`
tagged `type`, `ChatCommand` tagged `command`); `schema-chat.test.ts` parses every entry of
`src/plan/api/fixtures/chat-{snapshot,deltas,commands,frames}.json`. `schema-session-brief.ts`:
zod for plan 8's `SessionBriefView` over `session-brief.json`. `model.ts`: an atom holding the
`ChatSnapshot`, `applyChatFrame(store, frame)` for `snapshot` (replace), every `delta` op
(`session`, `turn_added`, `turn_updated`, `item_added`, `item_replaced`, `item_removed`, `queue`,
`cleared`), `result` (resolves W1's pending command promise by `client_id`) and `queue_returned`
(an atom W4 reads to refill the composer), subscribed through plan 2's
`subscribe("chat", ...)`. The snapshot's `previous` is the ended chat shown above the new one. Keep the previous session's turns when a new session starts (section 08: the
ended conversation stays above the new one until the new session writes its first item).
Derived atoms: `sessionAtom`, `turnsAtom`, `pendingPermissionAtom`, `queueAtom`. Holder and
read-only state come from plan 2's `web/src/plan/state/atoms.ts` (`isHolderAtom`,
`canAuthorAtom`, `authoringAtom`, `presenceAtom`, `serverInfoAtom.read_only`): import them, never
duplicate them.

## commands.ts

Thin senders over plan 2's `send(msg)` (`web/src/plan/api/socket.ts`) for plan 7's
`{ type: "chat", command: "start" | "send" | "withdraw" | "permission" | "interrupt" | "end", client_id, ... }`
(start takes an optional offer id, text and attachments; a fresh `client_id` per call)
and this plan's `{ type: "review", kind: "suggest" | "suggestion_answer", ... }`. Each returns a
promise that resolves on the server's accept and rejects with the server's refusal reason (plan
2's `refused` message, or `review` `refused` for this plan's commands). `toAttachmentInput(draft)` converts plan 3's draft into plan
7's `AttachmentInput`; its exact mapping is in "Fix-round additions" below (repo-relative paths,
`selected_hash: draft.hash`); the send command carries those. `getSessionBrief(id)` fetches plan 8's `GET /api/session/brief?session=`.

## review/api.ts

Zod schemas and fetchers for chat-server's routes (read `loom/src/plan_server/wire/review.rs`;
the fixtures `src/plan/api/fixtures/review-*.json` must parse): `postRestore`, `getTurnSections`,
`getHashes`, `resolveRef`, `getCompare`, `getAtCommit`, `getBuilt`, `getOpenQuestions`,
`getImplementedBy`, and a subscription to the `open_questions` WS message. Use plan 2's `apiGet(path, schema)` and
`apiPost` from `web/src/plan/api/client.ts`.

## drafts.ts, prefs.ts

`draftAtom`: `{ text: string; attachments: Attachment[] }` in `localStorage` under
`loom-plan:<project key>:draft` (project key from plan 2's bootstrap data); every read and write
in `try/catch`, reads validated with zod, a failure keeps memory. `prefs.ts`: panel width (rem,
clamped 20-40, default 26), collapsed flag, notification opt-in reuse
(`web/src/state/notifications.ts`'s atom), each through `atomWithStorage` with keys under
`loom-plan:`.

## format-attachments.ts

`formatMessage(text: string, attachments: Attachment[]): string`: section 10's block, a port of
plan 7's `loom::plan_session::attachments::format_message` matching it byte for byte (read
`loom/src/plan_session/attachments.rs` or its directory). Port the Rust code, not the spec's
example: header
`[n] <kind> <address>` then the optional fields in the module's order, quote lines prefixed `> `,
the 2,000-character cut at a line boundary with `> […N more characters; read the source]`,
escaping of a literal `<loom-attachments>` line in the user text exactly as the Rust module
does, the blank line between text and block. Characters are counted as the Rust module counts
them (if it counts Unicode scalar values, iterate with `for...of`, never `.length`). The browser builds `Attachment` values from its drafts for
the count, with `locked` from the document's lock (plan 3's `DocView.lock`) and
`changed_since_selected` from its hash comparison; the server resolves both again, so a
difference costs a few bytes of estimate, never a wrong delivery. Also
`deliveredBytes(text, attachments): number` (UTF-8 bytes via `TextEncoder`). Tests:
`format-attachments.test.ts` runs every case in `loom/tests/fixtures/plan_session/attachments/`
(read with `node:fs` relative to `import.meta.dirname`).

## Required test cases (coverage audit; named tests)

`schema-chat.test.ts`: every entry of the four `chat-*.json` fixtures parses; "every Item type,
ToolStatus, PermissionStatus, DenyReason, NoticeKind and Origin value is accepted".
`model.test.ts`: "snapshot replaces state"; one test per delta op (session, turn_added,
turn_updated, item_added, item_replaced, item_removed, queue, cleared); "result resolves the
pending command by client_id"; "queue_returned feeds the composer refill atom"; "previous kept
until first item". `drafts.test.ts`: "storage failure keeps memory", "bad stored draft dropped".
`prefs` (in `drafts.test.ts`): "width and collapsed persist in localStorage", "width clamps to
20-40". `format-attachments.test.ts`: every case of plan 7's attachment fixtures;
"deliveredBytes counts UTF-8 bytes".

## Fix-round additions (pressure test px-browser-06, -09, -11, -13, -25, px-seams-04, -08, px-security-09)

`storage.ts` and `visible-controls.ts` below are codex units C3 and C4 (reference); the rest of
this section is yours where it names your files.

- `storage.ts`: `safeStorage` (a jotai `createJSONStorage` over `localStorage` whose `getItem`
  and `setItem` catch every error) and `storageFullAtom` (true after a write failed with a quota
  error, false after the next success). `draftAtom` and every `prefs.ts` atom use it, keyed
  `loom-plan:<ServerInfo.project_key>:...`. W4 shows the note.
- `visible-controls.ts`: `visibleControls(text: string): string` (each Cf character listed in
  common.md becomes `⟨U+XXXX⟩`) and `<Untrusted text />` (a `span` with `unicode-bidi: plaintext;
  direction: ltr` around `visibleControls(text)`).
- Commands: when plan 2's `send()` returns false (any phase but `open`), reject at once with "Not connected; the message
  is kept"; on the socket phase turning `reconnecting` (plan 2 sets it at once on close) reject every
  pending command with "Connection lost; check the
  transcript before resending" and keep its `client_id` in `unconfirmedSendsAtom`. After the next
  snapshot, an unconfirmed send whose text appears as a user item is confirmed and dropped from
  the atom; one that does not stays offered for Resend.
- `pendingPermissionsAtom`: every Permission item with status `pending`, oldest first (replaces
  the singular atom).
- `queueReturnedAtom` is set only when this tab has a pending interrupt command (plan 7 sends
  `queue_returned` to the interrupting connection only; the holder's other tabs get a `notice`
  item); the refill reads only this atom.
- `toAttachmentInput(draft)`: source arm `{ kind: "source", path: draft.path (repo-relative `doc/...`),
  anchor: draft.anchor, lines: draft.lines && [start, end], stage: draft.stageId, summary_sentence:
  null, rests_on: [], quote: draft.text, selected_hash: draft.hash }`; summary arm `{ kind:
  "summary", path: "doc/loom/summaries/" + draft.summary + ".md", anchor: draft.sentence,
  summary_sentence: draft.sentence, rests_on: draft.restsOn.map(r => r.path + "#" + r.anchor)
  (repo-relative), quote: draft.text, lines: null, stage: null, selected_hash: draft.hash }`.
- `src/plan/review/live.ts` (yours; codex unit C5's `review/api.ts` stays fetch-only):
  `openQuestionsAtom` (counts by repo-relative path) and `hashesAtom` (plan 9's `/api/hashes`
  answer), seeded with C5's `getOpenQuestions()` and `getHashes()`; `useReviewLive()` (W2's
  `ChatRegistrations` calls it once) subscribes to `subscribe("review", m => m.kind ===
  "open_questions" ...)` to replace the counts, and in plan 2's `onResync` callback refetches both
  counts and hashes. W6 reads both atoms; nothing else fetches them.
- Tests: "send while closed rejects and keeps the draft"; "drop before result rejects and keeps
  the client_id unconfirmed"; "snapshot confirms an unconfirmed send"; "two pending permissions oldest first";
  "queue_returned ignored without a pending interrupt"; "toAttachmentInput source and summary
  arms" (in `model.test.ts` or `commands` tests); and in `src/plan/review/live.test.ts` (yours):
  "open_questions arrives as a review kind"; "resync refetches counts and hashes". The storage and
  visible-control tests are codex units C3 and C4.
