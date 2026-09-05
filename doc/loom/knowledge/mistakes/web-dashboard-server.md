# Web Dashboard Server (loom status --web)

> Concurrency, security and testing lessons from building the hand-rolled HTTP/WebSocket
> server under `loom/src/commands/status/web/` and its React frontend. See
> [architecture/web-dashboard.md](../architecture/web-dashboard.md) for the shape of the
> system these fixes apply to.

## clippy::never_loop Caught a Real Drain Bug No Test Could (2026-09-04)

**What happened:** `loom/src/commands/status/web/ws.rs`'s `send_pending()` was meant to
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
(never moving while a connection stayed live); `web/src/components/connection-badge.tsx`
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
inherited CWD; `status::web::execute` (`loom/src/commands/status/web/mod.rs:41-42`) bails
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
