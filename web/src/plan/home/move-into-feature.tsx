import { useAtomValue, useStore } from "jotai";
import { useEffect, useId, useRef, useState } from "react";
import { useNavigate } from "react-router";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { startChat } from "@/plan/chat/commands";
import { noPanelRefusal, startError } from "@/plan/chat/offer-flow";
import { revealChat } from "@/plan/chat/sheet-state";
import { Untrusted } from "@/plan/chat/visible-controls";
import { IsolatedSentence } from "@/plan/concept/sentence-text";
import { repoRel, toAddress } from "@/plan/files/address";
import { FEATURE_NAME_HINT, isFeatureName } from "@/plan/new/feature-name";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  featuresAtom,
  standaloneAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { plural } from "@/plan/ui/plural";
import type { Store } from "@/state/store";

import "./move-into-feature.css";

/**
 * A misplaced document keeps the move text its offer has always sent (its feature directory is
 * under doc/product or doc/plans). Standalone plans, one or several, name doc/plans/<feature>/ as
 * the destination; several travel in ONE message so one conversation moves them together, listed
 * one per line so a file name holding ", " stays unambiguous.
 */
function moveText(paths: string[], name: string, standalone: boolean): string {
  if (paths.length === 1) {
    const where = standalone
      ? `doc/plans/${name}/`
      : `the matching doc/product/${name}/ or doc/plans/${name}/ directory`;
    return (
      `Move ${repoRel(paths[0]!)} into the feature "${name}": ` +
      `use git mv into ${where}, keep file names, and fix every reference to it.`
    );
  }
  return (
    `Move these plans into the feature "${name}" with git mv into doc/plans/${name}/, ` +
    "keeping file names and fixing every reference to them:\n" +
    paths.map((path) => `- ${repoRel(path)}`).join("\n")
  );
}

/**
 * Starts the move's chat and returns the route where its session shows: `/` mounts no chat panel,
 * so the dialog closing would otherwise leave nothing visibly changed. The chat belongs to the
 * destination feature when it exists, else to the whole project, which every panel shows, so the
 * first moved plan's page is where it is reached. Any other source is reached at its own document.
 */
async function beginMove(store: Store, paths: string[], name: string, signal: AbortSignal) {
  // The chat belongs to its destination. A new feature's name is refused as a start's owner until
  // the move has written it, so that chat starts for the whole project.
  const known = store.get(featuresAtom).some((item) => item.feature === name);
  const plans = store.get(standaloneAtom);
  const standalone = paths.every((path) => plans.some((item) => item.path === path));
  await startChat(store, null, moveText(paths, name, standalone), [], known ? name : undefined);
  if (signal.aborted) return null;
  revealChat(store);
  if (known) return `/f/${encodeURIComponent(name)}`;
  const first = paths[0]!;
  const plan = plans.find((item) => item.path === first);
  return plan ? `/plans/${encodeURIComponent(plan.slug)}` : toAddress(first);
}

function useMoveChat(onClose: () => void) {
  const store = useStore();
  const navigate = useNavigate();
  const request = useRef<AbortController | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => request.current?.abort(), []);
  const start = async (paths: string[], name: string) => {
    if (request.current || !store.get(canAuthorAtom)) return;
    const refusal = noPanelRefusal(store);
    if (refusal) {
      setError(refusal);
      return;
    }
    const controller = new AbortController();
    request.current = controller;
    setPending(true);
    setError(null);
    try {
      const destination = await beginMove(store, paths, name, controller.signal);
      if (!controller.signal.aborted) {
        if (destination) void navigate(destination);
        onClose();
      }
    } catch (cause) {
      if (!controller.signal.aborted) setError(startError(store, cause));
    } finally {
      request.current = null;
      if (!controller.signal.aborted) setPending(false);
    }
  };
  return { start, pending, error };
}

