# W2 — Header, needs-attention and activity panels, footer, legend overlay

Tier: codex `gpt-5.6-terra`, effort `xhigh`. Spawned after W0 returns, in parallel with W1 and W3.

## Files you own (write)

- `loom/src/commands/status/ui/tui/ledger/header.rs` — new
- `loom/src/commands/status/ui/tui/ledger/panels.rs` — new
- `loom/src/commands/status/ui/tui/ledger/legend.rs` — new

Each with its own `#[cfg(test)] mod tests` (pure line-building functions are the testable part;
keep the `Frame`-rendering wrappers thin).

Read-only: `ledger/mod.rs` (W0's types), `loom/src/lib.rs` (`LOGO`), `loom/src/commands/status/ui/theme.rs`,
`loom/src/commands/status/ui/tui/renderer.rs` (`render_compact_header`, `render_compact_footer`,
`render_activity_log` — the behaviour being replaced), `loom/src/commands/status/ui/tui/state.rs`
(`TuiActivityLog::render_lines`), `loom/src/commands/status/render/attention_model.rs`.

## `header.rs` — `pub fn render_header(frame: &mut Frame, area: Rect, view: &LedgerView)`

`area` is exactly 4 rows. Columns 0-18 hold the four `crate::LOGO` lines in `Theme::header()`.
Text starts at column 19:

| row | left | right-aligned |
| --- | --- | --- |
| 0 | plan name in `Theme::header()`; `(no plan name)` dimmed when None | `● daemon running` in `Theme::status_completed()` + `· tick {age}s ago` dimmed; `tick unknown` when None; when `age >= 60`: `● loop stalled {age}s` in `Theme::status_warning()` |
| 1 | `{spinner} {completed} of {total} stages complete` + bar + `{pct}%` | — |
| 2 | `● {executing} executing · ▶ {queued} queued · ○ {waiting} waiting · {n} need attention · ✓ {done} done` | — |
| 3 | `merged {m} · unmerged {u} · conflicts {c}` dimmed | — |

Bar: `min(40, available)` cells of `━` (`StatusColors::COMPLETED`) for the filled fraction and
`╌` (`StatusColors::DIMMED`) for the rest, `pct = completed / total` rounded. Counts: `executing`
and `done` from `data.progress`; `queued` and `waiting` counted from `data.stages` by status;
`need attention` is `view.attention.len()`, rendered `Theme::status_blocked()` bold when non-zero;
icons take that status's `tui_style()`. Row 3 uses the lengths of `data.merge.merged`,
`.pending`, `.conflicts`. Every line is cut to `area.width`.

## `panels.rs`

```text
pub fn render_attention(frame: &mut Frame, area: Rect, entries: &[AttentionEntry])
pub fn render_activity(frame: &mut Frame, area: Rect, log: &TuiActivityLog)
pub fn render_footer(frame: &mut Frame, area: Rect, present: &[StageStatus], last_error: Option<&str>)
pub fn attention_lines(entries: &[AttentionEntry], width: u16) -> Vec<Line<'static>>
pub fn footer_line(present: &[StageStatus], width: u16) -> Line<'static>
```

Attention: first line `NEEDS ATTENTION` in `Theme::status_pending()` bold; then per entry:

1. `{icon} {id padded to 20} {TITLE} · {detail}` — icon in the status style, title in the
   status style bold, detail = `reason` when Some, else `failure_label(failure_type)` when Some,
   else the cleanup warning's first line.
2. `{first evidence line}` dimmed — only when `evidence` is non-empty.
3. `→ {hint}` with `→` dimmed and the hint in `StatusColors::QUEUED`
   (cyan); for entries with `choices`, append `--approve | --reject <reason> | --force-complete`
   dimmed on the same line (build it from `choices`, never hard-code the flags).

Activity: `ACTIVITY` title as above, then `log.render_lines(area.height - 1)`; when the log is
empty print `Waiting for events...` dimmed (today's wording).

Footer: when `last_error` is Some, the whole line is `Error: {msg}` in `Theme::status_blocked()`
(today's behaviour). Otherwise the legend strip on the left — for each status in `present`, in
`legend::LEGEND` order: `{icon}` in its style then `{label lowercased}` dimmed, entries separated
by two spaces — and the key hints right-aligned: `?` bold + `legend` dimmed, ` · `, `↑↓` + `scroll`,
` · `, `q` + `quit`. When both do not fit in `width`, drop legend entries from the right until they do.

## `legend.rs`

```text
pub const LEGEND: [(StageStatus, &str); 13]
pub fn render_legend_overlay(frame: &mut Frame, area: Rect)
pub fn legend_lines() -> Vec<Line<'static>>
```

`LEGEND` order and meanings, verbatim:

| status | meaning |
| --- | --- |
| WaitingForDeps | waiting for its dependencies to complete and merge |
| Queued | ready; the daemon spawns a session when a slot frees up |
| Executing | a session is working in the stage's worktree |
| WaitingForInput | the agent is waiting for an answer in its session |
| NeedsHandoff | context ceiling reached; a fresh session resumes it |
| Completed | work done and verified — may still be unmerged |
| Skipped | skipped by you; does not satisfy its dependents |
| Blocked | errored; needs intervention → loom stage retry <id> |
| CompletedWithFailures | acceptance failed; retried automatically up to the limit |
| MergeConflict | merge conflict → loom stage merge <id> |
| MergeBlocked | merge errored (not a conflict) → loom stage merge <id> |
| NeedsHumanReview | asked for a decision → loom stage human-review <id> |
| NeedsAdjudication | a disputed acceptance criterion awaits the judge's verdict |

`legend_lines`: a blank line; the first seven entries as `{icon} {label padded to 12} {meaning}`;
a blank line; `needs you` in `Theme::status_blocked()` bold; the six remaining entries; a blank
line; `activity   working · idle · stale = no heartbeat for 5 min` and
`orphaned = executing with no session record` (label dimmed, `working` green, `stale`
warning, `orphaned` blocked); `context    resident tokens of the stage's session against its ceiling`.

`render_legend_overlay`: a `Clear` widget then a `Block` with `BorderType::Rounded`, border
`Theme::status_pending()`, title ` Stage states ` (`Theme::header()`), bottom title
` ? or esc to close ` dimmed, sized 78×22 and centred in `area`, clamped to `area` when smaller;
`Paragraph::new(legend_lines())` inside.

## Tests

`header.rs`: row 2 counts for a fixture with two Executing, one Queued, three WaitingForDeps, two
attention entries, two Completed reads `● 2 executing · ▶ 1 queued · ○ 3 waiting · 2 need attention · ✓ 2 done`.
`panels.rs`: `attention_lines` for a NeedsHumanReview entry has three lines and the third ends with
`--force-complete`; `footer_line` with `present = [Executing, Completed]` contains `executing` and
`done` and not `queued`; with `width = 40` it still ends with `q quit`.
`legend.rs`: `LEGEND` has 13 distinct statuses; `legend_lines` contains every `label()`.

## Done means

`cargo test --manifest-path loom/Cargo.toml --lib commands::status::ui::tui::ledger::` passes once the
sibling modules exist. Each file under 400 lines, each function under 50.

## Constraints the graph will not show you

- Never write the finalize verb next to `loom stage` in any shell command you run; the strings in
  `LEGEND` are Rust source, never commands.
- Do not run `git`, the full test suite, the linter or the formatter.
