# Web Dashboard Server (loom status --web)

> Races
> server under `loom/src/commands/status/web/` and its React frontend. See
> [architecture/web-dashboard.md](../architecture/web-dashboard.md) for the shape of the
> system these fixes apply to.

## clippy::never_loop Caught a Real Drain Bug No Test Could (2026-09-04)

**What happened:** `loom/src/web_server/ws.rs`'s `send_pending()` was meant to
drain the whole frame queue but its match arms returned on the FIRST iteration
(`Ok(frame) if send fails => return false`, `Ok(_) | Err(Empty) => return true`),
conflating "sent one frame" with "queue empty". A client that fell behind drained its
backlog at ~1 frame per 250ms read-timeout tick instead of catching up at once.

**Why:** every test asserted that A frame arrives, never that N queued frames all arrive.

**Prevention:** `clippy::never_loop` on a function named drain/flush/pump/consume_all is a
behaviour bug, never a lint to `#[allow]`. Give each outcome (`Ok(frame)` continues,
`Err(Empty)` returns true, `Err(Disconnected)` returns false) its own arm.

## Two Concurrency Defects a Test Suite Cannot Reach (2026-09-04)

**What happened:** (1) `subscribe()` built an unbounded `mpsc::channel()`, and `publish()`
only drops a subscriber when `send()` fails, which never happens on an unbounded channel
while the receiver lives — and the receiver lives forever if a peer stops reading (no
`set_write_timeout`), so a sleeping tab accumulates every snapshot for the process
lifetime. (2) `subscribe()` read `latest()` and sent frame N, THEN took the subscribers
lock and registered — a `publish()` landing in that window delivers frame N+1 to a list
that doesn't yet include the new client; downstream dedup on `last_body` means if the tree
then stops changing, no further frame is ever sent and the page is stuck stale.

**Prevention:** bound the channel (`sync_channel(N)`, `try_send`, drop on `Full`) and add a
write timeout; take the subscribers lock BEFORE reading `latest` so send-then-register is
atomic. When a "send current state then register" pair spans two locks, the gap is a lost
update, and dedup downstream turns a one-frame glitch into a permanent one.

## A Field Named `since` Meant Two Different Things Across a Worker Boundary (2026-09-04)

**What happened:** `web/src/api/ws.ts` stamped `connectionAtom.since` only on a PHASE change
(never moving while a connection stayed live); `web/src/components/header-lines.tsx`
rendered it as "last frame N ago", so a healthy socket delivering a frame every second showed
an age that climbed forever. Both sides type-checked; parallel workers never saw each other's
code.

**Prevention:** a field crossing a worker boundary needs its units AND meaning pinned in the
brief ("since = ms epoch of the last PHASE CHANGE, not of the last frame") — a number named
`since` passes typecheck against any interpretation. Fix: derive frame age from the
snapshot's own `generated_at` when live, `since` only for phase age.

## Storing `globalThis.fetch`/`setTimeout` as Instance Fields Broke in a Real Browser (2026-09-04)

**What happened:** `web/src/api/ws.ts` stored `globalThis.fetch` and `globalThis.setTimeout`
as instance fields and called them as `this.request(...)`/`this.schedule(...)`. Chrome
throws `Illegal invocation` for both, because they require the original receiver — but
jsdom's mocks don't enforce that, so the test suite passed while every real fetch and
reconnect failed.

**Prevention:** bind (`globalThis.fetch.bind(globalThis)`) or call through a closure. A
DOM-native method stored as a plain reference and invoked as `this.method()` is a receiver
mismatch jsdom will not catch — verify browser API wrappers against a real browser, not just
jsdom.

## A Second Copy of a Status Table, Pinned to Literals, Diverged Silently (fixed in-stage; 2026-09-04)

**What happened:** the dashboard kept two status-metadata tables — the live one in
`web/src/lib/states.ts` (reads the Rust-pinned fixture) and a dead `STATE_META`/`LEGEND`
table in `web/src/lib/format.ts` whose only consumer test (`web/src/lib/format.test.ts`)
asserted against hardcoded literals, not the shared source.

**Prevention:** a test that pins a copy against literals rather than the shared source makes
divergence look verified — worse than no test. Delete the dead copy rather than leaving a
second source of truth nobody reads.

## Two Negated Skip Predicates Over the Same Probe Made Two Suites Mutually Exclusive (2026-09-04)

**What happened:** `loom/src/commands/status/web/tests/socket.rs` had one test set skip
unless `assets::WEB_ASSETS.is_empty()` and another skip unless it was non-empty. Deleting
`web/dist` silently dropped all real-page coverage while the suite stayed green.

