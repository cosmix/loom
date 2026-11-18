import { CircleAlertIcon, InfoIcon } from "lucide-react";
import { useAtom, useAtomValue } from "jotai";
import { useId, type ReactElement } from "react";

import { ThemePicker } from "@/components/theme-picker";
import { useNow } from "@/components/hooks/use-now";
import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { panelLoadAtom, summaryReloadAtom } from "@/plan/summaries/state";
import { SummaryPanel } from "@/plan/summaries/summary-panel";
import { BusyText } from "@/plan/ui/busy-text";
import { notificationStatus, useNotificationToggle } from "@/state/notifications";
import { PlannerThemeSwatch } from "./planner-swatch";
import { autoRebuildAtom } from "./preferences";
import { PaneBlock, SettingsPane } from "./settings-pane";

/** The switch: a native checkbox in the switch role, labelled and described by the row's ids. */
function ToggleSwitch({
  id,
  checked,
  disabled,
  labelledBy,
  describedBy,
  onChange,
}: {
  id: string;
  checked: boolean;
  disabled: boolean;
  labelledBy: string;
  describedBy: string;
  onChange: () => void;
}): ReactElement {
  return (
    <span className="ps-switch">
      <input
        id={id}
        type="checkbox"
        role="switch"
        checked={checked}
        aria-labelledby={labelledBy}
        aria-describedby={describedBy}
        disabled={disabled}
        onChange={onChange}
      />
      <span className="ps-knob" aria-hidden="true" />
    </span>
  );
}

/** A reason the switch cannot do what it says; `info` is a step the person can still take. */
export type RowReason = { text: string; kind: "info" | "warn" };

/**
 * A switch row: its title and subtitle, the switch at the end. A row prints a second line only
 * when the switch cannot do what it says (`reason`); it never echoes "on" or "off".
 */
export function ToggleRow({
  title,
  subtitle,
  checked,
  onChange,
  disabled = false,
  reason,
}: {
  title: string;
  subtitle: string;
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  reason?: RowReason;
}): ReactElement {
  const id = useId();
  const titleId = `${id}-title`;
  const subtitleId = `${id}-sub`;
  const reasonId = `${id}-reason`;
  const Glyph = reason?.kind === "warn" ? CircleAlertIcon : InfoIcon;
  return (
    <div className="ps-row">
      <div className="ps-row-text">
        <label htmlFor={id} className="ps-row-label">
          <span id={titleId} className="ps-row-title">
            {title}
          </span>
          <span id={subtitleId} className="ps-row-sub">
            {subtitle}
          </span>
        </label>
        {reason && (
          <p id={reasonId} role="status" className="ps-row-reason" data-kind={reason.kind}>
            <Glyph aria-hidden="true" />
            <span>{reason.text}</span>
          </p>
        )}
      </div>
      <ToggleSwitch
        id={id}
        checked={checked}
        disabled={disabled}
        labelledBy={titleId}
        describedBy={reason ? `${subtitleId} ${reasonId}` : subtitleId}
        onChange={onChange}
      />
    </div>
  );
}

function sentence(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function NotificationRow(): ReactElement {
  const { on, disabled, support, toggle } = useNotificationToggle();
  // A granted permission needs no line: the switch itself says whether it is on.
  const reason: RowReason | undefined =
    support === "granted"
      ? undefined
      : {
          text: sentence(notificationStatus(support, on)),
          kind: support === "default" ? "info" : "warn",
        };
  return (
    <ToggleRow
      title="Notify when a session needs me"
      subtitle="A browser notification for permission prompts and finished runs"
      checked={on}
      disabled={disabled}
      onChange={toggle}
      reason={reason}
    />
  );
}

export function AppearancePane(): ReactElement {
  return (
    <SettingsPane title="Appearance" scope="browser">
      <PaneBlock title="Theme">
        <div className="ps-theme">
          <ThemePicker swatch={PlannerThemeSwatch} optionClassName="ps-theme-option" />
        </div>
      </PaneBlock>
    </SettingsPane>
  );
}

export function NotificationsPane(): ReactElement {
  return (
    <SettingsPane title="Notifications" scope="browser">
      <div className="ps-block ps-rows">
        <NotificationRow />
      </div>
    </SettingsPane>
  );
}

/**
 * The automatic-rebuild switch over the whole summary panel, read from the feed the shell runs:
 * token totals, last rebuild, rebuild activity and terminal claims, in that order. The provider
 * limits show only in the footer. Plain text stands in for the panel until the feed answers; a
 * status role here would collide with the notification row's.
 */
export function SummariesPane(): ReactElement {
  const load = useAtomValue(panelLoadAtom);
  const { reload } = useAtomValue(summaryReloadAtom);
  const [autoRebuild, setAutoRebuild] = useAtom(autoRebuildAtom);
  const nowSecs = Math.floor(useNow() / 1000);
  return (
    <SettingsPane title="Summaries" hint="Rebuild token totals">
      <div className="ps-block ps-rows">
        <ToggleRow
          title="Rebuild summaries automatically"
          subtitle="Up to 5 at a time; above that the app asks first"
          checked={autoRebuild}
          onChange={() => setAutoRebuild(!autoRebuild)}
        />
      </div>
      <div className="ps-block">
        {load.status === "ready" ? (
          <SummaryPanel panel={load.panel} nowSecs={nowSecs} />
        ) : load.status === "loading" ? (
          <p className="ps-hint">
            <BusyText>Loading summaries</BusyText>
          </p>
        ) : (
          <div className="space-y-2 text-[13px]">
            <p role="alert">
              <Untrusted text={load.message} />
            </p>
            <Button type="button" variant="outline" size="sm" onClick={reload}>
              Retry
            </Button>
          </div>
        )}
      </div>
    </SettingsPane>
  );
}
