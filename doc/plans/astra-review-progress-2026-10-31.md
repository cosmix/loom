# Astra review repair progress

Source: [review](../astra-code-review-2026-10-31.md). Consult this ledger before assigning work. Implemented means the code and focused regression are present; integrated verification is pending. Partial means further acceptance work remains. All findings not listed below remain open and unassigned unless noted as active.

Earlier verification: the library suite compiled and ran 10,959 tests: 10,948 passed, six failed, and five were ignored. All six failures passed focused reruns after fixture and expectation corrections.

After fixture commits aa922ad2 and cad81d94, the user reports Linux all-target Rust (12,082 passed, 20 ignored), web (4,111 passed), hook (107 passed), installer (6 passed), and isolated pre-commit (6 passed) suites passed. Native BSD/macOS remains unverified. Individual acceptance requirements still require reconciliation.

Tally at 2026-11-01 06:39 UTC (227 indexed findings): 54 done, 24 in progress, 20 implemented with verification pending, 4 partial, and 125 unassigned. Scoped notes and policy decisions outside the index are excluded. This is the first timed baseline; later tallies can establish an observed completion rate.

Tally at 2026-11-01 06:50 UTC: 62 done, 25 in progress, 20 implemented with verification pending, 4 partial, and 116 unassigned. Eight findings reached done over roughly 11 minutes; the rate is preliminary and does not include final integration verification.

Tally at 2026-11-01 07:00:36 UTC: 76 done, 31 in progress, 20 implemented with verification pending, 4 partial, and 96 unassigned. Fourteen findings reached done since the previous tally; integration verification remains outstanding.

Tally at 2026-11-01 07:10:46 UTC: 84 done, 32 in progress, 20 implemented with verification pending, 5 partial, and 86 unassigned. Eight findings reached done since the previous tally; integration verification remains outstanding.

Tally at 2026-11-01 07:23:07 UTC: 100 done, 18 in progress, 20 implemented with verification pending, 7 partial, and 82 unassigned. Sixteen findings reached done since the previous tally. New assignments are stopped; full verification and commits remain outstanding.

Tally at 2026-11-01 08:31:48 UTC: 116 done, none in progress, 20 implemented with verification pending, 9 partial, and 82 unassigned. New assignments remain stopped. Full Rust verification is running; the web, hook, installer and pre-commit suites passed on Linux.

Tally at 2026-11-01 08:51:14 UTC: unchanged at 116 done, none in progress, 20 implemented with verification pending, 9 partial, and 82 unassigned. The first full Rust run failed four targets; targeted fixes and maintainability refactors have passed their focused checks, and a fresh full run is underway.

Tally at 2026-11-01 09:36:08 UTC: unchanged at 116 done, none in progress, 20 implemented with verification pending, 9 partial, and 82 unassigned. The full Rust run passed; a lint gate exposed additional issues which were repaired, with lint and full Rust verification being rerun. No new findings have been assigned.

Integration verification: Linux web tests passed 4,111/4,111, hooks 107/107, installer tests 15/15 and isolated pre-commit cases 6/6. After repairs, the final Rust all-target run passed 12,301 tests with 0 failures and 20 ignored across 92 targets; nine duplicate handoff test registrations were removed while retaining all unique cases. Rust doctests passed 6 with 7 ignored, Clippy and formatting passed, and web typecheck, lint and build passed. Native macOS verification is left to CI.

