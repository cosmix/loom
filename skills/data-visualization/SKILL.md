---
name: data-visualization
description: Creates effective data visualizations, charts, dashboards, and reports across analytics, infrastructure monitoring, and ML domains. Covers library selection, UX design, and accessibility. Trigger keywords: chart, graph, plot, dashboard, report, visualization, matplotlib, plotly, d3, seaborn, grafana, tableau, superset, metabase, KPI, metric, analytics, histogram, heatmap, time-series, scatter, bar-chart.
allowed-tools: Read, Grep, Glob, Edit, Write, Bash
---

# Data Visualization

## Overview

This skill creates effective data visualizations that communicate insights clearly across multiple domains: analytics dashboards, infrastructure monitoring, and ML model performance tracking. It combines data analysis expertise, dashboard architecture patterns, UX design principles, and accessibility considerations.

## Instructions

### 1. Understand the Data and Context

- Analyze data structure, types, and granularity
- Identify key metrics, dimensions, and relationships
- Determine the analytical question or story to tell
- Consider the target audience and their technical level
- Assess update frequency and real-time requirements
- Identify domain-specific needs (analytics, monitoring, ML)

### 2. Select Visualization Domain

**Analytics Dashboards**: Business metrics, KPIs, trends, comparisons

- Tools: Tableau, Superset, Metabase, Plotly Dash, Streamlit
- Focus: Executive summaries, drill-down exploration, report generation

**Infrastructure Monitoring**: System health, resource usage, alerts

- Tools: Grafana, Prometheus, Datadog, CloudWatch
- Focus: Real-time metrics, alerting thresholds, SLA tracking

**ML Model Performance**: Training metrics, predictions, model diagnostics

- Tools: TensorBoard, Weights & Biases, MLflow, matplotlib
- Focus: Loss curves, confusion matrices, feature importance

### 3. Choose Chart Type and Layout

- Match chart type to data relationship (see Chart Selection Guide)
- Consider data volume and complexity
- Plan for interactivity needs (tooltips, filters, zoom)
- Design information hierarchy (KPIs first, details below)
- Account for accessibility (color, contrast, screen readers)

### 4. Design for Clarity and Usability

**Visual Design**:

- Choose accessible color schemes (colorblind-safe palettes)
- Label axes and data clearly with units
- Remove chart junk and unnecessary decorations
- Highlight key insights with annotations
- Use consistent styling across dashboard

**UX Considerations**:

- Provide context (baselines, targets, historical comparisons)
- Enable progressive disclosure (summary → details)
- Add clear legends and tooltips
- Include data freshness indicators
- Design for both desktop and mobile views

**Accessibility**:

- WCAG AA color contrast ratios (4.5:1 text, 3:1 graphics)
- Use patterns/textures in addition to color
- Provide alt text for static visualizations
- Support keyboard navigation for interactive charts
- Include data tables as fallback

### 5. Implement and Validate

- Build visualization with chosen tool
- Test with real data at production scale
- Validate calculations and aggregations
- Verify performance (load time < 3s for dashboards)
- Gather feedback from end users
- Iterate based on usage patterns and analytics

## Best Practices

### General Principles

1. **Right Chart for Data**: Match visualization to data relationship and question
2. **Less is More**: Remove unnecessary elements, maximize data-ink ratio
3. **Consistent Styling**: Use coherent color schemes and typography
4. **Accessible Design**: Support colorblind users, screen readers, keyboard navigation
5. **Clear Labels**: Descriptive titles, axis labels with units, data sources
6. **Context Matters**: Include baselines, targets, historical comparisons
7. **Interactive When Helpful**: Add tooltips, filters, zoom for exploration
8. **Performance First**: Optimize for < 3s dashboard load times
9. **Mobile-Responsive**: Design for both desktop and mobile viewports

### Domain-Specific Guidelines

**Analytics Dashboards**:

