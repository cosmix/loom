# Web Dashboard

> UI

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

## Quota layout compacted too early

**What happened**: The footer quota layout switched to stacked provider rows at 1100px of available width, before the inline meters needed to wrap.

**Why**: The responsive threshold was selected without checking the operator preferred keeping the compact inline meter pairs longer.

**Prevention**: Base quota wrapping on the measured width of the compact provider groups and preserve the inline layout while it fits.

**Fix**: Keep compact provider groups inline until the available width requires shared rows.

## Verify help illustrations alongside typography changes

**What happened**: A token-flow font adjustment did not explicitly verify the corresponding help illustration before reporting completion.

**Why**: The source component was checked without tracing its help consumer.

**Prevention**: Trace help illustrations for dashboard visual changes and verify the example uses the updated production component.

**Fix**: Check the token-flow help example alongside the responsive quota layout.

## Model history chart metric

**What happened**: The operator selected fresh tokens per minute for the By model history chart.

**Why**: The chart should compare model throughput over the same recent window as the token-flow sidebar.

**Prevention**: Keep the metric explicit on the chart and combine efforts into one series per model.

**Fix**: Plot a separate color-matched model series over the last 30 minutes.

## Reuse model totals for sidebar history

**What happened**: I proposed extending the token tracker before using the per-model cumulative totals already supplied for the pie chart; the operator directed me to reuse those totals.

**Why**: I focused on reconstructing history from agent data instead of sampling the existing model totals.

**Prevention**: For this chart, sample flow.models totals and derive fresh tokens per minute from their deltas, combining efforts per model.

**Fix**: Keep the change in the web UI, retain a bounded recent history, and explain that history starts when the dashboard observes samples.

## Model history uses stacked time bars

**What happened**: The operator rejected the model history line chart and requested a stacked time-series bar graph.

**Why**: The separate lines did not give the desired visual comparison of total throughput and each model contribution.

**Prevention**: Render model throughput as stacked bars with the existing per-model pie colors.

**Fix**: Keep cumulative-total sampling and fresh tokens per minute; group samples into time buckets and stack model contributions in each bar, including the production help illustration.

## Model history bars disappear during live updates

**What happened**: The operator reported that the new stacked model histogram populated inconsistently, with bars appearing and disappearing.

**Why**: The initial validation covered sample totals and static rendering but did not establish stable behavior across successive live updates.

**Prevention**: Verify time-bucket identity and retention across incremental snapshots, including idle periods and gaps.

**Fix**: Diagnose and correct live histogram bucketing with a regression covering consecutive updates.

## Separate model segments in both charts

**What happened**: The operator requested visible separation between model segments, including the pie chart, because neighboring hues were difficult to distinguish.

**Why**: Adjacent colored fills lacked a neutral boundary.

**Prevention**: Keep shared model colors and add visible separators in both stacked bars and pie slices.

**Fix**: Add thin background-colored boundaries while correcting the live histogram bucket stability.

## Histogram slots must use fixed clock boundaries

**What happened**: The operator confirmed that histogram slots must be fixed in time. The regression reproduced a completed bar disappearing when the latest timestamp advanced five seconds.

**Why**: All sixty bucket starts were calculated relative to the newest sample rather than aligned to clock boundaries.

**Prevention**: Anchor each 30-second slot to an epoch multiple of 30; advance the visible window only at bucket boundaries.

**Fix**: Align the chart window to fixed slots and preserve completed bars across subsequent snapshots.

## Use a more distinct shared model palette

**What happened**: After requesting segment separators, the operator also explicitly requested new model colors in both charts.

**Why**: The evenly spaced hue palette did not provide satisfactory visual distinction.

**Prevention**: Use a deliberately ordered palette starting with blue, amber, and magenta, shared by both charts and their legend.

**Fix**: Replace automatic equal hue spacing with distinct preset colors and retain separators.

## Histogram scale and rate comparison must stay consistent

**What happened**: The operator reported that chart rates disagreed with values by the pie and that the y-axis moved unpredictably.

**Why**: The chart recomputed its exact maximum at every update; its 30-second sampled rates also differ from the existing agent-derived five-minute legend rates.

**Prevention**: Use stable rounded axis limits and explicitly align the model chart and model rate legend to one calculation and averaging window.

**Fix**: Verify live samples, stabilize the y-axis, and share model rate values between the chart and legend.

## Convert fixed 30-second bar counts by multiplying by two

**What happened**: I proposed changing the model legend and averaging the histogram over five minutes. The operator rejected both: fix the bar chart alone, and convert each fixed 30-second token count to tokens per minute by multiplying by two.

**Why**: I confused rate normalization over partial observations with the units of a fixed-width histogram bin.

**Prevention**: Keep each bar tied to one fixed 30-second interval; multiply its accumulated count by 60/30, including a still-filling bar. Do not change the pie or its legend to match a chart bug.

**Fix**: Replace elapsed-time normalization with a fixed factor of two, retain fixed clock boundaries, and stabilize the axis independently.

## Verify the reported histogram rate discrepancy against raw counts

**What happened**: The operator still observed roughly 110k/min in a histogram bar while the existing fresh rate read 12.4k/min, and confirmed the pie was correct.

**Why**: A corrected bin conversion alone does not establish whether the displayed discrepancy is a data bug or different averaging periods.

**Prevention**: Capture the actual cumulative model deltas and existing rate over a full fixed interval; explain the arithmetic with evidence before claiming resolution.

**Fix**: Compare live 30-second token increases against the chart while leaving the pie and its legend unchanged.
