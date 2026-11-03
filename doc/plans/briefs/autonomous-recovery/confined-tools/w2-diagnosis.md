# confined-tools W2: a missing tool is reported as hidden or missing, never as a test failure

Tier: sonnet. Wave 2, after C1, in parallel with W1 (W1 adds `PreparedCommand::exposed_roots`;
code against that signature). Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `src/verify/criteria/tool_roots_contract_tests.rs`.

## Why

Plan 06's `before_stage` check exited 127 and loom blocked the stage as a `TestFailure`
(`orchestrator/core/stage_preflight.rs::before_stage_gate_passed`), with nothing telling the
operator whether `srt` was missing, hidden by the boundary, or broken. A tool hidden by the
boundary is loom's to report precisely; a tool missing on the host is a genuine host-install
need.

## Edits (you own these files)

1. New `src/verify/criteria/tool_diagnosis.rs`:

   ```rust
   pub(crate) enum ToolFinding {
       Hidden { path: PathBuf, target: PathBuf },
       MissingOnHost { name: String },
   }
   impl std::fmt::Display for ToolFinding { /* "tool hidden by confinement: <path> -> <target>"
       / "tool missing on the host: <name> (needs a host install)" */ }
   pub(crate) fn diagnose(words: &[String], path_var: Option<&OsStr>, cwd: &Path,
       exposed: &[PathBuf]) -> Vec<ToolFinding>;
   ```

   A word that does not resolve on the host is `MissingOnHost` unless it is a shell builtin
   (`cd`, `test`, `[`, `echo`, `printf`, `exit`, `true`, `false`, `export`, `set`, `read`,
   `.`, `source`). A word that resolves is `Hidden` when the resolved path, or any hop of its
   symlink chain up to the canonical target, lies outside every root in `exposed` and outside
   the fixed roots; `target` is the first hop outside them. A word whose chain lies inside is
   no finding (its failure is behavioural).
2. `src/verify/criteria/executor.rs` (`run_prepared_with_timeout`): when the command was
   confined and exited 126 or 127, append one line `loom: <finding>` per finding to the
   result's `stderr` (`tool_words` of a shell spec, or the program of a program spec;
   `PATH` and the working directory from the prepared command). The result's `success`,
   exit code and every other field are unchanged. The live-test-lane plan's
   `run_live_with_timeout` runs through the same function, so a live run gets the diagnosis
   too; its exposed roots already include the live boundary's paths (W1).
3. `src/verify/goal_backward/truths.rs` (`verify_truth_checks`): a gap's description carries
   every `loom: tool ...` line of the run's stderr after its failure text, so a `before_stage`
   or `after_stage` gap names the finding.
4. `src/orchestrator/core/stage_preflight.rs` (`before_stage_gate_passed`): when any gap names
   `tool missing on the host: <name>`, the blocked stage's first evidence line is `needs a host
   install: <name> (before_stage check <n>: <description>)`; a `tool hidden by confinement`
   gap's first evidence line is `tool hidden by confinement: <path> -> <target>: loom does not
   expose this install layout (before_stage check <n>)`. A behavioural failure keeps today's
   evidence.
5. Tests beside each module: the executor appends the findings for a 127 exit; truths keeps
   them in the gap; stage_preflight's evidence for each finding kind (drive
   `persist_blocked_stage` through the function the gate calls, or test the evidence builder
   you extract).

## Traps

- Diagnosis runs on the HOST side after the confined run returned: it never runs the tool.
- Bound the work: at most `tool_roots::MAX_TOOL_WORDS` words, at most 8 hops each.
- `stage_preflight.rs` does not decide the stage's status differently: it is blocked as today;
  only the evidence text changes.

## Done

Your one check after the stage compiles: `cargo test --lib verify::criteria::`.
