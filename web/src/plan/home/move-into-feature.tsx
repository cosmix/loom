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
import { panelCollapsedAtom } from "@/plan/chat/prefs";
import { chatSheetOpenAtom } from "@/plan/chat/sheet-state";
import { Untrusted } from "@/plan/chat/visible-controls";
import { repoRel, toAddress } from "@/plan/files/address";
import { FEATURE_NAME_HINT, isFeatureName } from "@/plan/new/feature-name";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  featuresAtom,
  standaloneAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import type { Store } from "@/state/store";

/**
 * Starts the move's chat and returns the route where its session shows: `/` mounts no chat panel,
 * so the dialog closing would otherwise leave nothing visibly changed. The chat belongs to the
 * destination feature when it exists, else to the whole project, which every panel shows, so the
 * moved plan's page is where it is reached. Any other source is reached at its own document.
 */
async function beginMove(store: Store, path: string, name: string, signal: AbortSignal) {
  const text =
    `Move ${repoRel(path)} into the feature "${name}": ` +
    `use git mv into the matching doc/product/${name}/ or doc/plans/${name}/ directory, ` +
    "keep file names, and fix every reference to it.";
  // The chat belongs to its destination. A new feature's name is refused as a start's owner until
  // the move has written it, so that chat starts for the whole project.
  const known = store.get(featuresAtom).some((item) => item.feature === name);
  await startChat(store, null, text, [], known ? name : undefined);
  if (signal.aborted) return null;
  store.set(panelCollapsedAtom, false);
  store.set(chatSheetOpenAtom, true);
  if (known) return `/f/${encodeURIComponent(name)}`;
  const plan = store.get(standaloneAtom).find((item) => item.path === path);
  return plan ? `/plans/${encodeURIComponent(plan.slug)}` : toAddress(path);
}

function useMoveChat(onClose: () => void) {
  const store = useStore();
  const navigate = useNavigate();
  const request = useRef<AbortController | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => request.current?.abort(), []);
  const start = async (path: string, name: string) => {
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
      const destination = await beginMove(store, path, name, controller.signal);
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

function PlanChoice({ value, onChange }: { value: string; onChange: (path: string) => void }) {
  const plans = useAtomValue(standaloneAtom);
  const group = useId();
  return (
    <fieldset className="space-y-2">
      <legend className="mb-2 font-medium">Standalone plan</legend>
      {plans.map((plan) => (
        <label key={plan.path} className="flex items-start gap-2">
          <input
            type="radio"
            name={group}
            value={plan.path}
            checked={value === plan.path}
            onChange={() => onChange(plan.path)}
            className="mt-1 accent-primary"
          />
          <span className="min-w-0 break-words">
            <Untrusted text={plan.title} />{" "}
            <span className="block text-xs text-muted-foreground">
              <Untrusted text={plan.path} />
            </span>
          </span>
        </label>
      ))}
      {plans.length === 0 && <p className="text-muted-foreground">No standalone plans to move.</p>}
    </fieldset>
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
    <div className="space-y-2">
      <label htmlFor={id} className="block">
        New feature name
      </label>
      <input
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled}
        aria-invalid={(!disabled && value !== "" && !isFeatureName(value)) || undefined}
        aria-describedby={`${id}-help`}
        className="w-full rounded-md border border-input bg-background px-3 py-2 focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50"
      />
      <p id={`${id}-help`} className="text-xs text-muted-foreground">
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

function FeatureChoice({ feature, name, onFeatureChange, onNameChange }: FeatureChoiceProps) {
  const features = useAtomValue(featuresAtom);
  const group = useId();
  return (
    <fieldset className="space-y-2">
      <legend className="mb-2 font-medium">Destination feature</legend>
      {features.map((item) => (
        <label key={item.feature} className="flex items-center gap-2">
          <input
            type="radio"
            name={group}
            value={item.feature}
            checked={feature === item.feature}
            onChange={() => onFeatureChange(item.feature)}
            className="accent-primary"
          />
          <Untrusted text={item.feature} />
        </label>
      ))}
      <label className="flex items-center gap-2">
        <input
          type="radio"
          name={group}
          value=""
          checked={feature === ""}
          onChange={() => onFeatureChange("")}
          className="accent-primary"
        />
        New feature
      </label>
      <FeatureName value={name} disabled={feature !== ""} onChange={onNameChange} />
    </fieldset>
  );
}

function useMoveReason(
  path: string | null,
  source: string,
  feature: string,
  name: string,
  pending: boolean,
) {
  const plans = useAtomValue(standaloneAtom);
  const features = useAtomValue(featuresAtom);
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  if (!canAuthor) return blockReason ?? "Authoring is unavailable";
  if (pending) return "Starting the move conversation";
  if (!source || (path === null && !plans.some((plan) => plan.path === source))) {
    return "Choose a standalone plan to move";
  }
  if (
    !isFeatureName(feature || name) ||
    (feature !== "" && !features.some((item) => item.feature === feature))
  ) {
    return "Enter a valid feature name or choose an existing feature";
  }
  return null;
}

function MoveForm({ path, onClose }: { path: string | null; onClose: () => void }) {
  const [selectedPath, setSelectedPath] = useState("");
  const [feature, setFeature] = useState("");
  const [name, setName] = useState("");
  const { start, pending, error } = useMoveChat(onClose);
  const source = path ?? selectedPath;
  const reason = useMoveReason(path, source, feature, name, pending);
  return (
    <div className="space-y-5">
      <fieldset disabled={pending} className="space-y-5">
        <legend className="sr-only">Document and destination</legend>
        {path === null ? (
          <PlanChoice value={selectedPath} onChange={setSelectedPath} />
        ) : (
          <p className="break-words">
            <Untrusted text={path} />
          </p>
        )}
        <FeatureChoice
          feature={feature}
          name={name}
          onFeatureChange={setFeature}
          onNameChange={setName}
        />
      </fieldset>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          <Untrusted text={error} />
        </p>
      )}
      <DisabledReason
        reason={reason}
        look="primary"
        onClick={() => {
          void start(source, feature || name);
        }}
      >
        Move into feature
      </DisabledReason>
    </div>
  );
}

export function MoveIntoFeatureDialog({
  path,
  open,
  onOpenChange,
}: {
  path: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[85vh] overflow-y-auto motion-reduce:animate-none"
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
            Start a conversation to move the document and fix its references.
          </DialogDescription>
        </DialogHeader>
        {open && (
          <MoveForm key={path ?? "choose-plan"} path={path} onClose={() => onOpenChange(false)} />
        )}
      </DialogContent>
    </Dialog>
  );
}
