import type { JSX } from "react";

import { useNow } from "@/components/hooks/use-now";
import { formatElapsed } from "@/lib/format";

export function Elapsed({ startMs, untilMs }: { startMs: number; untilMs: number }): JSX.Element {
  return <span>{formatElapsed(Math.max(0, Math.floor((untilMs - startMs) / 1000)))}</span>;
}

/** Counts up each second from `startMs`; without a start it reads 0s. */
export function LiveElapsed({ startMs }: { startMs?: number }): JSX.Element {
  const now = useNow();
  return <Elapsed startMs={startMs ?? now} untilMs={now} />;
}