- Start with executive summary (4-6 KPIs with trends)
- Support drill-down from summary to detail
- Include date range selectors and common filters
- Provide export options (PDF, CSV, PNG)
- Cache expensive queries, refresh on schedule

**Infrastructure Monitoring**:

- Use time-series line charts for metrics over time
- Set alert thresholds with colored bands
- Show current status prominently (healthy/degraded/down)
- Include percentile metrics (p50, p95, p99) not just averages
- Auto-refresh dashboards every 30-60 seconds

**ML Model Performance**:

- Plot training/validation curves together
- Show confusion matrices with normalized values
- Visualize feature importance with horizontal bars
- Include sample predictions for interpretability
- Track metrics across experiments for comparison

### Accessibility Checklist

- Use colorblind-safe palettes (avoid red/green only)
- Ensure 4.5:1 contrast ratio for text
- Provide alt text for static images
- Support keyboard navigation (tab, arrow keys)
- Include data tables as alternative representation
- Test with screen readers (NVDA, JAWS)
- Use patterns/textures in addition to color
- Avoid flashing or rapidly changing content

## Examples

### Example 1: Python with Matplotlib/Seaborn

```python
import matplotlib.pyplot as plt
import seaborn as sns
import pandas as pd
import numpy as np

# Set style for professional look
plt.style.use('seaborn-v0_8-whitegrid')
sns.set_palette("husl")

# Create figure with subplots
fig, axes = plt.subplots(2, 2, figsize=(14, 10))

# Example 1: Line chart for time series
df_sales = pd.DataFrame({
    'date': pd.date_range('2024-01-01', periods=12, freq='M'),
    'revenue': [100, 120, 115, 140, 155, 170, 165, 180, 195, 210, 225, 250],
    'target': [110, 115, 120, 130, 145, 160, 175, 185, 200, 215, 230, 245]
})

ax1 = axes[0, 0]
ax1.plot(df_sales['date'], df_sales['revenue'], marker='o', linewidth=2, label='Actual')
ax1.plot(df_sales['date'], df_sales['target'], linestyle='--', linewidth=2, label='Target')
ax1.fill_between(df_sales['date'], df_sales['revenue'], df_sales['target'],
                  alpha=0.3, where=(df_sales['revenue'] >= df_sales['target']), color='green')
ax1.fill_between(df_sales['date'], df_sales['revenue'], df_sales['target'],
                  alpha=0.3, where=(df_sales['revenue'] < df_sales['target']), color='red')
ax1.set_title('Monthly Revenue vs Target', fontsize=14, fontweight='bold')
ax1.set_xlabel('Month')
ax1.set_ylabel('Revenue ($K)')
ax1.legend()
ax1.tick_params(axis='x', rotation=45)

# Example 2: Bar chart for comparison
df_products = pd.DataFrame({
    'product': ['Product A', 'Product B', 'Product C', 'Product D', 'Product E'],
    'sales': [45, 32, 28, 22, 18]
})

ax2 = axes[0, 1]
colors = sns.color_palette("Blues_r", len(df_products))
bars = ax2.barh(df_products['product'], df_products['sales'], color=colors)
ax2.bar_label(bars, padding=3, fmt='$%.0fK')
ax2.set_title('Sales by Product', fontsize=14, fontweight='bold')
ax2.set_xlabel('Sales ($K)')
ax2.invert_yaxis()

# Example 3: Scatter plot with regression
np.random.seed(42)
df_scatter = pd.DataFrame({
    'ad_spend': np.random.uniform(10, 100, 50),
    'conversions': lambda x: x['ad_spend'] * 2.5 + np.random.normal(0, 15, 50)
}.__class__.__call__(pd.DataFrame({'ad_spend': np.random.uniform(10, 100, 50)})))
df_scatter['conversions'] = df_scatter['ad_spend'] * 2.5 + np.random.normal(0, 15, 50)

ax3 = axes[1, 0]
sns.regplot(data=df_scatter, x='ad_spend', y='conversions', ax=ax3,
            scatter_kws={'alpha': 0.6}, line_kws={'color': 'red'})
ax3.set_title('Ad Spend vs Conversions', fontsize=14, fontweight='bold')
ax3.set_xlabel('Ad Spend ($K)')
ax3.set_ylabel('Conversions')

# Example 4: Pie/Donut chart for composition
df_channels = pd.DataFrame({
    'channel': ['Organic', 'Paid Search', 'Social', 'Email', 'Direct'],
    'traffic': [35, 25, 20, 12, 8]
})

ax4 = axes[1, 1]
wedges, texts, autotexts = ax4.pie(
    df_channels['traffic'],
    labels=df_channels['channel'],
    autopct='%1.1f%%',
    pctdistance=0.75,
    wedgeprops=dict(width=0.5)
)
ax4.set_title('Traffic by Channel', fontsize=14, fontweight='bold')

plt.tight_layout()
plt.savefig('dashboard.png', dpi=150, bbox_inches='tight')
plt.show()
```

