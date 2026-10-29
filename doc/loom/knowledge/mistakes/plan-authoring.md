# Plan Authoring

> Plan gaps

## Web stages left the embedded bundle stale

**What happened:** In the planner-ui-redesign plan only design-foundation rebuilt and committed web/dist. kickoff-session, feature-ui and document-ui changed web/src but did not commit a rebuilt dist. The installed binary therefore served the old frontend against the new plan-server API, so the planner looked unchanged and broke in many places.
**Why:** web/dist is committed and embedded at compile time (architecture/web-dashboard.md). The later stages' files globs did not include web/dist, and the plan rebuilt it only for integration-verify review fixes.
**Prevention:** Any plan whose stages change web/src either lists web/dist in each such stage's files and requires a rebuild-and-commit (with an acceptance check that dist matches src), or has a final stage that always rebuilds and commits web/dist.
**Fix:** Run bun run build in web/, commit web/dist, then reinstall the binary with dev-install.sh.
