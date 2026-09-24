# review-harvest-gate / codex units — report parser and change fingerprint

For the orchestrator: two `loom-codex-forwarder` units, in the foreground,
`--model gpt-6-sol --effort xhigh`, an explicit 600000 ms Bash timeout. Paste the matching block
below verbatim into each prompt. Tell both NOT to run git themselves (the fingerprint code
calls git through `crate::git::run_git` at run time, which is different) and NOT to touch
`.loom/`. Check `git status --short` after each. W1 declares both modules in
`verify/review/mod.rs`; until W1 reports, a unit's proof is
`rustfmt --edition 2021 --check <file>`.

Shared: DESIGN is `doc/plans/briefs/verification-v2/DESIGN.md`, section D12. Files ≤ 400 lines,
functions ≤ 50. Tests in a `#[cfg(test)] mod tests` at the end of the file.

## CX-1 — `loom/src/verify/review/report.rs`

Parse the `loom-review` block of a reviewer's final message.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding { pub severity: String, pub file: String, pub line: u32, pub claim: String, pub scenario: Option<String>, pub rule: Option<String> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion { pub file: Option<String>, pub line: Option<u32>, pub text: String }
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedReview { pub findings: Vec<Finding>, pub suggestions: Vec<Suggestion>, pub resolved: Vec<String>, pub unresolved: Vec<String> }
/// Err(reason) = malformed: no block, or the last block is not the D12 JSON shape.
pub fn parse_review(final_text: &str) -> Result<ParsedReview, String>;
```

Steps:

1. Find the LAST fenced block whose info string is exactly `loom-review` (three or more
   backticks or tildes; the closing fence matches the opening). No block ⇒
   `Err("no loom-review block")`.
2. Parse it as JSON with `serde_json::Value`. It must be an object; missing arrays are empty;
   unknown keys are ignored. Non-string ids in `resolved`/`unresolved` ⇒ `Err`.
3. Normalise each finding: `severity` lowercased; anything other than `critical`, `major`,
   `minor` becomes `unspecified`. A finding with an empty `file`, a `line` < 1, an empty `claim`,
   or neither a non-empty `scenario` nor a non-empty `rule` becomes a `Suggestion` with its
   claim as `text` (and file/line when present).

Named tests (binding):

- `parses_loom_review_block`: a message with prose, an earlier ```` ```json ```` block, and a final
  ```` ```loom-review ```` block with one finding (scenario set), one suggestion,
  `resolved: ["F-1-2"]` → exactly those values; the earlier json block is ignored.
- `finding_without_scenario_or_rule_becomes_suggestion`: a finding with `scenario: null`,
  `rule: ""` → zero findings and one suggestion whose text is the claim.

Also test: no block ⇒ `Err`; two `loom-review` blocks ⇒ the last one wins; unknown severity ⇒
`unspecified`.

## CX-2 — `loom/src/verify/review/fingerprint.rs`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeFingerprint { pub value: String, pub base: String, pub files: BTreeMap<String, String> }
/// Pure: `entries` are (path relative to the worktree, Some(content) or None when deleted).
pub fn fingerprint_from(base: &str, entries: &[(String, Option<Vec<u8>>)]) -> ChangeFingerprint;
/// Git-backed: base = `git merge-base HEAD <target_branch>` in `worktree`.
pub fn compute(worktree: &Path, target_branch: &str) -> Result<ChangeFingerprint>;
pub fn changed_since(previous: &BTreeMap<String, String>, current: &BTreeMap<String, String>) -> Vec<String>;
```

- `fingerprint_from`: `files` maps path → lowercase sha256 hex of content, or the string
  `deleted`; `value` is `sha256:` + hex(sha256(`base:<base>\n` + for each path in sorted order
  `<path>\t<hash-or-deleted>\n`)).
- `compute`: paths = `git diff --name-only <base>` plus `git ls-files --others --exclude-standard`,
  deduplicated, minus `crate::git::worktree::is_worktree_scaffold_path`. Read each with a bounded
  read that refuses symlinks leaving the worktree (see `crate::fs::safe_read`). Run git with the
  crate's git helper (`loom map --find-all run_git`), never a hand-rolled `Command::new("git")`
  chain.
- `changed_since`: paths whose hash differs between the maps, including paths present in only
  one of them, sorted.

Named test (binding):

- `fingerprint_ignores_commits`: in a temp git repo on `main`, branch off, modify one file:
  compute `a`; `git commit -am x`; compute `b` → `a.value == b.value` and `a.files == b.files`.
  Then modify the file again → the value differs and `changed_since` returns that one path.
