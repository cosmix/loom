import { cn } from "cn";

import type { ProviderQuota, QuotaSnapshot, WindowKind } from "@/api/schema";
import { toneClass } from "@/components/state-badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatStamp, type Tone } from "@/lib/format";
import {
  ageText,
  displayPercent,
  providerRows,
  quotaHealth,
  resetText,
  windowLabel,
  windowOf,
  type ProviderName,
  type QuotaHealth,
} from "@/lib/quota";

const HEALTH_TONE: Record<QuotaHealth, Tone> = {
  green: "completed",
  yellow: "warning",
  red: "blocked",
};

const WINDOW_KINDS: readonly WindowKind[] = ["five-hour", "seven-day"];
const WINDOW_SECS: Record<WindowKind, number> = { "five-hour": 18000, "seven-day": 604800 };

function localTime(epochSecs: number): string {
  return formatStamp(epochSecs * 1000);
}

/// Provider quota for the status bar: one group per provider, each with a
/// `5h` and a `7d` slot. Every slot is a two-line gauge — a notched usage bar
/// that fills in the health tone over a hairline that drains as the window's
/// reset approaches — with the percent and countdown printed beside it, so a
/// window that reports later drops into place without moving its neighbours.
export function QuotaMeters({
  snapshot,
  nowSecs,
  className,
}: {
  snapshot: QuotaSnapshot;
  nowSecs: number;
  className?: string;
}) {
  const rows = providerRows(snapshot);
  if (rows.length === 0) return null;
  return (
    <div className={cn("flex flex-wrap items-center gap-x-6 gap-y-1", className)}>
      {rows.map(([provider, quota]) => (
        <ProviderGroup key={provider} provider={provider} quota={quota} nowSecs={nowSecs} />
      ))}
    </div>
  );
}

function ProviderGroup({
  provider,
  quota,
  nowSecs,
}: {
  provider: ProviderName;
  quota: ProviderQuota;
  nowSecs: number;
}) {
  const age = ageText(quota.observed_at, nowSecs);
  return (
    <div
      data-stale={age === null ? undefined : ""}
      className="group inline-flex flex-wrap items-center gap-x-4 gap-y-1"
    >
      <span className="eyebrow whitespace-nowrap">
        {provider}
        {age !== null && <span className="tracking-normal normal-case"> · {age}</span>}
      </span>
      {WINDOW_KINDS.map((kind) => (
        <QuotaGauge
          key={kind}
          provider={provider}
          quota={quota}
          kind={kind}
          nowSecs={nowSecs}
          age={age}
        />
      ))}
      {quota.error !== null && <QuotaError text={quota.error} />}
    </div>
  );
}

/// One window slot. A missing window keeps the slot's width and shows a
/// dimmed `—` where the percent would be.
function QuotaGauge({
  provider,
  quota,
  kind,
  nowSecs,
  age,
}: {
  provider: ProviderName;
  quota: ProviderQuota;
  kind: WindowKind;
  nowSecs: number;
  age: string | null;
}) {
  const reading = windowOf(quota, kind);
  const label = windowLabel(kind);
  const percent = reading === null ? null : displayPercent(reading.used_percent);
  const tone: Tone = reading === null ? "dimmed" : HEALTH_TONE[quotaHealth(reading.used_percent)];
  const resetsAt = reading === null ? null : reading.resets_at;
  const reset = resetText(resetsAt, nowSecs);
  const remaining =
    resetsAt === null ? 0 : Math.max(0, Math.min(1, (resetsAt - nowSecs) / WINDOW_SECS[kind]));
  const usage = percent === null ? "no reading" : `${percent}% used`;
  const ariaLabel = [
    `${provider} ${label} window ${usage}`,
    reset === null ? null : reset === "now" ? "resets now" : `resets in ${reset}`,
    age === null ? null : `reading ${age}`,
  ]
    .filter((part) => part !== null)
    .join(", ");

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          role="img"
          aria-label={ariaLabel}
          tabIndex={0}
          className={cn(
            "inline-flex items-center gap-1.5 rounded-sm whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            toneClass(tone),
          )}
        >
          <span className="w-[2ch] font-mono text-[11px] text-muted-foreground">{label}</span>
          <span
            aria-hidden="true"
            className="inline-flex w-16 flex-col gap-[2px] group-data-stale:opacity-60"
          >
            <span className="quota-bar relative block h-[5px] overflow-hidden rounded-[1px] bg-(--tone)/25">
              <span
                className="quota-fill absolute inset-y-0 left-0 bg-(--tone)"
                style={{ width: `${percent ?? 0}%` }}
              />
            </span>
            <span className="relative block h-px bg-border">
              <span
                className="quota-fill absolute inset-y-0 left-0 bg-muted-foreground"
                style={{ width: `${remaining * 100}%` }}
              />
            </span>
          </span>
          <span className="w-[4ch] text-right font-mono text-xs tabular-nums group-data-stale:text-muted-foreground">
            {percent === null ? "—" : `${percent}%`}
          </span>
          <span className="min-w-[6ch] font-mono text-[11px] tabular-nums text-muted-foreground max-[899px]:hidden">
            {reset}
          </span>
        </span>
      </TooltipTrigger>
      <TooltipContent side="top" align="start" className="flex-col items-start gap-0.5">
        <p className="font-medium">
          {provider} · {label} window · {usage}
        </p>
        {resetsAt !== null && <p className="opacity-80">resets {localTime(resetsAt)}</p>}
        <p className="opacity-80">
          observed {localTime(quota.observed_at)}
          {age !== null && ` · ${age}`}
        </p>
        {quota.plan !== null && <p className="opacity-80">plan {quota.plan}</p>}
      </TooltipContent>
    </Tooltip>
  );
}

/// The provider's last poll error, clipped in the bar and complete on hover.
function QuotaError({ text }: { text: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          tabIndex={0}
          className={cn(
            "inline-block max-w-40 truncate rounded-sm text-[11px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            toneClass("warning"),
          )}
        >
          <span aria-hidden="true">⚠ </span>
          {text}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top" align="start" className="max-w-sm">
        {text}
      </TooltipContent>
    </Tooltip>
  );
}
