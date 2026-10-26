# Codex units of stage bridge-runtime

Forwarding, rules and reading order as in `doc/plans/briefs/plan-session-bridge/bridge-core/codex-units.md`
(FOREGROUND `loom-codex-forwarder`, `--model gpt-6.1-terra --effort high`, Bash timeout 600000 ms,
compile checks only, no git, 10% size margin). Every unit reads
`doc/plans/briefs/plan-session-bridge/common.md` first. Codex cannot run the tmux tests; the
orchestrator runs every proof command.

## C14 authoring leases (wave 1)

- Owns: `loom/src/plan_server/chat/lease.rs` (the seed declared it in `plan_server/chat/mod.rs`).
- Entry points: `AuthoringLeases` (`Default`), `AuthoringLeases::hold(&self) -> AuthoringLease`,
  `AuthoringLeases::held(&self) -> bool`; `AuthoringLease` releases its hold on drop. A counter
  behind an `Arc<AtomicUsize>` is enough.
- Steps: 1. the types; 2. inline tests: held after one hold, still held with two and one dropped,
  not held after both drop; 3. compile and run.
- Done: `cargo test --lib plan_server::chat::lease` passes.

## C15 launch script (wave 2, after W4)

- Owns: `loom/src/plan_session/launch/script.rs` (declared from `launch.rs` by the seed).
- Reads: `loom/src/plan_session/bridge/host.rs` (W4, this stage: `LaunchSpec` with
  `opening_argv: Option<String>`), `loom/src/orchestrator/terminal/tmux/mod.rs` (the
  `shell_escape::escape(..).into_owned()` idiom near `new_session_argv`).
- Entry points: `pub fn launch_script(spec: &LaunchSpec, settings: &Path, env: &[(OsString, OsString)]) -> String`,
  exactly the script of `bridge-runtime/w5-launch-tmux.md` step 1 (no positional prompt unless
  `spec.opening_argv` is Some, then it is the last escaped word).
- Steps: 1. the function; 2. inline tests: every flag present once, `--permission-mode default`,
  no `--resume`, `LOOM_PLAN_SESSION` and `LOOM_PLAN_STATE` present, `LOOM_STAGE_ID`,
  `LOOM_SESSION_ID`, `LOOM_WORK_DIR` absent even when passed in `env`, a display name with a single
  quote, a dollar sign and a newline escaped, the opening text absent when `opening_argv` is None;
  3. run `sh -n` on the produced text inside a test (write it to a temp file).
- Done: `cargo test --lib plan_session::launch::script` passes.

## C16 tmux host (wave 3, after W5)

- Owns: `loom/src/plan_session/tmux_host.rs` (seeded).
- Reads: `loom/src/plan_session/launch.rs` and `loom/src/plan_session/tmux_io.rs` (W5, this
  stage: `launch::launch(spec) -> Result<Spawned>`, `tmux_io::{paste_text, press}`),
  `loom/src/plan_session/bridge/host.rs` (W4: the `SessionHost` trait).
- Entry points: `pub struct TmuxHost;` and `impl SessionHost for TmuxHost` mapping spawn,
  deliver, interrupt (`press(socket, target, "Escape")`), teardown
  (`crate::orchestrator::terminal::tmux::teardown_socket`, widened to `pub(crate)` by W5) and
  identity (`crate::process::identity::verify_process_identity`).
- Steps: 1. the impl; 2. compile.
- Done: `cargo build` passes.

## C17 planning sockets skip (wave 2)

- Owns: `loom/src/orchestrator/terminal/tmux/socket.rs` (351 lines; stay under 400).
- Entry points: `list_loom_sockets`, the `view-` skip inside it, and the test
  `list_loom_sockets_ignores_the_overview_viewer_socket`.
- Steps: 1. beside the `view-` skip add `if session_id.starts_with("plan-") { continue; }` with a
  two-line comment (planning sockets belong to `loom plan`, which tracks them by its own session
  records); 2. one test `list_loom_sockets_ignores_planning_sockets` mirroring the viewer test with
  a `loom-plan-3f9a0c41b7e2` file; 3. compile.
- Done: `cargo test --lib orchestrator::terminal::tmux::socket` passes.

## C18 fixed guidance (wave 2, after W4)

- Owns: `loom/src/plan_server/chat/guidance.rs` (declared by the seed).
- Reads: `loom/src/plan_session/bridge/mod.rs` (W4: `LaunchGuidance`), `loom/src/plan_session/kind.rs`
  (`SessionDir`), `loom/src/fs/safe_fs.rs`.
- Entry points: `GuidanceProvider`, `GuidanceSlot::{new, get, set}`, `FixedGuidance`, exactly as
  common.md "Guidance seam" (brief text, minimal plugin.json, model `opus`, effort `medium`,
  display name `loom plan: chat`, opening = first message or `Start the step "<offer id>".`, a
  generic opening for neither, `opening_on_argv: false`, the fixed closing text).
- Steps: 1. the trait and slot; 2. `FixedGuidance` writing through `safe_fs`; 3. inline tests for
  the three opening cases and the files written (0600).
- Done: `cargo test --lib plan_server::chat::guidance` passes.

## C19 turn pins (wave 2)

- Owns: `loom/src/plan_server/chat/pins.rs` (declared by the seed).
- Reads: plan 3 `loom/src/plan_server/files/texts.rs` (`TextStore::{begin_turn, end_turn}`,
  `TurnId`, `TurnChange`), `loom/src/plan_session/bridge/mod.rs` (W4: `TurnPins`).
- Entry points: `pub struct TextPins { store: Arc<TextStore>, ctx: Weak<PlanContext> }`,
  `impl TurnPins for TextPins`: `begin_turn(key)` -> `store.begin_turn(&TurnId(key.into()))`,
  `end_turn(key)` -> `store.end_turn(&TurnId(key.into()), ctx.root(), &ctx.project())` mapped to
  `(repo-relative path string, before_hash)`; a dropped context returns no changes.
- Steps: 1. the impl; 2. compile.
- Done: `cargo build` passes.

## C20 dispatch tests (wave 3, after W6)

- Owns: `loom/src/plan_server/chat/tests_dispatch.rs` (declared by the seed in `chat/mod.rs`).
- Reads: `loom/src/plan_server/chat/mod.rs`, `dispatch.rs`, `tests.rs` (W6, this stage: the
  `install_with` helper and its fake host).
- Steps: 1. the refusal tests of `bridge-runtime/w6-server-chat.md` step 5 (read-only browser
  commands refused while someone holds, slash command and over-100 KB refused with the
  `InputRefusal` text, 11 attachments and a `..` path refused, Start refused in read-only mode and
  while stopping); 2. the start forms (offer gives `Offer`, typed message gives no origin and the
  formatted block as `first_message`, neither gives `Start`); 3. `QueueReturned` reaches only the
  interrupting connection.
- Done: `cargo test --lib plan_server::chat::tests_dispatch` passes.
