# B1: the roundel on text-only busy messages (issue 9, D10)

Tier: haiku (`loom-software-developer`). Runs after V1 has created `web/src/plan/ui/busy-text.tsx` (`BusyText`, props `{ children: ReactNode; className?: string }`). Read `common.md` D10.

## Change

Wrap each busy message below in `<BusyText>…</BusyText>`. Keep the element around it, its role, its class and its exact words. Change nothing else.

| File | Message |
| --- | --- |
| `web/src/plan/new/document-starter.tsx` | "Uploading…", "Loading document…" |
| `web/src/plan/review/send-to-author.tsx` | "Sending…" |
| `web/src/plan/review/built-marker.tsx` | "Loading build markers…" |
| `web/src/plan/chat/turn-footer-plans.tsx` | "Loading implementing plans…" |
| `web/src/plan/chat/restore.tsx` | "Restoring…" |
| `web/src/plan/chat/pressure/pressure-dialog.tsx` | "Starting pressure test…" |
| `web/src/plan/chat/pressure/pressure-header.tsx` | "Stopping…" |

Find each one with `rg -n '<the words>' <file>`. A test that matches a message with `getByText("<words>")` still finds it, because the roundel adds no text. If one does not, change only that query to a regex, and report it.

## Run once at the end

`node_modules/.bin/vitest run src/plan/new src/plan/review src/plan/chat`, from `web/`.