### Example 2: Interactive Visualization with Plotly

```python
import plotly.express as px
import plotly.graph_objects as go
from plotly.subplots import make_subplots
import pandas as pd

# Create interactive time series
df = pd.DataFrame({
    'date': pd.date_range('2024-01-01', periods=365, freq='D'),
    'value': (pd.Series(range(365)) * 0.1 +
              np.sin(pd.Series(range(365)) * 0.1) * 20 +
              np.random.normal(0, 5, 365)).cumsum()
})

fig = go.Figure()

fig.add_trace(go.Scatter(
    x=df['date'],
    y=df['value'],
    mode='lines',
    name='Daily Value',
    line=dict(color='#1f77b4', width=1.5),
    hovertemplate='%{x|%B %d, %Y}<br>Value: %{y:.2f}<extra></extra>'
))

# Add moving average
df['ma_7'] = df['value'].rolling(7).mean()
fig.add_trace(go.Scatter(
    x=df['date'],
    y=df['ma_7'],
    mode='lines',
    name='7-day MA',
    line=dict(color='#ff7f0e', width=2, dash='dash')
))

fig.update_layout(
    title='Daily Performance with Moving Average',
    xaxis_title='Date',
    yaxis_title='Value',
    hovermode='x unified',
    template='plotly_white',
    xaxis=dict(
        rangeselector=dict(
            buttons=list([
                dict(count=7, label="1w", step="day", stepmode="backward"),
                dict(count=1, label="1m", step="month", stepmode="backward"),
                dict(count=3, label="3m", step="month", stepmode="backward"),
                dict(step="all")
            ])
        ),
        rangeslider=dict(visible=True)
    )
)

fig.write_html('interactive_chart.html')
fig.show()
```

### Example 3: Chart Type Selection Guide

```markdown
## Chart Selection by Data Type

### Comparison

- **Bar Chart**: Compare values across categories
- **Grouped Bar**: Compare multiple series across categories
- **Bullet Chart**: Show performance against target

### Distribution

- **Histogram**: Show frequency distribution
- **Box Plot**: Show distribution summary statistics
- **Violin Plot**: Show distribution shape

### Composition

- **Pie/Donut Chart**: Show parts of a whole (< 6 categories)
- **Stacked Bar**: Show composition across categories
- **Treemap**: Show hierarchical composition

### Relationship

- **Scatter Plot**: Show correlation between two variables
- **Bubble Chart**: Add third dimension via size
- **Heatmap**: Show correlation matrix

### Time Series

- **Line Chart**: Show trends over time
- **Area Chart**: Show cumulative trends
- **Candlestick**: Show OHLC financial data

### Geographic

- **Choropleth**: Show values by region
- **Point Map**: Show locations with values
- **Flow Map**: Show movement between locations
```

### Example 4: Analytics Dashboard with Streamlit

