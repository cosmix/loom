import { useAtomValue, useStore } from "jotai/react";
import { useState, type JSX } from "react";
import { Link, useSearchParams } from "react-router";

import { useNow } from "@/components/hooks/use-now";
import { Button } from "@/components/ui/button";
import { overviewKey, pageHref, summaryKey } from "@/plan/api/page-key";
import type { DocView, EntryKind, PlanView, ValidationView } from "@/plan/api/schema-files";
import { addAttachmentAtom } from "@/plan/attachments";
import { focusComposerAtom } from "@/plan/chat/composer-hooks";
import { panelCollapsedAtom } from "@/plan/chat/prefs";
import { usePressureRun } from "@/plan/chat/pressure/use-pressure";
import { chatSheetOpenAtom } from "@/plan/chat/sheet-state";
import { openDiff } from "@/plan/diff/open-diff";
import { canAuthorAtom } from "@/plan/state/atoms";
import { DocHeading, type HeadingChip, type HeadingCrumb } from "@/plan/ui/doc-heading";
import { OTHER_GLYPHS, PLAN_STATE_GLYPHS, type DocKind } from "@/plan/ui/glyphs";

import { toAddress } from "./address";
import { LockNotice } from "./lock-notice";
import { PlanActionsExtras } from "./plan/plan-actions-extras";
import { ValidationList, warningCount } from "./plan/validation-list";

function docKind(kind: EntryKind): DocKind | null {
  switch (kind) {
    case "vision":
    case "section":
    case "note":
    case "metaplan":
    case "plan":
    case "brief":
    case "knowledge":
    case "review":
      return kind;
    default:
      return null;
  }
}

function breadcrumb(doc: DocView): HeadingCrumb[] {
  const groups: Partial<Record<EntryKind, string>> = {
    plan: "Plans",
    vision: "Product",
    section: "Product",
    note: "Product",
    knowledge: "Knowledge",
    brief: "Briefs",
  };
  return [
    { label: "Planner" },
    ...(doc.feature === null
      ? []
      : [
          {
            label: doc.feature,
            to: `/f/${encodeURIComponent(doc.feature)}`,
          },
        ]),
    { label: groups[doc.kind] ?? "Files" },
    { label: doc.title },
  ];
}

function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

function summaryTarget(doc: DocView): string | null {
  if (doc.kind === "plan") {
    return doc.plan === null
      ? null
      : pageHref(summaryKey(doc.feature, "engineering", doc.plan.slug));
  }
  if (doc.feature === null) return null;
  if (doc.kind === "vision" || doc.kind === "metaplan") {
    return pageHref(overviewKey(doc.feature, doc.kind === "vision" ? "product" : "engineering"));
  }
  if (doc.kind !== "section") return null;
  const stem = doc.path
    .split("/")
    .pop()
    ?.replace(/\.[^.]+$/, "");
  return stem ? pageHref(summaryKey(doc.feature, "product", stem)) : null;
}

function relativeTime(stamp: string, now: number): string {
  const seconds = Math.floor((now - Date.parse(stamp)) / 1000);
  if (!Number.isFinite(seconds)) return "unknown";
  const age = Math.max(0, seconds);
  if (age < 60) return `${age} s ago`;
  if (age < 3600) return `${Math.floor(age / 60)} min ago`;
  if (age < 86400) return `${Math.floor(age / 3600)} h ago`;
  return `${Math.floor(age / 86400)} d ago`;
}

function validationText(validation: ValidationView): string {
  const warnings = plural(warningCount(validation), "warning");
  return validation.errors.length === 0
    ? `Valid · ${warnings}`
    : `${plural(validation.errors.length, "error")}, ${warnings}`;
}

function pressureText(date: string | null, active: boolean): string {
  if (active) return "Being pressure-tested";
  if (date === null) return "Not pressure-tested";
  const now = new Date();
  const today = [
    now.getFullYear(),
    String(now.getMonth() + 1).padStart(2, "0"),
    String(now.getDate()).padStart(2, "0"),
  ].join("-");
  return `Pressure-tested ${date === today ? "today" : date}`;
}

