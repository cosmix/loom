import { EllipsisIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { Presence } from "@/plan/header/presence";
import { Share } from "@/plan/header/share";

/** Below `lg`, presence and Share live behind one menu button. */
export function Overflow() {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label="More">
          <EllipsisIcon aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent aria-label="More" className="w-auto flex-row items-center">
        <Presence />
        <Share />
      </PopoverContent>
    </Popover>
  );
}
