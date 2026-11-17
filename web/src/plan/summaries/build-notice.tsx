import { useAtomValue } from "jotai/react";
import type { ReactNode } from "react";

import type { Progress } from "@/plan/api/schema-concept";
import { Untrusted } from "@/plan/chat/visible-controls";

import { PendingApprovalNotice } from "./approval-notice";
import { buildStatusOf, type BuildStatus } from "./build-status";
import { RebuildButton } from "./rebuild-button";
import { summaryPanelAtom } from "./state";

export type NoticeCopy = { title: string; description?: ReactNode };

export type View = NoticeCopy & { action: ReactNode };

export type Words = { noun: string; Noun: string; they: string; their: string };

export function wordsFor(plural: boolean): Words {
  return plural
    ? {
        noun: "summaries",
        Noun: "Summaries",
        they: "They build",
        their: "their",
      }
    : { noun: "summary", Noun: "Summary", they: "It builds", their: "its" };
}

function progressText(done: number, total: number): string {
  return `${done} of ${total} built.`;
}

function queuedText(status: Extract<BuildStatus, { kind: "queued" }>): ReactNode {
  const where = status.behind ? "Queued behind the rebuild that is running." : "Queued to start.";
  if (status.startError === null) return where;
  return (
    <>
      {`${where} The last start failed: `}
      <Untrusted text={status.startError} />
    </>
  );
}

export function viewOf(status: BuildStatus, ids: string[], words: Words, idle: NoticeCopy): View {
  const button = (label: string) => <RebuildButton ids={ids} label={label} />;
  switch (status.kind) {
    case "running":
      return {
        title: `Building ${words.noun}…`,
        description: status.total > 0 ? progressText(status.done, status.total) : undefined,
        action: null,
      };
    case "queued":
      return {
        title: `Building ${words.noun}…`,
        description: queuedText(status),
        action: null,
      };
    case "waiting":
      return {
        title: `${words.Noun} queued`,
        description: `${words.they} once ${words.their} sources go 5 minutes without an edit.`,
        action: button("Build now"),
      };
    case "asking":
      return {
        title: `${words.Noun} waiting for approval`,
        action: <PendingApprovalNotice />,
      };
    case "failed":
      return {
        title: `${words.Noun} build failed`,
        description: <Untrusted text={status.error} />,
        action: button("Retry"),
      };
    case "idle":
      return { ...idle, action: button(`Build ${words.noun}`) };
  }
}

/** What the page itself reports of its summary's build. */
export type Reported = {
  /** The page shows its summary claimed by a session (a chat or a terminal may hold the claim). */
  claimed: boolean;
  /** The page reports a build in progress. */
  building: boolean;
  /** The running rebuild's progress. */
  progress: Progress | null;
};

/**
 * The live panel's status settled with what the page reports: before the panel loads the page's
 * `building` decides, and a claimed summary reads as running whatever the panel lists.
 */
export function effectiveStatus(status: BuildStatus | null, reported: Reported): BuildStatus {
  const { claimed, building, progress } = reported;
  const done = progress?.done ?? 0;
  const running: BuildStatus = {
    kind: "running",
    done,
    total: progress?.total ?? 0,
  };
  if (status === null) return claimed || building ? running : { kind: "idle" };
  if (claimed && status.kind !== "running") return running;
  return status;
}

/**
 * Where the build of a page without a summary stands, with its one action: "Building summary…"
 * with progress while a build is queued or running; queued with "Build now" while it waits for
 * edits to stop; the approval question's answer while a large rebuild asks first; the error with
 * "Retry" after a failure; and `idle`'s text with "Build summary" when nothing will build it.
 */
export function SummaryBuildNotice({
  ids,
  press = ids,
  plural,
  reported,
  idle,
}: {
  /** The summaries the page waits on. */
  ids: string[];
  /** The summaries the action presses: by default `ids`. */
  press?: string[];
  /** Several summaries (an overview and its members) or one. */
  plural: boolean;
  reported: Reported;
  /** The text while nothing builds: what is missing. */
  idle: NoticeCopy;
}) {
  const panel = useAtomValue(summaryPanelAtom);
  const status = effectiveStatus(buildStatusOf(panel, ids), reported);
  const view = viewOf(status, press, wordsFor(plural), idle);
  return (
    <>
      <p className="plan-empty" role="status">
        <strong className="concept-empty-title">{view.title}</strong>
        {view.description}
      </p>
      {view.action}
    </>
  );
}