/** What moves: each listed plan by its title over its slug, any other document by its path. */
function MoveSubject({ paths }: { paths: string[] }) {
  const plans = useAtomValue(standaloneAtom);
  if (paths.length === 1 && !plans.some((plan) => plan.path === paths[0])) {
    return (
      <p className="move-dialog__path">
        <Untrusted text={paths[0]!} />
      </p>
    );
  }
  return (
    <div className="move-dialog__group">
      <p className="move-dialog__label">{`Moving ${plural(paths.length, "plan")}`}</p>
      <ul className="move-dialog__subjects">
        {paths.map((path) => {
          const plan = plans.find((item) => item.path === path);
          return (
            <li key={path}>
              {plan ? (
                <>
                  <span className="move-dialog__title">
                    <IsolatedSentence text={plan.title} />
                  </span>
                  <span className="move-dialog__slug">
                    <Untrusted text={plan.slug} />
                  </span>
                </>
              ) : (
                <span className="move-dialog__slug">
                  <Untrusted text={path} />
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function FeatureName({
  value,
  disabled,
  onChange,
}: {
  value: string;
  disabled: boolean;
  onChange: (name: string) => void;
}) {
  const id = useId();
  return (
    <div className="move-dialog__name">
      <label htmlFor={id} className="move-dialog__label">
        New feature name
      </label>
      <input
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled}
        aria-invalid={(!disabled && value !== "" && !isFeatureName(value)) || undefined}
        aria-describedby={`${id}-help`}
        className="move-dialog__input"
      />
      <p id={`${id}-help`} className="plan-fine">
        {FEATURE_NAME_HINT}
      </p>
    </div>
  );
}

interface FeatureChoiceProps {
  feature: string;
  name: string;
  onFeatureChange: (feature: string) => void;
  onNameChange: (name: string) => void;
}

/** Each destination is a radio named by the feature's slug; the last one names a new feature. */
function FeatureChoice({ feature, name, onFeatureChange, onNameChange }: FeatureChoiceProps) {
  const features = useAtomValue(featuresAtom);
  const group = useId();
  return (
    <fieldset className="move-dialog__group">
      <legend className="move-dialog__label">Destination feature</legend>
      <div className="move-dialog__choices">
        {features.map((item) => (
          <label key={item.feature} className="move-dialog__choice">
            <input
              type="radio"
              name={group}
              value={item.feature}
              checked={feature === item.feature}
              onChange={() => onFeatureChange(item.feature)}
            />
            <span className="move-dialog__slug">
              <Untrusted text={item.feature} />
            </span>
          </label>
        ))}
        <label className="move-dialog__choice">
          <input
            type="radio"
            name={group}
            value=""
            checked={feature === ""}
            onChange={() => onFeatureChange("")}
          />
          New feature
        </label>
      </div>
      <FeatureName value={name} disabled={feature !== ""} onChange={onNameChange} />
    </fieldset>
  );
}

function useMoveReason(paths: string[], feature: string, name: string, pending: boolean) {
  const features = useAtomValue(featuresAtom);
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  if (!canAuthor) return blockReason ?? "Authoring is unavailable";
  if (pending) return "Starting the move conversation";
  if (paths.length === 0) return "Choose a standalone plan to move";
  if (
    !isFeatureName(feature || name) ||
    (feature !== "" && !features.some((item) => item.feature === feature))
  ) {
    return "Enter a valid feature name or choose an existing feature";
  }
  return null;
}

function MoveForm({ paths, onClose }: { paths: string[]; onClose: () => void }) {
  const [feature, setFeature] = useState("");
  const [name, setName] = useState("");
  const { start, pending, error } = useMoveChat(onClose);
  const reason = useMoveReason(paths, feature, name, pending);
  return (
    <div className="move-dialog__form">
      <fieldset disabled={pending} className="move-dialog__fields">
        <legend className="sr-only">Documents and destination</legend>
        <MoveSubject paths={paths} />
        <FeatureChoice
          feature={feature}
          name={name}
          onFeatureChange={setFeature}
          onNameChange={setName}
        />
      </fieldset>
      {error && (
        <p role="alert" className="move-dialog__error">
          <Untrusted text={error} />
        </p>
      )}
      <DisabledReason
        reason={reason}
        look="primary"
        onClick={() => {
          void start(paths, feature || name);
        }}
      >
        Move
      </DisabledReason>
    </div>
  );
}

/** `paths` is what moves: one misplaced document, or the standalone plans the shelf selected. */
export function MoveIntoFeatureDialog({
  paths,
  open,
  onOpenChange,
}: {
  paths: string[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[85vh] overflow-y-auto motion-reduce:animate-none sm:max-w-md"
        onOpenAutoFocus={() => {
          returnFocus.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (returnFocus.current?.isConnected) returnFocus.current.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Move into a feature</DialogTitle>
          <DialogDescription>
            Start a conversation that moves the documents and fixes their references.
          </DialogDescription>
        </DialogHeader>
        {open && (
          <MoveForm key={paths.join("\n")} paths={paths} onClose={() => onOpenChange(false)} />
        )}
      </DialogContent>
    </Dialog>
  );
}
