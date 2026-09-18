import { useAtom } from "jotai";
import type { ReactElement } from "react";
import { useEffect, useState } from "react";

import { ThemePicker } from "@/components/theme-picker";
import {
  notificationSupport,
  notificationsEnabledAtom,
  requestNotificationPermission,
  type NotificationSupport,
} from "@/state/notifications";

const WEBUI_HAYSTACK =
  "dashboard theme appearance colour color light dark blue gray grey ledger aubergine pacific graphite notifications desktop alerts browser localstorage";

export function webuiMatches(query: string): boolean {
  const normalized = query.trim().toLowerCase();
  return normalized === "" || WEBUI_HAYSTACK.includes(normalized);
}

function notificationStatus(support: NotificationSupport, on: boolean): string {
  switch (support) {
    case "unsupported":
      return "this browser has no notification support";
    case "insecure":
      return "notifications need localhost or HTTPS; this page is plain HTTP on a LAN address";
    case "denied":
      return "the browser has blocked notifications for this page";
    case "granted":
      return on ? "on" : "off";
    case "default":
      return "the browser will ask when you turn this on";
  }
}

export function SettingsWebui({ query }: { query: string }): ReactElement | null {
  const [enabled, setEnabled] = useAtom(notificationsEnabledAtom);
  const [support, setSupport] = useState<NotificationSupport>(() => notificationSupport());
  const on = enabled && support === "granted";
  const disabled = support === "unsupported" || support === "insecure" || support === "denied";

  useEffect(() => {
    if (enabled && support !== "granted") setEnabled(false);
  }, [enabled, support, setEnabled]);

  if (!webuiMatches(query)) return null;

  async function toggleNotifications(): Promise<void> {
    if (on) {
      setEnabled(false);
      return;
    }

    if (support === "default") {
      const result = await requestNotificationPermission();
      setSupport(result);
      setEnabled(result === "granted");
      return;
    }

    if (support === "granted") setEnabled(true);
  }

  return (
    <section className="settings-webui border-b border-border px-[22px] py-5">
      <div className="settings-webui-heading flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <span className="eyebrow">dashboard</span>
        <span className="settings-help">
          this browser only — kept in localStorage, never written to loom&apos;s config
        </span>
      </div>

      <div className="settings-webui-row mt-5">
        <div className="settings-webui-row-heading mb-3">
          <span className="settings-webui-label block text-[13px] font-medium leading-5">
            theme
          </span>
          <span className="settings-help text-[13px] leading-5">
            Each swatch draws the stage graph in that theme&apos;s own palette.
          </span>
        </div>
        <ThemePicker />
      </div>

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
            onClick={toggleNotifications}
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
    </section>
  );
}
