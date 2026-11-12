import type { JSX } from "react";

import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { Button } from "@/components/ui/button";
import type { ValidationItem, ValidationView } from "@/plan/api/schema-files";
import { Untrusted } from "@/plan/chat/visible-controls";

const BUCKETS = [
  ["structural", "Structure"],
  ["knowledge", "Knowledge"],
  ["sandbox", "Sandbox"],
  ["baseline", "Baseline"],
] as const;

/** The 1-based lines of every error that names one, for the gutter. */
export function errorLines(validation: ValidationView | null): number[] {
  if (!validation) return [];
  return validation.errors.flatMap((item) => (item.line === null ? [] : [item.line]));
}

/** True when the panel has something to show. */
export function hasFindings(validation: ValidationView | null): validation is ValidationView {
  if (!validation) return false;
  const { warnings } = validation;
  return (
    validation.errors.length > 0 ||
    validation.notes.length > 0 ||
    BUCKETS.some(([key]) => warnings[key].length > 0)
  );
}

type Props = {
  validation: ValidationView;
  /** Scrolls the source to a 1-based line. */
  onJump(line: number): void;
  /** Offered only after a save was refused for errors. */
  onSaveWithErrors?: () => void;
};

function ErrorItem({ item, onJump }: { item: ValidationItem; onJump(line: number): void }) {
  const { line } = item;
  return (
    <li className="flex flex-wrap items-baseline gap-x-2">
      <span>
        <Untrusted text={item.message} />
      </span>
      {line !== null && (
        <button
          type="button"
          className="text-xs text-muted-foreground underline"
          onClick={() => onJump(line)}
        >
          line {line}
        </button>
      )}
    </li>
  );
}

function Group({ title, items }: { title: string; items: string[] }): JSX.Element | null {
  if (items.length === 0) return null;
  return (
    <section aria-label={title} className="space-y-1">
      <h3 className="text-xs font-medium text-muted-foreground">{title}</h3>
      <ul className="list-disc space-y-0.5 pl-5">
        {items.map((item, index) => (
          <li key={index}>
            <Untrusted text={item} />
          </li>
        ))}
      </ul>
    </section>
  );
}

function ValidationErrors({ validation, onJump }: Pick<Props, "validation" | "onJump">) {
  if (validation.errors.length === 0) return null;
  return (
    <section aria-label="Errors">
      <HazardPanel tone="error" title={`Errors (${validation.errors.length})`}>
        <ul className="list-disc space-y-0.5 pl-5">
          {validation.errors.map((item, index) => (
            <ErrorItem key={index} item={item} onJump={onJump} />
          ))}
        </ul>
      </HazardPanel>
    </section>
  );
}

function ValidationWarnings({ validation }: Pick<Props, "validation">) {
  if (!BUCKETS.some(([key]) => validation.warnings[key].length > 0)) return null;
  return (
    <HazardPanel tone="warning" title="Warnings" bodyClassName="space-y-3">
      {BUCKETS.map(([key, title]) => (
        <Group key={key} title={`Warnings: ${title}`} items={validation.warnings[key]} />
      ))}
    </HazardPanel>
  );
}

/** Errors, warnings by bucket and notes, as `loom plan verify` groups them. */
export function ValidationPanel({ validation, onJump, onSaveWithErrors }: Props): JSX.Element {
  return (
    <div
      role="region"
      aria-label="Plan validation"
      className="space-y-3 border-b border-hairline px-4 py-3 text-[13px]"
    >
      <ValidationErrors validation={validation} onJump={onJump} />
      <ValidationWarnings validation={validation} />
      <Group title="Notes" items={validation.notes} />
      {onSaveWithErrors && (
        <Button type="button" variant="destructive" size="sm" onClick={onSaveWithErrors}>
          Save with errors
        </Button>
      )}
    </div>
  );
}
