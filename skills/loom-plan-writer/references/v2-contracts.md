# Plan Version 2 — contracts, reachability, ratchets and the review gate

Read when: writing a `version: 2` plan: contracts, harness, reachable, ratchet files, the review gate.

A `version: 2` plan keeps every rule in `SKILL.md` and adds behavioural contracts, reachability checks, ratchet files and a recorded code review. `version: 1` plans keep the v1 rules and run none of the checks below; `loom plan verify` rejects a v2 field in a v1 plan (`` `contracts` requires `version: 2` ``).

## 1. Start with detection

Before writing any stage, run `loom project detect` in the repository (`loom project detect <dir>` for another checkout; `--json` prints one line of JSON). It prints the checkout root, whether the bounded scan was truncated, and one line per package:

```text
root: /repo
truncated: false
loom  kinds=rust  runner=cargo-test  skills=loom-rust
.  kinds=docker  runner=unsupported  skills=loom-docker
```

For every package a stage touches:

1. Load each skill on its `skills=` list (a catalogued skill through `Skill(skill="loom-skills", args="<name>")`) and name it in the stage's `skills:`.
2. Read that language skill's `## Loom Test Runner Adapter` section. It names the adapter, the single-test command loom runs from the package directory, the exact form of a contract's `test` value, what the runner does when the name matches nothing, and where contract tests go. Take each contract's adapter and `test` format from it. Never guess the form of a test name: under `cargo-test` a bare function name selects nothing.
3. `runner=unsupported` means no adapter reads that package's test output, and contracts there fall back to exit codes: loom runs the contract's `test` value as a shell command, the freeze accepts it on a non-zero exit with a warning, and completion passes it on exit 0 with a warning. `loom plan verify` warns about each such contract. Put the contract in a package with an adapter when the behaviour allows it; when detection misses a runner the package does use, set `runner:` to that adapter's name.
4. `truncated: true` means the scan stopped at its entry limit, so a package can be missing from the list. Set `runner:` on every contract whose package the list lacks.

A contract without `runner:` uses the runner detected for the package that owns its `file`.

## 2. The risk checklist

Each `standard` stage walks this list. For every area that applies to its change, the stage carries at least one contract whose `rejects` names the plausible wrong implementation for that area. Record the walk in the stage prose: the areas that apply and the contract covering each. Every area below comes with a defect that passed its stage's verification and escaped.

1. **Untrusted input** — the stage parses, stores or renders a value that a user, a file or another process controls. Reject the implementation that accepts the common shape and lets the edge through: `..`, an empty value, a control character, a multi-byte character at a slicing boundary. *Escaped:* an id allowlist written as a character class admitted `..`, and a truncation that sliced a UTF-8 string by bytes panicked.
2. **Filesystem paths and symlinks** — the stage opens, writes, copies or deletes a path. Reject the implementation that follows a symlink out of its tree or resolves `..` past its root. *Escaped:* an atomic write opened `<path>.tmp` without `O_NOFOLLOW`, so a tracked symlink named `<target>.tmp` redirected a locked write outside the repository.
3. **Process I/O volume and scale** — the stage spawns a process, reads a pipe, or walks a repository. Reject the implementation that passes on a five-file fixture and fails once output outgrows a pipe buffer or the tree is large. *Escaped:* a hook piped `git status --porcelain` into `head -10` under `pipefail`; with 116 changed paths git died of SIGPIPE and the hook exited 141 in 16 of 60 runs. Every test had used a handful of files.
4. **Configuration propagation** — the stage adds a setting or reads one. Reject the implementation that never reads the setting and runs on its default. A test that sets the default value cannot tell the two apart: set a non-default value and assert its effect. *Escaped:* a permission mode was written to a settings file whose reader ignores that key there, and sessions started in the default mode while the file looked right.
5. **Lifecycle and concurrency** — restart, retry, partial failure, shutdown, or two actors on one resource. Reject the implementation that is correct on one clean run and wrong on the second: a duplicate write, a lost update, a lock never released. *Escaped:* a retried `loom knowledge update` appended its block a second time.
6. **Reachability from the entry point** — the stage adds a unit that users reach through a command, a route, a render tree or a loop. Reject the implementation that builds and unit-tests the unit and never connects it. The contract drives the entry point (runs the command, requests the route, renders the root) and asserts the new behaviour; add a `reachable` check as well (Section 4). *Escaped:* `CountryLensTable`: a wiring pattern matched the component's own file, so a table that was never mounted passed.
7. **External data correctness** — the stage consumes data or a format it does not own: a dataset, an API response, another tool's output or identifiers. Reject the implementation that parses the shape its author remembers. Capture the contract's fixture from the real source; never write it from memory. *Escaped:* the agent harness began naming workers `<name>@session-<hex>`, and a parser written for the older form rejected every such id as `worker id is empty or unsafe`.

