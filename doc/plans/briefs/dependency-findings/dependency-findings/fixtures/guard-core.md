<!-- loom-memory-journal -->
# Memory Journal: guard-core

**Stage**: guard-core
**Created**: 2026-10-02 16:00:23 UTC

---

### ✅ Decision [2026-10-02 15:59:03] id=f475648d44a741da87f2cddf1fe2648d

contracts type-checked and their git steps exercised against a scratch stub crate of the shared interface before freezing

*Context:* unresolved imports mask every later type error in the real crate; the stub showed each contract fails only at its guard assertion, and the two positive contracts (knowledge-only accepts) pass against an accept-all stub, so they fail now only by compile
*Session:* session-7043a341-1790956466

---

### 📝 Note [2026-10-02 16:22:52] id=dbfda97e28de4f6c8128e75c328d89b0

mistake: set GIT_GRAFT_FILE=/dev/null in runner::git_command as the brief measured. git 2.53 opens the graft file and prints its 'info/grafts is deprecated' advice on stderr for every command that parses commits, which broke git::merge::fast_forward::tests::a_refusal_without_paths_is_fast_forward_refused (FastForwardRefused.detail is git's stderr). Prevention: disable grafts with a path that cannot exist; fopen ENOTDIR/ENOENT is silent. Fix: GIT_GRAFT_FILE=/dev/null/loom-no-grafts (runner.rs NO_GRAFT_FILE). The W2 hook script must not export GIT_GRAFT_FILE=/dev/null either: its stderr reaches the operator on every ref update.

*Evidence:* `src/git/runner.rs:52`
*Session:* session-2e4a4b3a-1790956824

---

### ✅ Decision [2026-10-02 16:22:52] id=3f8bf0d593e942989a60f39bf555fbbc

target guard chain walk runs at most 2*steps+2 iterations, not the brief's steps+2

*Context:* every non-returning iteration visits a new from, to or tip value (at most 2*steps+1 values); five alternating knowledge and attested moves over two ledger steps need six iterations and the brief's bound reported a false 'did not converge' (evaluate_tests::knowledge_gaps_between_attested_steps_are_walked_to_the_tip)
*Session:* session-2e4a4b3a-1790956824

---

### ✅ Decision [2026-10-02 16:22:52] id=0c1f9f0733a64af7a200d6aa80b705b8

gap_end finds candidate ledger 'from' values with one 'git rev-list --ancestry-path at..tip' instead of two merge-base --is-ancestor calls per ledger value

*Context:* same set (descendants of at that are tip or its ancestors); a run's ledger grows with every host move, and the gap search runs on every knowledge commit evaluation
*Session:* session-2e4a4b3a-1790956824

---

### ✅ Decision [2026-10-02 16:22:52] id=bceaf319f41947e3b8592bc5bd30a8c9

a re-recorded hold for the same observed tip keeps its since; check_locked writes the record only when the entry changed

*Context:* an Unevaluable hold is re-evaluated on every check; with since=now it would rewrite the record each tick and status would always show 'held since just now'
*Session:* session-2e4a4b3a-1790956824

---

### 📝 Note [2026-10-02 16:22:52] id=de0b32c382d24b1396e97ff8d7919e3d

found: stage-work listing uses for-each-ref --format=%(refname) and strips refs/heads/, not %(refname:short): a tag named like the branch turns :short into heads/loom/x. Also suspected (not tested): git/branch/operations.rs:91 list_loom_branches strips only '*' from 'git branch --list', and git marks branches checked out in linked worktrees with '+', so stage branches would come back as '+ loom/x'.

*Evidence:* `src/git/branch/operations.rs:91`
*Session:* session-2e4a4b3a-1790956824

---

### 📝 Note [2026-10-02 16:24:18] id=9aeb58c9245b470e9e42371d17a31306

gotcha: GIT_GRAFT_FILE=/dev/null makes git 2.53 print a grafts-deprecated advice on stderr; the runner and the hook use the never-existing path /dev/null/loom-no-grafts instead (common.md:89-90 and w2 brief:40 say /dev/null)

*Evidence:* `loom/src/git/runner.rs:52`
*Session:* session-2e4a4b3a-1790956824

---

### ✅ Decision [2026-10-02 16:35:33] id=6167f528361b41b6b1ffada83c8f8789

reference-transaction installer: UpToDate only when content equals SCRIPT AND mode is 0755; otherwise rewrite via staged file + rename (Installed)

*Context:* git silently skips a non-executable hook, and attestation_mode only checks the marker, so a 0644 loom hook would read Active while writing no ledger lines; staged write+rename means git never runs a partial or non-executable hook (dash reads scripts incrementally)
*Session:* session-2e4a4b3a-1790956824

---

### 📝 Note [2026-10-02 16:35:33] id=c9e392ddd63e48a0958b425e4fb54149

found: hook script uses a subshell append '(printf ... >>"$ledger") 2>/dev/null' as the writability test and the write in one step; a failed redirection on the ':' special builtin exits a non-interactive POSIX shell, so never test writability with a bare ': >>file'

*Evidence:* `../loom-hooks/git-reference-transaction-hook.sh:62`
*Session:* session-2e4a4b3a-1790956824

---

### 📝 Note [2026-10-02 16:39:04] id=14b42d01119f4448ade6b2d6b8050108

found: loom subagents watch exited 6 (hung) for W1 after W1 had already handed back its SubagentHandback report and its completion notification fired; treated as returned

*Session:* session-2e4a4b3a-1790956824

---

### 📝 Note [2026-10-02 16:49:01] id=9f88e0459e4f46c883d51516913f0aa6

found: no subagent of this session (W1, W2, reviewer round 1) produced a SubagentStop lifecycle record, so review round 1 was never recorded and the W1 watch exited 6; all three ended through a SubagentHandback tool call (each transcript ends on its tool_result). Replaying subagent-stop.sh with a well-formed payload against a scratch copy of the work dir reaches the review harvest, so the hook logic is sound: SubagentStop is not delivered, or arrives without agent_type/agent_transcript_path, for hand-back endings

*Evidence:* `loom-hooks/subagent-stop.sh:61`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=163fb323013f45ef84855e0b00bbc51f

loom/src/git/target_guard/evaluate.rs:1 Module doc sentence 'with only the record, the ledger and git run through the runner' is garbled; reword.

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=6fe8bf03bb5d444da3222e1b0fc17bb3

loom/src/git/target_guard/mod.rs:162 TargetEntry { hold: None, ..entry.clone() } clones the old hold only to discard it; build the struct field by field.

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=a8c24479a4df4a8eb0dc4b97e148fa49

loom/src/git/target_guard/mod.rs:354 accept replaces the record on any read error; replace only a parse error and propagate I/O errors (known).

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=95c77438fa424702a52ca62fca354aab

loom/src/git/target_guard/attestation.rs:55 attestation_mode ignores the hook's exec bit, so a non-executable loom hook reports Active (known).

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=a115dfa219c04db69807f0cb31bb7afa

loom/src/git/target_guard/mod.rs:311 pending_hold returns Err on an unreadable record where check_locked returns Held(Unevaluable) (known).

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=e453844c1c3e4977a9d7e6dae255cde2

loom/src/git/target_guard/text.rs:35 Second id-abbreviation helper beside mod.rs::abbrev; unify (known).

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=e93903c7faac46fd91432b6b3150db00

loom/src/git/hooks/reference_transaction.rs:89 A failed rename leaves reference-transaction.loom-new behind (known).

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=639cf1eff3ae43c9b7b19a994f83c537

loom/src/commands/repair/workspace.rs:304 loom repair reinstalls only the pre-commit hook; a removed reference-transaction hook silently turns attestation off until the next loom init. Plan scopes install to init and daemon startup, so handle in stage 2 or a follow-up.

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 16:56:05] id=f6ee4c0d6fe2452893ff1fae34485c6a

