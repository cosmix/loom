# Web Dashboard

> Dashboard UI

## Codex lanes clustered as one group instead of per forwarder/job pair

**What happened:** The stage token panel put every codex job and every loom-codex-forwarder lane in a single cluster, and the cluster's border and padding narrowed its heat strips so their cells no longer lined up with the plain lanes.
**Why:** AgentFlow had no link from a codex job to its forwarder, and the cluster frame's inset was never measured against the outer lane grid.
**Prevention:** Group codex lanes by AgentFlow.forwarder, which is set from the forward receipt's identity.agent_id. Any frame nested in the lane list must cancel its own border and padding with a negative margin, and the strip x ranges must be checked in a real browser.
**Fix:** d327afcf (codexPairs in web/src/lib/tokens.ts, -mx-[11px] on CodexCluster).

## Folded tokens section showed one unlabeled sparkline

**What happened:** the stage modal's tokens section was asked to show only the sparklines when folded. The folded view showed one stretched fresh-rate sparkline with no label, which the operator rejected. The folded view must keep every sparkline the expanded view shows, with its labels.
**Why:** the brief said "only the sparkline(s) show". The subagent read that as license to pick one, and the main agent passed its choice on as an open question instead of checking it against the request.
**Prevention:** when a fold keeps a subset of a section, the brief names exactly which elements stay and states that they keep their expanded look. Check a subagent's report against the literal request before relaying it.
**Fix:** the folded view renders the same chips' sparklines with their labels.
