import type { ReactElement } from "react";

import { ThemePicker } from "@/components/theme-picker";
import {
  notificationStatus,
  useNotificationToggle,
  type NotificationSupport,
} from "@/state/notifications";

const WEBUI_HAYSTACK =
  "dashboard theme appearance colour color light dark gray grey ledger walnut graphite notifications desktop alerts browser localstorage";

export function webuiMatches(query: string): boolean {
  const normalized = query.trim().toLowerCase();
  return normalized === "" || WEBUI_HAYSTACK.includes(normalized);
}

function ThemeRow(): ReactElement {
  return (
    <div className="settings-webui-row mt-5">
      <div className="settings-webui-row-heading mb-3">
        <span className="settings-webui-label block text-[13px] font-medium leading-5">theme</span>
        <span className="settings-help text-[13px] leading-5">
          Each swatch draws the stage graph in that theme&apos;s own palette.
        </span>
      </div>
      <ThemePicker />
    </div>
  );
}

function NotificationsRow({
  on,
  disabled,
  support,
  onToggle,
}: {
  on: boolean;
  disabled: boolean;
  support: NotificationSupport;
  onToggle: () => void;
}): ReactElement {
  return (
    <div className="settings-webui-row mt-6 border-t border-border pt-4">
      <div className="settings-webui-row-heading flex items-start justify-between gap-4">
        <div>
          <span className="settings-webui-label block text-[13px] font-medium leading-5">
            desktop notifications
          </span>
          <span className="settings-help text-[13px] leading-5">
            Raised when a stage needs you.
          </span>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={on}
          aria-label="desktop notifications"
          disabled={disabled}
          onClick={onToggle}
          className="settings-webui-switch relative inline-flex h-6 w-11 shrink-0 rounded-full border border-border bg-muted p-0.5 transition-colors enabled:cursor-pointer enabled:hover:border-primary disabled:cursor-not-allowed disabled:opacity-50"
        >
          <span
            aria-hidden="true"
            className={`h-4.5 w-4.5 rounded-full bg-muted-foreground transition-transform ${
              on ? "translate-x-5 bg-primary" : "translate-x-0"
            }`}
          />
        </button>
      </div>
      <p className="settings-webui-status mt-3 text-[13px] leading-5 text-muted-foreground">
        {notificationStatus(support, on)}
      </p>
      <p className="settings-webui-explanation mt-1 text-[13px] leading-5 text-muted-foreground">
        Fires when a stage needs you (a question, a block, a failed check, a merge conflict or a
        review), when one needs a handoff, and when the whole run finishes.
      </p>
    </div>
  );
}

export function SettingsWebui({ query }: { query: string }): ReactElement | null {
  const { on, disabled, support, toggle } = useNotificationToggle();

  if (!webuiMatches(query)) return null;

  return (
    <section className="settings-webui border-b border-border px-[22px] py-5">
      <div className="settings-webui-heading flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <span className="eyebrow">dashboard</span>
        <span className="settings-help">
          this browser only — kept in localStorage, never written to loom&apos;s config
        </span>
      </div>

      <ThemeRow />
      <NotificationsRow on={on} disabled={disabled} support={support} onToggle={toggle} />
    </section>
  );
}
