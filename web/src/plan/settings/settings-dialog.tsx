import { SlidersHorizontalIcon } from "lucide-react";
import { useRef, type ReactElement } from "react";

import { ThemePicker } from "@/components/theme-picker";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { PlannerThemeSwatch } from "./planner-swatch";
import {
  ReadingFaceGroup,
  ReadingSizeGroup,
  ReadingTogglesGroup,
  SettingsGroup,
} from "./settings-sections";
import "./settings.css";

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
        className="planner-settings-dialog font-sans"
        onOpenAutoFocus={() => {
          returnFocus.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocus.current?.focus();
        }}
      >
        <DialogHeader className="ps-head">
          <SlidersHorizontalIcon aria-hidden="true" />
          <DialogTitle className="ps-title">Settings</DialogTitle>
          <DialogDescription className="sr-only">Saved in this browser</DialogDescription>
        </DialogHeader>
        <div className="ps-body">
          <SettingsGroup title="Theme" hint="Remembered in this browser">
            <div className="ps-theme">
              <ThemePicker swatch={PlannerThemeSwatch} />
            </div>
          </SettingsGroup>
          <ReadingFaceGroup />
          <ReadingSizeGroup />
          <ReadingTogglesGroup />
        </div>
      </DialogContent>
    </Dialog>
  );
}
