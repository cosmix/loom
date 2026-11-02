# Review Continuation Verification

> Astra review repair lessons

## Global formatting during parallel edits

**What happened:** A worker ran workspace-wide formatting while other workers were editing the same checkout.
**Why:** Formatting was treated as an independent check despite shared file ownership.
**Prevention:** During parallel edits, format only owned files; run workspace-wide formatting after all workers have frozen their changes.
**Fix:** Alert affected workers, inspect the integrated diff, and run final formatting after freeze.

## Unix backlog saturation assumption

**What happened:** A timeout regression assumed that a zero-sized AF_UNIX listen backlog accepts exactly one pending connection; Linux admitted another.
**Why:** The fixture treated an implementation-dependent queue capacity as a fixed contract.
**Prevention:** Establish saturation with bounded connection attempts before asserting a deadline, and avoid fixed backlog capacity assumptions.
**Fix:** Fill the listener queue until a bounded connect times out, then exercise the timeout path.

## Tally movement text disagreed with counts

**What happened:** A generated review tally correctly counted eight findings returning to partial but described seven moves.
**Why:** The movement sentence was hard-coded instead of derived from the category delta.
**Prevention:** Calculate both the aggregate categories and movement description from the same prior and current counts.
**Fix:** Correct the movement sentence to eight and reconcile the 227 indexed IDs.

## Protocol test barrier and unsubscribe snapshot

**What happened:** A daemon framing test released its acknowledgement barrier without proving the broadcaster had attempted the writer lock. Review also found that a previously cloned subscriber snapshot could write after Unsubscribe acknowledgement.
**Why:** The fixture relied on scheduling, and list removal did not retire already retained writer references.
**Prevention:** Prove contention with an explicit signal after a failed try_lock; coordinate unsubscribe and all retained writers at the connection ownership boundary.
**Fix:** Strengthen the barrier test and make Unsubscribe serialize with pending broadcast writers before acknowledgement and shutdown.

## Removed parent import was used by child tests

**What happened:** A daemon client import was removed while nested tests still reached it through a parent glob import, causing test compilation failures.
**Why:** The import cleanup traced production callers but missed child-module scope.
**Prevention:** Search child glob imports and compile test targets after removing parent imports.
**Fix:** Give affected test modules explicit imports and rerun the test build.

## Partial frame failure poisons the whole connection

**What happened:** A broadcast frame timeout evicted only one subscription while another subscription or Ping could append bytes to the incomplete frame.
**Why:** Failure was handled at the subscriber-list boundary even though multiple producers shared a single framed stream.
**Prevention:** On any failed frame write, retire the connection-wide write side before another producer can use it, and test multiple subscriptions and control replies.
**Fix:** Shut down the shared write half while holding its writer lock on frame failure.

## Unix connect EAGAIN is not in-progress success

**What happened:** A saturated AF_UNIX listener allowed a nonblocking connect test to report success even though the queue had not accepted the connection.
**Why:** The transport treated EAGAIN like EINPROGRESS and accepted a writable poll with SO_ERROR zero as proof of connection.
**Prevention:** Treat backlog EAGAIN as a retry bounded by the original deadline and verify the socket is connected before reporting success.
**Fix:** Exercise actual queue saturation and correct the connect state machine.

## Retrying failed connect on the same socket

**What happened:** A saturated Unix socket retry reused its descriptor after connect failed.
**Why:** Linux behavior was assumed portable despite unspecified socket state after a failed connect.
**Prevention:** Create a fresh socket for failed connection retries and carry one absolute exchange deadline across attempts.
**Fix:** Rework the backlog retry and retain its bounded saturation regression.

## Socket send timeout did not bound a whole blocking write

**What happened:** A 120 ms framed-write test completed successfully after 2.57 seconds while a peer slowly drained a 1 MiB response.
**Why:** Recomputing SO_SNDTIMEO between write calls cannot constrain a single blocking write that keeps making progress.
**Prevention:** Perform nonblocking sends and poll with the remaining absolute deadline; test a reader that makes sustained slow progress.
**Fix:** Repair the framed writer and rerun the focused regression and full integration gate.

## Moving helpers changed module visibility

**What happened:** Splitting the daemon writer into a child module made its re-export too private and removed an import used by a nested test.
**Why:** The refactor did not account for the additional module boundary or inherited glob imports.
**Prevention:** Check the full re-export visibility chain and nested test imports when moving helpers, then compile test targets.
**Fix:** Set the minimum required server-scoped visibility and import std::io explicitly in the affected test.

