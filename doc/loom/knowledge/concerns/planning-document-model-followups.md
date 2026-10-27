---
sources:
- loom/src/planning/mod.rs
---
# Planning Document Model Followups

> Planning TODO

## Open Items in `loom::planning`

Reviewer suggestions not applied, with the file that owns each. See [Planning Document Model](../architecture/planning-document-model.md).

**Anchors and parsing**

- `sections.rs`: a punctuation-only heading yields anchor `""`, which `find("")` shadows with the preamble; later duplicates then take `-1`. Skip such headings in `scan_headings` or give them a distinct anchor.
- `sections.rs`: only ATX headings are recognised while `doc/product/section_03.md` says CommonMark; add a test or a note for setext headings.
- `xref.rs`: `at_token_start` tests any occurrence of the path in the text, so a line holding one path both with and without a `src/` prefix yields a duplicate `PathToken`; it also drops a path wrapped in `**`, `_` or written after `key=` (add `*`, `_`, `=`, `{` to the accepted preceding characters). A link with an empty fragment (a link ending in `#`) gets anchor `Some("")` and is reported broken while `find("")` addresses the preamble; filter empty fragments to `None`.
- `pressure.rs` and `vision.rs`: heading matches are exact for "Pressure tests" but case-insensitive for "Open questions" and "Specification sections"; pick one rule.
- `implements.rs`: a substring test finds where metadata starts; use `DocSections.metadata.start_line`.

**Hashing and cost**

- `project.rs`: the xref lookup closure clones a full `DocSections` per anchored reference (the owned return type is pinned by `xref::references`; memoise per target inside `references` if profiling shows it). `derive` recomputes `sections_of` for every document on every refresh, up to three times per plan (with `pressure.rs`); reuse the `ProjectDoc` of documents whose source was not re-read.
- `problems.rs`: `brief_problems` re-runs `collect_inputs` for every unlocked plan on each derive, so each watcher batch re-reads all briefs; `LockedSet::scan` does the same for locked plans, so caching one side changes nothing in order of magnitude.
- `Project::derive` walks `doc/` twice per refresh (`discover` in `project.rs` and inside `LockedSet::scan`), so tree and locked set can disagree for one refresh. Add `pub(crate) LockedSet::scan_tree(root, &DocTree)`.
- `Project::load` caches every Markdown file up to 8 MiB across up to 20,000 files; add a total byte budget that sets `read_error` once exceeded. `discover::list_dir_no_follow` materialises a whole directory before the entry cap applies.

**Paths, locks and symlinks**

- `paths.rs`: `fold` lowercases only; APFS also folds Unicode normalisation, so a non-ASCII slug spelled NFD misses the bundle prefix lock (slugs are ASCII in practice; note it in the module doc or normalise).
- `paths.rs`: `locked_paths()` is built from discovery, which skips dotfiles and editor temporaries, so hidden files in a locked plan's bundle are refused by `classify_write` but absent from the spawn-time deny list.
- `LockedSet::scan` fails entirely on one non-UTF-8 bundle name, one non-UTF-8 `.md` input or an unreadable directory (fail closed per spec), so one file creation disables load and every write decision. Consider recording it as invalid and locking the path. Cost is O(locked plans x inputs) per call, which matters once the hook calls it under a 10 s fail-open timeout.
- `plan::inputs::collect_inputs` follows symlinks (`is_dir`, `read_dir`, `read_to_string`) and `glob::glob` follows a symlinked directory inside the repo; both run unattended from `LockedSet::scan` and `brief_problems` over a session-writable `doc/`. A planted symlink can make them crawl, hang, or put outside entry names into a `broken-brief-reference` message. Reject a pattern whose literal directory prefix contains a symlink before globbing, or list with `list_dir_no_follow`.
- `discover::read_doc_bytes` accepts a leaf symlink reached through a symlinked directory component when it resolves inside `doc/`, unlike a non-symlink leaf and unlike discovery; gate callers on discovery membership or run `symlink_component` on the parent. `fs::safe_read::read_bounded` does not check `nlink` (`open_regular_no_follow` does).
- `runtime.rs`: `check_state` accepts a path with a trailing `/.`, because `file_name()` ignores it and `lstat` on `KEY/.` follows a symlink named `KEY`; reject a state path whose components are not only `RootDir` and `Normal`.
- `project.rs`: `read_error` carries `{error:#}`, which can include `Failed to open dirfd at <abs root>`; the planning server must not forward it raw to the browser.
- `plan::inputs` depends on `planning::discover::MAX_DOC_BYTES` while `planning::briefs` and `planning::paths` depend on `plan::inputs`; a neutral home such as `fs::safe_read` for the shared cap removes the cycle.
- `watch.rs`: an own summary write that pairs as a rename with an unrelated removed file of equal content is not suppressed (extreme edge).
- The lifecycle prefix grammar has copies with slightly different semantics: `commands/usage/discovery.rs` `normalize_plan` hardcodes the three literals, `plan::inputs::bundle_dir` trims repeatedly, `paths.rs` `case_canonical_plan` strips once. Import `fs::plan_lifecycle` everywhere.

**CLI and tests**

- `commands/plan/check.rs` prints step marks with `{mark:?}`; give `StepMark` an explicit label. `println!` panics on a closed pipe (EPIPE) like every `loom` command; the fix belongs in `main.rs`, not here.
- Not asserted: `ProjectState.sections_listed`, `sections_written` and `metaplans` in `checklist_json_shape` (it pins `plans_by_state` and `locked_files`); `Project::load` with an unreadable Markdown file raising no `missing-title`; exit 2 on a `Project::load` error such as `doc/` nested deeper than 16 levels; a symlinked document in the watcher and in `LockedSet::scan`; sparse-file and `..`-glob cases of `inputs.rs`.
- `doc/product/section_01.md` says the table's step 2 writes the "lowest-numbered missing section" but the code and `table_order_decides_the_first_missing_section` follow the vision table's order.
- Four plans (`PLAN-adopt-ecc-best-practices`, `PLAN-loom-ui-mods`, `PLAN-model-router-hooks`, `PLAN-web-host-graft-followthrough`) have no `Implements:` line, so `loom plan check` reports `implements-missing` and exits 1 on this repository; `Implements: none` under each title clears it (`section_03.md`).
