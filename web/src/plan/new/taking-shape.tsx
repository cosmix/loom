import {
  Circle,
  CircleCheck,
  CircleDashed,
  ListOrdered,
  MessageCircleQuestion,
  Telescope,
} from "lucide-react";
import { useId, useState, type ReactNode } from "react";

import type { KickoffShape } from "@/plan/api/schema-kickoff";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useProposalActions } from "@/plan/new/proposal";
import { RenameForm } from "@/plan/new/rename-field";
import { ShapeSize } from "@/plan/new/shape-size";
import { WriteKickoff } from "@/plan/new/write-kickoff";
import { StateMark } from "@/plan/ui/activity-glyphs";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import "./taking-shape.css";

type Tone = "ok" | "warn" | "muted";

const SECTION_STATES = {
  settled: { label: "Settled", icon: <CircleCheck aria-hidden="true" /> },
  wording: { label: "Being worded", icon: <StateMark kind="drafting" /> },
  open: { label: "Open", icon: <Circle aria-hidden="true" /> },
} as const;

export function presence(present: boolean): { icon: ReactNode; tone: Tone } {
  return present
    ? { icon: <CircleCheck aria-hidden="true" />, tone: "ok" }
    : { icon: <CircleDashed aria-hidden="true" />, tone: "muted" };
}

export function TakingShape({
  shape,
  target,
  children,
}: {
  shape: KickoffShape | null;
  target: string | null;
  /** The proposal, shown between the heading and the shape rows. */
  children?: ReactNode;
}) {
  const size = shape?.size ?? null;
  const questions = shape?.questions ?? [];

  // Design section 8: this panel stays in the UI face; the transcript uses the reading face.
  // The help scene targets the six rows as `section:nth-of-type(n)`, so nothing else in this
  // aside may be a `section`.
  return (
    <aside aria-label="Taking shape" className="kickoff-shape font-sans">
      <h2 className="kickoff-shape__title">
        <Telescope aria-hidden="true" />
        Taking shape
      </h2>
      {shape === null && (
        <p className="kickoff-muted">
          Nothing yet. The shape appears here as the conversation settles it.
        </p>
      )}
      {children}
      <ShapeRow title="Size" {...presence(size !== null)}>
        <ShapeSize size={size} />
      </ShapeRow>
      <ShapeIdentity shape={shape} target={target} />
      <QuestionsRow questions={questions} />
      {shape?.size === "feature" && <ShapeSections sections={shape.sections} />}
      {shape !== null && <ShapeFoot size={size} target={target} />}
    </aside>
  );
}

/** The Problem, Settled decisions and Open questions rows, shared with the ended draft's page. */
export function ProblemRow({ problem }: { problem: string | null }) {
  return (
    <ShapeRow title="Problem" {...presence(Boolean(problem))}>
      <p className="whitespace-pre-wrap">
        {problem ? (
          <Untrusted text={problem} />
        ) : (
          <span className="kickoff-muted">Not stated yet</span>
        )}
      </p>
    </ShapeRow>
  );
}

export function DecisionsRow({ decisions }: { decisions: string[] }) {
  return (
    <ShapeRow title="Settled decisions" {...presence(decisions.length > 0)}>
      <ShapeList items={decisions} empty="Nothing settled yet" />
    </ShapeRow>
  );
}

export function QuestionsRow({ questions }: { questions: string[] }) {
  return (
    <ShapeRow
      title="Open questions"
      icon={
        questions.length > 0 ? (
          <MessageCircleQuestion aria-hidden="true" />
        ) : (
          <CircleCheck aria-hidden="true" />
        )
      }
      tone={questions.length > 0 ? "warn" : "ok"}
    >
      <ShapeList items={questions} empty="No open questions" />
    </ShapeRow>
  );
}

