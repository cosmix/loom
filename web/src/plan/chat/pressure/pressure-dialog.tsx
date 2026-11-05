import { useAtomValue, useStore } from "jotai";
import { useEffect, useId, useRef, useState, type FormEvent, type ReactElement } from "react";

import { FlaskConical, Lock } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import type { PressureOptions, PressureSlots } from "@/plan/api/schema-pressure";
import { sessionAtom } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/session-home";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { ActionError } from "@/plan/ui/action-error";
import { BusyText } from "@/plan/ui/busy-text";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";

import { visibleControls } from "../visible-controls";
import { PressureConsequences, PressureHero, PressureRounds } from "./pressure-dialog-parts";
import { PressurePlanCard } from "./pressure-plan-card";
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
      <DialogContent className="max-h-[calc(100%-8px)] gap-0 overflow-y-auto rounded-[16px] border border-border bg-card p-0 ring-0 sm:max-w-[min(660px,100%)]">
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
            message={options?.ok === false ? visibleControls(options.message) : undefined}
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
    <form ref={form} onSubmit={submit} className="grid">
      <fieldset disabled={pending} className="m-0 grid min-w-0 gap-[18px] px-5 pt-[18px] pb-5">
        <PressurePlanCard plan={plan} plans={options.plans} setPlan={setPlan} />
        <section aria-label="What happens" className="grid gap-3">
          <PressureRounds
            rounds={slots.rounds}
            setRounds={(rounds) => setSlots({ ...slots, rounds })}
          />
          <ModelRows options={options} slots={slots} setSlots={setSlots} />
          <p className="flex items-center gap-2 text-[13px] text-muted-foreground">
            <span
              aria-hidden="true"
              className="size-1.5 shrink-0 rounded-full bg-(--tone-completed)"
            />
            {RECORD_LINE}
          </p>
        </section>
        <PressureConsequences
          rounds={slots.rounds}
          chatName={isActiveSession(session) && session.kind === "authoring" ? session.name : null}
        />
        {error !== null && <ActionError message={error} />}
      </fieldset>
      <PressureFooter
        reason={reason ?? (pending ? <BusyText>Starting pressure test…</BusyText> : null)}
        close={close}
        onStart={() => form.current?.requestSubmit()}
      />
    </form>
  );
}

const RECORD_LINE = "Then a record line goes at the end of the plan: date, models, rounds.";

function PressureFooter({
  reason,
  close,
  onStart,
}: {
  reason: string | ReactElement | null;
  close: () => void;
  onStart: () => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 border-t border-hairline bg-background px-5 py-3.5">
      <p className="mr-auto flex items-center gap-1.5 text-[13px] text-muted-foreground">
        <Lock aria-hidden="true" className="size-3.5" />
        Holds authoring until it finishes
      </p>
      {/* The ghost button: no fill, a 1px border, weight 500, at the primary button's measures. */}
      <Button
        type="button"
        variant="ghost"
        className="h-[30px] rounded-[8px] border-border px-3 text-[13px] font-medium"
        onClick={close}
      >
        Cancel
      </Button>
      <DisabledReason reason={reason} onClick={onStart}>
        <FlaskConical aria-hidden="true" />
        Start pressure test
      </DisabledReason>
    </div>
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
    <fieldset
      key={row.model}
      className="m-0 grid min-w-0 grid-cols-[minmax(0,1.5fr)_auto_minmax(0,1fr)] items-center gap-1.5 p-0"
    >
      <legend className="sr-only">{row.label}</legend>
      <OptionSelect
        label={`${row.label} model`}
        name={row.model}
        value={slots[row.model]}
        values={row.models}
        onChange={(value) => setSlots({ ...slots, [row.model]: value })}
      />
      <span aria-hidden="true" className="font-mono text-[13px] text-muted-foreground">
        ·
      </span>
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
    <div className="min-w-0">
      <label htmlFor={id} className="sr-only">
        {label}
      </label>
      <select
        id={id}
        name={name}
        value={value}
        required
        className="h-7 w-full rounded-[8px] border border-hairline bg-background px-2 font-mono text-[13px] focus-visible:outline-2 focus-visible:outline-ring"
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
