import { useAtom, useAtomValue } from "jotai";
import { useId, type ReactElement, type ReactNode } from "react";

import { useNow } from "@/components/hooks/use-now";
import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { panelLoadAtom, summaryReloadAtom } from "@/plan/summaries/state";
import { SummaryPanel } from "@/plan/summaries/summary-panel";
import { BusyText } from "@/plan/ui/busy-text";
import { notificationStatus, useNotificationToggle } from "@/state/notifications";
import {
  autoRebuildAtom,
  DEFAULT_READING_FACE,
  READING_FACES,
  READING_SIZES,
  readingFaceAtom,
  readingSizeAtom,
  sourceMarksAtom,
} from "./preferences";

const PREVIEW_ENGLISH =
  "The app stores no workflow state and works out the current step from the earliest gap in the doc/ tree.";
const PREVIEW_GREEK = "Η εφαρμογή δεν αποθηκεύει κατάσταση ροής εργασίας.";

export function SettingsGroup({
  title,
  hint,
  level = 3,
  children,
}: {
  title: string;
  hint?: string;
  /** The heading level; a group whose body has its own h3 sections takes 2 to keep the outline. */
  level?: 2 | 3;
  children: ReactNode;
}): ReactElement {
  const heading = useId();
  const Title = level === 2 ? "h2" : "h3";
  return (
    <section aria-labelledby={heading} className="ps-group">
      <div className="ps-group-head">
        <Title id={heading} className="ps-group-title">
          {title}
        </Title>
        {hint && <span className="ps-hint">{hint}</span>}
      </div>
      {children}
    </section>
  );
}

export function ReadingFaceGroup(): ReactElement {
  const [selected, setSelected] = useAtom(readingFaceAtom);
  const name = useId();
  return (
    <SettingsGroup title="Reading face" hint="Document text only; titles and controls stay Inter">
      <div role="radiogroup" aria-label="Reading face" className="ps-faces">
        {READING_FACES.map((face) => (
          <label key={face.id} className="ps-face">
            <input
              type="radio"
              name={name}
              aria-label={face.label}
              checked={selected === face.id}
              onChange={() => setSelected(face.id)}
              className="sr-only"
            />
            {face.id === DEFAULT_READING_FACE && <span className="ps-flag">Default</span>}
            <span className="ps-specimen" style={{ fontFamily: face.family }} aria-hidden="true">
              <span className="ps-specimen-latin">Aa</span>
              <span className="ps-specimen-greek" lang="el">
                Ωω
              </span>
            </span>
            <span className="ps-face-name">{face.label}</span>
            <span className="ps-face-caption">{face.scripts}</span>
          </label>
        ))}
      </div>
    </SettingsGroup>
  );
}

function ReadingPreview(): ReactElement {
  const face = useAtomValue(readingFaceAtom);
  const size = useAtomValue(readingSizeAtom);
  const family = READING_FACES.find((option) => option.id === face)!.family;
  return (
    <div className="ps-preview" style={{ fontFamily: family, fontSize: size, lineHeight: 1.6 }}>
      <p lang="en">{PREVIEW_ENGLISH}</p>
      <p lang="el">{PREVIEW_GREEK}</p>
    </div>
  );
}

export function ReadingSizeGroup(): ReactElement {
  const [selected, setSelected] = useAtom(readingSizeAtom);
  const name = useId();
  return (
    <SettingsGroup title="Reading size">
      <div role="radiogroup" aria-label="Reading size" className="ps-seg">
        {READING_SIZES.map(({ size, label }) => (
          <label key={size} className="ps-seg-item">
            <input
              type="radio"
              name={name}
              aria-label={`${label} ${size}px`}
              checked={selected === size}
              onChange={() => setSelected(size)}
              className="sr-only"
            />
            {label}
            <span className="ps-seg-px">{size}px</span>
          </label>
        ))}
      </div>
      <ReadingPreview />
    </SettingsGroup>
  );
}

function ToggleRow({
  title,
  subtitle,
  checked,
  onChange,
  disabled = false,
  status,
}: {
  title: string;
  subtitle: string;
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  status?: string;
}): ReactElement {
  const id = useId();
  const titleId = `${id}-title`;
  const subtitleId = `${id}-sub`;
  const statusId = `${id}-status`;
  return (
    <div className="ps-row-wrap">
      <div className="ps-row">
        <label htmlFor={id} className="ps-row-text">
          <span id={titleId} className="ps-row-title">
            {title}
          </span>
          <span id={subtitleId} className="ps-row-sub">
            {subtitle}
          </span>
        </label>
        <span className="ps-switch">
          <input
            id={id}
            type="checkbox"
            role="switch"
            checked={checked}
            aria-labelledby={titleId}
            aria-describedby={status ? `${subtitleId} ${statusId}` : subtitleId}
            disabled={disabled}
            onChange={onChange}
          />
          <span className="ps-knob" aria-hidden="true" />
        </span>
      </div>
      {status && (
        <p id={statusId} role="status" className="ps-status">
          {status}
        </p>
      )}
    </div>
  );
}

function NotificationRow(): ReactElement {
  const { on, disabled, support, toggle } = useNotificationToggle();
  return (
    <ToggleRow
      title="Notify when a session needs me"
      subtitle="A browser notification for permission prompts and finished runs"
      checked={on}
      disabled={disabled}
      onChange={toggle}
      status={notificationStatus(support, on)}
    />
  );
}

/**
 * The whole summary panel (last rebuild, token totals, terminal claims), read from the feed the
 * shell runs. The provider limits show only in the footer. Plain text stands in for the panel until the feed answers; a status
 * role here would collide with the notification row's.
 */
export function SummariesGroup(): ReactElement {
  const load = useAtomValue(panelLoadAtom);
  const { reload } = useAtomValue(summaryReloadAtom);
  const nowSecs = Math.floor(useNow() / 1000);
  return (
    <SettingsGroup title="Summaries" hint="Rebuild token totals" level={2}>
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
    </SettingsGroup>
  );
}

export function ReadingTogglesGroup(): ReactElement {
  const [sourceMarks, setSourceMarks] = useAtom(sourceMarksAtom);
  const [autoRebuild, setAutoRebuild] = useAtom(autoRebuildAtom);
  return (
    <SettingsGroup title="Reading">
      <div>
        <ToggleRow
          title="Show source marks after sentences"
          subtitle="The coloured squares that say which document a sentence draws on"
          checked={sourceMarks}
          onChange={() => setSourceMarks(!sourceMarks)}
        />
        <ToggleRow
          title="Rebuild summaries automatically"
          subtitle="Up to 5 at a time; above that the app asks first"
          checked={autoRebuild}
          onChange={() => setAutoRebuild(!autoRebuild)}
        />
        <NotificationRow />
      </div>
    </SettingsGroup>
  );
}