| Finding                                          | Status                                                                                                                                                                                                                                                 |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| RT-01                                            | Implemented; verification pending                                                                                                                                                                                                                      |
| RT-02, RT-03                                     | Done — resolver race and merge acceptance passed                                                                                                                                                                                                       |
| RT-12                                            | Done — filesystem and genuine restore regressions passed                                                                                                                                                                                               |
| RT-14                                            | Done — filesystem boundary comments corrected                                                                                                                                                                                                          |
| RT-15                                            | Done — filesystem work-directory suite passed                                                                                                                                                                                                          |
| RT-16                                            | Done — stage-service, socket and spool regressions passed                                                                                                                                                                                              |
| RT-17                                            | Done — filesystem and memory relay tests passed                                                                                                                                                                                                        |
| RT-13                                            | Done — filesystem suite passed; replacement sync failure handled                                                                                                                                                                                       |
| RT-18                                            | Done — ledger and repeated-restart tests passed                                                                                                                                                                                                        |
| RT-19                                            | Done — 14 completion-commit tests passed                                                                                                                                                                                                               |
| RT-20                                            | Done — compatibility facade deprecated; 16 tests passed                                                                                                                                                                                                |
| RT-04                                            | Done — merge classification and adapter regressions passed                                                                                                                                                                                             |
| RT-21                                            | Done — 129 permissions tests passed                                                                                                                                                                                                                    |
| RT-22                                            | Done — permission constant deduplicated and prose corrected                                                                                                                                                                                            |
| INT-01                                           | Done — five placement tests passed                                                                                                                                                                                                                     |
| INT-02                                           | Done — wrapper and Rust preflight tests passed                                                                                                                                                                                                         |
| INT-03                                           | Done — installation process ownership tests passed                                                                                                                                                                                                     |
| INT-04                                           | Partial — shell guard parity expanded; two focused hook scripts passed. Deprecated public Rust validator and legacy HooksConfig emitter remain; compatibility and migration still need assessment.                                                     |
| RT-27, RT-28                                     | Implemented; verification pending                                                                                                                                                                                                                      |
| VER-01 | Partial — exact selectors implemented; 165 adapter tests and live Dart and Bun collision checks passed. Flutter live execution and PHPUnit, Pest and Swift behavior remain unverified; RSpec is unavailable. |
| VER-02                                           | Done — policy binding, budget and expiry regressions passed                                                                                                                                                                                            |
| VER-04                                           | Implemented; verification pending                                                                                                                                                                                                                      |
| VER-05                                           | Done — scoped comment and diagnostic corrections                                                                                                                                                                                                       |
| VER-06                                           | Done — Markdown and frontmatter tests passed                                                                                                                                                                                                           |
| VER-03                                           | Done — test compilation, preserved v2 and freeze suites passed                                                                                                                                                                                         |
| VER-R01, VER-R02, VER-R03                        | Implemented; verification pending                                                                                                                                                                                                                      |
| CTX-01, CTX-02, CTX-03                           | Done — 1,018 context tests passed; one ignored                                                                                                                                                                                                         |
| CTX-04                                           | Done — language and project suites passed                                                                                                                                                                                                              |
| CTX-17 | Done — Unicode offsets repaired in both consumers; `cargo test unicode_ --lib` passed 7 tests. |
| VER-09 | Done — nonblocking bounded receipt reads; 16 locator tests passed including FIFO and symlink cases. |
| INT-09 | Done — guidance, direct-mode reasons, result parser and comments corrected; direct, result and wrapper hook scripts passed. |
| CON-B-02 | Done — admission failures retain retryability; post-read retry, service idempotency and permanent rejection regressions passed. |
| VER-08 | Done — stable state lock protects replacement and cleanup; 4 helper/release tests and 10 ordinary wait tests passed. |
| CTX-13 | Done — persistent quota lock inodes; 13 focused quota tests passed, including held and preopened lock cases. |
| CTX-20 | Done — spool-only current-stage query and shared enumeration; CLI regression passed. |
| RT-23 | Done — supported frontmatter mappings retain unknown values and body; unsupported values reject without mutation; 3 focused tests passed. |
| RT-05 | Done — persistence, shutdown, merge and monitoring comments corrected; scoped diff check passed. |
| CTX-05 | Done — graph-cache side-effect and concurrency comments corrected. |
| RT-37 | Done — stale waits revalidate session, revision and progress under lock; 12 focused tests passed. |
| RBR-16 | Done — bounded safe handoff reads; 31 generator tests passed, including special-file regressions. |
| VER-10 | Done — PlanPatch documentation describes the current persisted envelope. |
| VER-11 | Done — wiring comparisons use structural equality; 32 amendment tests passed. |
| RBR-10 | Done — missing dependency branch uses the configured target; 11 worktree-base tests passed. |
| RBR-06 | Done — rollback refuses occupied destinations and reports incomplete recovery; 42 focused tests passed. A failed write may leave a reported partial new file. |
| RBR-08 | Done — failed Git inspections refuse resolver readiness; 14 focused tests passed. |
| VER-14 | Done — malformed audit rows stop recovery and retry without writes; 12 retry and 3 recovery tests passed. |
| RBR-13 | Done — native window identity is project/session scoped and persisted; 45 Linux tests passed. Native macOS behavior awaits CI. |
| VER-21 | Done — review freshness uses exact source hashes with legacy evidence handling; 48 review tests passed. |
| CTX-31 | Done — canonical-path digest isolates equal-basename worktree overlays; 6 focused tests passed. |
| CTX-06 | Done — sibling function declarations retain distinct scopes and call ownership across JS, TS and TSX; focused regression passed. |
| CTX-27 | Done — bounded anchored delivery reads and publication; 36 delivery tests passed, including symlink refusal. |
| RBR-14 | Done — failed xdotool closes fall through to guarded PID teardown; 9 Linux tests passed. |
| CTX-12 | Done — bounded provider, requested-model and quota reads; 43 focused Linux tests passed. |
| CTX-15 | Done — telemetry uses the shared bounded spool; 14 focused Linux tests passed. |
| CTX-32 | Done — dirty overlays no longer advertise unproven OIDs; schema bumped and 41 refresh tests passed. |
| VER-20 | Done — incomplete probe output cannot certify contract or impact success; focused completion, impact and truncation checks passed. |
| VER-23 | Done — dependency evaluation and cache use typed outcomes; 28 dependency tests passed. |
| RT-24 | Done — shared fence scanner guards retrieval and edits; focused scanner, splice, chunker, reference, directory and planning tests passed. Successful edits retain pre-existing CRLF normalization. CON-B-08 is a duplicate. |
| CTX-18 | Done — deterministic first-valid skill ownership and shared loading; 37 focused tests passed. |
| VER-15 | Done — foreground startup errors cancel and join Codex; focused lifecycle, fallback and terminal tests passed. |
| CTX-33 | Done — anchored Rust paths resolve within the citing crate; resolver and qualified-call suites passed. |
| CTX-34 | Done — discarding an overlay clears its in-memory fallback; 26 graph-store tests passed. |
| RT-34 | Done — locked crash transitions recheck ownership and retry failures; 7 identity tests passed. |
| VER-19 | Done — fingerprint reads enforce the admitted byte budget; 5 focused tests passed. |
| CTX-35 | Done — JSON map identities retain raw IDs and paths; focused round-trip and window tests passed. |
| CTX-36 | Done — impact descriptions and kind labels reflect filtering and traversal; focused tests passed. |
| INT-08 | Partial — typed actions preserve the public diagnostic API and 59 repair tests pass; direct coverage of several host-dependent detector kinds remains incomplete. |
| RT-38 | Done — handoff watch retries uncertain reads and refreshes once per pass; 13 watch and 8 detection tests passed. |
| CTX-24 | Done — comparisons preserve provider/model/effort associations; 36 comparison tests passed. |
| CTX-25 | Done — usage comments and net classification updated with preserved JSON; 36 comparison tests passed. |
| CTX-37 | Done — required IDs respect requested corpus scope; scope and unmet-budget tests passed. |
| CTX-09 | Done — source windows stream and authenticate full files while retaining selected whole lines; 15 window tests passed. Selected lines have no separate size cap. |
| CTX-10 | Done — whole-file spans include unterminated final lines; 5 source-reader and span tests passed. |
| CTX-14 | Partial — subsecond parsing and state consolidation implemented; 175 token and 6 cross-surface tests passed. Equal timestamps retain existing tie-last behavior, contrary to one acceptance clause. |
| CTX-16 | Done — cache publication moved to filesystem ownership with compatible quota API; 73 focused filesystem/quota tests passed. |
| RT-35 | Done — runtime diagnostic and metadata reads are descriptor-bounded; focused FIFO, symlink, growth and counter tests passed. |
| RT-33 | Done — spawn preflights share sandbox policy resolution while retaining launch validation; focused preflight tests passed. |
| CTX-28 | Done — delivery filtering borrows records while preserving recipient and epoch behavior; focused regression passed. |
| RT-30 | Done — failed branch inspection no longer proves merge attribution; 10 attribution tests passed. |
| RT-31 | Done — uncertain session inventory defers ownership decisions; focused registry, adoption, recovery and adjudication suites passed. |
| INT-05 | Done — init and clean share socket-reaping policy; focused policy and failure tests passed. |
| INT-07 | Done — hook libraries separate parser, identity, heartbeat and spawn policy; Linux shell and 18 installed-layout integration tests passed. macOS Bash 3.2 awaits CI. |
| RBR-18 | Done — handoff YAML uses exact delimiter lines and preserves scalar content; 154 handoff tests passed. |
| CTX-19 | Done — skill comments describe current scoring and filesystem behavior; no runtime behavior changed. |
| VER-07 | Partial — one config-loading path implemented and 3 focused tests passed; public AsWorkDir retained for compatibility. |
| VER-12 | Done — validation loops consolidated and private forwarding module removed without changing public paths; 271 schema tests passed. |
| RBR-17 | Done — checkpoint components authenticate before trusted merge; 12 lookup and 4 trust tests passed. |
| RBR-19 | Done — semantic upsert preserves full V2 content; 9 merge and 7 schema tests passed. |
| CON-A-22 | Done — cwd-dependent memory tests run in isolated subprocesses; 47 focused tests passed. |
| CTX-38 | Done — marker commands are safely shell-quoted; pressure and bootstrap integration tests passed. |
| CTX-23 | Done — usage scope precedes Codex deduplication; 222 usage tests passed. |
| CTX-26 | Done — usage groupings share accounting values and preserve output; 222 usage tests passed. |
| RBR-04 | Done — subagent watch comments reflect current behavior; 9 stall tests passed. |
| RBR-12 | Done — terminal and worktree comments corrected; 13 settings tests passed. |
| CON-A-23 | Done — shared absence probes preserve exclusive daemon ownership; 5 lock tests passed. |
| CON-A-17 | Done — failed output retires only the owning unsubmitted request; 19 wait tests passed. |
| CON-A-24 | Done — manual launch commands shell-quote hostile paths and prompts; 5 tests passed. |
| INT-E34 | Done — status display sanitizes names and titles; focused collection/render test passed. |
| INT-E35 | Done — TUI acquisition rolls back on errors; 4 lifecycle tests passed. |
| VER-18 | Done — cache command parsing uses the shared lexer; 30 focused tests passed. |
| VER-16 | Partial — Loom writers share a durable lock and detected changes skip replacement; 42 tests passed. A noncooperating external editor can still write between final comparison and rename. |
| VER-22 | Done — Go benchmark flags do not scope regular tests; adapter, lint and 16 guard integration tests passed. |
| CTX-29 | Done — evaluator distinguishes source sites and gates ambiguity errors; 31 evaluator and 4 published-threshold tests passed with all declaration labels retained. |
| INT-10 | Partial — typed Sessions/Worktree dispatch and comment corrections passed 34 CLI tests; broader Run and Knowledge Context enum wrappers remain. |
| CTX-07 | Done — import bindings skip comments and avoid false local resolution; extractor suite passed. |
| CTX-08 | Done — extraction avoids retaining overlapping bodies; 202 extractor tests passed and retained payload was measured. |
| CON-B-09 | Done — durable retry ownership and ordered freeze refusals; focused host-job, store, refusal and recovery tests passed. Legacy jobs/refusals with unknown ordering defer safely. |
| RT-09 | Done — exhaustive relay policy preserves the public verdict and independent 72-pair coverage; 4 matrix tests passed. |
| RT-29 | Done — stage durations require real start times; focused regression passed. |
| RT-08 | Implemented; verification pending — stall and PID-zero comments corrected; renamed process-group test awaits execution. |
| DASH-01                                          | Done — browser keyboard regression passed                                                                                                                                                                                                              |
| DASH-02                                          | Done — handshake and shell focused tests passed                                                                                                                                                                                                        |
| DASH-03                                          | Done — graph tests and web typecheck passed                                                                                                                                                                                                            |
| DASH-04                                          | Done — remote bind and startup diagnostics tests passed                                                                                                                                                                                                |
| DASH-05                                          | Done — web tests, typecheck and legacy Rust regression passed                                                                                                                                                                                          |
| DASH-06                                          | Partial — bounded body reads verified; in-flight configuration commit race remains. Synchronous persistence can block, so a mutex covering the write would violate bounded shutdown; a cancellable publication boundary needs design and verification. |
| DASH-07                                          | Done — theme preview, web tests and typecheck passed                                                                                                                                                                                                   |
| DASH-08                                          | Done — focused web tests passed                                                                                                                                                                                                                        |
| INT-E01                                          | Done — updater installation tests passed                                                                                                                                                                                                               |
| INT-E02                                          | Done — HTTPS-only client regression passed                                                                                                                                                                                                             |
| INT-E03                                          | Done — live terminal shutdown regression passed                                                                                                                                                                                                        |
| INT-E04                                          | Done — configuration comments corrected                                                                                                                                                                                                                |
| INT-E05                                          | Done — direct-mode signal and lifecycle tests passed                                                                                                                                                                                                   |
| INT-E06                                          | Done — runtime configuration projection matrix passed                                                                                                                                                                                                  |
| RBR-01                                           | Done — wait, forwarding, classifier tests and production check passed                                                                                                                                                                                  |
| RBR-02                                           | Done — parent discovery tests and production check passed                                                                                                                                                                                              |
| RBR-03, RBR-09                                   | Implemented; verification pending                                                                                                                                                                                                                      |
| CON-B-01                                         | Partial — guarded records retain identity and retire genuine orphans; legacy bare-PGID records lack identity and fail closed, potentially leaving criteria running. Focused rerun cancelled while waiting for Cargo lock; prior Linux suite passed. |
| CON-B-03, CON-B-04, CON-B-05, CON-B-06, CON-B-07 | Implemented; verification pending                                                                                                                                                                                                                      |
| CON-B-10, CON-B-11, CON-B-12, CON-B-13, CON-B-15 | Implemented; verification pending                                                                                                                                                                                                                      |

Additional scoped work: provision-option validation, NUL-delimited worktree listing, and four Git comment corrections are implemented; verification is pending. CON-B-01 retains a legacy bare-PGID cleanup limitation. VER-01 has runner-specific verification outstanding.

Active independent repairs: none.
