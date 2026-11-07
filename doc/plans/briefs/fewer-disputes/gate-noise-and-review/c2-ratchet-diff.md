# gate-noise-and-review C2: ratchet-file loosening rule (codex unit)

Lane: codex terra (`--model gpt-6.1-terra --effort xhigh`), foreground, Bash timeout 1560000 ms.
Wave 1, in the same message as C1. Read `common.md` in the directory that holds this brief first.
Source paths below are relative to the worktree's `loom/` directory (the package root); `../` is
the repository root. Do not run git or touch `.loom/`. Compile-only check: `cargo build --lib`
(unused-item warnings are expected until the detector is wired).

## Files

Own: `src/verify/integrity/ratchet_diff.rs` (the orchestrator created it with one `//!` line and
declared `mod ratchet_diff;` in `src/verify/integrity/mod.rs`) and
`src/verify/integrity/ratchet_diff_tests.rs`. Never edit any other file. Read, for the formats this
parses: `maintainability-baseline.txt` (repository path `loom/maintainability-baseline.txt`; lines
`file <path> <lines>` and `function <path> <name> <lines>`, `#` comments) and
`../doc/loom/knowledge/check-baseline.txt` (issue lines `<kind> <file> [<detail>]` with no trailing
integer, which do NOT parse, see rule 4; the file holds only `#` comment lines today, so its first
issue line raises as a new key or an unparsable line).

## Why

`edits.rs::ratchet_events` raises `TI-ratchet-<path>` on ANY change of a listed ratchet file. A
stage that shrinks a function and lowers its ledger entry tightens the ratchet and still needs a
`dispute-integrity` (about 10 filings). Only a loosening is a weakening.

## Interface (pinned; W1 and the contracts call it)

```rust
/// True when a change of a ratchet file from `base` to `head` must raise `TI-ratchet`.
/// `None` is an absent file (new at head when `base` is `None`, deleted when `head` is `None`).
/// Callers pass a file only when its content differs from base.
pub(super) fn ratchet_loosens(base: Option<&[u8]>, head: Option<&[u8]>) -> bool;
```

## Rules (decided values)

1. Parse a file into `BTreeMap<String, u64>`. Decode as UTF-8; invalid UTF-8 means unparsable.
2. Per line: trim it; skip it when empty or when it starts with `#`. Otherwise split off the last
   whitespace-separated token with `rsplit_once(char::is_whitespace)`; that token must parse as
   `u64` made of ASCII digits only (no sign, no `%`, no unit). The KEY is the rest of the line:
   trimmed, every internal whitespace run collapsed to one space, non-empty. A line without such a
   token, a bare integer line, or a repeated key makes the whole file unparsable.
3. Compare parsed maps. LOOSENS when any head key is absent from base (a new key), or any head value
   is greater than the base value for the same key. A removed key or a lower value TIGHTENS. Equal
   maps (a whitespace or comment-only change) neither loosen nor tighten: return false.
4. All-or-nothing: ONE non-comment, non-blank line whose last token is not an unsigned integer makes
   the whole file unparsable, however many other lines parse. Return true (raise) when: `base` or `head` is `None`; either side is unparsable; or the maps
   loosen. A knowledge check baseline (`<kind> <file> <detail>` lines) is unparsable by rule 2 and
   so keeps raising on any change; that is the decided behaviour.
5. Direction assumption: a larger value permits more (a ceiling). A floor file (a coverage minimum)
   must keep a non-integer last token (for example `80%`) so it stays unparsable. Put this in the
   doc comment of `ratchet_loosens`.

## Steps

1. Write `ratchet_loosens` plus a private `parse(&[u8]) -> Option<BTreeMap<String, u64>>`, under 120
   lines in total.
2. Write `ratchet_diff_tests.rs` (`#[cfg(test)] #[path = "ratchet_diff_tests.rs"] mod tests;`):
   - lower value: false; removed key: false; both together: false;
   - higher value: true; new key: true; a `function` line with a new name: true;
   - comment-only and blank-line changes: false;
   - key whitespace normalization: `file  src/a.rs   420` equals `file src/a.rs 420`;
   - unparsable base or head (`threshold: 80%`, a line `max 1.5`, a bare `7`, a repeated key,
     invalid UTF-8): true;
   - `base: None` and `head: None`: true;
   - the real `maintainability-baseline.txt` text (read with `include_str!("../../../maintainability-baseline.txt")`)
     lowering its first `file` line by one: false; raising it by one: true. Compute the edit by
     string replace on the first line that starts with `file `.
3. `cargo build --lib`.

## Traps

- `u64::from_str` accepts a leading `+`; check ASCII digits before parsing.
- Collapse whitespace in the key with `split_whitespace().collect::<Vec<_>>().join(" ")`.
- This module reads bytes only; it never touches the filesystem or git.
