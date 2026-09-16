---
description: Add user-facing features since the last release to CHANGELOG.md, under the given next version. Run before cutting a release.
argument-hint: [next-version]
---

$1 is the version this changelog entry is being written for (e.g. `0.9.0`). If it is missing, tell the user to run `/changelog <next-version>` and stop — do not guess a version.

Find where the last release left off: run `git tag --list 'v*.*.*' --sort=-v:refname` and take the first line, or `git describe --tags --abbrev=0 --match 'v*.*.*'` if that is empty. If there are no matching tags, fall back to `git log -1 --format=%H -S'## [' -- CHANGELOG.md` to find the commit that most recently added a version heading. If neither resolves to a ref, stop and tell the user you cannot find a starting point.

Review every commit in `<that ref>..HEAD` for user-facing features only: new commands, subcommands, flags, stage types, plan-YAML fields, operator-visible hooks, new output surfaces, new platform support, and changed defaults an operator would notice, plus anything removed or renamed. Explicitly exclude bug fixes, refactors, internal tests, knowledge/doc commits, CI plumbing, and invisible performance work. `git diff <ref>..HEAD -- README.md` and the diff of `loom/src/main.rs` are the highest-signal cross-checks, since they surface new clap commands and flags directly. Be strict: a large range should still yield a handful of bullets, one per coherent capability rather than one per commit.

Update `CHANGELOG.md` in place:

- Sections are grouped by minor version, headed `## [X.Y.x] - YYYY-MM-DD`. Derive the group from $1: `0.9.0` becomes `0.9.x`; `0.8.2` becomes `0.8.x`.
- If that group's section already exists, merge the new bullets into its existing `### Added` / `### Changed` / `### Removed` subsections and bump the heading's date to today. Do not duplicate a capability already described there — extend the existing bullet instead.
- If it does not exist yet, prepend a new section directly below `## [Unreleased]`.
- Fold anything currently listed under `## [Unreleased]` into the version section being written, then leave `## [Unreleased]` in place with nothing under it.
- Never reorder or reword sections for versions older than the one you are writing.
- There is no `### Fixed` subsection in this file — do not add one.

Match the file's existing voice: one bullet per feature, a bolded short name, an em dash, then one or two sentences of concrete prose naming the actual command or flag involved. Keep each bullet to one long line, no hard wrapping. No marketing language and no LLMisms — no "seamless", "robust", "unlock", "leverage" as a verb, no "not X but Y" constructions, and no closing summary line recapping what you wrote.

When done, tell the user exactly what you wrote (the section heading and the bullets added or merged), and remind them this has to run before cutting the release: `.github/workflows/release.yml` pulls the tagged version's section straight out of `CHANGELOG.md` for the GitHub release notes, so an unwritten section means a release published with no notes.