## Finding rows separated from the ledger table

**What happened:** Moving completed and partial review rows back to the ledger inserted blank lines between rows, so Markdown no longer rendered them in the existing table.
**Why:** The tracker edit treated each inserted row as a separate paragraph rather than preserving the table block.
**Prevention:** Move finding rows directly into the continuous ledger table and inspect rendered table boundaries before committing.
**Fix:** Remove the separating blank lines and retain a blank only before the following prose.

## Stopping a repair assignment at preliminary blockers

**What happened:** The Astra review continuation ended with nine assigned findings still partial, even though the user had requested complete repairs or precise evidence of an essential blocker.
**Why:** I accepted several worker assessments and local design difficulties as sufficient stopping points without independently exhausting compatible designs and recording concrete decisions required for each remaining issue.
**Prevention:** Continue investigating assigned partial findings until acceptance is satisfied or an independently substantiated constraint truly prevents progress; separate missing local verification from unresolved implementation and state exactly what decision or resource is required.
**Fix:** Reopen the remaining assigned findings for independent design and acceptance work instead of treating a green broad suite and documented partials as completion.

## User assigned harder races to Astra workers

**What happened:** After being challenged on unfinished Astra review repairs, I restarted the work with Sol workers across all findings instead of reserving the hardest races for the stronger worker tier the user then specified.
**Why:** I reused the earlier worker allocation without re-evaluating the complexity and cost of the unresolved concurrency and data-preservation problems.
**Prevention:** Follow the latest explicit worker model and effort instructions, assigning difficult ownership, publication and recovery races to Astra high and remaining independent work to Sol high.
**Fix:** Reassign the hardest races to Astra high and keep the remaining findings with Sol high.

## Only native BSD verification may remain partial

**What happened:** The resumed Astra review work was framed as allowing other missing local runners or difficult implementation decisions to remain partial.
**Why:** I carried forward the previous session’s permissive blocker language after the user required completion.
**Prevention:** Finish every remaining implementation and acceptance check; only unavailable native BSD/macOS verification may remain explicitly partial under the user’s exception. Do not treat a missing local non-BSD runner or a difficult design as a stopping point.
**Fix:** Continue implementation and obtain the necessary non-BSD verification without provisioning Docker or alternative environments.

## Cargo run from the repository root

**What happened:** Four focused Cargo commands were started from the repository root, which has no Cargo.toml, so no tests ran.

**Why:** Worker briefs did not prominently state the crate working directory and the workspace layout was not checked before invoking Cargo.

**Prevention:** Include the loom/ Cargo working directory in worker briefs, run Cargo there or pass its manifest path, and confirm that the intended tests were selected.

**Fix:** Rerun the focused checks from loom/ and state the working directory in future verification assignments.

## Shutdown body cancellation was overgeneralized

**What happened:** Shared-server shutdown and handler comments claimed all incomplete request bodies cancel, while the planner has a separate body reader.

**Why:** The dashboard reader contract was assumed to cover every router using the shared server.

**Prevention:** Trace each router body reader before documenting cross-router shutdown guarantees; describe retained ownership separately from cancellation.

**Fix:** Limit the cancellation claim to the dashboard/shared-reader path and describe handler slot ownership for the full routing lifetime.

## Sandbox process listing missed Cargo lock owner

**What happened:** A worker tried to identify a shared Cargo build lock holder with ps inside its sandbox and saw only its own process namespace.

**Why:** Separate tool invocations had separate PID namespaces despite sharing the target directory.

**Prevention:** Coordinate build ownership through worker messages or use an authorized host process listing when necessary; do not infer that a lock is abandoned from a sandbox-local process list.

**Fix:** Wait for the active build and serialize focused checks on the shared target.

## Reused PGIDs conflated legacy and guardian records

**What happened:** A recovery change classified unverified legacy process-group records by subtracting guardian records with the same numeric PGID, so different generations could be conflated.

**Why:** A reusable process-group number was treated as durable record identity.

**Prevention:** Classify each persisted ownership record from its own control and identity fields; test a legacy and guardian generation that share a PGID.

**Fix:** Inspect each record independently and add a reuse regression.

## Worker slot released before shared server resources

**What happened:** A shutdown design counted a connection finished when its handler-local slot dropped, even though its worker still held the shared listener and router.