function planChips(
  plan: PlanView,
  now: number,
  active: boolean,
  open: boolean,
  toggle: () => void,
): HeadingChip[] {
  const StateGlyph = PLAN_STATE_GLYPHS[plan.state];
  const PressureGlyph = OTHER_GLYPHS.pressureTested;
  const stateLabels = { written: "Written", running: "Running", done: "Done", retired: "Retired" };
  const chips: HeadingChip[] = [
    { key: "state", label: stateLabels[plan.state], icon: <StateGlyph /> },
    ...plan.implements.map((link) => ({
      key: `implements:${link.number}`,
      label: `Implements §${link.number}`,
      ...(link.path === null
        ? { title: "not found", description: "not found" }
        : { to: toAddress(link.path) }),
    })),
    { key: "pressure", label: pressureText(plan.pressure_date, active), icon: <PressureGlyph /> },
  ];
  if (plan.validation !== null) {
    chips.push({
      key: "validation",
      label: validationText(plan.validation),
      tone: plan.validation.errors.length > 0 ? "danger" : "neutral",
      onClick: toggle,
      pressed: open,
    });
  }
  if (plan.last_change !== null) {
    chips.push({
      key: "changed",
      label: `Changed ${relativeTime(plan.last_change, now)}`,
      title: plan.last_change,
    });
  }
  return chips;
}

function HeadingActions({
  doc,
  pressureActive,
}: {
  doc: DocView;
  pressureActive: boolean;
}): JSX.Element {
  const canAuthor = useAtomValue(canAuthorAtom);
  const store = useStore();
  const [params] = useSearchParams();
  const showEdit = doc.editable && canAuthor && params.get("edit") !== "1" && !pressureActive;
  const discuss = () => {
    store.set(addAttachmentAtom, {
      kind: "source",
      path: doc.path,
      anchor: null,
      lines: null,
      stageId: null,
      // As in the document starter, the bounded quote previews a reference to the whole file.
      text: Array.from(doc.text).slice(0, 2_000).join(""),
      hash: doc.hash,
    });
    store.set(panelCollapsedAtom, false);
    store.set(chatSheetOpenAtom, true);
    store.set(focusComposerAtom, true);
  };
  return (
    <>
      {showEdit && (
        <Button asChild variant="outline" size="sm">
          <Link to={`${toAddress(doc.path)}?edit=1`}>Edit</Link>
        </Button>
      )}
      {canAuthor && (
        <Button type="button" variant="outline" size="sm" onClick={discuss}>
          Discuss
        </Button>
      )}
    </>
  );
}

export function FileDocHeading({ doc }: { doc: DocView }): JSX.Element {
  const now = useNow(60_000);
  const run = usePressureRun();
  const pressureActive = doc.plan !== null && run?.plan === doc.path && run.ended === null;
  const [validationPath, setValidationPath] = useState<string | null>(null);
  const validationOpen = validationPath === doc.path;
  const chips =
    doc.plan === null
      ? []
      : planChips(doc.plan, now, pressureActive, validationOpen, () =>
          setValidationPath((path) => (path === doc.path ? null : doc.path)),
        );
  return (
    // HeadingChip keeps labels as untrusted strings; style missing targets through their title.
    <div className="[&_[title='not_found']]:line-through">
      <DocHeading
        breadcrumb={breadcrumb(doc)}
        kind={docKind(doc.kind)}
        title={doc.title}
        path={doc.path}
        changes={{ to: openDiff("head", doc.path) }}
        views={{
          current: "source",
          sourceTo: toAddress(doc.path),
          summaryTo: summaryTarget(doc),
        }}
        actions={<HeadingActions doc={doc} pressureActive={pressureActive} />}
        chips={chips}
      >
        {doc.lock && <LockNotice notice={doc.lock.notice} />}
        {validationOpen && doc.plan?.validation && (
          <ValidationList validation={doc.plan.validation} />
        )}
        {doc.plan && <PlanActionsExtras plan={doc.plan} path={doc.path} />}
      </DocHeading>
    </div>
  );
}