loom/src/git/target_guard/evaluate.rs:199 A stage worktree created from an operator commit the guard has not yet accepted yields a false StageWork hold (merge base is the new tip); stage 2 must create worktrees from the accepted tip as plan 3.4 states.

*Evidence:* `review round 1`
*Session:* session-2e4a4b3a-1790956824

---

### 📝 Note [2026-10-02 16:57:01] id=6f319e9348ad481bb0b26e7ad2f58a92

found: the first reviewer's round was never recorded (no SubagentStop lifecycle row) while the second reviewer's was, both ending through SubagentHandback; earlier note about hand-back endings never firing SubagentStop is too strong - delivery is intermittent

*Session:* session-2e4a4b3a-1790956824

---

### ✅ Decision [2026-10-02 17:00:38] id=dc0fc79b517d4fb5859e853fe63c3a66

attestation_gap reports the unrestricted walk's gap when it starts at accepted or a descendant, else the forward-only walk's gap

*Context:* Reviewer F-1-1 proposed always reporting the forward-only gap. That regresses the reason after a legitimate backward chain (reset A->A0, commit B, merge A = M, then unattested C): forward walk names A..C with src/b.rs+src/c.rs, unrestricted walk names M..C with src/c.rs only. A gap behind accepted is the reset-and-undo signature that charges accepted commits. Test a_gap_after_a_backward_attested_step_starts_where_the_chain_ends pins it.
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 17:04:23] id=1df790581ca844ad8e5c96fd7cd47f5c

loom/src/git/target_guard/evaluate.rs:93 The two-walk design and the reason-selection rule depart from plan 3.3 (single chain walk); record them in the guard knowledge during knowledge-distill.

*Evidence:* `review round 2`
*Session:* session-2e4a4b3a-1790956824

---

### 💡 Suggestion [2026-10-02 17:04:23] id=419df9451d21471ea79be31f26c8fa58

loom/src/git/target_guard/evaluate.rs:183 Greedy 'latest in ledger order' step choice can dead-end at a gap when an earlier forward step would clear (false hold, not an unsound clear); pre-existing, note for later.

*Evidence:* `review round 2`
*Session:* session-2e4a4b3a-1790956824

---

### 📝 Note [2026-10-02 17:05:40] id=cc0be53b27814fa8b02e0c9181694475

gotcha: the repo pre-commit hook's markdown linter rewrites unrelated doc/plans files in the worktree (renumbered an ordered list, stripped a meaningful trailing space in `allow`); restore them with git checkout before completing

*Evidence:* `loom/.githooks`
*Session:* session-2e4a4b3a-1790956824

---

### 📝 Note [2026-10-02 17:09:46] id=069e08a6f2dc4b289fd750ab2c7019f7

found: loom stage complete fails in the impact-selected tests step, not in the stage's code: the selector joins every impacted test name into one 'cargo test -- <names>' string and confine.rs spawns it as a single 'sh -c' argument, which exceeds Linux MAX_ARG_STRLEN (128 KiB) and fails with E2BIG before cargo starts. A change touching git/runner.rs and git/merge impacts thousands of tests. Fix: batch the names or fall back to the full suite past a size bound.

*Evidence:* `loom/src/verify/impact_tests/runs.rs:63`, `loom/src/verify/criteria/confine.rs:28`
*Session:* session-f19913fe-1790960794

---
