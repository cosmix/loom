# WB3 — Trim the release workflow, correct the stale module doc

Tier: codex `gpt-5.6-luna`, effort `xhigh`.

## Files you own (write)

- `.github/workflows/release.yml`
- `loom/src/skills/install_layout.rs` — doc comments, and the deletion of `apply_install_layout`
- `loom/src/skills/mod.rs` — drop one re-export
- `README.md` — one line
- `loom/CONTRIBUTING.md` — one line

## Entry points

- `.github/workflows/release.yml`: three consecutive steps in the release job, identified by their
  `name:` values, NOT by line numbers — `Generate SHA256 checksums` (KEEP, untouched),
  `Package additional assets` (DELETE the whole step) and `Add asset checksums` (DELETE the whole
  step) — plus the three `release/CLAUDE.md.template`, `release/agents.zip`, `release/skills.zip`
  entries in the upload list near the end of the file (`release/SHA256SUMS.txt` sits immediately
  above them and stays).
- The module doc comment at the top of `loom/src/skills/install_layout.rs` (lines 1-9) AND the
  doc comment on `apply_install_layout` (around line 63-66, "Runs immediately after
  `loom self-update` extracts `skills.zip`"), which the first draft missed.

## What to change

### `release.yml`

Stop publishing the three config assets. `loom self-update` no longer downloads them — every asset
now travels inside the signature-verified binary.

Remove, as whole steps (a step's `- name:` line, its `run: |` line and its body go together — an
empty `run:` block passes every grep and fails the next release):

- the `Package additional assets` step (`cp CLAUDE.md.template release/` and both `zip -r`
  lines);
- the `Add asset checksums` step (`sha256sum CLAUDE.md.template agents.zip skills.zip >>
  SHA256SUMS.txt`);
- `release/CLAUDE.md.template`, `release/agents.zip` and `release/skills.zip` from the upload list.

Keep, untouched:

- every platform binary and its `.minisig`;
- the `Generate SHA256 checksums` step (`sha256sum loom-* > SHA256SUMS.txt`) and the
  `release/SHA256SUMS.txt` upload — it is still published for humans and the release notes still
  point at it;
- everything else in the workflow.

After the edit: `rg -q "agents\.zip|skills\.zip|CLAUDE\.md\.template" .github/workflows/release.yml`
finds nothing; `rg -qF "sha256sum loom-* > SHA256SUMS.txt"` still succeeds; the file still holds at
least three `.minisig` references; and
`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"` exits 0 — the
stage runs all four as acceptance criteria.

### `install_layout.rs` and `skills/mod.rs`

Its module doc says the layout is restored "after `loom self-update` extracts a fresh copy of every
skill" and that "`loom self-update` re-extracts the `skills.zip` release asset into
`~/.claude/skills/` on every run". Neither is true any more: `loom install-assets` writes each
skill directly to its final directory from the binary's embedded table, splitting core and
catalogued skills per skill as it goes.

`apply_install_layout` is now dead: its only production caller was `update_config_files`
(`self_update/mod.rs:328`), which another worker in this stage deletes, and the placer never
called it (under `Core` it moved ANY `loom-*` directory into the catalog, a user's own included,
and under `All` it removed the catalog wholesale — both contradict the placer's preservation
invariant). Delete `apply_install_layout`, `restore_all`, `split_core`, any helper only they
used, and their tests; delete `pub use install_layout::apply_install_layout;` from
`loom/src/skills/mod.rs` (the `SkillLayout` re-export the previous stage added stays). Keep
`SkillLayout`, `SkillLayout::read` and `infer`, and rewrite the module doc to describe what
remains: the recorded layout and how it is read. An acceptance criterion asserts
`rg -q "apply_install_layout|restore_all|split_core" loom/src` finds nothing.

### `README.md` and `loom/CONTRIBUTING.md`

Another worker in this stage renames the command from `self-update` to `update`, with no alias.
Two documents name the old spelling and must follow:

- `README.md:272` — the `loom self-update` command line becomes `loom update`.
- `loom/CONTRIBUTING.md:124` — the reference reads
  `` `src/commands/self_update.rs:18` (`MINISIGN_PUBLIC_KEY` constant) ``. That path is **already
  wrong**: the constant lives in `src/commands/self_update/signature.rs`, and it is not on line 18.
  Correct the path, drop the stale line number rather than guessing a new one, and leave the module
  name `self_update` alone — only the CLI spelling changes, not the module.

Afterwards `rg -q "self-update" README.md loom/CONTRIBUTING.md` must find nothing, and so must
`rg -q "src/commands/self_update\.rs" loom/CONTRIBUTING.md`, while
`rg -qF "self_update/signature.rs" loom/CONTRIBUTING.md` succeeds. Do not sweep
`doc/loom/knowledge/` — that is the knowledge-distill stage's job.

## Done means

- `cargo build --manifest-path loom/Cargo.toml` succeeds and
  `cargo test --manifest-path loom/Cargo.toml --lib skills::` passes.
- The `rg` checks above behave as described.
- The workflow parses under the `python3 -c "import yaml; …"` line above, and you have read the
  whole job you edited top to bottom.

## Constraints the graph will not show you

- Do not run `git` at all. Do not run the test suite, the linter or the formatter.
- Do not touch `install.sh` or anything under `loom/src/commands/self_update/` — other workers in
  this stage own those.
