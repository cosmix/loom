# W6 (bridge-runtime): server wiring, chat commands, end-to-end tests

Read `doc/plans/briefs/plan-session-bridge/common.md` first (the Bridge API, the wire enums,
"Guidance seam"), then the plan 2 entries it quotes. You run in wave 2, after W4 returned, beside
W5. `TmuxHost` is W5's; until W5's files compile, build failures inside W5's territory are not
yours: wait and rebuild, never edit them.

## Files you own (write only these)

- `loom/src/plan_server/chat/mod.rs` (`pub fn install(ctx: &Arc<PlanContext>)`,
  `pub(crate) fn install_with(ctx, host: Arc<dyn SessionHost>, config_overrides)` for tests)
- `loom/src/plan_server/chat/dispatch.rs` (`pub fn dispatch(ctx: &Arc<PlanContext>, conn: &ConnCtx, cmd: ChatCommand)`)
(Codex units own `chat/lease.rs` (C14), `chat/guidance.rs` (C18), `chat/pins.rs` (C19) and
`chat/tests_dispatch.rs` (C20), see `codex-units.md` in this directory; steps 1, 2 and the
dispatch tests of step 5 are their specification. The seed declared `lease`, `guidance`,
`pins`, `tests` and `tests_dispatch` in `chat/mod.rs`; keep those lines.)
- `loom/src/plan_server/chat/tests.rs`
- `loom/src/plan_server/wire/message.rs`: ONE variant each, `Chat(crate::plan_server::wire::chat::ChatFrame)`
  in `PlanMessage` and `Chat(crate::plan_server::wire::chat::ChatCommand)` in `ClientMessage`
- `loom/src/plan_server/ws/dispatch.rs`: ONE arm, `ClientMessage::Chat(cmd) => crate::plan_server::chat::dispatch(ctx, conn, cmd),`
- `loom/src/plan_server/routes.rs`: ONE line in `install_all()` calling `crate::plan_server::chat::install(ctx)`;
  and `pub mod chat;` wherever plan 2 declares route modules (`plan_server/mod.rs`; one line)
- `loom/tests/plan_session_bridge_e2e.rs` (the operator e2e and the recovery test)

Read each plan 2 file before editing it; if a published name differs, follow the tree and report.

## Steps

1. `guidance.rs`: as common.md "Guidance seam". `FixedGuidance` writes through `safe_fs`.
2. `pins.rs`: `struct TextPins { store: Arc<TextStore>, ctx: Weak<PlanContext> }`; `begin_turn(key)` -> `store.begin_turn(&TurnId(key.into()))`;
   `end_turn(key)` -> `store.end_turn(&TurnId(key.into()), ctx.root(), &ctx.project())` (upgrade the `Weak`; a dropped
   context returns no changes), mapping `TurnChange { path, before_hash, .. }` to `(path as repo-relative string, before_hash)`.
   When `ctx.extensions().get::<TextStore>()` is `None` (plan 3 absent in a test), pass `None` to the bridge.
