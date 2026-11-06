import { useAtomValue } from "jotai";
import { ArrowUp, Paperclip } from "lucide-react";
import { useId, useRef, useState, type RefObject } from "react";

import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { OpenDraftNotice } from "@/plan/new/draft-view";
import { FeaturePicker, PickedFeature, PickFeature } from "@/plan/new/feature-picker";
import { followUpFeatures, type FollowUpFeature } from "@/plan/new/follow-up-features";
import { KickoffAttachments } from "@/plan/new/kickoff-attachments";
import { useKickoffStart, type PickedDocument, type WorkKind } from "@/plan/new/kickoff-start";
import {
  FollowUpSuggestions,
  KickoffStarters,
  toggleSeed,
  type Starter,
} from "@/plan/new/kickoff-starters";
import { KindControl } from "@/plan/new/kind-control";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  featuresAtom,
  kickoffDraftsAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoomDoodle } from "@/plan/ui/doodle";
import { DocumentStarter, type StarterDocument } from "./document-starter";

import "./kickoff.css";
import "./threshold.css";

type KickoffDraft = ReturnType<typeof useKickoffDraft>;

/** The foot's line while Start is live; the blocked reason takes its place otherwise. */
const KEYS = "Enter to send · Shift+Enter for a new line";

/** The element that opened the picker, when it can take focus back. */
function focusable(element: Element | null): HTMLElement | null {
  return element instanceof HTMLElement && element !== document.body ? element : null;
}

/** Whether the feature picker floats under the switch, and what takes focus back when it closes. */
function usePicker(kindRoot: RefObject<HTMLFieldSetElement | null>) {
  const [picking, setPicking] = useState(false);
  const opener = useRef<HTMLElement | null>(null);
  const open = () => {
    opener.current = focusable(document.activeElement);
    setPicking(true);
  };
  const close = () => {
    setPicking(false);
    // The opener may have left with the starters; the chosen radio is then the way back.
    const back = opener.current?.isConnected
      ? opener.current
      : kindRoot.current?.querySelector<HTMLElement>("input:checked");
    back?.focus();
  };
  return { picking, open, close, dismiss: () => setPicking(false) };
}

/** The form's state: the kind and its feature, the words, the documents, and the open picker. */
function useKickoffDraft(initialKind: WorkKind) {
  const [kind, setKindState] = useState(initialKind);
  const [feature, setFeature] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [documents, setDocuments] = useState<PickedDocument[]>([]);
  const composer = useRef<HTMLTextAreaElement>(null);
  const kindRoot = useRef<HTMLFieldSetElement>(null);
  const picker = usePicker(kindRoot);
  const setKind = (next: WorkKind) => {
    setKindState(next);
    if (next === "follow-up" && feature === null) picker.open();
    else picker.dismiss();
  };
  const pick = (name: string) => {
    setFeature(name);
    picker.dismiss();
    composer.current?.focus();
  };
  const choose = (starter: Starter) => {
    setKind(starter.kind);
    if (starter.text === null) return;
    setText(starter.text);
    composer.current?.focus();
  };
  const attach = (document: StarterDocument) => {
    setDocuments((previous) => [...previous, { ...document, id: crypto.randomUUID() }]);
    composer.current?.focus();
  };
  const remove = (id: string) => setDocuments((previous) => previous.filter((d) => d.id !== id));
  const toggle = (seed: string) => setText((previous) => toggleSeed(previous, seed));
  return {
    kind,
    setKind,
    feature,
    text,
    setText,
    documents,
    picker,
    pick,
    choose,
    attach,
    remove,
    toggle,
    composer,
    kindRoot,
  };
}

function placeholderFor(picked: FollowUpFeature | null): string {
  if (picked === null) return "Say what's on your mind…";
  const ask = `What should change in ${visibleControls(picked.name)}?`;
  return picked.candidates.length
    ? `${ask} Pick a finding below, or say it in your own words.`
    : ask;
}

function KickoffComposer({
  draft,
  picked,
  disabled,
  start,
}: {
  draft: KickoffDraft;
  picked: FollowUpFeature | null;
  disabled: boolean;
  start: () => void;
}) {
  const id = useId();
  return (
    <>
      <label htmlFor={id} className="sr-only">
        What do you want to build?
      </label>
      <textarea
        id={id}
        ref={draft.composer}
        className="kickoff-composer"
        rows={3}
        value={draft.text}
        placeholder={placeholderFor(picked)}
        disabled={disabled}
        onChange={(event) => draft.setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
          event.preventDefault();
          start();
        }}
      />
    </>
  );
}

/** The field's foot: attach, the blocked reason or the keys, and the round primary Start. */
function KickoffFoot({
  draft,
  disabled,
  reason,
  error,
  submit,
}: {
  draft: KickoffDraft;
  disabled: boolean;
  reason: string | null;
  error: string | null;
  submit: () => void;
}) {
  const reasonId = useId();
  return (
    <div className="kickoff-field__foot">
      <DocumentStarter
        onPick={draft.attach}
        variant="ghost"
        look="quiet"
        shape="pill"
        className="kickoff-attach"
        disabled={disabled}
      >
        <Paperclip aria-hidden="true" />
        <span className="kickoff-attach__word">
          {draft.documents.length > 0 ? "Attach another" : "Attach a document"}
        </span>
      </DocumentStarter>
      <span id={reasonId} className="plan-fine kickoff-field__reason">
        {reason !== null ? <Untrusted text={reason} /> : KEYS}
      </span>
      <DisabledReason
        reason={reason}
        reasonId={reasonId}
        look="primary"
        shape="round"
        onClick={submit}
      >
        <ArrowUp aria-hidden="true" />
        <span className="sr-only">{error ? "Try again" : "Start the conversation"}</span>
      </DisabledReason>
    </div>
  );
}

