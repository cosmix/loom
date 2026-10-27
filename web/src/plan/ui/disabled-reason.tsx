import { useId, type ReactNode } from "react";

import { Button } from "@/components/ui/button";

export function DisabledReason({
  reason,
  onClick,
  children,
}: {
  reason: string | null;
  onClick: () => void;
  children: ReactNode;
}) {
  const reasonId = useId();
  const blocked = reason !== null;

  return (
    <div className="flex flex-col items-start gap-1">
      {/* Button styles only `disabled`; this stays focusable, so `aria-disabled` needs its own. */}
      <Button
        type="button"
        className="aria-disabled:cursor-not-allowed aria-disabled:opacity-50 aria-disabled:hover:bg-primary aria-disabled:active:translate-y-0!"
        aria-disabled={blocked || undefined}
        aria-describedby={blocked ? reasonId : undefined}
        onClick={blocked ? undefined : onClick}
      >
        {children}
      </Button>
      {blocked && (
        <span id={reasonId} className="text-xs text-muted-foreground">
          {reason}
        </span>
      )}
    </div>
  );
}