**Prevention:** if two skip predicates over the same probe are negations of each other, no
single configuration runs both. Assert the build fact once and unit-test the degraded branch
on a pure function instead.

## Bare Package Specifier Inside a CSS `url()` Needed Verification, Not Trust (2026-09-04)

**What happened:** a plan correction named `@fontsource-variable/inter/latin.css`, which the
installed package does not ship (only `index.css`, `wght.css`, etc). Substituted an explicit
`@font-face` pointing at the one `.woff2` file that exists. Open risk that acceptance did not
directly test: Vite resolving a bare specifier inside a CSS `url()` is not guaranteed the way
a `./`-prefixed path is — verify at `bun run build`, not by reading the CSS.

## A CI Smoke Script Depended on Ambient `.loom/work` (2026-09-04)

**What happened:** `scripts/smoke-web-dashboard.sh` launched `loom status --web 0` in its
inherited CWD; `status::web::execute` (`loom/src/commands/status/web/mod.rs`) bails
with "does not exist. Run loom init first" when `./.loom/work` is absent. Every
local/worktree run has that symlink, so the script passed everywhere except a clean CI
checkout, where `.loom/work` is gitignored and never exists.

**Prevention:** any script a CI job runs must be tested from a genuinely fresh checkout
(`mkdir $TMPDIR/x && cd $TMPDIR/x && git init -q && <script>`) — that is the only thing that
distinguishes "works" from "works because my working copy has state CI will not have."
Fixed by having the script build its own scratch workspace (`mktemp -d`, create
`.loom/work` there, run from it).

## React Flow Dropped Every Edge Because Rebuilt Nodes Carried No `measured` (2026-09-05)

**What happened:** the graph view rebuilds React Flow's `nodes` array from the snapshot on
every frame (`buildNodes`, `web/src/components/graph/stage-graph.tsx`). `adoptUserNodes`
(`@xyflow/system` 0.0.82) rebuilds the internal node for any object it has not seen by
reference, and its `parseHandles` keeps the measured handle bounds only when the incoming
node carries `measured`. Ours carried `width`/`height` but no `measured`, so every frame
wiped every node's handle bounds; `getEdgePosition` returns null without them, so all edges
dropped until a ResizeObserver round-trip restored them. When a frame landed inside that
round-trip, `useNodeObserver`'s `isInitialized` read false on two consecutive renders, its
effect (deps `[isInitialized, node.hidden]`) did not re-run, the observed element's size had
not changed, and nothing re-measured: the edges stayed gone until a card resized. Reported as
"occasionally, after the dashboard has been open a while, the edges disappear".

**Why:** rebuilding a prop every frame looks free for a controlled component, but React Flow
keeps per-node measurement state keyed by object identity, and hands it back only to an
object that declares it was measured.

**Prevention:** when a library keeps measured state for objects you hand it, either keep the
object identity stable across frames or declare the measurement on the object you pass.
`web/src/lib/graph.ts` already lays each card out at exactly the size it renders at, so
`measured: { width, height }` costs nothing and makes the graph independent of the observer.

**Fix:** `measured` on both node kinds in `buildNodes`. The regression test
(`web/src/components/graph/stage-graph.test.ts`) drives `adoptUserNodes` twice with freshly
built arrays and asserts the handle bounds survive — a jsdom render cannot catch this, since
the inert `ResizeObserver` stub never measures at all and a synchronous stub always recovers
before the assertion, so the test pins the library contract instead of the DOM.

## A Hover Focus Outlived the Chip That Set It (2026-09-05)

**What happened:** `StateKey` renders one chip per state present in the plan. A chip whose
last stage leaves that state unmounts under the pointer and never fires its `onMouseLeave`,
so `Canvas`'s `hovered` focus stayed on a state no stage was in. `tracedIds` returned an
empty set, which dims everything: nodes to 0.3 opacity, edges to 0.18 (`web/src/graph.css`).
That also reads as the edges disappearing.

**Why:** the focus was validated for one of the two shapes it can take — a pinned stage id
was checked against the snapshot, a status was not.

**Prevention:** state naming something in the data (an id, a status) needs re-validating
against each new snapshot, for every shape it can take, not just the shape whose staleness
was noticed first.

**Fix:** the status branch of `tracedIds` returns null when no stage has that status, the
same as the stage branch already did.

## A push-fed page read liveness out of the payload, not the transport

**What happened:** The dashboard header rendered `daemonLine(snapshot.daemon, snapshot.tick_age_secs)` straight off `snapshotAtom`. When the feed stopped — the serving process gone, not just the daemon — React kept the last frame, so the page claimed `daemon running · tick 3s ago` for as long as the tab stayed open, and the badge sat on `reconnecting` forever because `ConnectionState.since` was re-stamped on every retry.

