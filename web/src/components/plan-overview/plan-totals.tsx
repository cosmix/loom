import type { CSSProperties } from "react";

import type { PlanStats } from "@/api/schema";
import { compactTokens } from "@/lib/tokens";

import {
  EMPTY,
  allTokens,
  clockHm,
  formatSpan,
  freshText,
  hitText,
  parallelText,
} from "./plan-math";

/// Token kinds in mix-bar order, each with the theme token Settings' statistics already use.
export const TOKEN_KINDS = [
  { key: "input", label: "Input", color: "var(--stat-fresh)" },
  { key: "cache_write", label: "Cache write", color: "var(--stat-write)" },
  { key: "output", label: "Output", color: "var(--stat-output)" },
  { key: "cache_read", label: "Cache read", color: "var(--stat-read)" },
] as const;

function Kpi({ label, value, sub }: { label: string; value: string; sub: string }) {
  return (
    <div className="po-kpi">
      <span className="po-kpi-label">{label}</span>
      <span className="po-kpi-value">{value}</span>
      <span className="po-kpi-sub">{sub}</span>
    </div>
  );
}

export function PlanKpis({ record }: { record: PlanStats }) {
  const models = record.models.length;
  return (
    <div className="po-kpis" role="group" aria-label="Totals">
      <Kpi
        label="Wall time"
        value={formatSpan(record.wall_secs)}
        sub={`${clockHm(record.started_at)} to ${clockHm(record.ended_at)}`}
      />
      <Kpi
        label="Stage execution"
        value={formatSpan(record.execution_secs)}
        sub={parallelText(record) ?? EMPTY}
      />
      <Kpi label="Fresh tokens" value={freshText(record)} sub="input + cache write + output" />
      <Kpi
        label="Cache hit"
        value={hitText(record)}
        sub={`${compactTokens(record.totals.cache_read)} cache reads`}
      />
      <Kpi
        label="All tokens"
        value={compactTokens(allTokens(record))}
        sub={`across ${models} ${models === 1 ? "model" : "models"}`}
      />
    </div>
  );
}

export function TokenMix({ record }: { record: PlanStats }) {
  return (
    <div className="po-mix">
      <div className="po-mix-bar" role="img" aria-label="Token mix">
        {TOKEN_KINDS.map((kind) => (
          <span
            key={kind.key}
            title={kind.label}
            data-empty={record.totals[kind.key] === 0 || undefined}
            style={
              { "--po-grow": record.totals[kind.key], background: kind.color } as CSSProperties
            }
          />
        ))}
      </div>
      <ul className="po-legend" aria-label="Token mix legend">
        {TOKEN_KINDS.map((kind) => (
          <li key={kind.key}>
            <i style={{ background: kind.color }} aria-hidden="true" />
            {kind.label} <b>{compactTokens(record.totals[kind.key])}</b>
          </li>
        ))}
      </ul>
    </div>
  );
}
