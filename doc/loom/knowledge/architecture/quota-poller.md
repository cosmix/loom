# Quota Poller

> How loom learns the operator's Claude and Codex subscription budget, where it caches it, and what the two dashboards do with it. Module: `loom/src/quota/`.

## Sources

- **Claude**: `GET https://api.anthropic.com/api/oauth/usage` with `Authorization: Bearer <token>` and `anthropic-beta: oauth-2025-04-20` (`loom/src/quota/claude.rs:46` `fetch`). `parse_response` (`claude.rs:81`) accepts two shapes: `five_hour`/`seven_day` buckets with `utilization` 0-100 and an RFC 3339 `resets_at`, or a `limits` array whose `session`/`weekly_all` kinds map to the same two windows (a `percent: 0` limit with null `resets_at` is a placeholder and is ignored; buckets win when both shapes are present). A 429 becomes the typed `RateLimited { retry_after_secs }` error.
- **Codex**: one bounded `codex app-server` stdio exchange per poll (`loom/src/quota/codex.rs:101` `poll_once`): `initialize` (id 0), the `initialized` notification, then `account/rateLimits/read` (id 1). `rateLimits.primary`/`secondary` are classified by `windowDurationMins` (300 = five-hour, 10080 = seven-day); `planType` is kept as `plan`. On the current operator plan only the weekly window is populated, so one window per provider is normal.
- **Token lookup**: `credentials::access_token(home)` (`loom/src/quota/credentials.rs:42`) reads `.claudeAiOauth.accessToken` from the macOS keychain item `Claude Code-credentials` (bounded by `run_bounded`) or from `~/.claude/.credentials.json`; no login means no claude file, silently. The codex binary comes from `codex::find_codex_path()`; absent means no codex file.

## Cadence and backoff

`quota::poller::spawn_quota_poller` (`loom/src/quota/poller.rs:35`, wrapped by `daemon/server/broadcast.rs:145` and joined in `lifecycle.rs` next to the status broadcaster) runs one thread for both providers. Each polls immediately at daemon start, then every 180 s (`POLL_INTERVAL`); a failure doubles that provider's interval up to 15 min (`MAX_BACKOFF`); a Claude 429 waits `Retry-After` with a 300 s floor. Failures log once per distinct message and once on recovery, through the daemon's stderr log. The loop sleeps in 250 ms slices and checks `shutdown_flag` between polls; a Claude request in flight can hold the thread up to the 10 s HTTP timeout, a codex exchange up to 250 ms plus a 2 s child grace before the process group is SIGKILLed, after which the daemon's 5 s join abandons the thread and exits regardless. `HOST_ENV_ALLOWLIST` (`daemon/server/environment.rs:43-45`) forwards `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` so the poll honours a proxy from the scrubbed daemon environment.

## Cache files

`.loom/work/quota/claude.json` and `codex.json` (`loom/src/quota/cache.rs`), one pretty-printed `ProviderQuota` each: `observed_at` (epoch seconds of the last success), `windows` (five-hour first), `plan`, `error`. Written temp-file-then-rename with fsync into a 0700 directory; the writer refuses a symlink at the target and unlinks then `create_new`s the `.tmp` sibling so a planted link is never followed. Readers cap at 64 KiB, treat oversize or malformed content as absent, never delete, and re-run the hygiene (`clamp_percent`, one window per kind, `inline_safe` on `plan`/`error`). A failed poll rewrites the existing file with `error` set and the previous `observed_at`/`windows`/`plan` kept; with no existing file it writes nothing, so a provider that never succeeded has no row at all. Workspace-local by design: the collector, the web file-poll fallback, and tests all read the work dir they already have; nothing here reads `~/.loom`.

## Rules that must not regress

- The OAuth token exists only inside `credentials::access_token` and the single request in `claude::fetch`; it is never logged, cached, put in an error string, or sent over the daemon socket. HTTP failures carry the status code only.
- Every string from a network body or the codex subprocess (`plan`, `error`, JSON-RPC `message`) goes through `context::untrusted::inline_safe` before storage and again on read.
- Percentages are clamped 0..=100 and non-finite values dropped; unparsable reset times become `None` instead of failing the window; epoch values above 100 000 000 000 are treated as milliseconds (`normalize_epoch`).
- The codex child is bounded by `CODEX_DEADLINE` (15 s), killed as a process group on every exit path, and the `mpsc` receiver is dropped before the reader thread is joined so a chatty server cannot hang teardown.

## Renderers

- **TUI** (`loom status --live`): `ledger/quota.rs:72` `quota_line` draws one line above the legend line when either provider has data. `layout.rs` gives the footer 2 rows through `footer_height(has_quota)` and drops back to 1 as a fourth degrade step when the table would otherwise fall under 6 rows. `quota_layout(width)` picks the tier: 8-segment bars plus countdown from 120 columns, 6 segments from 90 (`MEDIUM_WIDTH`), percent only below; `quota_line` then falls back to plainer tiers before ever truncating a meter. Stale readings (`age_secs >= STALE_AFTER_SECS`, 600 s) and `error` append dimmed suffixes. Both window slots are always drawn, a missing one as a dimmed `—`. The same change made the `↑↓ scroll` hint conditional on `LedgerView.scrollable` (the table overflows its viewport).
- **Web** (`loom status --web`): `web/src/components/quota-meters.tsx` inside the sticky footer of `web/src/routes/shell.tsx`, fed by `web/src/lib/quota.ts` (mirrors `model.rs`: `quotaHealth`, `formatReset`, `resetText`, `ageText`, `providerRows`). Each window is a notched gauge over a time-to-reset track, `role="img"` with an aria-label and a tooltip carrying the exact reset time and observed age; stale dims the group, error appends the text in the warning tone. The countdown hides under 900 px so the bar stays one row.
