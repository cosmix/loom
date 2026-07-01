---
name: loom-data-visualization
description: Build effective charts, dashboards, and reports across analytics, infrastructure monitoring, and ML domains. Use for library selection, visualization UX, accessibility, and domain-specific dashboard design.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - chart
  - graph
  - plot
  - dashboard
  - report
  - visualization
  - matplotlib
  - seaborn
  - plotly
  - d3
  - vega-lite
  - echarts
  - grafana
  - tableau
  - superset
  - metabase
  - KPI
  - analytics
  - histogram
  - heatmap
  - time-series
  - scatter
  - bar-chart
  - colorblind
  - accessibility
---

# Data Visualization

## Overview

Match the chart to the data relationship, encode quantities in perceptually accurate channels, avoid distortion, and make it accessible. This skill is the design layer above the plotting library.

## Chart selection by data relationship

| Goal | Prefer | Avoid |
| --- | --- | --- |
| Compare across categories | horizontal bar (sorted), dot plot | pie with >~5 slices |
| Part-to-whole | stacked bar, treemap; pie only ≤5 slices | many pies / donuts for precise comparison |
| Distribution | histogram, box, violin, ECDF | bar of means (hides spread) |
| Two-variable relationship | scatter (+ trend), 2D density/hexbin when dense | scatter with 100k overplotted points |
| Correlation matrix | heatmap (diverging scale) | 3D surface |
| Trend over time | line; area for cumulative | connecting unordered categories with lines |
| Ranking | ordered bar / lollipop | pie |
| Performance vs target | bullet chart | gauge cluster |
| Geographic | choropleth (normalized), point/flow map | raw-count choropleth (just shows population) |

## Perceptual accuracy (why bars beat pies)

Cleveland–McGill ranking of how accurately humans decode a quantity:

**position on common scale > position on non-aligned scale > length > angle/slope > area > volume > color hue/saturation.**

- Encode the *most important* quantity in position/length (bar, dot, line), not area or color.
- Pie/donut = angle+area (weak); bubble = area (people underestimate large circles — area scales as r², so double the value ≠ double the radius). Reserve area/color for secondary dimensions.
- Sort categorical bars by value (not alphabetically) unless order is semantic — sorting *is* the insight.

## Avoiding distortion (the "lie factor")

- **Truncated y-axis** exaggerates change. Bar charts **must** start at 0 (bar length encodes the value). Line/time-series *may* zoom the axis to show variation — but label it clearly; don't imply a 2% change is a cliff.
- **Dual y-axes** manufacture spurious correlation and let you slide two series' scales arbitrarily. Replace with indexed series (all = 100 at t0), two small multiples, or a ratio.
- **Aim for lie factor ≈ 1** (graphic effect size / data effect size). Don't use 3D, shadows, or area to represent 1D quantities.
- **Chartjunk:** maximize data-ink ratio (Tufte) — drop gridline clutter, heavy borders, redundant legends, background gradients. Direct-label lines instead of a legend when few series.

## Color and accessibility

- **Sequential:** `viridis`/`cividis` — perceptually uniform *and* colorblind-safe. Avoid `jet`/rainbow (non-uniform; invents false boundaries; not CVD-safe).
- **Diverging** (signed around a midpoint): `RdBu`, `coolwarm` — set the neutral point at the meaningful zero.
- **Qualitative** (categories, ≤~8): Okabe–Ito or ColorBrewer `Set2`. Beyond ~8 colors, hue stops being distinguishable — use small multiples, top-N+Other, or direct labels instead.
- **Never rely on hue alone** (~8% of men have CVD): add redundant encoding — shape/linestyle, direct labels, or patterns. Red/green is the worst offender.
- WCAG AA: 4.5:1 contrast for text, 3:1 for meaningful graphics. Provide alt text and a data-table fallback for interactive charts; support keyboard nav.

## Handling data volume

- **Overplotting:** for dense scatter use hexbin / 2D density / alpha-blending / sampling — not 100k opaque points. For many time series use small multiples or highlight-one-fade-rest, not a spaghetti chart.
- **High-cardinality categoricals:** top-N by value + an "Other" bucket; horizontal sorted bars; never a 30-slice pie or 30-color legend.
- **Time-series downsampling:** don't push 1M points into 800px. Aggregate to the display resolution (LTTB — largest-triangle-three-buckets — preserves visual shape) or roll up to buckets. Show `p50/p95/p99`, not just the mean, for latency; show gaps for missing data rather than interpolating across them.
- **Log scale** for data spanning orders of magnitude or multiplicative/growth relationships — label it explicitly (readers assume linear). ⚠ Log can't show zero/negative values.

## Library selection

