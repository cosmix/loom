import type { ValidationView } from "@/plan/api/schema-files";
import { Untrusted } from "@/plan/chat/visible-controls";

const BUCKETS = ["structural", "knowledge", "sandbox", "baseline"] as const;

export function warningCount(validation: ValidationView): number {
  return BUCKETS.reduce((sum, bucket) => sum + validation.warnings[bucket].length, 0);
}

/// Errors first, then warnings by bucket, then notes. An item that names a
/// stage links to that stage's row in the Stages panel.
export function ValidationList({ validation }: { validation: ValidationView }) {
  return (
    <div className="space-y-2 text-[13px]" data-testid="validation-list">
      {validation.errors.length > 0 && (
        <ul aria-label="Errors" className="space-y-1">
          {validation.errors.map((item, index) => (
            <li key={`${index}:${item.message}`} className="text-destructive">
              {item.line !== null && (
                <span className="mr-1.5 font-mono text-xs">
                  {item.line}
                  {item.column !== null ? `:${item.column}` : ""}
                </span>
              )}
              {item.stage_id !== null && (
                <a href={`#stage:${item.stage_id}`} className="mr-1.5 font-mono text-xs underline">
                  <Untrusted text={item.stage_id} />
                </a>
              )}
              <Untrusted text={item.message} />
            </li>
          ))}
        </ul>
      )}
      {BUCKETS.map((bucket) =>
        validation.warnings[bucket].length === 0 ? null : (
          <div key={bucket}>
            <div className="text-xs font-medium text-muted-foreground">Warnings, {bucket}</div>
            <ul aria-label={`Warnings, ${bucket}`} className="space-y-1">
              {validation.warnings[bucket].map((message, index) => (
                <li key={`${index}:${message}`} className="text-[color:var(--tone-warning)]">
                  <Untrusted text={message} />
                </li>
              ))}
            </ul>
          </div>
        ),
      )}
      {validation.notes.length > 0 && (
        <ul aria-label="Notes" className="space-y-1 text-muted-foreground">
          {validation.notes.map((note, index) => (
            <li key={`${index}:${note}`}>
              <Untrusted text={note} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
