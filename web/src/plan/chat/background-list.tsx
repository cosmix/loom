import { useAtomValue } from "jotai";
import { Check, Square, X } from "lucide-react";
import { Fragment, type JSX } from "react";

import { useNow } from "@/components/hooks/use-now";
import { compactTokens } from "@/lib/tokens";
import type { BackgroundTaskView } from "@/plan/api/schema-background";
import { StateMark } from "@/plan/ui/activity-glyphs";
import { plural } from "@/plan/ui/plural";
import { backgroundWord, foldBackground } from "./background-fold";
import type { BackgroundDisclosure } from "./background-line";
import { chatBackgroundAtom, isUnended } from "./background-tasks";
import { Elapsed } from "./pressure/elapsed";
import { Untrusted, visibleControls } from "./visible-controls";

/**
 * The mark before a row's state word (`ui/DESIGN.md`, Status): the arrows while the task runs,
 * resting and dimmed while it is quiet, then the outcome's glyph.
 */
function StateGlyph({ state }: { state: BackgroundTaskView["state"] }): JSX.Element {
  switch (state) {
    case "running":
    case "tool-wait":
      return <StateMark kind="running" />;
    case "stalled":
      return <StateMark kind="running" paused />;
    case "done":
      return <Check />;
    case "failed":
      return <X />;
    case "stopped":
      return <Square />;
  }
}

/** What a row calls its task: the label, else the agent's type, else the kind. */
function taskLabel(task: BackgroundTaskView): string {
  const named = task.label?.trim() || task.agent_type?.trim();
  return named || (task.kind === "agent" ? "Agent" : "Shell");
}

/**
 * How long the task has run, read off the list's one clock. An ended task shows how long it ran,
 * to its last sign of life: a clock still counting beside "failed" would read as work under way.
 */
function TaskClock({ task, now }: { task: BackgroundTaskView; now: number }): JSX.Element | null {
  const start = Date.parse(task.started_at);
  const until = isUnended(task.state) ? now : Date.parse(task.last_activity_at);
  if (!Number.isFinite(start) || !Number.isFinite(until)) return null;
  return (
    <span className="chat-background-clock">
      <Elapsed startMs={start} untilMs={until} />
    </span>
  );
}

/**
 * An agent's model, tokens and rate. A shell and a fresh agent carry none of them, and a figure
 * of zero says nothing, so each is drawn only when it is set and above zero.
 */
function TaskFigures({ task }: { task: BackgroundTaskView }): JSX.Element | null {
  const figures: Array<{ key: string; node: JSX.Element | string }> = [];
  if (task.model) figures.push({ key: "model", node: <Untrusted text={task.model} /> });
  if (task.total) figures.push({ key: "total", node: `${compactTokens(task.total)} tokens` });
  if (task.tokens_per_min) {
    figures.push({ key: "rate", node: `${compactTokens(task.tokens_per_min)}/m` });
  }
  if (figures.length === 0) return null;
  return (
    <span className="chat-background-figures">
      {figures.map(({ key, node }, index) => (
        // The separator stays outside the figure's span: the line may wrap there.
        <Fragment key={key}>
          {index > 0 && " · "}
          <span>{node}</span>
        </Fragment>
      ))}
    </span>
  );
}

function TaskRow({ task, now }: { task: BackgroundTaskView; now: number }): JSX.Element {
  const label = taskLabel(task);
  return (
    <li data-state={task.state} data-ended={isUnended(task.state) ? undefined : ""}>
      <span className="chat-background-glyph" aria-hidden="true">
        <StateGlyph state={task.state} />
      </span>{" "}
      <span className="chat-background-state">{backgroundWord(task.state)}</span>{" "}
      <span className="chat-background-label" title={visibleControls(label)}>
        <Untrusted text={label} />
      </span>{" "}
      <TaskClock task={task} now={now} /> <TaskFigures task={task} />
    </li>
  );
}

/** The open list. One clock for every row: the interval runs only while the list is open. */
function TaskList({ id }: { id: string }): JSX.Element {
  const tasks = useAtomValue(chatBackgroundAtom);
  const now = useNow();
  return (
    <ul id={id} className="chat-background-list" aria-label="Background tasks">
      {foldBackground(tasks).map((entry) =>
        entry.kind === "task" ? (
          <TaskRow key={`task:${entry.task.id}`} task={entry.task} now={now} />
        ) : (
          <li key="earlier" className="chat-background-earlier">
            <span>{plural(entry.count, "earlier agent")}</span>
          </li>
        ),
      )}
    </ul>
  );
}

/**
 * What the background line discloses, rendered after the head's sub-line (which clips): the status
 * region that announces the line, and, while the disclosure is pressed, one row per task: its mark
 * and state word, its label, how long it has run and, for an agent, its model and tokens. The rows
 * are a hairline-divided list on the panel itself, with no box of their own. It is not a popover:
 * it takes no focus, so the composer keeps the caret while the list is read.
 */
export function BackgroundList({ disclosure }: { disclosure: BackgroundDisclosure }): JSX.Element {
  const { announcement, open, listId } = disclosure;
  return (
    <>
      {/* role="status", never aria-live: a modal's hideOthers keeps every [aria-live] exposed. */}
      {announcement !== null && (
        <div role="status" className="sr-only">
          {announcement}
        </div>
      )}
      {open && <TaskList id={listId} />}
    </>
  );
}