An area that does not apply needs no contract. A stage whose change touches none of them still carries one contract (`plan verify` requires at least one on every v2 `standard` stage): for the behaviour the stage exists to add.

## 3. Writing a contract

```yaml
contracts:
  - id: rejects-symlinked-spool
    file: tests/spool_contracts.rs
    test: rejects_symlinked_spool
    scenario: creates the spool directory as a symlink into a TempDir, then calls Spool::open
    rejects: a Spool::open that follows the symlink and writes into the link target
harness: ["tests/fixtures/spool/**"]
```

| Field | Rule |
| --- | --- |
| `id` | `^[a-z0-9][a-z0-9-]*$`, unique within the stage; named after what it rejects |
| `file` | the test file, relative to the stage's `working_dir`, no `..`, at a location the language skill names |
| `test` | the exact name the adapter selects, in the form the language skill gives |
| `runner` | optional adapter name that overrides detection; an unknown name is an error in v2 |
| `scenario` | the concrete input or state the test sets up |
| `rejects` | the plausible wrong implementation the test must fail on |

- **`scenario` names inputs and state**: "creates the spool directory as a symlink into a TempDir, then calls `Spool::open`". "Tests spool opening" names a feature and sets up nothing.
- **`rejects` names one wrong implementation an agent could write while believing it correct**: "a `Spool::open` that follows the symlink". "Incorrect behaviour", "a bug" or "a broken implementation" names none, and a test written from it tends to pass the near-miss. If you cannot name a wrong implementation, you have not yet found the behaviour to pin.
- **One contract per behaviour.** A test pinning two behaviours fails for either and cannot be disputed for one alone.
- **The contract file holds only contract tests.** Every contract file is frozen, so the stage can never change another test placed in it, and impact-selected tests skip every test in a contract file, so such a test runs nowhere before integration-verify. Several contracts may share one file.
- **The stage description must let the contracts be written first.** Loom starts a contract session before the implementation session, and it writes every contract test from the stage description alone. Name the public surface the contracts call: paths, signatures, command names, error types. A contract written against a guessed signature is frozen wrong, and correcting it costs a `dispute-contract`.
- **`harness` lists the extra files the contract session may write**: fixtures, test helpers, and a file that exists only to declare the contract test module. Globs are relative to `working_dir`; `*` stays within one directory and `**` descends. Every existing file a harness glob matches is frozen with the contracts, whether the contract session touched it or not, and completion fails when one differs. A harness therefore names test-only files. Never harness a production source file, which the implementation session could then no longer edit, or a broad glob such as `src/**/*.rs`, which freezes the crate (a freeze holds at most 500 files). Prefer a contract location that needs no new declaration: an existing test module, or a test file the runner discovers by itself (`tests/*.rs` for `cargo-test`).

The sequence loom runs:

1. The contract session writes the contract tests and harness files and implements nothing. Every contract must fail now; a compile or collection failure counts as failing.
2. `loom stage contracts freeze <stage-id>` refuses unless only contract and harness files changed, every contract file exists, and every contract is red. A contract that passes is refused because it "passes before implementation, so it cannot tell right from wrong"; one the runner does not select is refused as well.
3. Loom stores a hash and a copy of every frozen file, ends the contract session, and starts the implementation session, whose signal lists the frozen contracts. That session never edits a frozen file. `loom stage contracts show <stage-id>` prints them; `loom stage contracts restore <stage-id> [--contract <id>]` puts the frozen content back.

