# X1: the manual-mode line builders (codex unit)

Lane: codex, `loom-codex-forwarder`, `--model gpt-5.6-terra --effort xhigh`, unit id `x1-manual-start`. Sized for the 540 s wrapper deadline: one new file with its tests, three steps.

## What the unit does

In manual mode loom prints a bare `claude` command for the operator to run, at two sites. A session started that way runs outside the loom wrapper: no settings capsule and no environment allowlist. This unit writes one new module that returns the exact lines each site prints today, plus one notice line after the line that starts with two spaces and `To start:`. Another worker declares the module and changes both sites to print through it after this unit returns.

## Files

- Write: `loom/src/orchestrator/core/manual_start.rs` (new). Nothing else.
- Read, to copy the current text: `loom/src/orchestrator/core/stage_spawn.rs`, function `print_manual_start`; `loom/src/orchestrator/core/stage_knowledge.rs`, function `spawn_knowledge_worker`, the branch under `if self.config.manual_mode`.
- Never edit `loom/src/orchestrator/core/mod.rs`, `stage_spawn.rs` or `stage_knowledge.rs`: another worker declares `mod manual_start;` and wires the call sites. Until then the file is outside the module tree and the crate does not compile it, so run no cargo command; the orchestrator runs the tests after the module is declared.
- Run no git command and touch no path under `.loom/`.

## Steps

1. Module doc (`//!`): what a manual-mode run prints for a session it leaves the operator to start, and that the last line says the session runs unconfined. Then `use std::path::Path;` and this constant, verbatim:

   ```rust
   const CONFINEMENT_NOTICE: &str = "  Confinement: none. A session started this way runs outside the loom wrapper: no settings capsule (sandbox, write denies, hooks) and no environment allowlist.";
   ```

2. Two builders, each with a doc comment, signatures verbatim:

   ```rust
   pub(super) fn manual_stage_start_lines(stage_id: &str, worktree: &Path, session_id: &str, signal: &Path, work_dir: &Path) -> Vec<String>
   pub(super) fn manual_knowledge_start_lines(stage_id: &str, repo_root: &Path, signal: &Path) -> Vec<String>
   ```

   `manual_stage_start_lines` first computes `let absolute_work_dir = work_dir.canonicalize().unwrap_or_else(|_| work_dir.to_path_buf());` (the same fallback `print_manual_start` uses) and returns these five lines, where each `{...}` is the value named, paths through `.display()`:

   ```text
   Manual mode: Session setup for stage '{stage_id}'
     Worktree: {worktree}
     Signal: {signal}
     To start: cd {worktree} && LOOM_STAGE_ID={stage_id} LOOM_SESSION_ID={session_id} LOOM_WORK_DIR={absolute_work_dir} claude "Read the signal file at {signal} and execute the assigned stage work."
   CONFINEMENT_NOTICE
   ```

   `manual_knowledge_start_lines` returns:

   ```text
   Manual mode: Session setup for knowledge stage '{stage_id}'
     Directory: {repo_root}
     Signal: {signal}
     To start: cd {repo_root} && claude "Read the signal file at {signal} and execute the assigned stage work."
   CONFINEMENT_NOTICE
   ```

   The first four lines of each must equal, byte for byte, what the matching site prints with `println!` today; compare against the source before writing. The two leading spaces are part of lines two to five.

3. Tests in the same file, `#[cfg(test)] mod tests` with `use super::*;`. Pure: no file is created, and paths that do not exist are fine (the canonicalize fallback keeps them).
   - `the_stage_start_command_is_followed_by_the_confinement_notice`: the line after the one starting with two spaces and `To start:` equals `CONFINEMENT_NOTICE`; that start line contains `LOOM_STAGE_ID=`, `LOOM_SESSION_ID=`, `LOOM_WORK_DIR=` and `claude "Read the signal file at`; the result has exactly five lines.
   - `the_knowledge_start_command_is_followed_by_the_confinement_notice`: the same for the knowledge builder, with `cd <repo_root> && claude "Read the signal file at` in the start line.
   - `the_notice_names_what_the_session_lacks`: `CONFINEMENT_NOTICE` contains `capsule`, `sandbox` and `allowlist`, and starts with two spaces and `Confinement: none.`

## Constraints

- The file stays under 360 lines and every function under 45 lines: `cargo fmt` runs after this unit and can grow both.
- Literal braces inside `format!` are doubled; this file needs none.
- A placeholder such as `<stage-id>` inside a doc comment goes in backticks, or rustdoc with warnings denied fails.
- No `unwrap` outside tests; the canonicalize fallback above needs none.
- Comments state current behaviour only.

## Done means

The file exists with the constant, both builders and three tests, and the four copied lines of each builder match the two sites exactly. The orchestrator proves it with `cargo test --lib orchestrator::core::manual_start::` from `loom/` once the module is declared.
