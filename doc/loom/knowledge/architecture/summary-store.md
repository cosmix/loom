# Summary Store

>

## Store Layout and Ids

Summaries of the planning documents live in `doc/loom/summaries/` (`store::STORE_DIR`), one Markdown file per id, all
under `loom/src/planning/summary/`. `store::validate_id` accepts only `product`, `engineering` (the two overviews,
`store::is_overview`), `product/section_<2+ digits>` and `plans/<slug>` (first byte alphanumeric, rest `[A-Za-z0-9._-]`,
no `..`); the file is `<id>.md`. `summary_id_for` makes an id for a product section, or for a plan that is the winning
copy of its slug and either sits directly in `doc/plans/` or is named by a metaplan entry. `expected_ids` is every such
document plus `product` (when the vision exists) and `engineering` (when any metaplan exists). `Scope` lists the overview
members in source order (`product`: the vision's rows; `engineering`: the metaplans' entries) and the eligible documents
outside both. `validate_id`'s slug rule is stricter than `planning::slug::plan_slug`, so a plan whose slug holds a space silently gets no summary. `summary_path` returns an empty path for an invalid id; every caller
validates first.

The runtime directory (`planning-document-model.md`, Runtime Directory) holds the store's session state outside the
repository: `claims/`, `drafts/<id>.md` and `summaries.lock`.

## File Format

A summary file is front matter, one `#` title, then headings (`#`, `##`) and paragraphs of sentence lines
(`format.rs`, `format/front.rs`, `format/body.rs`). Front matter (`deny_unknown_fields`):

- `kind`: `document` or `overview`; `of`: the source path (document) or the overview id.
- `file`: the source's content hash, document only; a document must not hold `members`, an overview must not hold `file`.
- `cites`: `path#anchor` (or `path` for the preamble) to the hash of the cited span.
- `members`: overview only, member id to `members_hash`, the `normalized_hash` of that member's sentence ids joined by
  `\n` in order.

Body rules, each a rejection with a 1-based line: no indented lines, list items, table rows, code fences, block quotes,
headings deeper than level 2, HTML or conflict markers; a heading holds no link; a sentence is one line whose citation
links are a trailing run (`[label](target)` separated only by whitespace) with no link before them. At most 15
sentences in a document summary and 30 in an overview (`MAX_DOCUMENT_SENTENCES`, `MAX_OVERVIEW_SENTENCES`).

A sentence id is `s-` plus the first 8 hex digits of the SHA-256 of the whitespace-collapsed text with its links
removed, so rewording changes the id and a link edit does not. Identical text twice in one summary is a duplicate-id
rejection. Link targets classify against the summary's own location as `Source { path, anchor }`, `Sentence { summary,
id }` (another summary's sentence, written `<relative path to the summary>#s-xxxxxxxx`) or `Unresolved`. A plan summary
cites a stage as `PLAN-x.md#stage:<id>`. `scan_links` is linear: its `)` search is guarded by `line.rfind(')')` so a long
run of `[` stays O(n).

## States

`evaluate::evaluate` judges every expected id against the loaded `Project`; document summaries first, so an overview
sentence can ask whether the member sentence it cites is current (`evaluate/cite.rs`).

| Level | State | Meaning |
| --- | --- | --- |
| Sentence | `current` | it has at least one citation and every citation holds: the cited span's hash equals the `cites` entry, or the cited member sentence exists and is current |
| Sentence | `stale` | a cited span changed or vanished, or a cited member sentence is gone or stale |
| Summary | `missing` | no file, or a file that fails validation, holds conflict markers, or names another source; checked first, so a claim on a missing summary reads `missing [claimed by ...]` |
| Summary | `rebuilding` | a claim names it and the file is valid; its sentences keep their states |
| Summary | `stale` | any sentence stale |
| Summary | `incomplete` | all sentences current, but a document's `file` hash differs from its source's, or an overview's `members` map differs from the members' hashes now |
| Summary | `current` | none of the above |

`is_out_of_date` is stale, incomplete or missing. A deleted source drops its member summary and stales the overview
sentences citing it: seal requires every member cited, so the member-removed route to `incomplete` is reachable only by a
hand edit (`engine/tests.rs` pins the stale direction). A summary file that cannot be read (symlink, invalid UTF-8, oversize)
counts as missing for evaluation. `Evaluation.orphans` lists store files no expected id owns.

## Reads, Writes and the Lock

Every read of the store is no-follow and bounded (`write::read_summary`, `fs::safe_read::read_to_string_bounded`), never
`discover::read_doc`: a symlinked summary is a hand edit and counts as missing. Every write goes through `safe_fs` inside
`lock::with_store_lock`, to `.<name>.tmp-<pid>` beside the target, renamed over it; the leading dot keeps the watcher and
discovery from reporting the temp file. A planted symlink anywhere under the store fails the operation.

