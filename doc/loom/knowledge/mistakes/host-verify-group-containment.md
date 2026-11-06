# Host Verify Group Containment

>

## Group guardian assumed bubblewrap payload stayed in its host group

**What happened:** A recovery guardian initially used its host process group to retire every criterion descendant without checking the Linux bubblewrap wrapper.

**Why:** The wrapper uses --new-session, so sandbox payload processes leave the host group. The guardian still reaches bubblewrap itself; the wrapper --die-with-parent contract is what retires the detached sandbox process tree.

**Prevention:** Trace every wrapper and session boundary before claiming process-group containment, and run a confined-path lifecycle test beside shell stand-ins.

**Fix:** Keep bubblewrap --new-session and --die-with-parent; test that guardian retirement stops a live sandbox descendant, and treat that test as required acceptance evidence.

## A skipped bubblewrap regression looked like an actual pass

**What happened:** The focused group test printed an outer pass after its inner confinement probe skipped under sandbox restrictions. I initially reported that output as proof of real bubblewrap cleanup.

**Why:** The test uses a child test process for observer isolation, and the outer assertion saw the child exit successfully even when the child skipped. I read the exit status without forcing the probe to run.

**Prevention:** For acceptance that depends on host sandbox facilities, run with LOOM_TEST_REQUIRE_SANDBOX_FREE=1 and a writable TMPDIR; inspect the inner test output, not only the outer status.

**Fix:** The forced run failed inside the managed sandbox as expected, then passed outside it with a live confined heartbeat that stopped after group retirement.
