import { useEffect, useState } from "react";

import type { PlanStats } from "@/api/schema";
import { Button } from "@/components/ui/button";

import { archiveFileName } from "./plan-math";
import { summaryText } from "./summary-text";

type CopyState = "idle" | "copied" | "failed";

const LABEL: Record<CopyState, string> = {
  idle: "Copy summary",
  copied: "Copied",
  failed: "Copy failed",
};
const RESET_MS = 1600;

function CopySummary({ record }: { record: PlanStats }) {
  const [state, setState] = useState<CopyState>("idle");

  useEffect(() => {
    if (state === "idle") return;
    const timer = setTimeout(() => setState("idle"), RESET_MS);
    return () => clearTimeout(timer);
  }, [state]);

  // The write stays in the click handler: browsers grant clipboard access to a user gesture.
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(summaryText(record));
      setState("copied");
    } catch {
      setState("failed");
    }
  };

  return (
    <>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="po-copy"
        data-copy={state}
        onClick={() => void copy()}
      >
        {LABEL[state]}
      </Button>
      <span className="sr-only" role="status">
        {state === "idle" ? "" : LABEL[state]}
      </span>
    </>
  );
}

export function PlanFooter({ record }: { record: PlanStats }) {
  return (
    <footer className="po-foot">
      <span className="po-foot-text">
        Saved to <code>~/.loom/stats/plans/{archiveFileName(record)}</code> for cross-plan
        statistics. Planner chat tokens are not counted.
      </span>
      <CopySummary record={record} />
    </footer>
  );
}