```python
# Streamlit Dashboard Example
import streamlit as st
import pandas as pd
import plotly.express as px

st.set_page_config(page_title="Sales Dashboard", layout="wide")

# Header
st.title("Sales Performance Dashboard")
st.markdown("---")

# KPI Row
col1, col2, col3, col4 = st.columns(4)
with col1:
    st.metric("Total Revenue", "$1.2M", "+12%")
with col2:
    st.metric("Orders", "8,543", "+8%")
with col3:
    st.metric("Avg Order Value", "$140", "+3%")
with col4:
    st.metric("Conversion Rate", "3.2%", "-0.5%")

st.markdown("---")

# Filters
with st.sidebar:
    st.header("Filters")
    date_range = st.date_input("Date Range", [])
    region = st.multiselect("Region", ["North", "South", "East", "West"])
    category = st.selectbox("Category", ["All", "Electronics", "Clothing", "Home"])

# Main Charts
left_col, right_col = st.columns([2, 1])

with left_col:
    st.subheader("Revenue Trend")
    # Line chart here

with right_col:
    st.subheader("Sales by Region")
    # Pie chart here

# Detail Table
st.subheader("Recent Orders")
# Data table here
```

### Example 5: Grafana Dashboard for Infrastructure Monitoring

```yaml
# Grafana dashboard JSON structure
# Save as dashboard.json and import via Grafana UI

apiVersion: 1
providers:
  - name: "default"
    folder: "Infrastructure"
    type: file
    options:
      path: /etc/grafana/provisioning/dashboards

---
# Dashboard panels configuration
{
  "dashboard":
    {
      "title": "Service Health Dashboard",
      "tags": ["infrastructure", "monitoring"],
      "timezone": "browser",
      "panels":
        [
          {
            "id": 1,
            "title": "Request Rate",
            "type": "graph",
            "targets":
              [
                {
                  "expr": "rate(http_requests_total[5m])",
                  "legendFormat": "{{service}}",
                },
              ],
            "yaxes": [{ "format": "reqps", "label": "Requests/sec" }],
            "alert":
              {
                "conditions":
                  [
                    {
                      "evaluator": { "params": [100], "type": "gt" },
                      "query": { "params": ["A", "5m", "now"] },
                    },
                  ],
              },
          },
          {
            "id": 2,
            "title": "Error Rate",
            "type": "graph",
            "targets":
              [
                {
                  "expr": "rate(http_errors_total[5m]) / rate(http_requests_total[5m])",
                  "legendFormat": "Error %",
                },
              ],
            "fieldConfig":
              {
                "defaults":
                  {
                    "thresholds":
                      {
                        "steps":
                          [
                            { "value": 0, "color": "green" },
                            { "value": 0.01, "color": "yellow" },
                            { "value": 0.05, "color": "red" },
                          ],
                      },
                  },
              },
          },
          {
            "id": 3,
            "title": "Response Time (p95)",
            "type": "graph",
            "targets":
              [
                {
                  "expr": "histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))",
                  "legendFormat": "p95",
                },
              ],
          },
        ],
      "refresh": "30s",
      "time": { "from": "now-1h", "to": "now" },
    },
}
```

### Example 6: ML Model Performance Visualization