| Library | Sweet spot | Trade-off |
| --- | --- | --- |
| **Vega-Lite** | Declarative grammar-of-graphics; standard statistical charts fast; JSON spec = embeddable/serializable | Limited for bespoke/novel viz |
| **D3** | Maximal control; custom/novel visualizations; SVG+canvas | Steep; high build effort — overkill for standard charts |
| **Observable Plot** | Concise grammar-of-graphics in JS; lighter middle ground vs D3 | Younger ecosystem |
| **Plotly / Dash** | Interactive out-of-the-box across Py/JS/R; notebooks + web apps | Heavy JS bundle |
| **ECharts** | High-perf canvas; large datasets; rich chart variety; web dashboards | Config-heavy |
| **Matplotlib / Seaborn** | Static publication figures; Python analysis/ML | Not interactive; verbose |
| **Grafana** | Ops/time-series dashboards over live data sources; alerting | Not for ad-hoc exploratory analytics |

Rule of thumb: **standard statistical chart → Vega-Lite/Plotly/Seaborn; bespoke/interactive-novel → D3/Observable Plot; ops time-series → Grafana.** Reach for D3 only when a grammar-of-graphics tool genuinely can't express the design.

## Dashboard design

- **Most-important-top-left** (F/Z reading pattern); lead with 4–6 KPIs (value + trend/delta + context/target), details below, progressive disclosure via drill-down.
- Consistent axes, color meaning, and units across panels — the same color must mean the same thing everywhere.
- Provide context on every number: baseline, target, or prior period. A lone "1,240" is noise.
- **Analytics:** date-range + filters; export (CSV/PNG); cache expensive queries.
- **Monitoring:** time-series lines; threshold bands; current status prominent; percentiles not averages; auto-refresh 30–60s.
- **ML:** train+val curves on one axis (gap = overfit); normalized confusion matrix; horizontal feature-importance bars; per-experiment comparison.

## Reference examples

Matplotlib multi-panel with target band and value labels:

```python
import matplotlib.pyplot as plt
import pandas as pd

df = pd.DataFrame({
    "date": pd.date_range("2024-01-01", periods=12, freq="ME"),
    "revenue": [100,120,115,140,155,170,165,180,195,210,225,250],
    "target":  [110,115,120,130,145,160,175,185,200,215,230,245],
})
fig, ax = plt.subplots(figsize=(8, 5))
ax.plot(df.date, df.revenue, marker="o", lw=2, label="Actual")
ax.plot(df.date, df.target, ls="--", lw=2, label="Target")
ax.fill_between(df.date, df.revenue, df.target,
                where=df.revenue >= df.target, alpha=0.25, color="#029E73")  # ahead
ax.fill_between(df.date, df.revenue, df.target,
                where=df.revenue <  df.target, alpha=0.25, color="#D55E00")  # behind
ax.set(title="Monthly Revenue vs Target", xlabel="Month", ylabel="Revenue ($K)")
ax.legend(); ax.margins(x=0.01)
plt.tight_layout()
```

Interactive Plotly time series with range selector + moving average:

```python
import plotly.graph_objects as go
fig = go.Figure()
fig.add_scatter(x=df.date, y=df.value, mode="lines", name="Daily",
                hovertemplate="%{x|%b %d}<br>%{y:.1f}<extra></extra>")
fig.add_scatter(x=df.date, y=df.value.rolling(7).mean(), mode="lines",
                name="7d MA", line=dict(dash="dash"))
fig.update_layout(template="plotly_white", hovermode="x unified",
                  xaxis=dict(rangeslider=dict(visible=True)))
```

Colorblind-safe qualitative palette (Okabe–Ito) with redundant value labels:

```python
OKABE_ITO = ["#E69F00", "#56B4E9", "#009E73", "#F0E442", "#0072B2", "#D55E00", "#CC79A7"]
bars = ax.bar(labels, values, color=OKABE_ITO[:len(values)])
ax.bar_label(bars)  # direct labels: readable without color perception
```

## Gotchas

- `matplotlib` `freq="M"` is deprecated → use `"ME"` (month-end) in pandas ≥2.2.
- Seaborn style names are versioned: `plt.style.use("seaborn-v0_8-whitegrid")` (the bare `seaborn-*` names were removed in Matplotlib 3.6+).
- Pie charts of percentages that don't sum to 100, or with negatives, are meaningless — validate the data is a true part-to-whole first.
- Rainbow/`jet` colormaps create perceptual banding that fabricates structure — banned for quantitative encoding.
- Averaging percentiles or averaging rates across groups (Simpson's paradox) misleads — aggregate raw counts, then compute the ratio.

## Checklist — before shipping a chart/dashboard

- [ ] Chart type matches the data relationship (not just what's easy)
- [ ] Key quantity encoded in position/length, not area/color alone
- [ ] Bars start at 0; no deceptive dual axes; lie factor ≈ 1
- [ ] Colorblind-safe palette; meaning not conveyed by hue alone; 4.5:1 text contrast
- [ ] Axes labeled with units; log scale (if used) labeled; legend or direct labels present
- [ ] Dense data downsampled/aggregated; high-cardinality categories bucketed (top-N + Other)
- [ ] Context shown (baseline/target/prior); percentiles not just means for latency
- [ ] Alt text / data-table fallback for accessibility; consistent color meaning across panels
- [ ] Validated against real production-scale data; dashboard loads < 3s