/** The Name, Problem and Decisions rows, which read straight from the shape. */
function ShapeIdentity({ shape, target }: { shape: KickoffShape | null; target: string | null }) {
  return (
    <>
      <ShapeRow title="Name" {...presence(Boolean(shape?.name))}>
        <NameRow name={shape?.name ?? null} editable={shape !== null && target === null} />
      </ShapeRow>
      <ProblemRow problem={shape?.problem ?? null} />
      <DecisionsRow decisions={shape?.decisions ?? []} />
    </>
  );
}

/**
 * The agent's name for the work, editable until the work is confirmed (`target` set): the server
 * refuses a rename after that, because the write grant already names the old one.
 */
function NameRow({ name, editable }: { name: string | null; editable: boolean }) {
  const actions = useProposalActions();
  const [renaming, setRenaming] = useState(false);
  const close = () => setRenaming(false);
  const editing = renaming && editable;
  return (
    <>
      {editing ? (
        <RenameForm
          className="kickoff-rename"
          actionsClass="kickoff-rename__actions"
          name={name ?? ""}
          reason={actions.reason}
          onConfirm={(value) => actions.rename(value, close)}
          onCancel={close}
        />
      ) : (
        // The value, then its one action at the value's end.
        <div className="kickoff-name">
          {name ? (
            <span className="kickoff-chip font-mono">
              <Untrusted text={name} />
            </span>
          ) : (
            <span className="kickoff-muted">Not named yet</span>
          )}
          {editable && (
            <DisabledReason
              reason={actions.reason}
              look="quiet"
              size="sm"
              reasonPlacement="inline"
              onClick={() => setRenaming(true)}
            >
              {name ? "Rename" : "Name it"}
            </DisabledReason>
          )}
        </div>
      )}
      {actions.status !== null && (
        <p role="status" className="plan-fine">
          <Untrusted text={actions.status} />
        </p>
      )}
    </>
  );
}

export function ShapeRow({
  title,
  icon,
  tone,
  children,
}: {
  title: string;
  icon: ReactNode;
  tone: Tone;
  children: ReactNode;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="kickoff-row">
      <span className="kickoff-row__icon" data-tone={tone} aria-hidden="true">
        {icon}
      </span>
      <h3 id={headingId} className="kickoff-row__label">
        {title}
      </h3>
      {children}
    </section>
  );
}

export function ShapeList({ items, empty }: { items: string[]; empty: string }) {
  if (items.length === 0) return <p className="kickoff-muted">{empty}</p>;
  return (
    <ul className="kickoff-row__list">
      {items.map((text, index) => (
        <li key={`${index}:${text}`}>
          <Untrusted text={text} />
        </li>
      ))}
    </ul>
  );
}

function ShapeSections({ sections }: { sections: KickoffShape["sections"] }) {
  return (
    <ShapeRow title="Sections" icon={<ListOrdered aria-hidden="true" />} tone="muted">
      {sections.length === 0 ? (
        <p className="kickoff-muted">No sections proposed yet</p>
      ) : (
        <ul className="kickoff-sections">
          {sections.map(({ title, state }, index) => {
            const { label, icon } = SECTION_STATES[state];
            return (
              <li key={`${index}:${title}`}>
                <span className="kickoff-sections__no" aria-hidden="true">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <span className="min-w-0">
                  <Untrusted text={title} />
                </span>
                <span className="kickoff-state" data-state={state}>
                  {icon}
                  {label}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </ShapeRow>
  );
}

function ShapeFoot({ size, target }: { size: KickoffShape["size"]; target: string | null }) {
  const plan = size === "plan";
  const created =
    target === null ? (
      plan ? (
        "the plan"
      ) : (
        "the vision"
      )
    ) : (
      <code>
        <Untrusted text={target} />
      </code>
    );
  return (
    <div className="kickoff-shape__foot">
      <p className="plan-fine kickoff-notice">
        {`Nothing is written yet. '${plan ? "Write the plan" : "Write the vision"}' creates `}
        {created}
        {plan ? "." : " with the open questions in it."}
      </p>
      <WriteKickoff size={size} target={target} />
    </div>
  );
}