**Why:** Every liveness field on the wire (`daemon`, `tick_age_secs`, `generated_at`) is a statement about the moment the frame was generated. A frame is evidence of the past; only the socket is evidence of the present. Nothing in the payload can report its own staleness.

**Prevention:** In a push-fed UI, derive "is this current?" from the transport state (`connectionAtom.phase`), never from the frame's contents. Frame AGE is also the wrong signal here — `loom/src/commands/status/web/broadcast.rs` suppresses unchanged frames, so a quiet live feed looks old while being perfectly current. Model an outage as one event with one start time held across retries, or every reconnect resets the age the operator is reading.

**Fix:** `daemonLine` took a third `staleSecs` argument that wins over every daemon state (`web/src/lib/format.ts`); `DaemonLine` computes it from the connection phase — stale for `reconnecting`/`offline`/`error`, not for `connecting` (the initial `/api/status` fetch lands during that phase). `ws.ts` preserves the outage `since` across retries and flips to a new `offline` phase after the fourth consecutive failure. `web/src/routes/shell.test.tsx` pins the wiring at the rendered-page level, which is the layer the unit tests could not reach: `daemonLine` and the socket state machine were both individually correct while the page was wrong.

## An Integration-Verify Reviewer Cannot See the Plan, So It Confidently Reports Settled Decisions as Defects (2026-09-09)

**What happened:** two of three independent code reviewers flagged the terminal bridge's
`Resize` handling in view mode ("should not resize the agent's real PTY") as a bug. It was a
documented, deliberate plan decision — `-f read-only` blocks input only, and `ignore-size` was
explicitly considered and rejected because it would force the agent's real column count into
the browser's well. A third reviewer, working from the same tree-only vantage point, correctly
found a real defect (the front-truncation issue above) that looked structurally identical in
its report: "this design choice looks wrong, here is why." Nothing in a diff-and-code review
distinguishes "the reviewer hasn't seen the rationale" from "the rationale doesn't exist."

**Why:** an integration-verify reviewer reasons from the tree alone; it has no channel to the
plan document that recorded _why_ a behaviour is the way it is.

**Prevention:** before briefing a fix for any review finding that would change a deliberate
behaviour, grep the plan's decisions table for the same words. When two reviewers disagree
about whether something is a defect, or a finding contradicts an earlier settled decision, halt
before briefing an implementer — do not let a "fix" get written (or, worse, a test that PINS
the wrong behaviour as correct) before the contradiction is resolved.

## `SessionType::Contract` Reached The Wire Before `schema.ts` Knew About It

**What happened:** the `Contract` session type (the v2 contract-phase writer) was added on the
Rust side without adding `"contract"` to `web/src/api/schema.ts`'s `sessionTypeSchema` zod enum.
Every status frame carrying a contract-writer stage then failed zod parsing, so the dashboard
rejected the WHOLE snapshot and showed "connection error" — not a missing tag on one stage, no
page at all.

**Why:** nothing tied the two together. `cargo build`/`cargo test` for `loom/src/models/session`
has no dependency on a TypeScript file, so the new variant compiled and every Rust-side test
passed while the TS schema went stale.

**Prevention:** `loom/src/commands/status/web/schema_parity.rs` — one test per status-relevant
enum (`SessionType`, `StageStatus`, `StageType`, `FailureType`, `SessionExitReason`), each opening
with an exhaustive `match` over a placeholder value and no wildcard arm, so adding, removing, or
renaming a variant fails that match to compile until the hand-maintained list right below it is
updated; the test then asserts every serialized wire value appears as a quoted string literal in
`schema.ts`. A future enum gaining status-relevant wire exposure needs the same treatment here.

**Fix:** `"contract"` added to `sessionTypeSchema` in `web/src/api/schema.ts`;
`schema_parity.rs` added so the next such omission also fails `cargo test`.

## Empty-Response (408) Candidates in the Web Server's Connection Path (2026-10-06)

**What happened:** while tracing a dashboard connection that closed with no response, the paths that return
nothing were: `peek_head` returning `Ok(0)` and its catch-all `Err` arm (`ErrorKind::Interrupted`, EINTR under
`SO_RCVTIMEO`); `complete()` returning `None` when running clears; `gate()` returning `None` when
`set_read_timeout` or `set_write_timeout` fails; and `spawn_connection` dropping the stream when
`set_nonblocking`, `local_addr` or the thread spawn fails.

**Prevention:** every early return on a connection path answers the client or has a reason to stay silent;
retry EINTR on a peek. `web_server/head.rs` retries the EINTR case and `unserved.rs` answers 503 when
the spawn fails; the other candidates stay as listed.

