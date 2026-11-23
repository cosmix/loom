# hooks-and-delivery / w2-workflows: pinned actions, hardened release job, CI selection, syntax check

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/delivery.md`, sections CR-02, I2, I3, I4 and I5 (I4 only), CR-69 (sub-claims a, c, d).

You run no `git` command, touch no path under `.loom/`, and run no tests. You answer from the base tree: no other worker's edit is visible to you. The only dependency on another worker is the file `scripts/tests/test_install.py` and the step names below; both exist in the base tree.

## Findings covered
- CR-02: the minisign download is unverified, `${{ }}` sits inside `run:` bodies of `release.yml`, no `uses:` is pinned to a commit, `ci.yml` and `release.yml` have no top-level `permissions:`, and nothing updates pins.
- I2: the Rust change filter in `ci.yml` omits `plan-plugin/**`, an embedded asset root.
- I3: `ci.yml` never runs `scripts/tests/test_install.py`.
- I4 / CR-69(b): the macOS hooks job never builds `loom` and never requires the real-binary hook test.
- CR-69(a): CI never runs `loom knowledge check --strict`.
- CR-69(c) and (d): `scripts/check-hook-syntax.sh` skips Python hooks and the two installers.

## Files
Own (write):
- `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/workflows/nightly.yml`, `.github/workflows/codeql.yml`
- `.github/dependabot.yml` (new)
- `scripts/tests/test_workflows.py` (new)
- `scripts/check-hook-syntax.sh`
Read only: `loom/build/assets.rs` (`*_ROOT` and `*_TEMPLATE` constants, `ASSET_ROOTS`), `install.sh`, `scripts/tests/test_install.py`, `scripts/tests/test_dev_install.py`.
Frozen (never edit): `loom/tests/workflow_pin_contracts.rs`, `loom/tests/installer_decline_contracts.rs`.
No shell test goes into `loom-hooks/tests/`; none is needed.

## Changes

### 1. CR-02 - pin every `uses:` to a commit
In all four workflow files, replace each `uses: <action>@<tag>` with `uses: <action>@<sha> # <tag>` (a single space, then `#`, then the original tag text; keep any `with:` block). Use exactly these SHAs (resolved 2026-11-23 with `git ls-remote`, annotated tags peeled):

```text
actions/checkout@v7                    3d3c42e5aac5ba805825da76410c181273ba90b1
actions/download-artifact@v8           9000827ccba6bdab643e8b6fd33ac0654aef8333
actions/setup-go@v5                    40f1582b2485089dde7abd97c1529aa768e1baff
actions/setup-java@v4                  cf277c60eb25467037889841efdb72551f06f6c3
actions/setup-node@v4                  49933ea5288caeca8642d1e84afbd3f7d6820020
actions/upload-artifact@v7             cf430e030ddbb5b0abf93d22962f4752f3646cd9
astral-sh/setup-uv@v6                  d0cc045d04ccac9d8b7881df0226f9e82c39688e
dorny/paths-filter@v4                  ceb8a2b8f2d89434be7ff52d3de7ec3738c5cc9d
EmbarkStudios/cargo-deny-action@v2     17aeb3c839e42274e2b8adf721c8c65edfdc74dd
github/codeql-action/*@v4              1bd86f2fdb4b86187fa4059f16f31a3202acc9ea
gradle/actions/setup-gradle@v4         ed408507eac070d1f99cc633dbcf757c94c7933a
mozilla-actions/sccache-action@v0.0.9  7d986dd989559c6ecdb630a3fd2557667be217ad
oven-sh/setup-bun@v2                   0c5077e51419868618aeaa5fe8019c62421857d6
softprops/action-gh-release@v3         8a0cd651e5b2365bd640854a3fa5dfc70e4875f3
Swatinem/rust-cache@v2                 d1527c8f67ef9af357290475d4af4b0fab8f12e0
dtolnay/rust-toolchain@stable          39008827e6b4319a349e9a42dfc90579dd25f1ca
```

`github/codeql-action/init` and `.../analyze` (in `codeql.yml`) both take the `github/codeql-action/*` SHA, comment `# v4`. `dtolnay/rust-toolchain@stable` is a branch head: comment `# stable`, and every use keeps its explicit `with: toolchain: stable` (add `with:\n  toolchain: stable` where a use has no `with:` block; where a block exists without `toolchain`, add the key). Counts to reach: `ci.yml` 34 lines, `nightly.yml` 30, `codeql.yml` 3, `release.yml` 10 (an `rg 'uses:\s*\S+@[0-9a-f]{40}'` per file must equal these).

### 2. CR-02 - permissions
Add a top-level `permissions:\n  contents: read` block to `ci.yml` (between the `on:` block and `concurrency:`) and to `release.yml` (between `on:` and `env:`). Keep the job-level blocks: `ci.yml` `changes` (`contents: read`, `pull-requests: read`) and `audit` (`contents: read`), `release.yml` `sign-and-release` (`contents: write`). `nightly.yml` and `codeql.yml` already have top-level blocks.

### 3. CR-02 - `release.yml` run bodies
- Step "Install minisign" (symbol: `name: Install minisign`): add `MINISIGN_LINUX_SHA256: "f0a0954413df8531befed169e447a66da6868d79052ed7e892e50a4291af7ae0"` to the workflow `env:` next to `MINISIGN_VERSION`. Change the download to `curl -fsSL "<same URL>" -o minisign.tar.gz`, then `echo "${MINISIGN_LINUX_SHA256}  minisign.tar.gz" | sha256sum -c -`, then the unchanged `tar`, `sudo mv`, `rm`, `minisign -v` lines. The digest is the 0.11 Linux tarball's, computed from the published asset; the check line must come before `tar`.
- Move EVERY `${{ ... }}` out of every `run:` body in `release.yml` into a step-level `env:` and read the shell variable instead. Step by step:
  - "Check release asset contract": `env: TARGET: ${{ matrix.target }}` and `BINARY_NAME: ${{ matrix.binary_name }}`; the awk `-v target="$TARGET"` and `test "$expected" = "$BINARY_NAME"`.
  - "Build release binary", "Strip binary", "Rename binary": `env: TARGET` / `BINARY_NAME` likewise; quote every use.
  - "Extract version from tag": `env: EVENT_NAME: ${{ github.event_name }}`; the test becomes `[ "$EVENT_NAME" = "workflow_dispatch" ]`.
  - "Extract changelog section": `env: VERSION: ${{ steps.version.outputs.version }}`; `bash scripts/changelog-section.sh CHANGELOG.md "$VERSION"`.
  - "Generate release notes": `env: VERSION`, `TAG: ${{ steps.version.outputs.tag }}`, `REPO: ${{ github.repository }}`, `CHANGELOG_FOUND: ${{ steps.changelog.outputs.found }}`. Build `release_notes.md` so its text is byte-identical to today's output: use `printf` with `%s` for the three lines that carry a value (the `## loom <version>` heading, the `curl -fsSL https://raw.githubusercontent.com/<repo>/main/install.sh | bash` line, and the "Pinned downloads ..." paragraph with `<repo>` and `<tag>`), and quoted heredocs (`<< 'RELEASE_NOTES_EOF'`) for the static text between them. No `${{` may remain in the body.
  - `with:` inputs of `softprops/action-gh-release` are not shell; leave them.
- A `${{ }}` in `name:`, `if:`, `with:`, `env:` or `matrix` values stays.

### 4. I2, I3, I4, CR-69(a) in `ci.yml`
- I2: add `- "plan-plugin/**"` to the `rust:` filter after `- "codex/skills/**"`.
- I3: in job `hook-syntax`, after the step "Development installer regression", add `- name: Installer regression` with `run: python3 -m unittest discover -s scripts/tests -p test_install.py`, and after it `- name: Workflow regression` with `run: python3 -m unittest discover -s scripts/tests -p test_workflows.py`.
- I4: in job `hooks-macos`, after the step "Assert bash is 3.2" (id `bash32`) and before "Parse every shell hook", add the same `if: ${{ !cancelled() && steps.bash32.outcome == 'success' }}` guard on: `Install Rust toolchain` (`dtolnay/rust-toolchain` pin, `toolchain: stable`) and `Build the loom binary` (`run: cargo build --locked --bin loom --manifest-path loom/Cargo.toml`). Change the macOS `Run the hook tests` step to `run: LOOM_HOOK_TEST_REQUIRE_BINARY=1 bash loom-hooks/tests/run-all.sh`. Update the job comment: the real-binary hook test must run on macOS.
- CR-69(a): the binary is built in job `hook-syntax` (step "Build the loom binary"). After that step add `- name: Knowledge check` with `run: loom/target/debug/loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt`. Add `- "doc/loom/knowledge/**"` to the `shell:` filter. The baseline file is tracked and the command exits 0 on the base tree.

### 5. `.github/dependabot.yml` (new)
`version: 2`; one `updates` entry: `package-ecosystem: "github-actions"`, `directory: "/"`, `schedule: {interval: "weekly"}`, `commit-message: {prefix: "chore(ci)"}`.

### 6. CR-69(c) and (d) in `scripts/check-hook-syntax.sh`
- (d) append `"$repo_root/install.sh"` and `"$repo_root/dev-install.sh"` to the list produced inside the process-substitution group at the end of the file loop (add a `printf '%s\n' ...` of the two paths to that group). Update the header comment to name them.
- (c) after the shell loop, add a Python loop: require `python3` (`command -v python3` or print `check-hook-syntax: python3 is required to parse the Python hooks` to stderr and exit 1); iterate every `*.py` under `$repo_root/loom-hooks` plus every `*.sh` there whose first line contains `python` (`skill-trigger.sh`); parse each with `python3 -c 'import ast, sys; ast.parse(open(sys.argv[1], "rb").read(), sys.argv[1])' "$file"` (writes no bytecode). Count failures into the existing `failed` and `checked`, print `SYNTAX ERROR: <relative path>` on stderr like the shell loop. Feed the loop from a here-string (`done <<<"$py_files"`, with `[ -n "$file" ] || continue`), not a new process substitution. Bash 3.2 compatible.

### 7. `scripts/tests/test_workflows.py` (new)
Standard library only (no PyYAML): scan lines. Helpers: `ROOT = Path(__file__).resolve().parents[2]`; a function that yields every `run:` body of a workflow (inline value or following lines indented deeper than the key, `- run:` form included). Tests, each a `unittest.TestCase` method:
- every `uses:` line in `.github/workflows/*.yml` has a ref matching `[0-9a-f]{40}` and a trailing `# <tag>` comment;
- `ci.yml` and `release.yml` have `^permissions:` at column 0 followed by `contents: read`;
- no `run:` body of `release.yml` contains `${{`;
- in `release.yml`, the `curl` line that downloads minisign has `-fsSL`, and a `sha256sum -c` line sits between it and the `tar` line;
- every `scripts/tests/test_*.py` file name appears in a `run:` body of `ci.yml`;
- the `rust:` filter of `ci.yml` covers each constant `const <NAME>_ROOT|_TEMPLATE: &str = "<path>";` in `loom/build/assets.rs`: a pattern equal to the path, `<path>/**`, or a glob (`fnmatch`) matching a template file name; include `WEB_DIST_ROOT` (`web/dist/**`); fails today for `plan-plugin`;
- the `hooks-macos` job text contains `cargo build` and `LOOM_HOOK_TEST_REQUIRE_BINARY=1`;
- the `hook-syntax` job text contains `knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt` and the `shell:` filter contains `doc/loom/knowledge/**`;
- `.github/dependabot.yml` names the `github-actions` ecosystem;
- `scripts/check-hook-syntax.sh`, copied into a temp tree (`scripts/` plus `loom-hooks/ok.sh`, `install.sh`, `dev-install.sh` holding `true`), exits 0; with `install.sh` replaced by `if then` it exits 1 and names `install.sh` on stderr; with an added `loom-hooks/bad.py` holding `def (` it exits 1 and names `bad.py`.

## Interfaces shared with other workers
None. Names the contract tests read: `.github/workflows/*.yml`, `loom/build/assets.rs` constants, step text above.

## Tests
`scripts/tests/test_workflows.py` as listed. Contracts to turn green (file `loom/tests/workflow_pin_contracts.rs`): `no_action_is_pinned_to_a_mutable_tag`, `release_run_bodies_hold_no_workflow_expression`, `rust_filter_covers_every_embed_root`. The orchestrator runs the Python tests; you do not.

## Proof
You run no tests. Check YAML shape by reading; run `bash -n scripts/check-hook-syntax.sh` and `python3 -B -c "import ast,sys; ast.parse(open('scripts/tests/test_workflows.py').read())"` (parse only). State in your report that `cargo check --all-targets` does not apply because no Rust file changed.

## Traps
- A pin without the `# <tag>` comment passes a naive SHA check and breaks Dependabot's tag tracking.
- `${MINISIGN_VERSION}` inside the existing curl URL is a shell variable fed by workflow `env:`; it is fine. A `${{ env.X }}` in a run body is not.
- The release-notes body has Markdown fences and backticks: an unquoted heredoc would run them as command substitution. Quoted heredocs for static text only.
- `ci.yml` `matrix.cargo-args` and `matrix.test-args` stay in the `run:` of the Rust test job: only `release.yml` is held to "no expression in a run body".
- Adding the top-level `contents: read` to `ci.yml` must not remove `pull-requests: read` from `changes`; job-level blocks replace, not merge.
- `check-hook-syntax.sh` already runs under `set -euo pipefail`: a failing `python3` in a command substitution needs `|| ...` handling like the shell loop's `if ! bash -n`.
- Do not edit `skills/**`, `doc/**`, `loom/tests/**` or `loom-hooks/**`.
