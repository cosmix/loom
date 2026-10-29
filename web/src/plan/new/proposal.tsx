import { useAtomValue, useStore } from "jotai";
import { useEffect, useId, useRef, useState } from "react";

import type { KickoffShape, KickoffSize } from "@/plan/api/schema-kickoff";
import { sendChatCommand, sendMessage } from "@/plan/chat/commands";
import { Untrusted } from "@/plan/chat/visible-controls";
import { FEATURE_NAME_HINT, isFeatureName } from "@/plan/new/feature-name";
import { authorBlockReasonAtom, canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

type ProposedTarget = { size: KickoffSize; name: string };
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

function useProposalActions() {
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
  // DisabledReason takes plain text; isolate the server's detailed reason in ProposalFeedback.
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
  return <ProposalContent size={size} name={name} />;
}

function ProposalContent({ size, name }: ProposedTarget) {
  const actions = useProposalActions();
  const [renaming, setRenaming] = useState(false);
  return (
    <section
      aria-label="Proposal"
      className="min-w-0 space-y-3 rounded-lg border border-border bg-muted/30 p-4 font-sans text-sm text-foreground [overflow-wrap:anywhere]"
    >
      <p>
        I suggest a {size === "feature" ? "feature" : "standalone plan"} called{" "}
        <span className="font-mono">
          <Untrusted text={name} />
        </span>
        .
      </p>
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
    </section>
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
    <div className="flex flex-wrap gap-2">
      <DisabledReason reason={actions.reason} onClick={() => actions.confirm(size, name)} size="sm">
        Use this
      </DisabledReason>
      <DisabledReason reason={actions.reason} onClick={onRename} size="sm">
        Rename
      </DisabledReason>
    </div>
  );
}

function ProposalAlternatives({ size, actions }: { size: KickoffSize; actions: ProposalActions }) {
  const feature = size === "feature";
  return (
    <div className="flex flex-wrap gap-2">
      {feature && (
        <DisabledReason
          reason={actions.reason}
          onClick={() => actions.message("I want to change the proposed sections.")}
          size="sm"
        >
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
        size="sm"
      >
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
        <DisabledReason
          reason={reason ?? (valid ? null : "Choose a valid name")}
          onClick={confirm}
          size="sm"
        >
          Confirm
        </DisabledReason>
        <DisabledReason reason={reason} onClick={onCancel} size="sm">
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
