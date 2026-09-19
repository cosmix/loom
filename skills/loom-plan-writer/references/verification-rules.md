# Verification Rules — realizability, gates, produced artifacts

Read when: writing any `acceptance` or `wiring_tests` entry, prescribing a test, or asserting a fact about an artifact the stage will produce.

**Realizability — a prescribed check must be able to PROVE what it claims.** Grounding claims about code (`SKILL.md` Section 1) is half the job; the tests/acceptance the plan PRESCRIBES must themselves be grounded. A green check that verifies nothing is worse than none — it reads as "covered." Every `acceptance`/`wiring_tests` command, and every test a stage description prescribes, must clear four gates:

1. **Expressible** — the existing harness can already do this. "Stub the response," "intercept the request," "seed this store" are NOT free — confirm the suite already has that mechanism, or the plan must add it as explicit work.
2. **Executes the code under test** — the runtime that runs the check actually loads the code being asserted. A value baked only by the prod bundler is undefined under the unit runner; an inline script the module graph never imports is never executed; a symbol defined for one package is absent in another that also runs the file. If the code lives outside the harness's normal load path, the "test" is a grep — say so and add a real one. Corollaries (each a logged failure): a production build cannot verify a module nothing in the entry graph imports (tree-shaken out — the test runner that loads it is the real proof); a bundler neither type-checks nor compiles shader/TSL graphs (`build` proves bundling only); a round-trip test on the raw in-memory value proves nothing about the emitted artifact — decode the emitted bytes.
3. **Assertion strength matches the claim** — a substring/contains check cannot guard a "byte-unchanged / identical" contract (use exact-equality); a presence check cannot guard behavior. **A `wiring` grep proves the call site EXISTS, not that the logic is correct** — any change with real logic needs a check that RUNS it.
4. **Actually selected** — the command runs the NEW artifact. A test file a CI filter (`--grep @smoke`, a path glob, a tag) never selects is dead coverage; an asset/CSS defect only `build` catches means `build` belongs in `acceptance`. **For EACH artifact a stage produces, ensure at least one acceptance command would FAIL if that artifact were broken.**
5. **Grounded like a code claim** — a cited test precedent ("mirror how X is tested") must EXIST in the named file (grep it, never assume); the fixture must DISCRIMINATE — a plausible-WRONG implementation must fail it (a golden case where right and wrong agree proves nothing) — and must drive the specific BRANCH making the asserted call; and the test targets the layer where the behavior actually LIVES (find the implementing file before naming the test file).

**Per-stage gate coverage — the producing stage gets its own signal.** Each stage's `acceptance` must include every repo-wide gate (lint, typecheck, FULL test suite, build/bundle budget) that would catch defects in the files THAT stage writes; deferring lint/typecheck to a downstream dependent stage means the defect surfaces where it wasn't written (a logged #1 recurring failure). If a stage edits file X, its acceptance runs the command that exercises X — a spike that edits a shared test file runs the full test command, not only its own subsystem's. **Copy the repo's FULL canonical gate VERBATIM** — read the real scripts (`package.json` / Makefile / cargo aliases) and use them: frozen-lockfile install, typecheck across ALL configs, lint with warnings-as-errors, format-check, full tests, build. Scoped subsets (`eslint src/foo`, a single-config `tsc --noEmit`, lint without `--max-warnings=0`, skipping `format:check`) under-cover the stage's own files — a logged repeat failure across four plans.

**Every gate must be GREEN at BASELINE, in the environment that will run it.** "Copy the full
canonical gate" is a floor on COVERAGE, never a licence to ship a command the plan's author has
never watched pass. Acceptance is what `loom stage complete` runs, so a criterion that is red for
reasons the stage's diff cannot touch does not report a problem — it STRANDS a finished,
committed stage, and the agent cannot wave it through (`--no-verify` needs a one-time operator
proof derived from `.loom/work/admin.token` (or the legacy `.work/admin.token`), which the session sandbox denies by design). Before any
command enters `acceptance`:

1. **Run it, at HEAD, before the plan exists.** Not "it is the repo's standard command" — RUN it.
   Record the observed baseline in the plan prose (`cargo test --all-targets` at HEAD: N passed,
   0 failed) so a stage agent inheriting a red gate can tell your evidence from your assumption.
2. **Run it where the STAGE will run it** — from a worktree, under the stage's own sandbox — not
   from your main checkout with your own permissions (`SKILL.md` Section 8, `sandbox.md`).
3. **A red baseline is a fork in the plan, never a footnote.** Either the plan OWNS the repair (a
   first stage that fixes or guards the failing target, with that repair as its own acceptance),
   or the gate EXCLUDES the known-red target by an explicit narrow filter (`--skip <name>`,
   `-E 'not test(...)'`, a self-skip guard on the test itself) plus a one-line note naming the
   coverage given up and why. Inheriting someone else's red gate makes EVERY stage in the plan
   un-completable, and the failure surfaces at the worst possible moment: after the work is
   finished and committed.
4. **Environment-dependent tests are the usual culprit** — anything needing a daemon, a socket, a
   display, a container, or the network. A test that cannot pass in the environment the gate runs
   in belongs behind a self-skip guard, not inside a stage's acceptance list. (In this repo the
   tmux e2e suite cannot create an `AF_UNIX` socket under a session sandbox, and says so in its
   own file header — which is exactly the kind of header a plan author must read before writing
   `--all-targets` into a gate.)