## 4. The other v2 fields

### `reachable`: entry-point wiring

```yaml
reachable:
  - symbol: run_export             # the new unit
    from: main                     # the entry point it must be reached from
    description: "export is dispatched from the CLI"
    min_confidence: 0.5            # optional, 0.0..=1.0, default 0.0
```

Loom builds the worktree's source graph and walks backwards from `symbol` over calls, references, implements, extends, contains and imports edges with no depth limit; the check passes when the walk reaches `from`. Both names resolve exactly and case-sensitively, as `loom map --find-all <name>` lists them, and a symbol wins over a file of the same name. The walk follows resolved edges only, so a path through dispatch the extractor cannot resolve reads as unreachable: before writing a check, run `loom map --impact <unit>` on an existing unit wired the same way and confirm the walk reaches `from`. A name found only in a language without an extractor skips the check with a warning. Gaps read `symbol not found: <name>` or `<symbol> is not reachable from <from>`.

Prefer `reachable` to a regex `wiring` for entry-point wiring: `wiring` proves that a line of text exists in one file, `reachable` proves a path of edges from the entry point. Keep `wiring` for seams the graph does not model: configuration files, templates, registration by string, languages without an extractor.

`reachable` runs with the stage's goal-backward checks; `loom stage complete` runs them whenever the stage has `reachable`, `artifacts`, `wiring`, `wiring_tests`, `dead_code_check` or `regression_test`, so a `reachable`-only stage is verified on its own. Integration-verify re-runs every completed stage's `reachable` checks on the merged tree.

### `wiring` in v2

- **Glob `source`.** A `source` containing `*`, `?` or `[` is a glob relative to `working_dir`. The check passes when any matched file matches the pattern; a glob that matches no file is a gap (`no file matches source glob`).
- **`literal: true`** matches `pattern` as plain text. Use it for a pattern full of `(`, `.`, `[` or `?`.
- **Definition-site exclusion.** A match on the line that defines a name the file defines does not count. When every match is a definition, the gap reads `pattern matches only the definition of <name> in <file>; point it at a consumer`, so a pattern aimed at a declaration fails: aim it at the call, mount or dispatch site (`SKILL.md` Section 6). Definitions in a language without an extractor are not excluded, and a pass resting only on such files prints a warning.

### `ratchet_files`

```yaml
loom:
  version: 2
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
```

A plan-level list of every baseline or ledger file a stage could loosen to turn its own gate green: a size ledger such as this repository's `loom/maintainability-baseline.txt`, a knowledge-check baseline, a lint allowlist, a coverage floor. Entries are exact paths relative to the repository root, with no globs and no `..`. Any change to a listed file in a `standard` or `integration-verify` stage, tightening included, raises `TI-ratchet-<path>` (Section 5), which the stage reverts or disputes. A stage the plan expects to change one (lowering a ledger entry after shrinking a function) says so in its description, so its agent expects the `dispute-integrity`.

## 5. What completion enforces in v2

`loom stage complete` runs acceptance first, with the zero-test guard, then the goal-backward checks, then the v2 checks in the order below:

| Check | Stages | Fails when | Resolved by |
| --- | --- | --- | --- |
| Zero-test guard | every stage of a v2 plan | an acceptance command an adapter recognises ran zero tests: `selected zero tests (<adapter>)` | fixing the filter; `loom stage dispute-criteria` for an impossible criterion |
| Contract check | `standard` | the contracts were never frozen; a frozen file changed or is missing; a contract fails, does not build, or is not selected | an implementation that passes, `loom stage contracts restore`, or `loom stage dispute-contract <stage> --contract <id> --reason ...` |
| Test integrity | `standard`, `integration-verify` | an event is not accepted, or is worse than accepted | reverting the change, or `loom stage dispute-integrity <stage> --event <id> ... --reason ...` |
| Impact-selected tests | `standard` | a test that reaches the stage's changes fails | fixing the regression |
| Reachable re-verification | `integration-verify` | a completed stage's `reachable` check fails on the merged tree | wiring the unit |
| Review gate | `standard`, `integration-verify` | no well-formed review round; the worktree changed since the latest round; any finding open | fixing and re-reviewing, or `loom stage dispute-findings <stage> --finding <id> ... --reason ...` |