## Stage Bands Wore State Tones by Rank

**What happened:** The web Token flow chart coloured each stage band with a `--tone-*` state colour picked by the stage's index in `flow.stages`. The first stage in id order always wore `--tone-executing`, the blue the ledger and graph give the running stage, so a finished stage's tokens read as the current stage's. A stage that sorts earlier joining the flow repainted every later stage.

**Why:** The palette reused the state tones to avoid adding colour tokens, and the colour followed rank, not the stage.

**Prevention:** Chart series take identity colours apart from the state tones, and a series keeps its colour when the set of series changes. Validate a categorical palette with the dataviz validator against the card surface in both themes.

**Fix:** `--series-1..6` in `web/src/index.css`, assigned in `token-flow-panel.tsx` `stageColors` by the stage's place among all plan stage ids in id order. Test: `token-flow-panel.test.tsx` "keeps a stage's colour as other stages join the flow".

## Header icon review scope

**What happened**: A request to review other icons referred specifically to the header.
**Why**: The surrounding conversation concerned the header attention indicator.
**Prevention**: Keep follow-up visual reviews within the established component unless broader scope is requested.
**Fix**: Limit the icon updates to the header rails and controls.

## Model pie keeps effort totals combined

**What happened**: The requested model colour review initially included separate effort shades; the user clarified that efforts should remain combined.
**Why**: The confusing colours belong to different models sharing nearby state tones.
**Prevention**: Keep one slice per model and maximise hue separation across the model set.
**Fix**: Use evenly spaced model hues for both pie slices and legend swatches.

## CSS colour assertions allow browser normalization

**What happened**: A model pie swatch assertion compared a raw SVG colour with a CSS serialized colour and failed on decimal rounding.
**Why**: CSS serialization rounds hue values while SVG attributes retain the source string.
**Prevention**: Normalize expected colours through the same CSS API before comparing.
**Fix**: Compare swatches with a CSS-normalized expected background.

## Commit task memory with the implementation

**What happened**: Task memory notes and their generated index update remained uncommitted after the implementation commits.
**Why**: Shared knowledge changes were excluded instead of separating the task-owned edits.
**Prevention**: Include task-owned memory and index hunks in the requested commit scope while preserving unrelated changes.
**Fix**: Commit the dashboard notes with only their matching index row.

## Testing Library role options differ from Playwright

**What happened:** A help-dialog test passed an unsupported `exact` option to Testing Library `getByRole`; the TypeScript build rejected it even though Vitest passed.

**Why:** Playwright locator options were carried into a Testing Library test.

**Prevention:** Use a string `name` for exact accessible-name matching in Testing Library; reserve `exact` for APIs that support it.

**Fix:** Remove the unsupported option and run the frontend type check and build.

## Help illustrations must match production UI

**What happened:** The first help illustrations recreated stage cards, terminal controls, and resource meters with approximate graphics; the operator rejected their appearance and a decorative sidebar tagline.
**Why:** Explanatory visuals were implemented independently of the components they described.
**Prevention:** Use production components with isolated sample state, local interaction handlers, and utility copy. Inspect the real UI before composing examples.
**Fix:** Render StageNode, React Flow controls, ModeKey, ContextMeter, NodeTokens, TokenFlowPanel, and QuotaMeters in the help guide.

## A Half-Open Status Socket Froze the Dashboard With a Live Badge (2026-10-22)

**What happened:** a dashboard tab viewed over the LAN (`loom status --web --host 0.0.0.0`, through a router) kept a stage on `writing contract tests`/executing after it was blocked and later merged, with no terminal and the connection badge on `live`; a reload showed the truth. The server was publishing correct frames and held no connection for that tab: the socket was dead on the server and still open in the browser.
**Why:** the status socket had no liveness check on either side. The server writes only when `Broadcaster::publish` sees a changed frame, so silence is normal; the client reacted only to `onclose`/`onerror`, which a half-open TCP connection (sleeping device, dropped NAT mapping) never fires. The rule in "A push-fed page read liveness out of the payload" (trust the socket) assumes the socket reports its own death; it does not.
**Prevention:** a push feed whose server suppresses unchanged frames needs an application-level heartbeat the client can observe (browser JS cannot see protocol pings), and the client must treat a missed heartbeat as an outage.
**Fix:** `web_server/ws.rs` resends the socket's last frame after 10 s of silence; `web/src/api/ws.ts` reconnects after 30 s without any message (checked every 5 s and on `visibilitychange`), and `applySnapshot` skips a frame whose `generated_at` is not newer, so a keepalive does not re-render. A frame republished with a notice is restamped for the same reason.
