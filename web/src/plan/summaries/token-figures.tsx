import { useEffect, useId, useState, type ReactElement } from "react";

import { useMediaQuery } from "@/lib/use-media-query";
import type { TokenVector } from "@/plan/api/schema-summaries";

/** The token components in the order every figure, bar and legend uses, with their labels. */
export const COMPONENTS = [
  { key: "fresh_input", label: "fresh" },
  { key: "cache_read", label: "cache reads" },
  { key: "cache_write", label: "cache writes" },
  { key: "output", label: "output" },
] as const;

/** A figure as printed: grouped digits, or "unknown" for a component the wire did not record. */
export function formatTokens(value: number | undefined): string {
  return value === undefined ? "unknown" : value.toLocaleString("en-US");
}

/** The sum of the recorded components; null when none is recorded. */
export function totalTokens(tokens: TokenVector): number | null {
  const recorded = COMPONENTS.map(({ key }) => tokens[key]).filter((v) => v !== undefined);
  return recorded.length === 0 ? null : recorded.reduce((sum, value) => sum + value, 0);
}

/** What Today's track stands for: the larger of today's total and the week's daily average. */
export function todaySpan(todayTotal: number, weekTotal: number): number {
  return Math.max(todayTotal, weekTotal / 7);
}

/**
 * One labelled figure: the label, then the figure. In a legend the label carries the component's
 * swatch (`ramp`); the last rebuild's cost line uses it bare. A missing figure reads "unknown" in
 * muted text; a recorded zero prints as 0.
 */
export function TokenFigure({
  label,
  value,
  ramp,
}: {
  label: string;
  value: number | undefined;
  ramp?: number;
}): ReactElement {
  return (
    <div className="sp-figure">
      <dt>
        {ramp !== undefined && <span className="sp-swatch" data-ramp={ramp} aria-hidden="true" />}
        {label}
      </dt>
      <dd className={value === undefined ? "sp-unknown" : undefined}>{formatTokens(value)}</dd>
    </div>
  );
}

/**
 * The stacked bar: one segment per component above zero, each announcing its label and figure,
 * widths in proportion to `span` (the value a full track stands for). `tick` marks a reference
 * value on the same scale; it is left out when there is none.
 */
export function TokenBar({
  tokens,
  span,
  tick,
}: {
  tokens: TokenVector;
  span: number;
  tick?: number;
}): ReactElement {
  const percent = (value: number) => (span > 0 ? (value / span) * 100 : 0);
  return (
    <div className="sp-bar">
      <div className="sp-track">
        {COMPONENTS.map(({ key, label }, index) => {
          const value = tokens[key];
          if (value === undefined || value === 0) return null;
          return (
            <span
              key={key}
              role="img"
              aria-label={`${label} ${formatTokens(value)}`}
              className="sp-segment"
              data-ramp={index}
              style={{ width: `${percent(value)}%` }}
            />
          );
        })}
      </div>
      {tick !== undefined && tick > 0 && span > 0 && (
        <span className="sp-tick" aria-hidden="true" style={{ left: `${percent(tick)}%` }} />
      )}
    </div>
  );
}

const COUNT_UP_MS = 400;
const easeOut = (t: number): number => 1 - (1 - t) ** 3;

/**
 * The count-up's progress, 0 to 1, eased over 400ms of animation frames; 1 at once when the
 * caller asks for no motion. The frame is cancelled on unmount. Both ends of the clock come from
 * `performance.now()`, so a frame's own timestamp (a different origin in some agents) is not mixed
 * with the start.
 */
function useCountUp(animate: boolean): number {
  const [progress, setProgress] = useState(0);
  useEffect(() => {
    if (!animate) return;
    const start = performance.now();
    let frame = requestAnimationFrame(function step() {
      const t = Math.min(1, (performance.now() - start) / COUNT_UP_MS);
      setProgress(easeOut(t));
      if (t < 1) frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [animate]);
  return animate ? progress : 1;
}

/**
 * One period's card: the total in the mono face, the stacked bar and the legend of all four
 * components. The card is a group named by its caption, so the bar's segments are read under it.
 */
export function TokenCard({
  caption,
  tokens,
  span,
  tick,
  animate,
}: {
  caption: string;
  tokens: TokenVector;
  span: number;
  tick?: number;
  animate: boolean;
}): ReactElement {
  const captionId = useId();
  const total = totalTokens(tokens);
  const progress = useCountUp(animate && total !== null);
  const shown = formatTokens(total === null ? undefined : Math.round(total * progress));
  return (
    <div role="group" aria-labelledby={captionId} className="sp-card">
      <span id={captionId} className="sp-card-caption">
        {caption}
      </span>
      <p className="sp-total">
        <span className={total === null ? "sp-total-figure sp-unknown" : "sp-total-figure"}>
          {shown}
        </span>
        <span className="sp-total-unit">tokens</span>
      </p>
      <TokenBar tokens={tokens} span={span} tick={tick} />
      <dl className="sp-legend">
        {COMPONENTS.map(({ key, label }, index) => (
          <TokenFigure key={key} label={label} value={tokens[key]} ramp={index} />
        ))}
      </dl>
    </div>
  );
}

/**
 * Today and 7 days side by side. The week's bar fills its track; Today's track stands for the
 * larger of today's total and the week's daily average, which a tick marks, so today reads as
 * above or below the average. The totals count up once per mount, only where the agent affirms
 * no motion preference and does not report a reduced one.
 */
export function TokenCards({
  today,
  week,
}: {
  today: TokenVector;
  week: TokenVector;
}): ReactElement {
  const noPreference = useMediaQuery("(prefers-reduced-motion: no-preference)");
  const reduced = useMediaQuery("(prefers-reduced-motion: reduce)");
  const animate = noPreference && !reduced;
  const todayTotal = totalTokens(today) ?? 0;
  const weekTotal = totalTokens(week) ?? 0;
  const average = weekTotal / 7;
  return (
    <div className="sp-cards">
      <div className="sp-cards-grid">
        <TokenCard
          caption="Today"
          tokens={today}
          span={todaySpan(todayTotal, weekTotal)}
          tick={average > 0 ? average : undefined}
          animate={animate}
        />
        <TokenCard caption="7 days" tokens={week} span={weekTotal} animate={animate} />
      </div>
    </div>
  );
}