type Threshold = ReturnType<typeof useThreshold>;

/** The threshold: one round field holding the kind switch, the words, the documents and the foot. */
function KickoffField({ draft, features, picked, disabled, reason, error, submit }: Threshold) {
  return (
    <div className="kickoff-field">
      <KindControl
        ref={draft.kindRoot}
        kind={draft.kind}
        onChange={draft.setKind}
        disabled={disabled}
        beside={
          picked !== null ? (
            <PickedFeature feature={picked} onChange={draft.picker.open} />
          ) : draft.kind === "follow-up" ? (
            <PickFeature onPick={draft.picker.open} />
          ) : undefined
        }
      >
        {draft.picker.picking && (
          <FeaturePicker
            features={features}
            onPick={draft.pick}
            onClose={draft.picker.close}
            onLeave={draft.picker.dismiss}
          />
        )}
      </KindControl>
      <KickoffComposer draft={draft} picked={picked} disabled={disabled} start={submit} />
      <KickoffAttachments documents={draft.documents} disabled={disabled} onRemove={draft.remove} />
      <KickoffFoot
        draft={draft}
        disabled={disabled}
        reason={reason}
        error={error}
        submit={submit}
      />
    </div>
  );
}

function KickoffTitle() {
  return (
    <header className="kickoff-title kickoff-threshold__head">
      <p className="plan-eyebrow kickoff-eyebrow">New work</p>
      <h1 className="kickoff-heading">What do you want to build?</h1>
      <p className="kickoff-lead">
        Describe it the way you would to a colleague: the problem, who has it, what you have in
        mind. Loom asks questions, pushes back, and suggests a name and the sections a spec needs.
      </p>
    </header>
  );
}

/** Under the field: the starters, or the picked feature's findings; nothing while none is picked. */
function UnderField({ draft, picked, disabled }: Threshold) {
  if (draft.kind !== "follow-up") {
    return <KickoffStarters disabled={disabled} onChoose={draft.choose} onAttach={draft.attach} />;
  }
  if (picked === null) return null;
  return (
    <FollowUpSuggestions
      feature={picked}
      text={draft.text}
      disabled={disabled}
      onToggle={draft.toggle}
    />
  );
}

function blockedReason(
  draft: Pick<KickoffDraft, "kind" | "text" | "documents">,
  picked: FollowUpFeature | null,
  authoring: { canAuthor: boolean; blockReason: string | null },
  pending: boolean,
): string | null {
  if (!authoring.canAuthor) return authoring.blockReason ?? "Authoring is unavailable";
  if (pending) return "Starting the conversation";
  if (draft.kind === "follow-up" && picked === null) return "Pick a feature first";
  if (!draft.text.trim() && !draft.documents.length) {
    return "Describe what you want to build or attach a document";
  }
  return null;
}

/** The form, its start and what blocks it. */
function useThreshold(small: boolean, onFollowUp: ((feature: string) => void) | undefined) {
  const features = followUpFeatures(useAtomValue(featuresAtom));
  const draft = useKickoffDraft(small ? "plan" : "feature");
  const { start, pending, error } = useKickoffStart(onFollowUp);
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const picked =
    draft.kind === "follow-up"
      ? (features.find((entry) => entry.name === draft.feature) ?? null)
      : null;
  const disabled = !canAuthor || pending;
  const reason = blockedReason(draft, picked, { canAuthor, blockReason }, pending);
  const submit = () => {
    void start(draft.kind, draft.text, draft.documents, picked?.name ?? null);
  };
  return { draft, features, picked, disabled, reason, error, submit };
}

/**
 * `/new` with no session: the kind of work, the words, the documents and Start. `small` opens on
 * Standalone plan. A follow-up start is a feature chat: `onFollowUp` gets the feature once it has
 * started.
 */
export function EmptyKickoff({
  small = false,
  onFollowUp,
}: {
  small?: boolean;
  onFollowUp?: (feature: string) => void;
}) {
  const form = useThreshold(small, onFollowUp);
  // The form is always empty; a kickoff still running is offered above it, never reopened here.
  const open = useAtomValue(kickoffDraftsAtom).find((entry) => entry.live);
  return (
    <div className="kickoff-page" data-state="empty">
      <section className="kickoff-main kickoff-threshold" aria-label="Start something new">
        <LoomDoodle className="kickoff-doodle" draw />
        <KickoffTitle />
        {open && <OpenDraftNotice draft={open} />}
        <KickoffField {...form} />
        {form.error && (
          <p role="status" className="kickoff-error">
            <Untrusted text={form.error} />
          </p>
        )}
        <UnderField {...form} />
        <p className="kickoff-note">Nothing runs until you send.</p>
      </section>
    </div>
  );
}
