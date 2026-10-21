# W6: retire the session-trusted and nested verification paths (stage daemon-completion)

Read `doc/plans/briefs/host-verification/common.md` first: sections 1, 4.4, 4.6 and 5. After this stage no session runs a confined command. Everything that existed only to carry a session's verdict to the daemon, or to let a session nest confinement, goes. Paths are relative to `loom/`.

## Territory

You own:

- `src/commands/stage/completion_evidence.rs`, `src/commands/stage/completion_evidence/**`, `src/commands/stage/control_complete.rs`, `src/commands/stage/tests/control_complete.rs`, `src/commands/stage/mod.rs`;
- in `src/daemon/`: `mod.rs`, `inherited.rs`, `inherited/**`, `protocol.rs`, `protocol_debug.rs`;
- in `src/daemon/server/`: `mod.rs`, `observer.rs`, `peer_identity.rs`, `client.rs`, `client_tests.rs`, `client_dispatch.rs`, `self_service.rs`, `self_service_tests.rs`, `tests/self_service_client.rs`, `completion_evidence.rs`, `completion_content_tests.rs`, `completion_dispatch.rs`, `completion_dispatch/**`, `control_complete.rs`, `control_complete_tests.rs`;
- `src/verify/criteria/confine_host.rs`, `src/verify/impact_tests/report_tests.rs`, `src/verify/impact_tests/observed.rs` (a doc comment);
- `src/orchestrator/terminal/native/{session_settings/contents.rs, tests_capsule_contents.rs, tests_session_settings.rs, wrapper.rs}` (`wrapper.rs`: a comment only), `src/orchestrator/core/knowledge_completion_tests.rs` (a comment only);
- `src/sandbox/settings/policy.rs`, `src/sandbox/settings/policy/tests.rs`;
- `src/fs/permissions/hooks/config.rs`, `src/fs/permissions/hooks/registration_tests.rs`, `src/fs/permissions/tests/hooks_tests.rs`;
- `src/codex.rs`;
- `tests/completion_replay.rs`, `tests/completion_replay/**`, `tests/integration/binary_spawn_guard.rs`;
- `../loom-hooks/loom-control-complete.sh`, `../loom-hooks/codex-forward.sh`, `../loom-hooks/tests/loom-control-complete*.sh`, `../loom-hooks/tests/loom-control-tokenized-prefilter.sh`, `../loom-hooks/tests/codex-forward-direct.sh`, `../loom-hooks/tests/codex-forward-wrapper.sh`, `../loom-hooks/tests/run-all.sh`.

