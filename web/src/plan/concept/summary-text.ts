import type { ConceptPage, OutOfDate } from "@/plan/api/schema-concept";

/**
 * The reason a summary is out of date, in words (marks, chips and the header share it). A summary
 * that cannot be rebuilt reads "Out of date" alone: the Rebuild button states why it cannot run.
 */
export function outOfDateText(outOfDate: OutOfDate): string {
  switch (outOfDate.reason) {
    case "source_changed":
      return "Out of date: the source changed";
    case "rebuilding":
      return "Out of date: rebuilding";
    case "awaiting_approval":
      return "Out of date: waiting for approval";
    case "cannot_rebuild":
      return "Out of date";
    case "failed":
      return "Out of date: build failed";
  }
}

export function planStateText(state: string): string {
  return state.replaceAll("_", " ");
}

export function summaryStatus(state: ConceptPage["state"]): string | null {
  if (!state) return null;
  if (state.out_of_date) return outOfDateText(state.out_of_date);
  switch (state.state) {
    case "current":
      return "Up to date";
    case "incomplete":
      return "Summary incomplete";
    case "rebuilding":
      return "Rebuilding";
    case "missing":
      return "No summary yet";
    case "stale":
      return "Out of date";
  }
}