5. **Scratch directories and `HOME` — enforced by `loom plan verify`.** It rejects a bare
   `mktemp -d`, `HOME=` assigned from a variable or substitution, a `TMPDIR=<absolute path>`
   override, a literal `/tmp/` path or grant, an absolute grant missing on the host, and a
   `mkdir`, `touch` or redirect aimed outside the worktree (logged: a criterion with `HOME=""`
   "passed" by writing the operator's real `~/.loom/config.toml`). Write
   `H=$(mktemp -d "${TMPDIR:-/tmp}/<name>.XXXXXX") && [ -n "$H" ] && ...`. Repair at run time
   with `loom stage amend --field acceptance|wiring|wiring-tests`.
6. **The full suite runs once, in integration-verify.** A standard stage's acceptance proves the
   code that stage wrote: `cargo test --lib <module>::`, `cargo test --test <target>`, or a name
   filter over its own tests, plus the build and lint lines. Do not put `cargo test --all-targets`
   (or an unfiltered `cargo test` or `cargo nextest run`) on every stage: each copy runs the whole
   suite in the agent's own checks, again in `loom stage complete`, and again for every
   adjudication of that criterion.
   Reserve the unfiltered run for the integration-verify stage, and let sandbox-sensitive tests
   self-skip rather than carrying a `--skip` list. `loom plan verify` warns when a full-suite run
   appears outside integration-verify.

**Criteria about an artifact the stage has yet to PRODUCE — the baseline rule cannot reach them.**
A criterion asserting a fact about a file the stage will generate MUST be red at HEAD, so "run it
and watch it pass" tells the author nothing and the command gets skipped. The expression is then
never executed even once, and every constant inside it is never measured. This is the costliest
plan defect there is: it strands a finished, committed, CORRECT stage at the very end, after all
the work is paid for. (Logged: one stage, implemented correctly and committed, went green on 86 of
its 88 criteria and stalled on two that no artifact could ever satisfy. The agent's route out —
`loom stage dispute-criteria`, `SKILL.md` Section 9 — is real and autonomous, and still costs far more than
one dry run at authoring time.) Four rules, each of them from that stage:

**1. Invariant or measured constant — only ONE of the two may carry a number.** An INVARIANT holds
for every correct implementation ("no month band references a city id absent from the cities map";
"every country carries 12 monthly entries"). A MEASURED CONSTANT asserts a value that comes from
the data ("DE has 1 zone"; "nodata count is 0"; "198 countries join"). Prefer the invariant every
time: it states what you actually mean and survives a source-data bump. A measured constant enters
the plan ONLY with provenance — you ran the measurement against the real source and recorded the
command and its output in the plan prose. Recalled, reasoned-about, and arithmetic-derived
constants are plan defects. (Logged: `.zonesByCode.DE | length == 1`, written because "Germany has
one timezone" is a plausible fact. The plan pinned tzdb 2026c, whose `zone1970.tab` gives DE two
rows — Europe/Berlin, and Europe/Zurich for the Büsingen exclave — and which uses Germany as its
own worked example of a multi-zone country. Under the stage's own parse rule the value was 2, and
the implementer was right to refuse to corrupt the artifact to make the criterion green.) When the
measurement cannot be made at authoring time, write the invariant, never a guessed constant.
**Absolutes — "zero", "always", "never", "every" — are measurements too.** Measure them, or write
the bounded criterion a correct implementation can satisfy. (Logged, same plan: the brief promised
"measured nodata count is ZERO for both sources"; one city sits on a water cell in one of the two
rasters, which the implementer had to discover and handle.)

**2. A criterion you have not RUN is not a criterion — dry-run it against TWO fixtures.**
Hand-build the smallest artifact carrying the shape the stage will produce; a few lines of JSON is
enough.

```text
1. run the EXACT criterion string against the good fixture   → observe exit 0
2. break the fixture in the specific way the criterion exists to catch
3. run the EXACT same string again                           → observe a non-zero exit
```

Passes both or fails both = not discriminating = dead weight (Realizability gate 5). Record the
fixture and both observed exits in the plan prose, the way the baseline rule records an observed
gate baseline. One run against a three-line fixture would have caught rule 3's first example.

**3. jq and shell hygiene inside `acceptance`** (the mechanics that made that criterion possible):

```text
# ❌ the pipe rebinds `.` to the id array, so `.cities` indexes an array and jq ERRORS (exit 5)
[.countries[].months[] | .lowCityId, .highCityId] | unique - ([.cities | keys[]]) | length
# ✅ bind the root FIRST, cross-reference through it, and let -e decide the exit code
jq -e '. as $d | (([$d.countries[].months[] | .lowCityId, .highCityId] | unique) - [$d.cities | keys[]] | length) == 0' out.json
# ❌ prints `false` and exits 0 — a criterion that can only ever pass
jq '.x == 1' out.json
```

- **`.` is rebound by every `|`.** An expression touching two parts of one document binds the root
  before the first pipe (`. as $d | ...`). Cross-referencing after a pipe is silent until executed.
- **One assertion per criterion.** Two simple criteria beat one clever pipeline: a compound
  expression that fails does not say which half broke.
- **Every expression decides its own exit code** — `jq -e`, or a numeric comparison on the output.
- **Prefer a checked-in script, or a case in the repo's own suite, to a clever YAML one-liner.**
  Code in the repo gets review, tooling, and its own failure message; a one-liner in YAML gets none
  of those. Reach for jq in `acceptance` only for a one-line shape check you have dry-run.

**4. A number stated twice must agree with itself.** Before STOP, list every number the plan states
— counts, thresholds, versions, indices — and confirm each appears with ONE value throughout; for
each derived number, name the command that produced it. (Logged: the same plan gave 197 joined / 40
without a city in one section and 198 / 39 in another, and the implementing agent had to re-derive
both.)