`summaries.lock` is one exclusive `flock` serialising the engine, planning sessions and a terminal `seal`. A `flock`
belongs to the open file description, so the lock is NOT re-entrant: `write_summary_if` runs its `build` closure under it,
and a build (seal's) that calls `with_store_lock`, `write_summary` or `delete_summary` blocks forever. `lock_exclusive`
has no timeout, so any process holding the file stalls seal, brief and the engine.

`write_summary_if(root, paths, id, expected_file_hash, build, own)` re-reads the file under the lock and writes only when
its `normalized_hash` equals `expected_file_hash` (`None`: absent); otherwise it returns `WriteOutcome::Changed`. The write
functions are `pub` although only the module uses them: `pub(crate)` ones with no caller trip `dead_code` under
`clippy -D warnings`. `OwnWrites` (`planning/watch.rs`) registers the write before the rename so the watcher does not
report it. `Watcher::poll` delivers no batch when the only change is a registered own write, so a test of "own writes
are not reported" must add an unrelated `doc/` change in the same window.

## Seal

`loom plan summary seal <id> [--force]` (`seal.rs`): validate the id, load the `Project`, require the id to be expected,
read the draft `drafts/<id>.md` from the runtime directory (bounded to 256 KiB, no-follow), check the claim, parse the
draft, then in one hold of the store lock check every link and write the canonical file. Rules (`seal/links.rs`):

- every link must resolve; a `Source` link must name a source of this summary (a document: its own source; `product`: the
  vision; `engineering`: the metaplans) and an existing anchor, and records its span hash under `cites`;
- a `Sentence` link is allowed only in an overview, only to a member, and only to a current sentence of a non-missing
  member; a document summary cites only its source (a `stage:` anchor counts as a source anchor);
- an overview must cite every member and every source document at least once, or the seal is refused;
- the front matter is computed, never taken from the draft: `file` from the source's content hash, `members` from each
  member's `members_hash` at seal time.

The seal retries up to 3 times when the file changes between its read and its locked write, then fails with "kept
changing". On success it deletes the claim first, then the draft; a draft-removal failure is logged with `tracing::warn`
and the seal still succeeds. Resealing unchanged content produces no diff (`render` is canonical). A rejection is
`SealError::Rejected` with one entry per problem and draft line (CLI exit 1); I/O failures are `SealError::Io` (exit 2).

## Claims

A claim marks a summary as being rebuilt, so two sessions do not rebuild one (`claims.rs`). It is
`claims/<id with '/' replaced by "__">.json` in the runtime directory, created with `O_EXCL`: the create failing because
the file exists IS the refusal. It records the summary, the session (`LOOM_PLAN_SESSION`, sanitised to 128 characters with
every control character stripped; absent from a terminal), the pid and `created_at`. `claims/` is session-writable, so
reads are no-follow and bounded (4 KiB), and a claim that cannot be parsed still counts as a claim on its file-name id.
`create_claim_as` retries the create once when the existing claim vanished between the failed create and the read, and
errors rather than fabricating a terminal claim; `ClaimHeld` is the typed error the CLI downcasts to exit 1.

`loom plan summary brief <id>... [--note <text>]` de-duplicates the ids, claims all (releasing earlier claims if a later
one is held), and prints the rebuild instructions with the draft path; each current sentence is marked `[keep]`.
`seal` accepts a claim of the caller's own session, or any claim from a terminal with `--force`; a planning session never
forces. `release <id>...` deletes claims; `status [--json]` lists every summary with its state, sentences and claim
(`claim` key, `session` null for a terminal claim; `render_status` passes `claim.session` through `inline_safe`).

## Engine and Rename Rewrites

`SummaryEngine::on_batch` (`engine.rs`) runs once per watcher batch under the server: rewrite renamed sources, delete
orphans, evaluate with the live claims, and report the summaries that newly went out of date (`invalidated`).

A document summary is rewritten when its `of` is an earlier path of its source (`evaluate::same_source`: for a plan, any
`doc/` path classifying as a plan of the same slug) and its `file` hash equals the source's current hash. The rewrite
(`with_source`) moves `of`, the `cites` keys and every body link naming the old path, recomputing the links relative to the
summary, and goes through `write_summary_if` with the file's own hash and the shared `OwnWrites`. The batch's `renamed`
list is only a hint and is not read, so a rename made while no server ran is also caught. A failed rewrite or deletion is
logged and retried by the next batch.

## Configuration

Summary models resolve per key, project tier then user tier then default (`summary/config.rs`, `SummaryModels::resolve`).
See `conventions/model-and-effort-config.md` (`[plan]` Keys and Defaults).

## Tests

Contract tests: `loom/tests/planning_summary_contracts.rs` (harness `tests/planning_summary_support/`) and
`planning_summary_cli_contracts.rs`. A mutation run broke each contract's rule and confirmed red. Two contracts are weaker
than they look: `overview-members-ignore-link-edits` stays green when `members` is not hashed at all, because rewording a
member also stales the citing sentence; the symlinked-store and id-traversal contracts go red only when every guard layer
is broken, since the layers back each other up. `summary_cli_uses_session_state` needs a non-root uid (the runtime check
uses `access(W_OK)`). Tests that mutate `LOOM_HOME` carry `#[serial]`, which excludes only other serial tests.

## Reachability Gotchas

The `reachable` check's source graph does not resolve a module-path call such as `seal::seal(...)` through a module
import: import the function directly or call it by its full `crate::` path, as `dispatch` does for `check::execute`.
