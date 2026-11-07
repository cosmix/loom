# gate-noise-and-review W1: the statement detector, cross-file moves, ratchet direction

Tier: sonnet (`loom-software-engineer`). Wave 2, after C1 and C2 are proven. Read
`doc/plans/briefs/fewer-disputes/gate-noise-and-review/common.md` first (design, the pin addition,
code rules).

## Files owned

`src/verify/integrity/mod.rs`, `edits.rs`, `count.rs` (visibility of `read_current` and
`MAX_TEST_FILE_BYTES` only), `gate.rs` (the `describe` wording only; `shortfall` and the pin logic
stay byte for byte), `moves.rs` (new), `moves_tests.rs` (new), `tests.rs`, `tests_statements.rs`
(new), `tests_moves.rs` (new), `tests_ratchet.rs` (new), `src/orchestrator/adjudication/prompt/integrity.rs` (one label).
Read, never edit: `statements.rs` and `ratchet_diff.rs` (C1 and C2 wrote them; their pinned
interfaces are in the stage description and below), `src/testrun/languages.rs`,
`src/context/extract/mod.rs`, `src/context/source_graph/node.rs`.

## Seams (locate by symbol)

- `verify::integrity::scan_changes` (mod.rs): calls `count::base_test_files`,
  `count::total_events`, `edits::assertion_edits`, `edits::ratchet_events`. Callers (unchanged
  signatures): `integrity::scan`, `integrity::current_events`, `stage_service/observer.rs`,
  `commands/stage/integrity_status.rs`, `commands/stage/dispute_kinds.rs`.
- `edits.rs`: `assertion_edits` (runs `git diff --unified=0` per file), `lost_assertions`, `hunks`
  (both become dead: delete them), `normalize` (now `pub(super)`, keep), `file_event`,
  `ratchet_events`.
- `count.rs`: `base_test_files`, private `read_current` and `MAX_TEST_FILE_BYTES` (make
  `pub(super)`), `matchers(profile).assertion` regex.
- C1: `statements::{Statement, assertion_statements(content: &str, assertion: &Regex, profile: &str)
  -> Vec<Statement>}`; `Statement { text, key, start, end }`, `start`/`end` byte offsets into the
  string passed in.
- C2: `ratchet_diff::ratchet_loosens(base: Option<&[u8]>, head: Option<&[u8]>) -> bool`.
- Extractor: `crate::context::extract::{registry, extract_file}`; `FileExtraction.nodes` with
  `SourceNode { kind: SourceNodeKind::Function, scope: Vec<String>, span: Span { start_byte,
  end_byte, .. }, coverage }`. The function name is the LAST element of `scope`. Use only nodes
  whose `coverage.has_symbols()` holds (Full or Partial). Build `registry()` once per scan.
- Pattern to follow for tests: `tests.rs::Repo` (temp repo, base commit on `main`, branch
  `feature`, `repo.events(&ratchet)`). Do NOT copy `events_after_edit` blindly: it writes without
  committing, which the fingerprint counts as changed; that is fine and intended.

## Steps

1. `count.rs`: `pub(super)` on `read_current` and `MAX_TEST_FILE_BYTES`.
2. `moves.rs` (about 200 lines). Define

   ```rust
   pub(super) struct FileDelta {
       pub path: String,
       pub profile: &'static LanguageProfile,
       pub existed_at_base: bool,
       pub base: String,           // lossy UTF-8 content at base, "" for a new file
       pub head: String,           // lossy UTF-8 content now, "" for a deleted file
       pub lost: Vec<Statement>,   // base statements not matched at head; offsets index `base`
       pub added: Vec<Statement>,  // head statements not matched at base; offsets index `head`
   }
   /// Cancel lost statements that moved verbatim into another file; returns, per file whose
   /// statements were cancelled, the paths of the files that cancelled them.
   pub(super) fn cancel_moves(deltas: &mut [FileDelta]) -> BTreeMap<String, BTreeSet<String>>;
   ```

   Cancellation rule, per lost statement `s` of delta A: find the INNERMOST function node of A's
   base content whose span contains `s.start`; then look in every other delta B with the same
   `profile.name` for an added statement `t` with `t.key == s.key` whose innermost head-function
   node has the same last scope segment and the same `normalize(body_text, profile.name)`, where
   identity is the one defined in common.md ("Function identity and collection"): the `normalize`d
   decoration prefix, the line-head text before the span, and the lossy UTF-8 body. Write
   `fn identity(content: &str, node: &SourceNode, profile) -> String` once, used for both sides, in
   `moves.rs` (or `moves_identity.rs` beside it if `moves.rs` nears 400 lines). First match wins; both
   statements are removed, one-for-one. A destination delta with `existed_at_base == false` also needs
   `collected(..)` true (common.md): Rust via `chain_problems` over a head-file reader; other
   languages via the profile's `test_file_globs`. No function node (module-level assert, no
   grammar, parse error, oversized) means no cancellation. Ground the `chain_problems` argument shape
   by reading `src/verify/contracts/collection/rust/chain.rs::candidates`; if it cannot answer for a
   `src/*_tests.rs` file, write the smaller walk (root files `src/lib.rs`, `src/main.rs`,
   `src/bin/*.rs`, `tests/*.rs`, each following plain `mod x;` declarations, `#[path]` ignored) in
   `moves_collect.rs` and `loom memory note` the choice. Extract a delta's functions at most once and only when
   it has lost or added statements.
