# H5 — read receipts shared across a session tree

Tier: opus (`loom-senior-software-engineer`). Read `../common.md` first.

## Goal

When an agent is about to read a whole file that a sibling agent in the same session tree has
already read whole, and the file has not changed since, it is told so and pointed at the outline
and a range. Evidence: report section 4.5 — about 44% of all reads repeat a sibling's read, flat
for three weeks (48.2%, 44.8%, 43.7%), while same-agent re-reads halved once the per-agent ledger
shipped.

A receipt cannot hand one agent's context to another. What it can do is turn a second whole-file
read into an outline plus the range the agent needs, and give the orchestrator a list of files
its workers all opened, so the next brief carries ranges.

## Files you own (write)

- `loom-hooks/_read_ledger.sh`, `loom-hooks/_read_discipline.sh`, `loom-hooks/read-guard.sh`
- `loom-hooks/tests/read-guard-*.sh` and new test scripts. Do NOT edit
  `loom-hooks/tests/run-all.sh`: list the `run_test` lines your scripts need in your report; H6
  adds them. Run your own scripts directly (`bash loom-hooks/tests/<script>.sh`).

## Where things are

Ledger rows are `path, kind (full|range), lines, timestamp`, appended by `_loom_ledger_append`
(`_read_ledger.sh:95-120`), capped at 300 rows (46, 67-80). `_loom_ledger_file`
(`_read_discipline.sh:135-154`) stores them at
`$LOOM_WORK_DIR/hooks/reads/<LOOM_SESSION_ID>/<agent_id>.tsv` in a stage and
`$TMPDIR/loom-reads/<session>.tsv` outside one. In a stage, `LOOM_SESSION_ID` is the parent
session, so sibling ledgers already sit in one directory.

## Steps

1. Outside a stage, key the ledger directory by the hook input's `session_id` and the file by
   `agent_id` (main agent: `main`), so an interactive session's subagents share a directory the
   same way.
2. On a whole-file Read (no offset, no limit) of a file above 200 lines: scan the other ledgers
   in the session directory for a `full` row with the same resolved path whose timestamp is not
   older than the file's mtime. If one exists, warn once per agent and path: which agent read
   it, how many lines, that it is unchanged since, and the two cheaper moves —
   `loom map --outline <path>` then a ranged Read. Advisory only. Keep the scan bounded: read at
   most the 20 most recent sibling ledgers, with `rg -F` on the path, no shell loop per row.
3. Record the receipt: append to `<session dir>/_shared.tsv` a row `path, lines, agent count`
   whenever step 2 fires. This is what the orchestrator reads.
4. `_shared.tsv` is a documented file and nothing in this stage prints it. State its path and
   row format in your report; the doctrine stage names it in the orchestration skill as the
   place an orchestrator looks before writing its next round of briefs.
5. The existing same-agent re-read advisory and the 300-row cap keep working unchanged.

## Traps

- `doc/loom/knowledge/mistakes/parallel-worktree-shared-state.md`: `$LOOM_WORK_DIR` is shared
  by every parallel stage; appends must be single `printf >>` writes of one line, and a reader
  must tolerate a torn last line.
- `doc/loom/knowledge/mistakes/writer-reader-address.md`: a ledger written under a key its
  reader does not use looks identical to a no-op. Test writer and reader through the real path
  derivation, not with a hand-built directory.
- mtime portability: `stat -c %Y` on Linux, `stat -f %m` on macOS; `_common.sh` may already wrap
  it.

## Proof

Your new test scripts, each run once. Cases: sibling full read, unchanged file → one
warning, second attempt silent; file modified after the sibling's read → silent; sibling ranged
read → silent; file of 150 lines → silent; outside a stage with two `agent_id`s in one
`session_id` → warning; unwritable ledger directory → hook exits 0 silently.