3. `mod.rs::install`: `BridgeConfig` from the context (`root()`, `runtime()`, `loom_bin` =
   `std::env::current_exe()` canonicalised, `claude_bin` = `crate::claude::find_claude_path()`
   (skip and leave the bridge refusing starts when it fails: read-only mode already explains it),
   `home` = `std::env::var_os("HOME")` (unset or empty: the bridge refuses starts with "HOME is not set"), `closing_message` = a closure over the
   `GuidanceSlot`, the default durations); `Bridge::new(config, Arc::new(TmuxHost), pins)`; store
   `Arc<Bridge>` and `Arc<GuidanceSlot>` (with `FixedGuidance`) in `ctx.extensions()`;
   `bridge.run(stop)`; `bridge.subscribe_chat` -> `ctx.hub().broadcast(&PlanMessage::Chat(ChatFrame::Delta(delta)))`
   except `Queue` deltas, which go to everyone with `text: None` and to `send_to_holder` with the
   texts; `ctx.hub().on_open(|conn| vec![PlanMessage::Chat(ChatFrame::Snapshot(bridge.chat_snapshot(ctx.authoring().is_holder(&conn.browser))))])`;
   store `Arc<AuthoringLeases>` in `ctx.extensions()`;
   `ctx.authoring().subscribe(..)`: `Freed { reason: GraceExpired }` -> when the chat session is an
   `Authoring` session, `bridge.end(<chat id>, FinalWrite, EndCause::GraceExpired)` (never for a
   pressure session: plan 11 decides that);
   `bridge.subscribe(..)`: `Ended` of an `Authoring` session ->
   `ctx.authoring().release_if(&<starter PublicId recorded at Start>, Released("chat ended"))`
   unless `AuthoringLeases::held()` (a summary or pressure session's end never releases),
   `Started`/`Ended` -> one terminal line each through `lifecycle::line` with `lifecycle::safe` on
   every interpolated value: `session started: <kind> "<name>" (watch: tmux -L <socket> attach -r)`
   and `session ended: <kind> "<name>" (<outcome>)`; `ctx.shutdown().register(ShutdownHook { name: "chat sessions", drain: bridge.drain(), abort: bridge.abort() })` (`drain` ends with `EndCause::Draining`);
   `Ended { Failed }` whose failure is a failed teardown prints `teardown failed: session <id>, socket <socket>`;
   then `bridge.recover()` on its own thread, printing one line per `left_alone` entry
   (`left alone: session <id>, socket <socket>: <why>`).
4. `dispatch.rs` (every refusal answers `ChatFrame::Result { ok: false, error }` to that
   connection only, via `send_to_conn`; success answers `ok: true`):
   - `Start`/`Send`: `ctx.is_stopping()` -> refused; `ctx.read_only().is_read_only()` -> refused
     (`Start` only; a `Send` with a running chat follows the holder rule); `ctx.require_authoring(&conn.browser, Take::TakeIfFree)`;
     at most `MAX_ATTACHMENTS`; each `attachments::resolve(&ctx.project(), input)`;
     `input::check_send(text)`; `formatted = input::sanitize(&attachments::format_message(text, &resolved))`;
     `input::check_delivered(&formatted)`. `Start` with no chat running: `bridge.start_session(StartRequest { kind: Authoring, guidance: <GuidanceSlot provider's authoring(ctx, dir, offer, first_message)>, opening_origin, offer, extra_env: vec![] })`
     with `first_message` = the FORMATTED text (attachments included, sanitised, checked) or
     `None`, and `opening_origin` = `Some(Origin::Offer)` with an offer, `None` with a first message,
     `Some(Origin::Start)` with neither (New chat; px-seams-07). The bridge delivers the provider's
     `opening_message` by paste after `SessionStart` (never on the launch line). `Send` with a
     chat running: `bridge.send(id, formatted, resolved)`; with none running it acts as `Start`
     with that text.
   - `Withdraw`, `Permission`, `Interrupt`, `End`: `ctx.require_authoring(&conn.browser, Take::HolderOnly)` first. A refusal's
     error text is the `Refusal`'s `Display` (it names the holder).
     `Interrupt` sends the returned texts as `ChatFrame::QueueReturned { texts }` through
     `ctx.hub().send_to_conn(conn.id, ..)` only (the tab that pressed Stop; the other holder tabs
     see the bridge's notice item; px-state-23).
     `End` -> `bridge.end(id, FinalWrite, EndCause::Holder)`, and its returned queue goes back to
     the same connection as `QueueReturned`. Send/Withdraw refused by the bridge during Ending
     answer `Result { ok: false, error: "the chat is closing" }`.
5. Tests (`tests.rs`, `tests_dispatch.rs`) through `install_with` and a fake host (copy the shape
   of W4's `fake_host.rs` into your test module; do not import a `cfg(test)` item across modules
   unless W4 made it `pub(crate)`): a read-only browser's `Send`, `Permission`, `Interrupt`, `End`
   refused while someone holds; a `Start` while nobody holds takes authoring; a slash command and
   an over-100 KB message refused with the `InputRefusal` text; 11 attachments refused; an
   attachment with `..` refused; `Start` refused in read-only mode and while stopping; a snapshot
   on open hides queue texts from a non-holder and shows them to the holder; `GraceExpired` ends
   the chat with a final write; the shutdown hook's drain ends the chat; a session `Ended` with `Failed` releases authoring; a pressure session's `Ended` does not, and an
   `Authoring` session's `Ended` does not while an `AuthoringLease` is held (the holder is unchanged
   afterwards); `GraceExpired` with a pressure session running ends nothing; New chat (no offer, no
   text) starts with `opening_origin: Some(Start)`; `Start` with a typed first message and one
   attachment passes the formatted block (with `<loom-attachments>`) as `first_message`;
   `QueueReturned` reaches only the interrupting connection; browser B that takes authoring during
   a grace-expired chat's Ending still holds after that chat's `Ended`; a teardown failure prints its line;
   the session-start terminal line names the socket with `tmux -L <socket> attach -r` and passes a
   display name holding an escape sequence through `lifecycle::safe`; `queue_returned` reaches
   only the holder's connections.
6. `loom/tests/plan_session_bridge_e2e.rs` (integration test; uses only `pub` items:
   `loom::plan_session::{bridge::*, tmux_host::TmuxHost, kind::*}`; spawn nothing through
   `Command::new(env!(..))`):
   - `recovery_closes_a_live_session_and_leaves_an_unverifiable_one` (not ignored; skipped through
     `skip_unless` when tmux is missing or a Unix socket cannot be bound, with `TMUX_TMPDIR` and
     `LOOM_HOME` pinned to temp dirs, `#[serial]`): bridge A starts an authoring session whose
     `claude_bin` is a fake script that `exec sleep 600`; A's driver is stopped and A dropped
     without teardown (the server "died"); bridge B (`final_write_bound` 2 s) `recover()`s:
     the session is in `closed` (closing text pasted, bound reached, torn down: socket gone,
     directory removed); the skip goes through `skip_unless`, so the operator run with
     `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` fails instead of skipping; a second record written by hand with `start_time: null` for a live pid is
     in `left_alone` with its socket, its directory untouched.
   - `operator_end_to_end_with_claude_code` (`#[ignore]`, run by the operator with a logged-in
     Claude Code, through the integration-verify block's `--include-ignored` command): in a temp
     git repository with `doc/product/index.md`, start an authoring session with real
     `claude_bin` and an opening message, wait for Idle (the opening arrived by paste: a user item
     with `origin: offer` and `/proc/<pid>/cmdline` of the claude process does not contain it),
     send a message, send a second while it works (queued, then delivered), ask the session to run
     `date` so a `PermissionRequest` arrives and answer it Allow, interrupt a third turn and assert
     a `TurnEnded` for it whether or not a Stop followed (print which), send `/compact` and assert
     its delivery is confirmed and the session returns to Idle (print the observed event order),
     ask it to write a file under Claude Code's temporary directory (print `$TMPDIR` as the session
     sees it, for plan 6's temp-dir rule), then `end(FinalWrite)` and wait for Ended. Print each
     state change, each hook event's payload keys (for the field names common.md assumes) and the
     item counts; assert the queue drained, the permission item `allowed`, a `notice` of kind
     `interrupted`, a user item with `origin: closing`, and the session directory removed.
     Then (px-seams-R04) open a second session whose opening is a PASTED plugin slash command:
     the test's plugin dir holds `commands/probe.md` (body: "Reply with the single word
     PROBE-OK."), the opening is `/loom-plan:probe doc/plans/PLAN-x.md`; assert the transcript
     shows the command's expansion and an assistant text containing PROBE-OK, and print
     "pasted slash command: ran|not run" (plan 11 picks `opening_on_argv` from this line; when it
     did not run, repeat with `opening_on_argv: true` and assert that run works).
     A missing `claude` or login is a panic, never a skip.

## Proof

`cargo test --lib plan_server::chat::`, `cargo test --test plan_session_bridge_e2e` (the ignored
test does not run) and `cargo clippy --all-targets -- -D warnings` from `loom/`. Report anything
surprising (and `loom memory note` it).
