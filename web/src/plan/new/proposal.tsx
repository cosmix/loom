import { useAtomValue, useStore } from "jotai";
import { Check, ListPlus, Pencil, Sparkles, Wrench } from "lucide-react";
import { Fragment, useEffect, useId, useRef, useState } from "react";

import type { KickoffShape, KickoffSize } from "@/plan/api/schema-kickoff";
import { sendChatCommand, sendMessage } from "@/plan/chat/commands";
import { Untrusted } from "@/plan/chat/visible-controls";
import { plural } from "@/plan/feature/steps";
import { FEATURE_NAME_HINT, isFeatureName } from "@/plan/new/feature-name";
import { authorBlockReasonAtom, canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import "./proposal.css";

type ProposedTarget = { size: KickoffSize; name: string };
type ProposalContentProps = ProposedTarget & { sections: string[] };
type ProposalActions = ReturnType<typeof useProposalActions>;
type RenameFormProps = {
  name: string;
  reason: string | null;
  onConfirm: (name: string) => void;
  onCancel: () => void;
};
type RenameFieldProps = {
  value: string;
  onChange: (value: string) => void;
  disabled: boolean;
  valid: boolean;
};

export function useProposalActions() {
  const store = useStore();
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const inFlight = useRef(false);
  const [pending, setPending] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const run = async (action: () => Promise<unknown>) => {
    if (!store.get(canAuthorAtom) || inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setStatus(null);
    try {
      await action();
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    } finally {
      inFlight.current = false;
      setPending(false);
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
  const [renaming, setRenaming] = useState(false);
  return (
    // A div, not a section: the help scene counts the panel's `section` rows.
    <div role="region" aria-label="Proposal" className="kickoff-proposal font-sans">
      <p className="kickoff-proposal__title">
        <Sparkles aria-hidden="true" />
        Proposed: a {size === "feature" ? "feature" : "standalone plan"}
      </p>
      <ProposalSummary name={name} sections={sections} />
      {renaming ? (
        <RenameForm
          key={`${size}:${name}`}
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
        <p className="text-xs text-muted-foreground">
          <Untrusted text={actions.blockReason} />
        </p>
      )}
      {actions.status !== null && (
        <p role="status" className="text-sm text-muted-foreground">
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
      <DisabledReason reason={actions.reason} onClick={() => actions.confirm(size, name)}>
        <Check aria-hidden="true" />
        Use this
      </DisabledReason>
      <div className="kickoff-ghost">
        <DisabledReason reason={actions.reason} onClick={onRename}>
          <Pencil aria-hidden="true" />
          Rename
        </DisabledReason>
      </div>
    </div>
  );
}

function ProposalAlternatives({ size, actions }: { size: KickoffSize; actions: ProposalActions }) {
  const feature = size === "feature";
  return (
    <div className="kickoff-proposal__actions kickoff-ghost">
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
        {feature ? "Make it standalone" : "Make it a feature"}
      </DisabledReason>
    </div>
  );
}

function RenameForm({ name, reason, onConfirm, onCancel }: RenameFormProps) {
  const [value, setValue] = useState(name);
  const valid = isFeatureName(value);
  const confirm = () => {
    if (reason === null && valid) onConfirm(value);
  };
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        confirm();
      }}
      className="space-y-2"
    >
      <RenameField value={value} onChange={setValue} disabled={reason !== null} valid={valid} />
      <div className="flex flex-wrap gap-2">
        <DisabledReason reason={reason ?? (valid ? null : "Choose a valid name")} onClick={confirm}>
          Confirm
        </DisabledReason>
        <DisabledReason reason={reason} onClick={onCancel}>
          Cancel
        </DisabledReason>
      </div>
    </form>
  );
}

function RenameField({ value, onChange, disabled, valid }: RenameFieldProps) {
  const hintId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    inputRef.current?.focus();
  }, []);
  return (
    <>
      <label className="flex flex-col gap-1">
        Name
        <input
          ref={inputRef}
          type="text"
          autoCapitalize="none"
          spellCheck={false}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          disabled={disabled}
          aria-invalid={!valid || undefined}
          aria-describedby={valid ? undefined : hintId}
          className="min-w-0 rounded-md border border-input bg-background px-3 py-2 font-mono text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
        />
      </label>
      {!valid && (
        <p id={hintId} role="alert" className="text-destructive">
          {FEATURE_NAME_HINT}
        </p>
      )}
    </>
  );
}
