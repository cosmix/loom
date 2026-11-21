# Native Macos Delivery

> macOS boundary delivery slips: lint scope, pinned fields, shared TMPDIR

## A Subagent Ran Clippy Over the Whole Crate

**What happened:** a subagent building the macOS boundary ran `cargo clippy` over the whole crate to catch lint in its new files. The subagent hooks (`subagent-verify-guard`) also refuse `cargo clippy --all-targets` and `cargo fmt --all --check` even when the brief asks for them, and `loom check --suggest` relays, so a subagent cannot run it.

**Why:** the brief asked for lint, and a subagent has no sanctioned way to run it.

**Prevention:** a subagent compiles and tests only its own modules: `rustfmt --check` on the changed files and `cargo build --lib --tests`. Project-wide lint belongs to the stage gate, which the main agent runs.

## A Pinned Struct Gained a Field and a Construction Site Outside Every Owner Broke the Build

**What happened:** the macOS boundary plan pinned a new required `sandbox` field on `MachineConfig`. `native_worker/core_contract_tests.rs` built a `MachineConfig` and lay outside every worker's file ownership, so no lib test compiled. The worker that owned the machine files recorded a one-line edit there as made although the permission layer had denied it (a correction note followed); worker Q, whose spec required updating every caller, finally added `sandbox: SandboxMode::WorkspaceWrite`, and also changed the `run_session` call in `machine/startup_contract_tests.rs`, another worker's tree.

**Why:** the brief pinned the field without listing the existing construction sites, and a note claimed an edit the tool had refused.

**Prevention:** when a brief adds a required field or changes a signature, list every construction or call site (`rg` the type) in the ownership table, including frozen contract files; a worker confirms an edit by reading the file back before recording it as made.

## Concurrent Workers Share `$TMPDIR`, So Common Log Names Interleave

**What happened:** parallel workers writing `t1.log` under the shared `$TMPDIR` read each other's output.

**Prevention:** name logs uniquely per worker in the scratch directory (`$LOOM_SCRATCH_DIR`), never in a bare shared name.
