import { cn } from "cn";

import { maxOf } from "@/lib/tokens";

interface SparklineProps {
  values: readonly number[];
  width: number;
  height: number;
  label: string;
  className?: string;
  stretch?: boolean;
}

export function Sparkline({
  values,
  width,
  height,
  label,
  className,
  stretch = false,
}: SparklineProps) {
  const max = maxOf(values);
  const points = values.map((value, index) => ({
    x: values.length === 1 ? width : (index / (values.length - 1)) * width,
    y: max === 0 ? height : height - (value / max) * height,
  }));
  const first = points[0] ?? { x: 0, y: height };
  const last = points.at(-1) ?? { x: width, y: height };
  const line = points.length
    ? points.map(({ x, y }, index) => `${index === 0 ? "M" : "L"} ${x} ${y}`).join(" ")
    : `M 0 ${height} L ${width} ${height}`;

  return (
    <svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${width} ${height}`}
      width={stretch ? "100%" : width}
      height={height}
      preserveAspectRatio={stretch ? "none" : undefined}
      className={cn("overflow-visible", className)}
    >
      <path
        d={`${line} L ${last.x} ${height} L ${first.x} ${height} Z`}
        fill="currentColor"
        fillOpacity={0.2}
      />
      <path
        d={line}
        stroke="currentColor"
        strokeWidth={1.25}
        fill="none"
        vectorEffect="non-scaling-stroke"
      />
      <SparklineDot x={last.x} y={last.y} stretch={stretch} />
    </svg>
  );
}

function SparklineDot({ x, y, stretch }: { x: number; y: number; stretch: boolean }) {
  if (!stretch) return <circle cx={x} cy={y} r={1.75} fill="currentColor" />;
  // A round, non-scaling stroke keeps the endpoint circular as the chart stretches.
  return (
    <path
      d={`M ${x} ${y} h 0`}
      stroke="currentColor"
      strokeWidth={3.5}
      strokeLinecap="round"
      vectorEffect="non-scaling-stroke"
    />
  );
}

export function HeatStrip({
  values,
  max,
  label,
  className,
}: {
  values: readonly number[];
  max: number;
  label: string;
  className?: string;
}) {
  return (
    <div
      role="img"
      aria-label={label}
      className={cn("grid h-2 gap-px", className)}
      style={{ gridAutoFlow: "column", gridAutoColumns: "minmax(0, 1fr)" }}
    >
      {values.map((value, index) => {
        const ratio = max > 0 ? Math.min(1, Math.max(0, value / max)) : 0;
        return (
          <span
            key={index}
            data-level={Math.ceil(4 * ratio)}
            style={{
              background:
                ratio === 0
                  ? "transparent"
                  : `color-mix(in oklch, var(--tone-executing) ${Math.round(18 + 82 * ratio)}%, transparent)`,
            }}
          />
        );
      })}
    </div>
  );
}
