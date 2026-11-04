import { useEffect, useId, useRef, useState } from "react";

import { FEATURE_NAME_HINT, isFeatureName } from "@/plan/new/feature-name";
import { DisabledReason } from "@/plan/ui/disabled-reason";

type RenameFormProps = {
  name: string;
  /** Why confirming is blocked (authoring), or null. */
  reason: string | null;
  onConfirm: (name: string) => void;
  onCancel: () => void;
  /** The class of the Confirm/Cancel row; the owner's stylesheet lays it out. */
  actionsClass?: string;
  className?: string;
};

/**
 * A name field with Confirm and Cancel, validated by `isFeatureName`. Enter confirms a valid
 * name and Escape cancels; used by the proposal's Rename and by Taking shape's Name row.
 */
export function RenameForm({
  name,
  reason,
  onConfirm,
  onCancel,
  actionsClass,
  className,
}: RenameFormProps) {
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
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onCancel();
        }
      }}
      className={className}
    >
      <RenameField value={value} onChange={setValue} disabled={reason !== null} valid={valid} />
      <div className={actionsClass}>
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

function RenameField({
  value,
  onChange,
  disabled,
  valid,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled: boolean;
  valid: boolean;
}) {
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