W4 removes every call into the broker from the completion command files it owns (and `relay/emit.rs`'s `control_broker`). W5 changes the freeze handlers. Never edit their files. Before deleting anything, run `loom map --impact <symbol>` and `rg -n <name> src tests ../loom-hooks`. A consumer outside your territory and outside W4's list (common.md and W4's brief) must be reported, not edited. W4 reads the files you delete through `git show HEAD:`, so delete them whenever you are ready.

`maintainability-baseline.txt` has no row for any file you delete; leave it untouched.

## Steps

1. **Completion broker and evidence protocol.**
   - Delete `src/commands/stage/completion_evidence.rs` and `completion_evidence/` (`broker.rs`, `producer.rs`, `output_criteria.rs`, `impact_record.rs`, and their tests).
   - Delete `src/commands/stage/control_complete.rs` and its test file. (Its `mod` line is in `complete.rs`, W4's file; W4 removes it.)
   - Remove the evidence modules' declarations from `commands/stage/mod.rs`.
   - `verify/impact_tests/report_tests.rs` imports `format_impact_record` and `parse_impact_record` from `impact_record.rs` (about lines 5-7) and tests the deleted printer (about 164-190): delete those tests and imports. `record_observed` lives in `verify/impact_tests/observed.rs` and stays; fix that file's doc comment that names the broker. W4 ports `after_failed_completion` for the operator path.
2. **Daemon completion RPCs.**
   - Delete `daemon/server/completion_evidence.rs`, `completion_dispatch.rs` and `completion_dispatch/`, `control_complete.rs`, and their test files.
   - In `protocol.rs`, remove `Request::CompleteStage` and `Request::RecordCompletionEvidence` and every match arm on them (`client_dispatch.rs`, `self_service.rs`'s list, `protocol_debug.rs`), and the `AuthorizedCompletion` minting in `client.rs` if nothing else uses it. Change `Request::FreezeContracts` to carry `head: String, tree_digest: String` instead of `reports` (common.md 4.6).
   - Test helpers in the deleted files that other tests use (`TrustedCheckpointFixture`, `trusted_checkpoint_fixture`, `scratch_git_fixture`, `record_trusted_test_checkpoint`; find their users) move to a `#[cfg(test)]` support module beside their remaining users.
   - `observer.rs::check_completion_gates` stays and becomes reachable from `host_verify`: raise it to `pub(crate)`, add `pub(crate) use observer::check_completion_gates;` to `daemon/server/mod.rs`, and add it to the `pub(crate) use server::{..}` list of `daemon/mod.rs`, so W4 calls `crate::daemon::check_completion_gates`.
   - `peer_identity.rs`: fix the doc comment that names `CompleteStage`.
   - `user.token` itself stays; only its completion use goes. Check `rg -n 'user.token|completion_credential' src` and report any other user.
3. **macOS inherited confinement.**
   - Delete `daemon/inherited.rs` and `daemon/inherited/`, and `Request::ValidateInheritedConfinement` with its dispatch and self-service entry.
   - Delete the inherited branch in `verify/criteria/confine_host.rs`: the `inherited = network == Denied && daemon::inherited::available(&tree)` decision and the early return in `apply_environment`. Linux keeps bubblewrap and macOS keeps `sandbox-exec`, unconditionally.
   - In `session_settings/contents.rs`, delete the `#[cfg(target_os = "macos")]` block that adds the daemon socket to `sandbox.network.allowUnixSockets`, with its comment.
   - `daemon/server/mod.rs` has `#[cfg(target_os = "macos")] pub(crate) use peer_identity::peer_pid;`, used only by `daemon/inherited/`: delete it, and anything it leaves unused under a macOS cfg (for example a `peer_pid` parameter of `client_dispatch.rs::control`).
   - `orchestrator/terminal/native/tests_session_settings.rs` holds a macOS-only assertion that `allowUnixSockets` equals the daemon socket (about lines 270-276): assert instead, on every platform, that the capsule grants no daemon socket.
   - This code is `cfg(target_os = "macos")`, and nothing here compiles it. Delete whole items. Leave no dangling `use` under a macOS cfg. `rg -n 'target_os = "macos"' src` afterwards, and read every remaining hit near your edits.
4. **The opt-out.**
   - Delete the `allowNestedUserNamespaces` block and its comment from `sandbox/settings/policy.rs::sandbox_settings` (`filesystem_settings` keeps its 52 lines).
   - In `sandbox/settings/policy/tests.rs`, replace `enabled_sandbox_allows_nested_user_namespaces_on_linux_only` with `enabled_sandbox_never_allows_nested_user_namespaces`. It asserts, for an enabled config on every platform, that no key of `json["sandbox"]` names nested user namespaces, without spelling the key: `assert!(!json["sandbox"].as_object().unwrap().keys().any(|k| k.ends_with("UserNamespaces")))`. After this stage no file under `src/` or `../loom-hooks/` contains `allowNestedUserNamespaces` or `LOOM_CONTROL_EVIDENCE` (common.md section 5); the stage gates grep for both.
5. **The completion hook.**
   - `../loom-hooks/loom-control-complete.sh`: delete the PostToolUse half (the broker call, output capture, outcome parsing and its messages). Keep the PreToolUse half: the pin of `loom stage complete <stage>` and the completion heartbeat (`loom_write_completion_heartbeat`). The stall monitor's `COMPLETION_ACTIVITY` depends on it.
   - Keep `heredoc_openers_are_plain` and `completion_rejection_reason` byte for byte, with every helper the PreToolUse rejection calls. Plan 5 of the token-optimization programme edits them by name after this plan.
   - In `src/fs/permissions/hooks/config.rs`, remove the hook's PostToolUse registration and keep its PreToolUse one. Three tests pin the registration: `fs/permissions/hooks/registration_tests.rs` (about 70-86), `fs/permissions/tests/hooks_tests.rs` (about 214-218) and `orchestrator/terminal/native/tests_capsule_contents.rs` (about 227-228): each now asserts the PreToolUse registration and no PostToolUse one.
   - Hook tests:
     - Keep `../loom-hooks/tests/loom-control-complete-heredoc.sh` and every `expect_allowed`/`expect_refused` case and its label; delete only the broker stub, the `post` branch of `invoke_hook`, and the post phases of `expect_allowed` and `expect_refused`. Plan 5 edits this file by case label.
     - `../loom-hooks/tests/loom-control-tokenized-prefilter.sh` stubs the broker (`BROKER_LOG`, "reached the broker"): keep its PreToolUse cases and remove the broker assertions.
     - Delete any other `loom-control-complete*.sh` test that only pinned broker behaviour, and its `run_test` line in `../loom-hooks/tests/run-all.sh` (a static list).
   - `bash ../scripts/check-hook-syntax.sh` must stay green.
6. **Codex lane on Linux** (`../loom-hooks/codex-forward.sh`, `src/codex.rs` doc comment).
   - Today the wrapper probes `sandbox-exec` and falls back to direct mode (`codex exec --sandbox danger-full-access`) when the outer sandbox refuses a nested Seatbelt.
   - Add the Linux probe: when `uname -s` is `Linux` and `bwrap` is on PATH, run `bwrap --ro-bind / / --dev /dev true` (PATH lookup, so tests can stub it). When it fails, use direct mode. Without `bwrap` on PATH, keep today's companion mode (a deliberate limit: the probe needs bwrap; revisit if codex runs on a Linux host without it).
   - The `mode: direct (codex exec --sandbox danger-full-access; nested Seatbelt refused)` line stays byte-identical on both platforms: `../loom-hooks/codex-forward-result.sh`, `../loom-hooks/tests/codex-forward-result.sh` and `../agents/loom-codex-forwarder.md` match it exactly. Name bubblewrap only in the note line that follows it.
   - Keep the macOS probe unchanged.
   - Extend `../loom-hooks/tests/codex-forward-direct.sh` with a Linux case: a stub `bwrap` that exits 1 → direct mode; one that exits 0 → companion mode. Mirror the existing `sandbox-exec` stub. `../loom-hooks/tests/codex-forward-wrapper.sh` stubs only `sandbox-exec`: add a `bwrap` stub that exits 0, so its companion-mode cases do not depend on the host.
   - Update the `codex.rs` comment about the nested bubblewrap to say direct mode applies when nesting is refused.
7. **Delete `tests/completion_replay.rs` and `tests/completion_replay/`.** They drove the hook → broker → daemon path end to end. W5's `tests/host_verification_e2e.rs` replaces them. Remove the `completion_replay/support.rs` entry from `tests/integration/binary_spawn_guard.rs` (about lines 42-50).
8. **Stale comments.** Fix the comments naming the broker, the evidence record or `CompleteStage` in `orchestrator/terminal/native/wrapper.rs` (about 199-200) and `orchestrator/core/knowledge_completion_tests.rs` (line 2).

## Proof (run once each, from `loom/`, after W4 and W5 have written; if the build fails only on their missing items, report that and stop)

```bash
cargo build 2>&1 | tail -20
cargo test --lib sandbox::settings 2>&1 | tail -10
cargo test --lib daemon:: 2>&1 | tail -10
cargo test --lib fs::permissions:: 2>&1 | tail -10
cargo test --lib orchestrator::terminal::native:: 2>&1 | tail -10
cd .. && bash loom-hooks/tests/run-all.sh 2>&1 | tail -15
rg -n 'allowNestedUserNamespaces|LOOM_CONTROL_EVIDENCE|LOOM_CONTROL_BROKER|ValidateInheritedConfinement|RecordCompletionEvidence|CompleteStage' loom/src loom-hooks
```

The last command must print nothing. Report any hit in a file you do not own.

## Report

Every file deleted; every symbol moved and where; consumers you found outside your territory; the macOS cfg sites you edited; the proof output.
