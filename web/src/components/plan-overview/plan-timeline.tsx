import type { PlanStats } from "@/api/schema";

import {
  BAR_HEIGHT,
  BAR_TOP,
  PHASE_LABEL,
  ROW,
  TOP,
  WIDTH,
  timelineLayout,
  type TimelineLayout,
  type TimelineRow,
} from "./timeline-layout";

const HATCH_ID = "plan-overview-hatch";
const KEY_PHASES = ["contract", "executing", "adjudication", "waiting", "retry"] as const;

function SkippedStub({ row }: { row: TimelineRow }) {
  return (
    <>
      <line
        x1={row.barX}
        x2={row.barX + 26}
        y1={row.y + 18}
        y2={row.y + 18}
        className="po-tl-skip"
      />
      <text x={row.barX + 32} y={row.y + 22} className="po-tl-sub">
        skipped
      </text>
    </>
  );
}

/// One stage's bar: the hatched span from first start to completion, the phase segments over
/// it, both clipped to the same rounded outline so a segment at either end keeps the corners.
function StageBar({ row, clipId }: { row: TimelineRow; clipId: string }) {
  const y = row.y + BAR_TOP;
  return (
    <>
      <clipPath id={clipId}>
        <rect x={row.barX} y={y} width={Math.max(row.barWidth, 2)} height={BAR_HEIGHT} rx={4} />
      </clipPath>
      <g clipPath={`url(#${clipId})`}>
        <rect
          x={row.barX}
          y={y}
          width={row.barWidth}
          height={BAR_HEIGHT}
          fill={`url(#${HATCH_ID})`}
        />
        {row.segments.map((segment) => (
          <rect
            key={segment.key}
            x={segment.x}
            y={y}
            width={segment.width}
            height={BAR_HEIGHT}
            className={`po-phase po-phase-${segment.phase}`}
            data-phase={segment.phase}
          >
            <title>{segment.title}</title>
          </rect>
        ))}
      </g>
      <text x={row.durationX} y={row.y + 22} className="po-tl-dur">
        {row.duration}
      </text>
    </>
  );
}

function StageRow({ row, index }: { row: TimelineRow; index: number }) {
  return (
    <g data-stage={row.id} data-skipped={row.skipped || undefined}>
      {index % 2 === 1 && (
        <rect x={0} y={row.y} width={WIDTH} height={ROW} className="po-tl-band" />
      )}
      <text x={14} y={row.y + 16} className="po-tl-id">
        {row.label !== row.id && <title>{row.id}</title>}
        {row.label}
      </text>
      <text x={14} y={row.y + 30} className="po-tl-sub">
        {row.sub}
      </text>
      {row.skipped ? (
        <SkippedStub row={row} />
      ) : (
        <StageBar row={row} clipId={`plan-overview-bar-${index}`} />
      )}
    </g>
  );
}

/// The hour grid over the rows, the plan's end as a dashed green line, and on a long timeline
/// the same labels again under the last row.
function Axis({ layout }: { layout: TimelineLayout }) {
  const marks = [
    ...layout.ticks.map((tick) => ({ ...tick, end: false })),
    { ...layout.end, end: true },
  ];
  return (
    <g>
      {marks.map((mark) => (
        <g key={`${mark.end}-${mark.x}`}>
          <line
            x1={mark.x}
            x2={mark.x}
            y1={TOP - 8}
            y2={layout.gridBottom}
            className={mark.end ? "po-grid-end" : "po-grid"}
          />
          {[TOP - 14, layout.axisRepeatY].map(
            (y) =>
              y !== null && (
                <text
                  key={y}
                  x={mark.x}
                  y={y}
                  textAnchor="middle"
                  className={mark.end ? "po-tl-tick po-tl-end" : "po-tl-tick"}
                >
                  {mark.label}
                </text>
              ),
          )}
        </g>
      ))}
    </g>
  );
}

function HatchPattern() {
  return (
    <defs>
      <pattern
        id={HATCH_ID}
        width={5}
        height={5}
        patternUnits="userSpaceOnUse"
        patternTransform="rotate(45)"
      >
        <rect width={5} height={5} className="po-hatch-bg" />
        <rect width={1.6} height={5} className="po-hatch-line" />
      </pattern>
    </defs>
  );
}

export function TimelineKey() {
  return (
    <ul className="po-key" aria-label="Timeline colors">
      {KEY_PHASES.map((phase) => (
        <li key={phase}>
          <i className={`po-sw po-phase-${phase}`} aria-hidden="true" />
          {PHASE_LABEL[phase]}
        </li>
      ))}
      <li>
        <i className="po-sw po-sw-skip" aria-hidden="true" />
        Skipped
      </li>
    </ul>
  );
}

function TimelineChart({ record }: { record: PlanStats }) {
  const layout = timelineLayout(record);
  return (
    // Focusable so the keyboard can scroll the drawing sideways where the dialog is narrower.
    <div className="po-scroll po-tl" tabIndex={0} role="group" aria-label="Timeline chart">
      <svg
        viewBox={`0 0 ${WIDTH} ${layout.height}`}
        role="img"
        aria-label={`Timeline of ${record.stages.length} stages`}
      >
        <HatchPattern />
        <Axis layout={layout} />
        {layout.rows.map((row, index) => (
          <StageRow key={`${row.id}-${index}`} row={row} index={index} />
        ))}
      </svg>
    </div>
  );
}

export function PlanTimeline({ record }: { record: PlanStats }) {
  return (
    <section className="po-section" aria-labelledby="po-timeline-title">
      <div className="po-section-head">
        <h3 id="po-timeline-title">Timeline</h3>
        <span className="po-note">
          Bars run from first start to completion; the hatched part is waiting and merge.
        </span>
      </div>
      {record.stages.length === 0 ? (
        <p className="po-empty">This plan recorded no stages.</p>
      ) : (
        <>
          <TimelineChart record={record} />
          <TimelineKey />
        </>
      )}
    </section>
  );
}