**Why:** Slot accounting ended before the actual ownership boundary.

**Prevention:** Drop worker-held shared resources before decrementing the final connection slot and test the teardown ordering, including unwind paths.

**Fix:** Move the slot to the outer worker scope and make pending completion observe the final resource release.

## Test duplicated an existing Origin header prefix

**What happened:** A planning-server shutdown regression timed out before mutation admission because it sent a malformed doubly prefixed Origin header.

**Why:** The shared fixture helper already returned the complete header, but the test treated it as only a URL.

**Prevention:** Inspect shared protocol fixture return contracts before composing request headers, and diagnose admission failures before changing lifecycle code.

**Fix:** Use the helper output unchanged and rerun the authenticated request regression.

## Singleton ownership transferred after worker startup

**What happened:** A planning-server shutdown repair transferred the singleton lock only after server startup returned successfully, leaving startup failure paths able to release it while a worker remained alive.

**Why:** Ownership was attached to the success result rather than established before spawning workers.

**Prevention:** Transfer singleton and resource ownership before the first worker starts, and inject startup failure after a worker becomes live.

**Fix:** Retain the singleton from pre-start through worker cleanup and surface late cleanup errors through the pending result.

## Missing worktree pointer did not prove admin refs absent

**What happened:** A half-removal recovery design treated a missing worktree .git pointer as proof that no Git administrative HEAD survived, so pruning could discard an unmerged detached commit.

**Why:** Checkout filesystem state was mistaken for complete Git registry ownership evidence.

**Prevention:** Inspect and validate the registered worktree administrative HEAD and its ancestry before pruning, and retain unknown or detached unmerged refs.

**Fix:** Add fallible registry/ref verification and a real-Git regression for a detached unmerged HEAD after pointer loss.

## Caller-side context drop escaped shutdown deadline

**What happened:** A bounded planning-server cleanup supervisor could finish before the caller dropped its final PlanContext, allowing a blocking child destructor to run outside the shutdown bound.

**Why:** Context destruction was not included in the retained worker ownership boundary.

**Prevention:** Transfer the server-owned context to the cleanup supervisor before waiting and test a blocked destructor; distinguish externally held context references from server-owned resources.

**Fix:** Drop the server-owned context under supervisor ownership before reporting completion.

## Runner verification scope needs user context

**What happened:** I pursued acquiring several language test runners without explaining why the review asked for live selection checks or considering whether authoritative documentation could answer the remaining questions.

**Why:** I followed the original acceptance wording mechanically rather than checking the user’s current preference about verification cost.

**Prevention:** Explain the specific adapter behavior being verified, use authoritative runner documentation where it establishes the exact selector contract, and avoid unnecessary tool acquisition or broad language testing.

**Fix:** Limit work to the exact-name selection defect and reassess remaining runner checks using official documentation and already available tools.

## Process continuity was mistaken for forward authorization

**What happened:** A direct-forward progress design accepted a stage-created codex-named descendant and matching environment as the authorized worker, so an impostor could extend the liveness hold.

**Why:** Kernel PID/start identity and ancestry prove continuity but the command name and environment are stage-controlled.

**Prevention:** Bind progress to an authenticated installed supervisor and its actual worker identity; test a fake descendant with matching name and environment.

**Fix:** Replace name/environment-only authorization and add impostor coverage before accepting the liveness signal.

## Expand Astra review scope only when directed

**What happened:** The user expanded the work from the nine reopened partial findings to all 34 previously unassigned findings, requesting two per Terra high worker while the partial repairs continue.

**Why:** The earlier instruction to leave those 34 unassigned was superseded by an explicit later direction.

**Prevention:** Apply the latest scope and worker-model instructions, preserve disjoint ownership and central tracker/integration control, and report progress at the requested interval.

**Fix:** Assign all 34 indexed findings in two-finding Terra high briefs, coordinating overlapping files and continuing the nine partial repairs separately.

## Existing test removed before coverage approval

**What happened:** A worker removed a constructor-only test before the user-required explanation and coverage mapping were reviewed; its import had also been removed too early, breaking compilation.

**Why:** The review suggestion to simplify low-value tests was treated as authorization to remove an existing test despite the task’s preservation constraint.

**Prevention:** Preserve existing tests by default. Before deleting any file or test, explain why, map meaningful coverage to replacements, and assess compatibility; coordinate the decision before editing.

**Fix:** Restore the test and import while keeping the new production-route assertions.

