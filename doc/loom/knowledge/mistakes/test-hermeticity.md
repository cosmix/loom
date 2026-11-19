# Test Hermeticity

> Tests that reached real network, keychain or home state

## Test Planning Servers Ran the Real Quota Poller

**What happened:** plan_server tests failed intermittently on macOS. Two wildcard-bind tests got 408 (request head timed out) and others reported `still stopping`.

**Why:** the tests dialed the listener's `local_addr()`, `0.0.0.0:<port>`, and a host network filter (Little Snitch) can hold a new binary's non-loopback flows past the server's 5 s head timeout. Separately, every test server started the real quota poller: it read the login keychain, called the usage endpoint, ran `codex` and wrote `~/.loom/quota`. A joined poll outlasted the 3 s stop timeout, in production too.

**Prevention:** a test client dials loopback for a wildcard bind. No test starts a worker that reaches the keychain, the network or the real home directory. On macOS the keychain ignores HOME, so a temp HOME does not isolate it. A worker that writes only timestamped caches is never joined by server stop.

**Fix:** `dial` in `plan_server/tests/support.rs`. `TEST_LOOM_NO_QUOTA_POLL`, set in the workspace `.cargo/config.toml` `[env]`, makes `run_quota_poller` return at once; it is not `LOOM_`-prefixed because server test helpers strip `LOOM_*` from spawned processes. `summaries::start_quota_poller` detaches the poller from stop. Regression: `a_quota_poll_in_flight_does_not_hold_up_stop`.
