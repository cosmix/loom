# The journey script

`scripts/plan-journeys.sh <loom-binary> <out-dir>` drives what a user does, in a real browser, against a real `loom plan` server. Plan 12 passed every gate and the feature screen was unusable. This script is what stands between that outcome and this plan.

## Setup

`scripts/plan-journeys.sh` follows `scripts/plan-screens.sh` for the server start: a shallow sparse clone of HEAD with the worktree's `doc/` copied over it, a scratch `LOOM_HOME`, `--port 0 --no-open`, and the token URL read from the log. It adds three things:

- **A scratch `HOME`.** `HOME` is set to a scratch directory for the server only. `plan-screens.sh` leaves it alone; the journey script must not write a trust entry into the operator's `~/.claude.json`.
- **A fake `claude` first on `PATH`,** written by the script into the scratch directory. Only a launch carrying `--session-id` sleeps; everything else exits at once:

  ```sh
  #!/bin/sh
  case "$*" in
    *--session-id*) exec sleep 98765 ;;
    *auth*status*) printf '{"loggedIn":true,"authMethod":"claude.ai"}\n' ;;
    *--version*) printf '9.9.9 (Claude Code)\n' ;;
    *) exit 0 ;;
  esac
  ```

  This shape was run on 2026-10-30: the server is writable, summary rebuilds and chat starts launch it in tmux, and a clean stop tears it down. The session never reports `SessionStart`, so a journey can assert that a chat started and what its first message carried, and never a model reply.
- **Cleanup.** On exit, `trap` stops the server by SIGTERM, then runs `tmux -L <socket> kill-server` for every `loom-plan-*` socket whose server started after the script did.

`web/scripts/plan-journeys.mjs` uses `playwright-core`, as `plan-screens.mjs` does, at 1440x900 and, for J1 and J7, 390x844. It prints one line per journey, `PASS`, `FAIL <why>` or `PENDING <what is missing>`, saves a screenshot per step, and exits non-zero on any `FAIL` or any console or page error. `--strict` makes `PENDING` a failure; integration-verify uses it.

`loom.provision` installs `playwright-core` in a stage worktree; a main checkout with a stale `node_modules` needs `bun install` from `web/`. This script is not an acceptance criterion: a real `HOME`, tmux and the browser cache are outside what the completion runner grants. Stages run it as the step their brief names and put the output in the review brief.

## Journeys

**J1: every row opens real content.** Open `/f/feat-planner` and assert it lands on `/f/feat-planner/product`. Click each row of `nav[aria-label="Feature documents"]`, every expandable group's first child included. After each click assert that the URL changed to the row's route, and that `main` holds a heading and at least 200 characters of visible text, or, for a page with nothing to show yet, an element that states why. Rejects: a blank Run page, a path shown as text, a row that lands on another row's page.

**J2: review down to source.** On `/f/feat-planner/product`, click the first sentence with `[data-sentence-id][role=button]`, repeating until a source passage opens, and assert that the passage's text is in `doc/product/feat-planner/*.md`. Fold it and assert the sentence is back. Open a section row and assert Summary and Source both work, and that Source shows the file's heading.

**J3: direct edit.** On a section's Source view, use Edit, change one word in CodeMirror, save, and assert that the file changed in the clone and the page shows the new text. Assert that a dashed (out of date) mark or a rebuild notice follows, or record why not.

**J4: chat knows the page.** On `product/section_05`, assert that the panel's context line names `section_05` and the composer placeholder names it. Navigate to the metaplan and assert both change. Assert that clicking a prompt fills the composer and starts no session (no new `tmux` socket). Type a message and send it. Assert that a session starts and that the first message in the session's record or the chat frame carries an attachment for `doc/product/feat-planner/section_05.md`. Dismiss the line on another page, send, and assert no attachment.

**J5: start a new feature.** From `/` click "Start a conversation", assert `/new` shows a focusable composer, type, press Start, and assert a session starts. Do not assert anything about a reply.

**J6: no offer twice.** On every route J1 visits, collect the visible text of every `button`, and assert that no offer label appears twice. Assert no primary button's rendered height is above 40px or its text wraps.

**J7: names and layout.** On `/f/feat-planner/*` assert that `document.title` starts with `feat-planner`, and that the switcher shows `feat-planner`. Assert that "Loom Plan" appears nowhere in the app bar, the spine or the home card heading. At 390px assert that no route scrolls sideways (`scrollWidth <= clientWidth` on the document).

**J8: sessions end with the server.** The shell script, after the browser closes: with a session running from J4 or J5, SIGTERM the server, wait up to 10 s, and assert that the fake claude and its tmux server are gone.
