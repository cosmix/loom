# Seatbelt Loopback Grant

> Seatbelt localhost listener re-added as if it held binds to loopback

## A Seatbelt `localhost` Listener Was Re-Added as If It Held Binds to Loopback

**What happened:** commit `720a9f856` granted every confined macOS command `(allow network-bind network-inbound (local ip "localhost:*"))`, with a comment and a kernel test saying only a loopback address could bind. Hours earlier `36916f769` had recorded that Seatbelt cannot do that and removed the same grant. Measured on macOS 27.2.1: the filter matches every local address, a `0.0.0.0` and a LAN bind succeed, `"127.0.0.1:*"` is a parse error, and the listener alone makes no test pass because a self-connect is denied. The kernel test fails on macOS 27 but skips in the host job, and the commit was never pushed to CI.

**Why:** the rule's documentation was trusted over the earlier mistake record, and the only test that could contradict it ran where the sandbox cannot start a nested sandbox, so a skip read as a pass. SBPL `localhost` means every local address, not 127.0.0.1.

**Prevention:** accept a Seatbelt network rule only with a kernel test run outside the host job (an unconfined shell or CI) that shows the denied case failing, and read this file and `concerns/sandbox-and-confinement-gaps.md` before re-adding a grant that was removed. A skipped kernel test proves nothing.

**Fix:** no IP networking by default; a stage that sets `sandbox.network.allow_local_binding` gets one loopback grant, with its residual written next to it (`confine_seatbelt.rs::LOOPBACK_NETWORK`) and kernel tests for both modes (`confine_seatbelt_loopback_tests.rs`).
