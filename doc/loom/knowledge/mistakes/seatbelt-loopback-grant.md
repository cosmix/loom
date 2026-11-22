# Seatbelt Loopback Grant

> Seatbelt localhost listener re-added as if it held binds to loopback

## A Seatbelt `localhost` Listener Was Re-Added as If It Held Binds to Loopback

**What happened:** commit `720a9f856` granted every confined macOS command `(allow network-bind network-inbound (local ip "localhost:*"))`, with a comment and a kernel test saying only a loopback address could bind. Hours earlier `36916f769` had recorded that Seatbelt cannot do that and removed the same grant. Measured on macOS 27.2.1: the filter matches every local address, a `0.0.0.0` and a LAN bind succeed, `"127.0.0.1:*"` is a parse error, and the listener alone makes no test pass because a self-connect is denied. The kernel test fails on macOS 27 but skips in the host job, and the commit was never pushed to CI.

**Why:** the rule's documentation was trusted over the earlier mistake record, and the only test that could contradict it ran where the sandbox cannot start a nested sandbox, so a skip read as a pass. SBPL `localhost` means every local address, not 127.0.0.1.

**Prevention:** accept a Seatbelt network rule only with a kernel test run outside the host job (an unconfined shell or CI) that shows the denied case failing, and read this file and `concerns/sandbox-and-confinement-gaps.md` before re-adding a grant that was removed. A skipped kernel test proves nothing.

**Plan drift:** decision 5 of the plan and the `completion-sandbox` stage contract (`a_confined_command_may_bind_only_localhost`) named a default loopback grant; the tree grants loopback only on the `allow_local_binding` opt-in (`confine_levels.rs`, `seatbelt_grant_contract_tests.rs`). A plan that asks for a default loopback grant asks for this mistake again.

**Fix:** no IP networking by default; a stage that sets `sandbox.network.allow_local_binding` gets one loopback grant, with its residual written next to it (`confine_seatbelt.rs::LOOPBACK_NETWORK`) and kernel tests for both modes (`confine_seatbelt_loopback_tests.rs`).

## Seatbelt Port Deny Ignored After a Remote IP Grant on macOS 26

**What happened:** the loopback opt-in granted `(allow network-outbound (remote ip "localhost:*"))` followed by `(deny network-outbound (remote tcp "localhost:4226"))`. It held on the macOS 27.2.1 dev host, and the nightly `Test (macos, lib)` lane on `macos-26-arm64` (macOS 26.7) failed `the_loopback_grant_reaches_host_listeners_except_the_sccache_port`: the confined connect to the runner sccache on 4226 succeeded.

**Why:** Seatbelt network-filter precedence varies by filter spelling and OS release. A throwaway CI matrix showed that on 26.7 no deny after a `remote ip` allow holds, whatever its spelling (`tcp`, `tcp4`, `ip`, `ip4`, `*:4226`) or order, and neither does a deny after a lone `remote tcp` allow. On 27.2.1 an `ip` deny is ignored where a `tcp` deny holds. `require-not` inside a network allow is ignored on both.

**Prevention:** spell an IP grant that a later deny must override as separate `(remote tcp ...)` and `(remote udp ...)` allow rules, and measure any Seatbelt network rule on the CI macOS image before relying on it. Local `sandbox-exec` results do not carry over between macOS releases.

**Fix:** `confine_seatbelt.rs::LOOPBACK_NETWORK` grants outbound as a `tcp` rule then a `udp` rule. `SCCACHE_PORT_DENY` then refuses 4226 over IPv4 and IPv6 on 26.7 and 27.2.1, while other loopback TCP and UDP stay allowed.