3. `edits.rs`: replace the diff pipeline.
   - `file_deltas(repo, changes) -> Result<Vec<FileDelta>>`: for every path in `changes.files` that
     `languages::for_path` maps to a profile: base content via `git cat-file blob <base>:<path>`
     when the path is in the base listing (use the `base_files` list, `existed_at_base`), else "";
     head content via `count::read_current` unless `current_sha256(changes, path)` is `None`
     (deleted). Statements via `assertion_statements(.., &matchers(profile).assertion,
     profile.name)`. `lost`/`added` are multiset differences by `key`: walk one side in order,
     consuming matching keys of the other side's counts; leftovers are the result.
   - `assertion_events(changes, deltas, partners)`: one `EventKind::AssertionEdit` event per delta
     with `existed_at_base` and a non-empty `lost`; `detail` = each lost `text` cut with
     `truncate_to_byte_limit(.., 600)`; `current_sha256` per the pin rule in common.md (own sha when
     the path has no partner).
   - `ratchet_events(repo, changes, ratchet_files)`: for each distinct listed path that is in
     `changes.files`, read base (`git ls-tree` first: absent means `None`; then `cat-file blob`) and
     head (`None` when deleted, else `read_current`-style bounded read), and keep the event only
     when `ratchet_loosens(base, head)`.
   - Delete `lost_assertions` and `hunks`. Keep `normalize` and its two unit tests.
4. `mod.rs`: add `mod moves; mod statements; mod ratchet_diff;` if the foundation did not, and
   rewrite `scan_changes`: `base_files`, `total_events`, `let mut deltas = edits::file_deltas(..)?`,
   `let partners = moves::cancel_moves(&mut deltas)`, `edits::assertion_events(..)`,
   `edits::ratchet_events(..)`. Update the module doc and `IntegrityEvent.detail` doc ("the lost
   assertion statements") and the `EventKind::Ratchet` meaning ("a ratchet file that loosens").
5. `gate.rs::describe`: `({n} assertion statement(s) removed or changed)`; the ratchet line reads
   `(ratchet file loosened from base)`. Do not touch `shortfall`.
6. `prompt/integrity.rs`: the label `Removed or changed assertion statements:`.
7. Tests (all in-crate; keep every file under 400 lines; `tests.rs` is 265 lines, put new cases in
   the new files and share `Repo` by making it `pub(super)`):
   - `tests_statements.rs`: reflow beside a real edit in the same hunk raises nothing; a changed
     continuation line (`3,` to `4,`) raises one event whose `detail[0]` holds the base statement;
     a comment-only edit beside an assertion raises nothing; a statement moved within its file
     raises nothing; a deleted statement raises; python `assert x == (1,\n 2)` reflowed raises
     nothing; a deleted test file raises with every statement in `detail`.
   - skip markers and collection (both raise): a moved Rust test that gains `#[ignore]`; a moved
     python test that gains `@pytest.mark.skip`; a moved test whose body gains `t.Skip()` (go);
     a Rust test moved into a new file no `mod` line reaches (raises) versus the same move with
     `mod calc_more_tests;` added to `src/lib.rs` (cancels); a new Python file named `helper.py`
     (outside the test globs) receiving a moved test raises, `test_more.py` cancels.
   - the accepted pin: a `TI-edit` event accepted at its `current_sha256`; a second assert is then
     lost in the same file; `shortfall` returns Some (this floor is a unit test because it holds at
     the base commit).
   - `moves_tests.rs` (unit: `FileDelta` built by hand from small Rust sources) and `tests_moves.rs`
     (scan level, through `Repo`):
     identical move into a second Rust test file raises nothing and keeps the counts equal; the same
     move with one literal changed raises on the source file; a move into a file of another
     language does not cancel; a Kotlin pair (no grammar) does not cancel; a module-level python
     assert moved raises (no function); the enclosing `mod` differing between files still pairs by
     last scope segment.
   - the pin: an event with a cancelling partner has `current_sha256` different from the file's own
     sha256 and equal to the documented composition; editing the partner changes it; an event
     without partner keeps the plain sha (existing tests assert this already).
   - `tests_ratchet.rs`: a lowered ledger line raises nothing; a raised value and a new key raise;
     a new ratchet file at head raises; a deleted one raises; a file with one `coverage 80%` line among
     otherwise integer lines is unparsable as a whole and raises on any change; the ledger line change is still pinned to the file's sha256.
8. Run once: `cargo test --lib verify::integrity::`.

## Decided values and traps

- Cap per-statement detail at 600 bytes; no cap on the number of lost statements here (the hook and
  the CLI cap their own output).
- Cancellation never removes a count event: `total_events` is computed from lines and stays as is.
- `scan_changes` is also run by the daemon for completion; a panic or an `Err` on an odd file would
  fail the gate for every stage. A file that is not valid UTF-8 is read lossy; an extractor `Err` is
  already folded into `FileCoverage` by `extract_file`.
- The registry is empty when the `source-graph` feature is off: then nothing cancels and the
  behaviour is the pre-cancellation statement comparison. Test that path with an empty `&[]` slice
  passed to the function under test, so `cancel_moves` takes the extractor list from a private
  helper you can substitute (`fn cancel_with(extractors: &[BoxedExtractor], ..)`).
- INTEGRITY: no existing assertion in `tests.rs` should change. If one must, record it with
  `loom memory note "integrity: <test> assertion changed because <reason>"` and tell the
  orchestrator in your report.
- `maintainability-baseline.txt` rows for the integrity files: none change; keep files under 400
  lines by splitting tests.
