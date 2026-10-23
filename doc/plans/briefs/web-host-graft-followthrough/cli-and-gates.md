# Shared CLI integration and maintainability

Run after the web and memory workers have finished. Use GPT-5.6 Terra.
Read their briefs and changed files directly: the published graph predates them.
Do not run git, builds, tests, linters, or formatters; return the implementation
and any unresolved issue to the orchestrator. Do not spawn workers.

## Exclusive write territory

- `loom/src/cli/mod.rs`
- `loom/src/cli/dispatch.rs`
- New `loom/src/cli/dispatch_memory.rs`
- New `loom/src/cli/dispatch_commands.rs`
- `loom/src/cli/types_status_web.rs`
- `loom/src/cli/types_memory.rs`
- New `loom/src/cli/tests_web_host_graft.rs`
- New `loom/tests/web_host_graft.rs`
- `loom/maintainability-baseline.txt`

Read-only starting points: `StatusWebArgs`, `MemoryCommands`,
`dispatch_memory`, `dispatch`, the public `commands::status::web::execute`
and `commands::memory::prepare` exports. Existing `Cli` is re-exported by
`cli/mod.rs`; use `clap::Parser::try_parse_from` for parsing tests.
Existing memory fixtures in `handlers/tests_resolve_pending.rs` illustrate
temporary repositories and RAII restoration of process environment/cwd.

## Host option

Add `host: Option<std::net::IpAddr>` to `StatusWebArgs`, with
`#[arg(long, requires = "web", value_parser = parse_web_host)]`.
Implement `parse_web_host(&str) -> Result<IpAddr, String>` in that small module.
Accept IP literals or the exact alias `localhost`, mapped to IPv4 loopback.
Reject DNS names, whitespace, URLs, bracketed port authorities and embedded
ports. Accept raw IPv6 literals (`::`, `::1`). Omission resolves at dispatch to
`IpAddr::V4(Ipv4Addr::LOCALHOST)`; avoid a clap default that makes ordinary
status require web mode. Revise the web help text to describe the configurable
host and preserve existing optional PORT semantics.

Replace the Status web call with
`status::web::execute(port, web_args.terminals, resolved_host)`.
The settled public function has those three parameters in that order.
Preserve ordinary status dispatch, conflicts and terminal flag requirements.

## Memory preparation

Add `MemoryCommands::Prepare { stage: Option<String>, budget_tokens: usize,
json: bool }`. Stage uses the existing ID validator and `-S/--stage` convention;
budget defaults to 2000 and uses `clap_budget_tokens_validator`; JSON is a flag.
Make that validator's error wording describe a context frame generically so
it also accurately describes preparation, without changing its existing minimum.
Dispatch to `memory::prepare(stage, budget_tokens, json)`.
Re-export and handler wiring are the memory worker's responsibility.
Do not change pending, resolve, or receipt outcome semantics.

## Refactor the shared dispatch safely

`dispatch.rs` is already near the file limit and its main `dispatch` function
exceeds the function limit. Move the existing memory match into the new module,
preserving every variant, and register it in `cli/mod.rs`. Split the large main
match into cohesive exhaustive helpers in `dispatch_commands.rs` (including the
Status web call, which the plan's consumer wiring check locates there), with functions
<=50 lines and files <=400. Follow existing `dispatch_stage` module visibility.
Preserve `resolve_completion_proof` and `print_minted_proof` callers. Keep any
already-merged Router dispatch arm at the source location asserted by the
model-router sibling's wiring checks; extract other arms around that contract.
Keep `dispatch_knowledge` (with any `KnowledgeCommands::Bootstrap` arm) in `dispatch.rs`; the
knowledge-bootstrap plan's wiring checks grep it there.
Enumerate
all `MemoryCommands` consumers and all dispatch function callers before edits;
this is relocation plus the two new routes, not a CLI behavior redesign.

After all workers finish, remove only baseline rows for violations that this
plan eliminated (including extracted distill/cache code). Never increase an
allowed count or blanket-regenerate the ledger. Check the scanner's exact-row
semantics in `loom/tests/maintainability/{baseline,scanner}.rs` before editing.
The orchestrator runs the maintainability gate and sends concrete failures back.

## Executable coverage

Register `tests_web_host_graft.rs` in `cli/mod.rs`. Cover ordinary status;
default web; host requiring web; localhost/IPv4/IPv6; malformed host; explicit
PORT and zero; terminals requiring web; preparation defaults and invalid budget.
Inspect the parsed command fields, not just successful parse or help substrings.

Create Cargo's automatically discovered `loom/tests/web_host_graft.rs` target.
Use `env!("CARGO_BIN_EXE_loom")` so tests run the built binary, never PATH loom.
This target owns the CLI end-to-end proof for both features:

- Spawn `status --web 0 --host 0.0.0.0` in a disposable initialized workspace.
  Poll startup with a bounded deadline, capture the actual port/token, verify
  no-cookie refusal, valid bootstrap redirect and cookie-authenticated status.
  Prove explicit port and default CLI host parsing separately. Use no live
  operator state. Keep a child RAII guard that kills and waits on every exit.
- Build disposable memory fixtures; drive `memory prepare --json`, existing
  `memory resolve`, and prepare again. Assert group IDs, remaining work and final
  `memory pending --strict`, plus nonzero oversize/conflict behavior. Use a
  fixed fixture budget that demonstrably omits a second group. Do not insert
  receipts directly for the resolve-cycle test.
- Clear inherited `LOOM_*` routing/session values in child environments, use
  absolute binary paths, and scope user config to a disposable fixture.
  Never repurpose HOME as a shell scratch variable. Socket-restricted platforms
  may explicitly skip the socket subcase; pure parsing and memory cases run.

The orchestrator executes `cargo test --manifest-path loom/Cargo.toml --test
web_host_graft` and the existing CLI/unit/maintainability gates. A broken route,
unregistered test target, ignored flag or unavailable preparation command must
fail, not report zero matching tests.
