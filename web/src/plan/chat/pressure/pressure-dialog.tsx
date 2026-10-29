import { useAtomValue, useStore } from "jotai";
import { useEffect, useId, useRef, useState, type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter } from "@/components/ui/dialog";
import type { PressureOptions, PressureSlots } from "@/plan/api/schema-pressure";
import { sessionAtom } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/offer-flow";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { ActionError } from "@/plan/ui/action-error";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";

import { visibleControls } from "../visible-controls";
import {
  PressureConsequences,
  PressureHero,
  PressurePlanCard,
  PressureRounds,
} from "./pressure-dialog-parts";
import { RoundDiagram } from "./round-diagram";
import {
  refreshPressureOptions,
  startPressure,
  useStartReason,
  usePressureOptions,
} from "./use-pressure";

// Options are otherwise fetched only at install and resync, so a plan written since then
// would be missing; refetch on every open. The form reads its initial plan once, so it
// mounts only after this open's refetch settles (a failure leaves the error in the atom).
function useOpenRefresh(store: Store, open: boolean): boolean {
  const [refreshed, setRefreshed] = useState(false);
  useEffect(() => {
    if (!open) {
      setRefreshed(false);
      return;
    }
    let current = true;
    void refreshPressureOptions(store).then(() => {
      if (current) setRefreshed(true);
    });
    return () => {
      current = false;
    };
  }, [store, open]);
  return refreshed;
}

export function PressureDialog() {
  const store = useStore();
  const dialog = useAtomValue(pressureDialogAtom);
  const options = usePressureOptions();
  const refreshed = useOpenRefresh(store, dialog.open);

  if (!dialog.open) return null;
  const close = () => store.set(pressureDialogAtom, { ...dialog, open: false });

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-3xl">
        <PressureHero />
        {options?.ok && refreshed ? (
          <PressureForm
            key={dialog.plan}
            options={options.value}
            initialPlan={dialog.plan}
            close={close}
          />
        ) : (
          <LoadState
            state={options?.ok === false ? "error" : "loading"}
            title={
              options?.ok === false ? "Could not load pressure options" : "Loading pressure options"
            }
            message={options?.ok === false ? options.message : undefined}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

interface PressureFormProps {
  options: PressureOptions;
  initialPlan: string | null;
  close: () => void;
}

function PressureForm({ options, initialPlan, close }: PressureFormProps) {
  const form = useRef<HTMLFormElement>(null);
  const session = useAtomValue(sessionAtom);
  const { plan, setPlan, slots, setSlots } = usePressureFields(options, initialPlan);
  const reason = useStartReason(options);
  const { submit, pending, error } = usePressureStart(plan, slots, reason, close);

  return (
    <form ref={form} onSubmit={submit} className="space-y-5">
      <fieldset disabled={pending} className="min-w-0 space-y-5">
        <PressurePlanCard plan={plan} plans={options.plans} setPlan={setPlan} />
        <section aria-label="What happens" className="space-y-3">
          <h3 className="text-sm font-semibold">What happens</h3>
          <PressureRounds
            rounds={slots.rounds}
            setRounds={(rounds) => setSlots({ ...slots, rounds })}
          />
          <ModelRows options={options} slots={slots} setSlots={setSlots} />
          <p className="text-xs text-muted-foreground">The record line is written at the end.</p>
        </section>
      </fieldset>
      <PressureConsequences
        rounds={slots.rounds}
        endsChat={isActiveSession(session) && session.kind === "authoring"}
      />
      {error !== null && <ActionError message={error} />}
      <PressureFooter
        reason={reason ?? (pending ? "Starting pressure test…" : null)}
        close={close}
        onStart={() => form.current?.requestSubmit()}
      />
    </form>
  );
}

function PressureFooter({
  reason,
  close,
  onStart,
}: {
  reason: string | null;
  close: () => void;
  onStart: () => void;
}) {
  return (
    <DialogFooter className="flex-col sm:items-center">
      <p className="mr-auto text-xs text-muted-foreground">Holds authoring until it finishes.</p>
      <Button type="button" variant="outline" onClick={close}>
        Cancel
      </Button>
      <DisabledReason reason={reason} onClick={onStart}>
        Start pressure test
      </DisabledReason>
    </DialogFooter>
  );
}

function usePressureFields(options: PressureOptions, initialPlan: string | null) {
  const [plan, setPlan] = useState(
    initialPlan !== null && options.plans.includes(initialPlan)
      ? initialPlan
      : (options.plans[0] ?? ""),
  );
  const [slots, setSlots] = useState<PressureSlots>(options.defaults);
  return { plan, setPlan, slots, setSlots };
}

function usePressureStart(
  plan: string,
  slots: PressureSlots,
  reason: string | null,
  close: () => void,
) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (reason !== null || pending || !event.currentTarget.reportValidity()) return;
    setPending(true);
    setError(null);
    const result = await startPressure({ plan, ...slots });
    setPending(false);
    if (result.ok) close();
    else setError(result.message);
  }
  return { submit, pending, error };
}

function modelRows(options: PressureOptions) {
  return [
    {
      label: "/pressure",
      model: "claude_model",
      effort: "claude_effort",
      models: options.claude_models,
      efforts: options.claude_efforts,
    },
    {
      label: "Codex",
      model: "codex_model",
      effort: "codex_effort",
      models: options.codex_models,
      efforts: options.codex_efforts,
    },
    {
      label: "/address",
      model: "address_model",
      effort: "address_effort",
      models: options.claude_models,
      efforts: options.claude_efforts,
    },
  ] as const;
}

function ModelRows({
  options,
  slots,
  setSlots,
}: {
  options: PressureOptions;
  slots: PressureSlots;
  setSlots: (slots: PressureSlots) => void;
}) {
  const [review, secondReview, address] = modelRows(options).map((row) => (
    <fieldset key={row.model} className="grid min-w-0 grid-cols-2 gap-3">
      <legend className="mb-1 text-sm font-medium">{row.label}</legend>
      <OptionSelect
        label={`${row.label} model`}
        name={row.model}
        value={slots[row.model]}
        values={row.models}
        onChange={(value) => setSlots({ ...slots, [row.model]: value })}
      />
      <OptionSelect
        label={`${row.label} effort`}
        name={row.effort}
        value={slots[row.effort]}
        values={row.efforts}
        onChange={(value) => setSlots({ ...slots, [row.effort]: value })}
      />
    </fieldset>
  ));
  return (
    <RoundDiagram
      rounds={slots.rounds}
      review={review}
      secondReview={secondReview}
      address={address}
    />
  );
}

function OptionSelect({
  label,
  name,
  value,
  values,
  onChange,
}: {
  label: string;
  name?: string;
  value: string;
  values: readonly string[];
  onChange: (value: string) => void;
}) {
  const id = useId();
  return (
    <div className="min-w-0 space-y-1">
      <label htmlFor={id}>{label}</label>
      <select
        id={id}
        name={name}
        value={value}
        required
        className="h-8 w-full rounded-md border border-input bg-background px-2 text-sm focus-visible:outline-2 focus-visible:outline-ring"
        onChange={(event) => onChange(event.currentTarget.value)}
      >
        {values.map((option) => (
          <option key={option} value={option}>
            {visibleControls(option)}
          </option>
        ))}
      </select>
    </div>
  );
}
