import { cn } from "cn";
import { useId, type ComponentProps, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";

// The planner's primary button: 30px high, `0 12px`, radius 8px, 13px/600. A caller that passes a
// `size` wants that size's measures instead. A ghost or danger look laid over the button from a
// wrapper (`[&_button]:` classes, `.x button` rules) out-ranks these on every property it sets, so
// such a wrapper must set `font-weight` too or the button reads 600.
const PRIMARY_MEASURES = "h-[30px] rounded-[8px] px-3 text-[13px] font-semibold";

export function DisabledReason({
  reason,
  onClick,
  size,
  children,
}: {
  reason: string | null;
  onClick: () => void;
  size?: ComponentProps<typeof Button>["size"];
  children: ReactNode;
}) {
  const reasonId = useId();
  const blocked = reason !== null;

  return (
    <div className="flex flex-col items-start gap-1">
      {/* Button styles only `disabled`; this stays focusable, so `aria-disabled` needs its own. */}
      <Button
        type="button"
        size={size}
        className={cn(
          size === undefined && PRIMARY_MEASURES,
          "aria-disabled:cursor-not-allowed aria-disabled:opacity-50 aria-disabled:hover:bg-primary aria-disabled:active:translate-y-0!",
        )}
        aria-disabled={blocked || undefined}
        aria-describedby={blocked ? reasonId : undefined}
        onClick={blocked ? undefined : onClick}
      >
        {children}
      </Button>
      {blocked && (
        <span id={reasonId} className="text-xs text-muted-foreground">
          {/* The reason can carry a server name or message (`authorBlockReasonAtom`). */}
          <Untrusted text={reason} />
        </span>
      )}
    </div>
  );
}
