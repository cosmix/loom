# D1: the chat in the product spec and in help (R13, docs)

Tier: sonnet (`loom-software-engineer`). Wave 3, after the visual check. Read `../common.md`, the stage's `git diff --stat` and the wave reports the main agent passes you, then the files below. You write prose only; the code is the truth, so read the component before describing it.

## Build

1. `doc/product/feat-planner/section_08.md`, rewritten to the shipped chat (no history, no "was"):
   - The panel sketch (around line 30) and **Header** (around lines 65-76): the title with a sub-line naming the live state and the place the chat concerns; New chat as the head's round button while a chat is live; no menu, no End chat, no run line (authoring browser, watchers, tokens), no relative time.
   - The item table (around line 121): thinking renders nothing; an offer's opening is one quiet line with its label and Show; the closing message is never shown.
   - Around lines 145-150: the closing turn is drawn as the wrap-up card while it runs and as "Here's what this chat left behind" when it is done (documents changed with their counts, decisions, open questions, read from the `loom-closing` block the closing message asks for, section 09); the collapsed turn label carries no tokens.
   - The session-state table and **Empty state** (around lines 206-224): none shows the greeting, the context prompts as pills and the composer, and the first message starts the session; Idle has no state line; Ended and Failed replace the composer with "This chat has ended." and "Start a new chat"; Ending replaces it with "The composer comes back with your next chat."; the one-roundel-per-pane rule.
   - **Cost** (around lines 458-470): the chat shows no token counts; say where cost is still visible (Settings > Summaries, the footer limits) only if the code shows it.
   - Around lines 480 and 501-508: New chat clears the panel and starts nothing; a chat ends when the user starts a new chat, opens another offer, or leaves the planner (the holder's grace period); the closing write runs only when the user sent a message (section 07).
   - Around lines 538 and 552-554: drop "End chat" from the run and read-only lists.
2. `web/src/plan/help/topics.ts`: the chat topic's intro (around line 111) drops "its menu holds End chat" and says New chat starts nothing until you send, and a chat ends when you start another or leave. Keep every sentence ending in one `.`, as `topics.test.tsx` and `chat-scene.test.tsx` require. Update `topics.test.tsx` only where it pins the old words.
3. `rg -n -i 'end chat|chat menu|waiting for you|thought for|fresh tokens|no step' doc/product/feat-planner web/src/plan/help` and fix every hit that describes the chat panel. "Waiting for you" stays where it names the activity pill's idle-chat row (`help/topics.ts` getting-around, `header/`), which this stage keeps.

## Files

`doc/product/feat-planner/section_08.md`, `web/src/plan/help/topics.ts`, `web/src/plan/help/topics.test.tsx`. Sections 07 and 09 were B1's.

## Run once at the end

`node_modules/.bin/vitest run src/plan/help`, from `web/`.

## Report

Files changed, the test result, and any spec statement you could not confirm in the code.
