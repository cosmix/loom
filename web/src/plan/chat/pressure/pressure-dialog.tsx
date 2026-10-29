import { useAtomValue, useStore } from "jotai";
import { useEffect, useId, useRef, useState, type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Pressure-test a plan</DialogTitle>
          <DialogDescription>
            This takes a while, edits the plan, and writes planning-omissions.md at the repository
            root.
          </DialogDescription>
        </DialogHeader>
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

function PressureForm({
  options,
  initialPlan,
  close,
}: {
  options: PressureOptions;
  initialPlan: string | null;
  close: () => void;
}) {
  const form = useRef<HTMLFormElement>(null);
  const session = useAtomValue(sessionAtom);
  const { plan, setPlan, slots, setSlots } = usePressureFields(options, initialPlan);
  const reason = useStartReason(options);
  const { submit, pending, error } = usePressureStart(plan, slots, reason, close);

  return (
    <form ref={form} onSubmit={submit} className="space-y-4">
      {isActiveSession(session) && session.kind === "authoring" && (
        <p>The open chat ends first, with its closing write.</p>
      )}
      <fieldset disabled={pending} className="space-y-4">
        <PlanFields
          options={options}
          plan={plan}
          setPlan={setPlan}
          slots={slots}
          setSlots={setSlots}
        />
        <ModelRows options={options} slots={slots} setSlots={setSlots} />
      </fieldset>
      {error !== null && <ActionError message={error} />}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={close}>
          Cancel
        </Button>
        <DisabledReason
          reason={reason ?? (pending ? "Starting pressure test…" : null)}
          onClick={() => form.current?.requestSubmit()}
        >
          Start
        </DisabledReason>
      </DialogFooter>
    </form>
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

function PlanFields({
  options,
  plan,
  setPlan,
  slots,
  setSlots,
}: {
  options: PressureOptions;
  plan: string;
  setPlan: (plan: string) => void;
  slots: PressureSlots;
  setSlots: (slots: PressureSlots) => void;
}) {
  const roundsId = useId();
  return (
    <div className="grid gap-3 sm:grid-cols-[1fr_5rem]">
      <OptionSelect label="Plan" value={plan} values={options.plans} onChange={setPlan} />
      <div className="space-y-1">
        <label htmlFor={roundsId}>Rounds</label>
        <input
          id={roundsId}
          className="h-8 w-full rounded-md border border-input bg-background px-2 text-sm focus-visible:outline-2 focus-visible:outline-ring"
          name="rounds"
          type="number"
          min={1}
          max={10}
          step={1}
          required
          value={Number.isNaN(slots.rounds) ? "" : slots.rounds}
          onChange={(event) => setSlots({ ...slots, rounds: event.currentTarget.valueAsNumber })}
        />
      </div>
    </div>
  );
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
  return (
    <div className="space-y-3">
      {modelRows(options).map((row) => (
        <fieldset key={row.model} className="grid grid-cols-2 gap-3">
          <legend className="mb-1 font-medium">{row.label}</legend>
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
      ))}
    </div>
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
