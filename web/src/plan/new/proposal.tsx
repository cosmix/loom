import { atom, useAtomValue, useStore } from "jotai";
import { Check, ListPlus, Pencil, Sparkles, Wrench } from "lucide-react";
import { Fragment, useState } from "react";

import type { KickoffShape, KickoffSize } from "@/plan/api/schema-kickoff";
import { renameKickoff, sendChatCommand, sendMessage } from "@/plan/chat/commands";
import { Untrusted } from "@/plan/chat/visible-controls";
import { plural } from "@/plan/ui/plural";
import { RenameForm } from "@/plan/new/rename-field";
import { authorBlockReasonAtom, canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import "./proposal.css";

type ProposedTarget = { size: KickoffSize; name: string };
type ProposalContentProps = ProposedTarget & { sections: string[] };
type ProposalActions = ReturnType<typeof useProposalActions>;

/**
 * Whether a proposal command waits for its answer. It belongs to the store, not to the hook: the
 * size choice (ShapeSize) and the proposal both call the hook, and a click on either must hold the
 * other until the command answers, or two controls send the same message twice.
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
    /** Renames the work in the running kickoff (`rename_kickoff`); refused once it is confirmed. */
    rename: (name: string, onDone?: () => void) => {
      void run(async () => {
        await renameKickoff(store, name);
        onDone?.();
      });
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
  const [renaming, setRenaming] = useState(false);
  return (
    // A div, not a section: the help scene counts the panel's `section` rows.
    <div role="region" aria-label="Proposal" className="kickoff-proposal font-sans">
      <p className="plan-section-title kickoff-proposal__title">
        <Sparkles aria-hidden="true" />
        Proposed: a {size === "feature" ? "feature" : "standalone plan"}
      </p>
      <ProposalSummary name={name} sections={sections} />
      {renaming ? (
        <RenameForm
          key={`${size}:${name}`}
          className="kickoff-proposal__form"
          actionsClass="kickoff-proposal__actions"
          name={name}
          reason={actions.reason}
          onConfirm={(value) => actions.confirm(size, value)}
          onCancel={() => setRenaming(false)}
        />
      ) : (
        <ProposalAcceptance
          size={size}
          name={name}
          actions={actions}
          onRename={() => setRenaming(true)}
        />
      )}
      <ProposalAlternatives size={size} actions={actions} />
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

function ProposalAcceptance({
  size,
  name,
  actions,
  onRename,
}: ProposedTarget & { actions: ProposalActions; onRename: () => void }) {
  return (
    <div className="kickoff-proposal__actions">
      <DisabledReason
        reason={actions.reason}
        look="primary"
        onClick={() => actions.confirm(size, name)}
      >
        <Check aria-hidden="true" />
        Use this
      </DisabledReason>
      <DisabledReason reason={actions.reason} onClick={onRename}>
        <Pencil aria-hidden="true" />
        Rename
      </DisabledReason>
    </div>
  );
}

function ProposalAlternatives({ size, actions }: { size: KickoffSize; actions: ProposalActions }) {
  const feature = size === "feature";
  return (
    <div className="kickoff-proposal__actions">
      {feature && (
        <DisabledReason
          reason={actions.reason}
          onClick={() => actions.message("I want to change the proposed sections.")}
        >
          <ListPlus aria-hidden="true" />
          Change sections
        </DisabledReason>
      )}
      <DisabledReason
        reason={actions.reason}
        onClick={() =>
          actions.message(
            feature ? "Make this a standalone plan instead." : "Make this a feature instead.",
          )
        }
      >
        <Wrench aria-hidden="true" />
        {feature ? "Make standalone" : "Make a feature"}
      </DisabledReason>
    </div>
  );
}