```python
# TensorBoard-style training visualization
import matplotlib.pyplot as plt
import numpy as np
from sklearn.metrics import confusion_matrix, classification_report
import seaborn as sns

# Training history visualization
fig, axes = plt.subplots(2, 2, figsize=(14, 10))

# Loss curves
ax1 = axes[0, 0]
epochs = range(1, 51)
train_loss = np.exp(-np.array(epochs) / 10) + np.random.normal(0, 0.05, 50)
val_loss = np.exp(-np.array(epochs) / 10) + np.random.normal(0, 0.08, 50) + 0.1

ax1.plot(epochs, train_loss, label='Training Loss', linewidth=2)
ax1.plot(epochs, val_loss, label='Validation Loss', linewidth=2)
ax1.axvline(x=30, color='red', linestyle='--', alpha=0.5, label='Best Model')
ax1.set_xlabel('Epoch')
ax1.set_ylabel('Loss')
ax1.set_title('Training and Validation Loss', fontsize=14, fontweight='bold')
ax1.legend()
ax1.grid(alpha=0.3)

# Accuracy curves
ax2 = axes[0, 1]
train_acc = 1 - train_loss
val_acc = 1 - val_loss

ax2.plot(epochs, train_acc, label='Training Accuracy', linewidth=2)
ax2.plot(epochs, val_acc, label='Validation Accuracy', linewidth=2)
ax2.axvline(x=30, color='red', linestyle='--', alpha=0.5)
ax2.set_xlabel('Epoch')
ax2.set_ylabel('Accuracy')
ax2.set_title('Training and Validation Accuracy', fontsize=14, fontweight='bold')
ax2.legend()
ax2.grid(alpha=0.3)

# Confusion Matrix
ax3 = axes[1, 0]
y_true = np.random.randint(0, 3, 500)
y_pred = y_true.copy()
y_pred[np.random.random(500) < 0.15] = np.random.randint(0, 3, (y_pred.shape[0]))

cm = confusion_matrix(y_true, y_pred)
sns.heatmap(cm, annot=True, fmt='d', cmap='Blues', ax=ax3,
            xticklabels=['Class A', 'Class B', 'Class C'],
            yticklabels=['Class A', 'Class B', 'Class C'])
ax3.set_xlabel('Predicted')
ax3.set_ylabel('Actual')
ax3.set_title('Confusion Matrix', fontsize=14, fontweight='bold')

# Feature Importance
ax4 = axes[1, 1]
features = ['Feature A', 'Feature B', 'Feature C', 'Feature D', 'Feature E']
importance = np.array([0.35, 0.28, 0.18, 0.12, 0.07])

colors = plt.cm.viridis(importance / importance.max())
bars = ax4.barh(features, importance, color=colors)
ax4.set_xlabel('Importance')
ax4.set_title('Feature Importance', fontsize=14, fontweight='bold')
ax4.invert_yaxis()

for i, (bar, val) in enumerate(zip(bars, importance)):
    ax4.text(val + 0.01, i, f'{val:.2f}', va='center')

plt.tight_layout()
plt.savefig('ml_performance_dashboard.png', dpi=150, bbox_inches='tight')
plt.show()
```

### Example 7: Accessible Color Palettes

```python
# Colorblind-safe palettes for data visualization
import matplotlib.pyplot as plt
import seaborn as sns

# Define accessible color schemes
palettes = {
    'colorblind_safe': ['#0173B2', '#DE8F05', '#029E73', '#CC78BC', '#CA9161'],
    'high_contrast': ['#000000', '#E69F00', '#56B4E9', '#009E73', '#F0E442'],
    'okabe_ito': ['#E69F00', '#56B4E9', '#009E73', '#F0E442', '#0072B2', '#D55E00', '#CC79A7']
}

# Test visualization with accessible palette
data = [25, 20, 18, 15, 12, 10]
labels = ['A', 'B', 'C', 'D', 'E', 'F']

fig, axes = plt.subplots(1, 3, figsize=(15, 4))

for ax, (name, colors) in zip(axes, palettes.items()):
    ax.bar(labels, data, color=colors[:len(data)])
    ax.set_title(f'{name.replace("_", " ").title()}', fontsize=12, fontweight='bold')
    ax.set_ylabel('Value')

    # Add value labels for accessibility
    for i, (label, value) in enumerate(zip(labels, data)):
        ax.text(i, value + 0.5, str(value), ha='center', va='bottom')

plt.tight_layout()
plt.savefig('accessible_palettes.png', dpi=150, bbox_inches='tight')
```