- **Zero-test guard.** A run that an adapter recognises and that executed no test fails the criterion although the runner exited 0, so a module filter matching nothing is caught. A command no adapter recognises is judged by its exit code.
- **Test-integrity events**, computed against the stage's merge base for every language with a test profile (other languages are noted and skipped): `TI-decl-<lang>` when test declarations in that language's test files fell; `TI-assert-<lang>` when assertions fell; `TI-edit-<path>` when a test file that existed at base lost or changed an assertion line whose text does not reappear in that file (moved lines do not count); `TI-ratchet-<path>` when a `ratchet_files` entry differs from base. `loom stage review integrity <stage-id>` lists them. An accepted dispute records the event, and the gate then holds the stage to the accepted counts or content.
- **Impact-selected tests.** Loom walks the worktree graph from every node in the changed files to the test nodes that reach them, groups those by the runner detected for their package, leaves out contract files, and runs each runner's selection. A runner that cannot select by name or file, a timeout, or no reached test leaves a note; the full suite still runs in integration-verify.
- **Review gate.** A round is recorded only from a `loom-code-reviewer` subagent whose final message ends with a `loom-review` block; a message without a valid block is recorded as a malformed round that counts for nothing. Every finding blocks completion whatever its severity. Each suggestion becomes a `suggestion` memory entry and never blocks. A finding closes when a later round lists it under `resolved` or an adjudicator dismisses or defers it; an upheld finding stays open. The stage signal carries this procedure, so the stage description need not repeat it.
- **Disputes.** Each dispute command files one request, sends the stage to adjudication and ends the session. A stage may file 3 disputes of each kind; an exhausted budget sends it to human review. List every disputed finding of one round in one `dispute-findings` command, repeating `--finding`. A findings verdict upholds, dismisses or defers each finding: `defer` carries it to a later, uncompleted stage that depends on this one, where it blocks completion as `<origin-stage>/F-<round>-<n>`. Contract and integrity verdicts accept or reject; an accepted contract dispute re-freezes that contract at its current content.
- **Integration-verify never defers a finding.** An adjudicator's `defer` from integration-verify is refused, so that stage fixes or disputes every finding. It also weighs every pending reviewer suggestion its signal lists: it implements, defers or ignores each one, and resolves each one it implements with `loom memory resolve <id> --outcome implemented --reason <what changed>`. Knowledge-distill records the rest in knowledge.
- **Integration-verify lists the full test command.** In v2, `loom plan verify` fails an integration-verify stage whose `acceptance` has no command an adapter recognises as a whole-suite run (`cargo test`, `cargo test --all-targets`). A filter such as `--lib <module>::`, a positional test name, or `-t` makes a run partial.

## 6. Token discipline

- **Keep contracts few**: one per risk area that applies, plus the behaviour the stage exists to add. Each one costs contract-session work, a freeze run and a completion run.
- **Contract, impact-selected and zero-test runs reuse the certified criteria cache**, so an identical run already certified is not repeated. Keep a standard stage's acceptance module-filtered (`cargo test --lib <module>::`); the impact selection adds the tests that reach the change.
- **The full suite runs once, in integration-verify.**
- **Re-reviews are incremental.** A re-review covers the files changed since the previous round plus the open findings; its brief carries the output of `loom stage review status <stage-id>`.
- **Dispute in batches.** File one `dispute-findings` per review round with every disputed finding in it: each dispute ends the session, so two disputes cost two respawns.
- **Review order.** Fix every finding, run the full gate, then run the final review round, then complete. Any edit after the final round, formatting included, changes the change fingerprint and needs another round; a commit does not. The stage signal and the `loom-orchestration` skill carry the canonical text.
- **Size the stage for its reviews.** Count at least one review round and its fixes in the stage's 500,000-token budget (`SKILL.md` Section 4); the contract session runs in its own context.
