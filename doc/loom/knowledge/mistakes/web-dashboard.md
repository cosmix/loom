# Web Dashboard

> Dashboard UI

## Codex lanes clustered as one group instead of per forwarder/job pair

**What happened:** The stage token panel put every codex job and every loom-codex-forwarder lane in a single cluster, and the cluster's border and padding narrowed its heat strips so their cells no longer lined up with the plain lanes.
**Why:** AgentFlow had no link from a codex job to its forwarder, and the cluster frame's inset was never measured against the outer lane grid.
**Prevention:** Group codex lanes by AgentFlow.forwarder, which is set from the forward receipt's identity.agent_id. Any frame nested in the lane list must cancel its own border and padding with a negative margin, and the strip x ranges must be checked in a real browser.
**Fix:** d327afcf (codexPairs in web/src/lib/tokens.ts, -mx-[11px] on CodexCluster).
