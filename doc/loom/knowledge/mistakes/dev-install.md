# dev-install.sh and the Running Daemon
>
> Daemon kill after install; installing unasked

## A Daemon Started During dev-install.sh Keeps the Old Binary (2026-10-27)

**What happened:** an adjudication spawn failed with "loom hook scripts in ~/.claude/hooks/loom are missing or differ from this loom build: _common.sh, spawn-guard.sh" although the operator had restarted the daemon after reinstalling. The daemon had started at 21:57:20; `install.sh` replaced the binary at 21:57:24 and `install-assets` rewrote the hooks at 21:57:25.

**Why:** `dev-install.sh` stopped the daemon before `cargo build`, then left it stopped. A daemon started during the build or install ran the binary still installed, so its embedded hooks no longer matched the ones written seconds later.

**Prevention:** stop the daemon after the binary and hooks are both installed. When the drift refusal appears, compare the daemon's start time with the binary's install time before assuming the hook files are wrong.

**Fix:** `dev-install.sh` runs its `pkill -x loom` block after `install.sh`.

## Installer process fixture must preserve executable dispatch

**What happened:** The development-installer regression copied `sleep` to a different basename; on a multicall coreutils system, both fixture processes exited before the assertion.

**Why:** The fixture assumed the executable did not dispatch by its invoked name.

**Prevention:** Use a test-owned process with explicit runtime arguments and verify it is alive before exercising installer behavior.

**Fix:** Run the Python interpreter with a sleep program in each checkout and wait for both child processes during cleanup.

## Reinstalling the operator's binary unasked

**What happened:** While diagnosing a stale dashboard bundle during a running plan, the agent started `dev-install.sh` on its own. The operator had asked only why the dashboard failed, and replied that they reinstall themselves. The attempt failed on a missing execute bit, so the binary did not change.
**Why:** The fix was treated as implied by the complaint. Installing replaces the binary and global configuration every session on the machine uses.
**Prevention:** Report the diagnosis and the install command. Run an install only on an explicit request.
**Fix:** The operator runs the install.

## dev-install.sh Stopped Killing the Daemon on a Review Finding

**What happened:** finding INT-03 of an external review (`doc/astra-code-review-2026-10-31.md`) rated `dev-install.sh`'s `pkill -x loom` CRITICAL because it signals every same-user loom process, not only this checkout's daemon. Commit `573f4f66` replaced the kill with a printed `loom stop && loom run` hint, bundled into an unrelated hooks commit. The operator had not asked for the change.
**Why:** the review's scope argument was applied without weighing the failure the kill exists to prevent: any daemon left running keeps the old binary and refuses spawns on hook drift (see the 2026-10-27 entry above). Killing every repository's daemon is the intended behaviour for a dev install.
**Prevention:** a review finding that reverses a behaviour recorded in a mistakes entry needs the operator's decision before it is applied. Keep installer behaviour changes in their own commit.
**Fix:** the post-install `pkill -x loom` block is restored; `scripts/tests/test_dev_install.py` asserts it runs after a successful install and never after a failed build or install.
