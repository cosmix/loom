import {
  BellIcon,
  ChartColumnIcon,
  ContrastIcon,
  GlobeIcon,
  SlidersHorizontalIcon,
  TypeIcon,
} from "lucide-react";
import { useId, useRef, useState, type ReactElement } from "react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ReadingPane } from "./reading-sections";
import { AppearancePane, NotificationsPane, SummariesPane } from "./settings-sections";
import { panelId, SettingsRail, tabId, type RailPane } from "./settings-rail";
import { WebFetchPane } from "./web-fetch-section";
import "./settings.css";

type PaneId = "appearance" | "reading" | "notifications" | "web-fetch" | "summaries";

/** The rail's tabs in order; `PANES` below holds what each draws. */
const RAIL: readonly RailPane<PaneId>[] = [
  { id: "appearance", label: "Appearance", Icon: ContrastIcon },
  { id: "reading", label: "Reading", Icon: TypeIcon },
  { id: "notifications", label: "Notifications", Icon: BellIcon },
  { id: "web-fetch", label: "Web fetches", Icon: GlobeIcon },
  { id: "summaries", label: "Summaries", Icon: ChartColumnIcon },
];

const PANES: Record<PaneId, () => ReactElement> = {
  appearance: AppearancePane,
  reading: ReadingPane,
  notifications: NotificationsPane,
  "web-fetch": WebFetchPane,
  summaries: SummariesPane,
};

export function SettingsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}): ReactElement {
  // The header opens this dialog without a Radix trigger to restore focus to.
  const returnFocus = useRef<HTMLElement | null>(null);
  const [current, setCurrent] = useState<PaneId>("appearance");
  const idBase = useId();
  const Pane = PANES[current];
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
          <DialogDescription className="sr-only">
            Saved in this browser; web fetches are saved for this project
          </DialogDescription>
        </DialogHeader>
        <div className="ps-shell">
          <SettingsRail panes={RAIL} current={current} onSelect={setCurrent} idBase={idBase} />
          {/* Keyed by pane, so a new pane starts at the top of its scroll. */}
          <div
            key={current}
            role="tabpanel"
            id={panelId(idBase)}
            aria-labelledby={tabId(idBase, current)}
            tabIndex={0}
            className="ps-pane"
          >
            <Pane />
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
