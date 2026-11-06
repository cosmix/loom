import { SlidersHorizontalIcon } from "lucide-react";
import { useRef, type ReactElement } from "react";

import { ThemePicker, type ThemeId } from "@/components/theme-picker";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { PlannerThemeSwatch } from "./planner-swatch";
import { ReadingFaceGroup, ReadingSizeGroup } from "./reading-sections";
import { ReadingTogglesGroup, SettingsGroup, SummariesGroup } from "./settings-sections";
import "./settings.css";

/** The planner offers three themes; Aubergine is dashboard-only. */
const PLANNER_THEMES: readonly ThemeId[] = ["ledger", "walnut", "graphite"];

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
              <ThemePicker themes={PLANNER_THEMES} swatch={PlannerThemeSwatch} optionClassName="ps-theme-option" />
            </div>
          </SettingsGroup>
          <ReadingFaceGroup />
          <ReadingSizeGroup />
          <ReadingTogglesGroup />
          <SummariesGroup />
        </div>
      </DialogContent>
    </Dialog>
  );
}
