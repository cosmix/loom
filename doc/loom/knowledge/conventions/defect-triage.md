# Defect Triage

> Blocking and security defects are fixed when found, not deferred

## Fix Blocking and Security Defects When Found

A loom defect that blocks a stage or opens a security hole is fixed in the session that finds it. It is never parked as a concern for a later plan, and the operator does not have to approve it as an extra. This covers a gate or guard that lets a stage park itself with no structured route, a sandbox rule that fails a correct stage, and a confinement hole. A concern entry is for gaps that block nothing and expose nothing. When such a fix needs its own plan (a daemon or schema change), say so and propose the plan in the same session.

**Why:** the operator's standing direction, after a session found a block-guard hole while unblocking a stage and asked whether to include the fix.
