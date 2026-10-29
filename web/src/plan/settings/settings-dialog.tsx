import { useAtom, useAtomValue } from "jotai";
import { useId, useRef, type ReactElement, type ReactNode } from "react";

import { ThemePicker } from "@/components/theme-picker";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { notificationStatus, useNotificationToggle } from "@/state/notifications";
import { PlannerThemeSwatch } from "./planner-swatch";
import {
  autoRebuildAtom,
  DEFAULT_READING_FACE,
  READING_FACES,
  READING_SIZES,
  readingFaceAtom,
  readingSizeAtom,
  sourceMarksAtom,
} from "./preferences";

function SettingsSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}): ReactElement {
  const heading = useId();
  return (
    <section aria-labelledby={heading} className="min-w-0 space-y-3">
      <h3 id={heading} className="font-sans text-sm font-semibold text-foreground">
        {title}
      </h3>
      {children}
    </section>
  );
}

function RadioCard({
  name,
  label,
  checked,
  onChange,
  children,
}: {
  name: string;
  label: string;
  checked: boolean;
  onChange: () => void;
  children: ReactNode;
}): ReactElement {
  return (
    <label
      className={`relative flex min-w-0 cursor-pointer flex-col gap-2 rounded-lg border bg-card p-3 text-card-foreground focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-ring ${
        checked ? "border-primary ring-1 ring-primary" : "border-border"
      }`}
    >
      <input
        type="radio"
        name={name}
        aria-label={label}
        checked={checked}
        onChange={onChange}
        className="absolute top-3 right-3 size-4 accent-primary"
      />
      {children}
    </label>
  );
}

function ReadingFaceSection(): ReactElement {
  const [selected, setSelected] = useAtom(readingFaceAtom);
  const name = useId();
  return (
    <SettingsSection title="Reading face">
      <div
        role="radiogroup"
        aria-label="Reading face"
        className="grid grid-cols-1 gap-3 sm:grid-cols-3"
      >
        {READING_FACES.map((face) => (
          <RadioCard
            key={face.id}
            name={name}
            label={face.label}
            checked={selected === face.id}
            onChange={() => setSelected(face.id)}
          >
            <span className="text-3xl leading-relaxed" style={{ fontFamily: face.family }}>
              Aa <span lang="el">Ωω</span>
            </span>
            <span className="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-sm font-medium">
              {face.label}
              {face.id === DEFAULT_READING_FACE && (
                <span className="text-xs text-muted-foreground">Default</span>
              )}
            </span>
            <span className="text-xs leading-5 text-muted-foreground">{face.scripts}</span>
          </RadioCard>
        ))}
      </div>
    </SettingsSection>
  );
}

function ReadingPreview(): ReactElement {
  const face = useAtomValue(readingFaceAtom);
  const size = useAtomValue(readingSizeAtom);
  const family = READING_FACES.find((option) => option.id === face)!.family;
  return (
    <p
      className="rounded-lg border border-border bg-muted/30 p-4 text-foreground"
      style={{ fontFamily: family, fontSize: size, lineHeight: 1.6 }}
    >
      <span lang="en">A quiet morning leaves room for a new idea.</span>{" "}
      <span lang="el">Ένα ήσυχο πρωινό αφήνει χώρο για μια νέα ιδέα.</span>
    </p>
  );
}

function ReadingSizeSection(): ReactElement {
  const [selected, setSelected] = useAtom(readingSizeAtom);
  const name = useId();
  return (
    <SettingsSection title="Reading size">
      <div
        role="radiogroup"
        aria-label="Reading size"
        className="grid grid-cols-1 gap-3 sm:grid-cols-3"
      >
        {READING_SIZES.map(({ size, label }) => (
          <RadioCard
            key={size}
            name={name}
            label={`${label} ${size}px`}
            checked={selected === size}
            onChange={() => setSelected(size)}
          >
            <span className="text-sm font-medium">{label}</span>
            <span className="text-xs text-muted-foreground">{size}px</span>
          </RadioCard>
        ))}
      </div>
      <ReadingPreview />
    </SettingsSection>
  );
}

function ToggleRow({
  label,
  checked,
  onChange,
  disabled = false,
  status,
}: {
  label: string;
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  status?: string;
}): ReactElement {
  const id = useId();
  return (
    <div className="py-3">
      <label
        htmlFor={id}
        className="flex min-h-6 items-center justify-between gap-4 text-sm leading-6"
      >
        <span>{label}</span>
        <input
          id={id}
          type="checkbox"
          role="switch"
          checked={checked}
          aria-describedby={status ? `${id}-status` : undefined}
          disabled={disabled}
          onChange={onChange}
          className="size-5 shrink-0 cursor-pointer accent-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-50"
        />
      </label>
      {status && (
        <p
          id={`${id}-status`}
          role="status"
          className="mt-2 text-xs leading-5 text-muted-foreground"
        >
          {status}
        </p>
      )}
    </div>
  );
}

function NotificationRow(): ReactElement {
  const { on, disabled, support, toggle } = useNotificationToggle();

  return (
    <ToggleRow
      label="Browser notification when a session needs you"
      checked={on}
      disabled={disabled}
      onChange={toggle}
      status={notificationStatus(support, on)}
    />
  );
}

function ReadingTogglesSection(): ReactElement {
  const [sourceMarks, setSourceMarks] = useAtom(sourceMarksAtom);
  const [autoRebuild, setAutoRebuild] = useAtom(autoRebuildAtom);
  return (
    <SettingsSection title="Reading toggles">
      <div className="divide-y divide-border">
        <ToggleRow
          label="Show source marks"
          checked={sourceMarks}
          onChange={() => setSourceMarks(!sourceMarks)}
        />
        <ToggleRow
          label="Rebuild summaries automatically"
          checked={autoRebuild}
          onChange={() => setAutoRebuild(!autoRebuild)}
        />
        <NotificationRow />
      </div>
    </SettingsSection>
  );
}

function SettingsSections(): ReactElement {
  return (
    <div className="min-w-0 space-y-6">
      <SettingsSection title="Theme">
        <div className="max-sm:[&_[role=group]]:flex-col max-sm:[&_button]:w-full">
          <ThemePicker swatch={PlannerThemeSwatch} />
        </div>
      </SettingsSection>
      <ReadingFaceSection />
      <ReadingSizeSection />
      <ReadingTogglesSection />
    </div>
  );
}

export function SettingsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}): ReactElement {
  // The header opens this dialog without a Radix trigger to restore focus to.
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="planner-settings-dialog max-h-[calc(100dvh-2rem)] overflow-y-auto font-sans sm:max-w-2xl"
        onOpenAutoFocus={() => {
          returnFocus.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocus.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle className="font-sans">Settings</DialogTitle>
          <DialogDescription>Saved in this browser</DialogDescription>
        </DialogHeader>
        <SettingsSections />
      </DialogContent>
    </Dialog>
  );
}
