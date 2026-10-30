import { EllipsisIcon } from "lucide-react";
import { useState } from "react";

import { ThemeToggle } from "@/aurora-ui/theme/ThemeToggle";
import { Button } from "@/components/ui/button";
import { HelpButton, SettingsButton } from "@/plan/header/header-buttons";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { Presence } from "@/plan/header/presence";
import { Share } from "@/plan/header/share";
import { Tabs } from "@/plan/header/tabs";

type OverflowActions = { onOpenSettings: () => void; onOpenHelp: () => void };

/**
 * Below `lg` the header keeps the logo, identity and activity pill; everything else sits behind
 * this menu. Settings and Help close it first so the dialog does not open over a live popover.
 */
export function Overflow({ onOpenSettings, onOpenHelp }: OverflowActions) {
  const [open, setOpen] = useState(false);
  const then = (action: () => void) => () => {
    setOpen(false);
    action();
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="icon-sm" className="plan-ibtn" aria-label="More">
          <EllipsisIcon aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      {/* Portaled out of the header, so it carries the planner root and touch-size class itself. */}
      <PopoverContent data-loom-plan aria-label="More" className="header-controls plan-overflow">
        <Tabs />
        <div className="plan-overflow-row">
          <Presence />
          <Share />
          <ThemeToggle />
          <SettingsButton onClick={then(onOpenSettings)} />
          <HelpButton onClick={then(onOpenHelp)} />
        </div>
      </PopoverContent>
    </Popover>
  );
}
