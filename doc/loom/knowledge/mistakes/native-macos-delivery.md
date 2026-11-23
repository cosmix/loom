# Native Macos Delivery

> macOS boundary delivery slips: lint scope, pinned fields, shared TMPDIR

## A Subagent Ran Clippy Over the Whole Crate

**What happened:** a subagent building the macOS boundary ran `cargo clippy` over the whole crate to catch lint in its new files. The subagent hooks (`subagent-verify-guard`) also refuse `cargo clippy --all-targets` and `cargo fmt --all --check` even when the brief asks for them, and `loom check --suggest` relays, so a subagent cannot run it.

**Why:** the brief asked for lint, and a subagent has no sanctioned way to run it.

**Prevention:** a subagent compiles and tests only its own modules: `rustfmt --check` on the changed files and `cargo build --lib --tests`. Project-wide lint belongs to the stage gate, which the main agent runs.

## A Pinned Struct Gained a Field and a Construction Site Outside Every Owner Broke the Build

**What happened:** the macOS boundary plan pinned a new required `sandbox` field on `MachineConfig`. `native_worker/core_contract_tests.rs` built a `MachineConfig` and lay outside every worker's file ownership, so no lib test compiled. The worker that owned the machine files recorded a one-line edit there as made although the permission layer had denied it (a correction note followed); worker Q, whose spec required updating every caller, finally added `sandbox: SandboxMode::WorkspaceWrite`, and also changed the `run_session` call in `machine/startup_contract_tests.rs`, another worker's tree.

**Why:** the brief pinned the field without listing the existing construction sites, and a note claimed an edit the tool had refused.

**Prevention:** when a brief adds a required field or changes a signature, list every construction or call site (`rg` the type) in the ownership table, including frozen contract files; a worker confirms an edit by reading the file back before recording it as made.

## Concurrent Workers Share `$TMPDIR`, So Common Log Names Interleave

**What happened:** parallel workers writing `t1.log` under the shared `$TMPDIR` read each other's output.

**Prevention:** name logs uniquely per worker in the scratch directory (`$LOOM_SCRATCH_DIR`), never in a bare shared name.

## A Harness-Relative Binary Lookup Missed the Cargo 1.100 Test Layout

**What happened:** the six macOS native-worker boundary kernel tests looked for the `loom` they start as the Seatbelt bridge at `current_exe().parent().parent()/loom`. Cargo 1.100 puts a lib test harness at `target/<profile>/build/loom/<hash>/out/loom-<hash>`, so the lookup never found `target/debug/loom` and the tests always skipped. The nightly strict lane, which also built the binary only after the tests, failed all six. Because they had never run, nobody saw that the proxy-tunnel test failed on a real bug: `proxy/tunnel.rs::splice` dropped replies after a client half-close (`mistakes/native-macos-delivery.md`).

**Why:** the lookup hard-coded the old `target/<profile>/deps/` depth, and a skip, which non-strict runs only print, reads like a pass.

**Prevention:** find cargo outputs from a test harness by walking its ancestors, never at a fixed depth. Anything a strict lane gates on must be built before that lane runs. A new kernel test counts as proven only once a strict run has executed it.

**Fix:** `native_worker/boundary/loom_program.rs::built_loom` takes the nearest of up to five ancestors that holds a `loom` file. `.github/workflows/nightly.yml` builds `--bin loom` before `Run tests` in the macOS lib suite.

## The Tunnel Copier Dropped Replies After a Client Half-Close

**What happened:** `native_worker/proxy/tunnel.rs::splice` shut down both sockets as soon as either direction reached end of stream. A client that half-closes after its request, as `printf ping | nc -w 5` does, had its tunnel cut before the reply was relayed back. The macOS kernel test `native_macos_the_proxy_tunnel_round_trips_and_other_sockets_are_denied` failed six times in seven runs. No Seatbelt denial was involved.

**Why:** the copier treated a clean end of stream as teardown and never passed it on as a half-close. The kernel test that exercises it had never run (`mistakes/native-macos-delivery.md`).

**Prevention:** a byte relay passes a clean end of stream on with a write-side shutdown, and closes both sides only on an error or when the far side ends. Test a relay with a client that half-closes and then reads.

**Fix:** `splice` treats `a` as the side that opened the connection. A clean end of `a` is passed on with `Socket::close_write`, and the reply from `b` still flows. Any error, or an end of `b`, closes both sockets. A half-closed tunnel waits only on `b`, which the proxy bounds with its head and idle timeouts. The regression test is `bridge_tests.rs::a_half_closed_client_still_receives_the_reply`.

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