## Refactor edits crossed preservation boundaries

**What happened:** One worker briefly removed a tracked validation file before recreating it, and another temporarily removed adjacent skill guidance that was outside the intended deduplication.

**Why:** Replacement and section-boundary edits were applied without treating tracked content and neighboring guidance as preservation constraints.

**Prevention:** Edit tracked files in place, never delete files or tests without prior explanation and mapped coverage, and audit exact section boundaries and downstream references before deduplicating guidance.

**Fix:** Both files and unrelated guidance were restored; subsequent changes retain the existing coverage and scoped content.

## Relative import assumed the wrong module depth

**What happened:** A daemon client-dispatch change imported the status module through the immediate parent, but dispatch is nested inside client and requires the server grandparent.

**Why:** The source file path was treated as the Rust module nesting without checking its declaration.

**Prevention:** Trace module declarations before adding relative imports and compile the affected target.

**Fix:** Correct the relative path and rerun the focused build.

## Failure injection did not fail the config update

**What happened:** A finalization rollback test used an absent config to force an update error, but the write helper creates missing config.

**Why:** The fixture assumed failure semantics without tracing the actual helper.

**Prevention:** Verify that an injected failure reaches the intended point and fails under the real production helper.

**Fix:** Use invalid TOML to make the update fail after rename and assert rollback.

## Module visibility and patch context need verification

**What happened:** One test helper was initially referenced through a private module and a formatting-sensitive patch failed to match the current test text.

**Why:** Module exports and source context had changed during concurrent edits.

**Prevention:** Resolve public or crate-visible reexports and reread the exact changed range after formatting before applying a patch.

**Fix:** Use the existing visible reexport and reapply against the current context.

## Bubblewrap overlay lower paths are host paths

**What happened:** A cache overlay initially used a synthetic mount path as its lower source, producing an empty cache during a real offline fixture.

**Why:** Bubblewrap resolves overlay sources on the host rather than through earlier sandbox mount destinations.

**Prevention:** Probe the actual overlay on the target filesystem and pass the real host cache as the read-only lower; verify offline dependency reuse and host-cache immutability.

**Fix:** Use the host cache path as the lower source with isolated upper/work directories.

## Concurrent refactors exposed import and UTF-8 boundaries

**What happened:** A test lost a helper inherited through a parent import, an Option-returning root helper was used with Result propagation, and bounded output parsing rejected a valid UTF-8 string cut inside a codepoint.

**Why:** Concurrent changes obscured module scope, return types and byte-boundary semantics.

**Prevention:** Trace child glob consumers before import changes, convert optional roots with explicit context, and use the existing strict UTF-8 prefix decoder for bounded text.

**Fix:** Restore explicit imports, add Option context, and add Unicode prefix coverage.

## Fixture already created the work directory

**What happened:** A pinned-handoff regression called WorkDir::initialize after its worktree fixture had already created .loom/work, so setup failed before the assertion.

**Why:** The test did not inspect fixture side effects.

**Prevention:** Check fixture-created state before selecting initialization versus adoption, and confirm the intended assertion actually runs.

**Fix:** Adopt the existing work directory and rerun the focused test.

## Formatting was launched while a test was queued

**What happened:** A worker ran formatting while its Cargo test was waiting on the shared target lock.

**Why:** The queued command was treated as inactive even though it could start compiling when the lock released.

**Prevention:** Finish formatting before starting tests, or cancel and restart a queued test after any source edit; coordinate shared builds.

**Fix:** Formatting completed before this test began compiling; rerun against the stable tree.

## Scoped rustfmt traversed sibling modules

**What happened:** Formatting a parent Rust module without skip_children visited sibling modules owned by parallel workers.

**Why:** Rustfmt module traversal was assumed to be limited to the named file.

**Prevention:** During parallel edits, format explicit owned files with skip_children where applicable; inspect formatting-only diff spillover before committing.

**Fix:** Stop edits before integrated formatting and review the combined diff.

## Close the assigned wave before expanding review scope

**What happened:** The user directed the review wave to stop taking new findings and finish work already in flight before full verification and commits.

**Why:** The available quota and integration cost made additional scope inappropriate.

**Prevention:** Do not start additional findings or unrelated follow-ups after a stop-scope instruction; finish or truthfully report the already assigned work, run the requested gates, update the tracker, and commit scoped changes.

**Fix:** Close the active assignment wave and move to integration verification and reviewable commits.
