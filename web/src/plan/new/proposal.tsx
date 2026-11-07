import { atom, useAtomValue, useStore } from "jotai";
import { ListPlus, Sparkles } from "lucide-react";
import { Fragment, useState } from "react";

import type { KickoffShape, KickoffSize } from "@/plan/api/schema-kickoff";
import { sendChatCommand, sendMessage } from "@/plan/chat/commands";
import { Untrusted } from "@/plan/chat/visible-controls";
import { plural } from "@/plan/ui/plural";
import { authorBlockReasonAtom, canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import "./proposal.css";

type ProposalContentProps = { size: KickoffSize; name: string; sections: string[] };
type ProposalActions = ReturnType<typeof useProposalActions>;

/**
 * Whether a proposal command waits for its answer. It belongs to the store, not to the hook:
 * the proposal's Change sections and the Write button both call the hook, and a click on either
 * must hold the other until the command answers, or two controls send twice.
 */
const commandPendingAtom = atom(false);

export function useProposalActions() {
  const store = useStore();
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const pending = useAtomValue(commandPendingAtom);
  const [status, setStatus] = useState<string | null>(null);
  const run = async (action: () => Promise<unknown>) => {
    if (!store.get(canAuthorAtom) || store.get(commandPendingAtom)) return;
    store.set(commandPendingAtom, true);
    setStatus(null);
    try {
      await action();
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    } finally {
      store.set(commandPendingAtom, false);
    }
  };
  // These reasons are fixed strings; the server's own wording (`blockReason`) shows in
  // ProposalFeedback. DisabledReason isolates whatever reason it is given.
  const reason = !canAuthor
    ? "Proposal actions require authoring"
    : pending
      ? "Waiting for the proposal command"
      : null;
  return {
    reason,
    blockReason: canAuthor ? null : blockReason,
    status,
    confirm: (size: KickoffSize, name: string) => {
      void run(() => sendChatCommand(store, { command: "confirm_kickoff", size, name }));
    },
    message: (text: string) => {
      void run(() => sendMessage(store, text));
    },
  };
}

export function Proposal({ shape }: { shape: KickoffShape }) {
  const { proposal, size, name } = shape;
  if (!proposal || size === null || name === null) return null;
  const sections = size === "feature" ? shape.sections.map((section) => section.title) : [];
  return <ProposalContent size={size} name={name} sections={sections} />;
}

function ProposalSummary({ name, sections }: { name: string; sections: string[] }) {
  return (
    <p>
      Name{" "}
      <span className="font-mono">
        <Untrusted text={name} />
      </span>
      {sections.length > 0 && ` and ${plural(sections.length, "section")}: `}
      {sections.map((title, index) => (
        <Fragment key={`${index}:${title}`}>
          {index > 0 && ", "}
          <Untrusted text={title} />
        </Fragment>
      ))}
      .
    </p>
  );
}

function ProposalContent({ size, name, sections }: ProposalContentProps) {
  const actions = useProposalActions();
  return (
    // A div, not a section: the help scene counts the panel's `section` rows.
    <div role="region" aria-label="Proposal" className="kickoff-proposal font-sans">
      <p className="plan-section-title kickoff-proposal__title">
        <Sparkles aria-hidden="true" />
        Proposed: a {size === "feature" ? "feature" : "standalone plan"}
      </p>
      <ProposalSummary name={name} sections={sections} />
      {size === "feature" && <ChangeSections actions={actions} />}
      <ProposalFeedback actions={actions} />
    </div>
  );
}

function ProposalFeedback({ actions }: { actions: ProposalActions }) {
  return (
    <>
      {actions.blockReason !== null && (
        <p className="plan-fine">
          <Untrusted text={actions.blockReason} />
        </p>
      )}
      {actions.status !== null && (
        <p role="status" className="plan-fine">
          <Untrusted text={actions.status} />
        </p>
      )}
    </>
  );
}

function ChangeSections({ actions }: { actions: ProposalActions }) {
  return (
    <div className="kickoff-proposal__actions">
      <DisabledReason
        reason={actions.reason}
        onClick={() => actions.message("I want to change the proposed sections.")}
      >
        <ListPlus aria-hidden="true" />
        Change sections
      </DisabledReason>
    </div>
  );
}
