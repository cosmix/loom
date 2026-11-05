import { useAtomValue, useStore } from "jotai/react";
import {
  Lock,
  MessageSquarePlus,
  Pencil,
  ShieldAlert,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";
import { useState, type JSX } from "react";
import { Link, useSearchParams } from "react-router";

import { useNow } from "@/components/hooks/use-now";
import { pageHref, summaryKey } from "@/plan/api/page-key";
import type { DocView, EntryKind, PlanView, ValidationView } from "@/plan/api/schema-files";
import { addAttachmentAtom, docAttachment } from "@/plan/attachments";
import { focusComposerAtom } from "@/plan/chat/composer-hooks";
import { offerErrorAtom } from "@/plan/chat/offer-error";
import { elsewhereRefusal } from "@/plan/chat/offer-flow";
import { usePressureRun } from "@/plan/chat/pressure/use-pressure";
import { elsewhereSessionAtom } from "@/plan/chat/session-home";
import { chatSheetOpenAtom } from "@/plan/chat/sheet-state";
import { IsolatedSentence } from "@/plan/concept/sentence-text";
import { openDiff } from "@/plan/diff/open-diff";
import { sourceHref } from "@/plan/feature/source-href";
import { canAuthorAtom } from "@/plan/state/atoms";
import {
  DocHeading,
  HEADING_BUTTON,
  type HeadingChip,
  type HeadingCrumb,
} from "@/plan/ui/doc-heading";
import type { DocKind } from "@/plan/ui/glyphs";
import { plural } from "@/plan/ui/plural";
import type { Store } from "@/state/store";

import { toAddress } from "./address";
import {
  changedChip,
  featureCrumb,
  planStateChip,
  pressureChip,
  sectionLabel,
  sectionNumberLabel,
  useFeatureFacts,
} from "./heading-parts";
import { LockNotice } from "./lock-notice";
import { PlanActionsExtras } from "./plan/plan-actions-extras";
import { ValidationList, warningCount } from "./plan/validation-list";
import { docSummaryKey, stemOf, useDocChips } from "./section-chips";

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

const AREAS: Partial<Record<EntryKind, string>> = {
  plan: "Plans",
  vision: "Product spec",
  section: "Product spec",
  note: "Product spec",
  metaplan: "Engineering",
  knowledge: "Knowledge",
  brief: "Briefs",
};

/** A plan's crumb is its slug in mono, a section's reads "Section 05 of 12". */
function documentCrumb(doc: DocView, sections: number | null): HeadingCrumb {
  if (doc.plan !== null) return { label: doc.plan.slug, mono: true };
  const stem = stemOf(doc.path);
  const section = doc.kind === "section" && stem ? sectionLabel(stem, sections) : null;
  // The crumb is plain text, so the title's code-span backticks go.
  return { label: section ?? doc.title.replaceAll("`", "") };
}

function summaryTarget(doc: DocView): string | null {
  if (doc.kind === "plan") {
    return doc.plan === null
      ? null
      : pageHref(summaryKey(doc.feature, "engineering", doc.plan.slug));
  }
  if (doc.feature === null) return null;
  const key = docSummaryKey(doc);
  return key ? pageHref(key) : null;
}

function validationChip(validation: ValidationView, open: boolean, toggle: () => void) {
  const warnings = plural(warningCount(validation), "warning");
  const errors = validation.errors.length;
  const Glyph = errors === 0 ? ShieldCheck : ShieldAlert;
  return {
    key: "validation",
    label: errors === 0 ? `Valid · ${warnings}` : `${plural(errors, "error")}, ${warnings}`,
    icon: <Glyph />,
    tone: errors > 0 ? "danger" : warningCount(validation) > 0 ? "warning" : "ok",
    onClick: toggle,
    pressed: open,
  } satisfies HeadingChip;
}

function implementsChip(plan: PlanView): HeadingChip[] {
  if (plan.implements.length === 0) return [];
  return [
    {
      key: "implements",
      label: "Implements",
      links: plan.implements.map((link) => ({
        label: sectionNumberLabel(link.number),
        ...(link.path === null
          ? { title: "not found", description: "not found" }
          : { to: toAddress(link.path) }),
      })),
    },
  ];
}

function planChips(
  plan: PlanView,
  now: number,
  active: boolean,
  open: boolean,
  toggle: () => void,
): HeadingChip[] {
  return [
    planStateChip(plan.state),
    ...implementsChip(plan),
    pressureChip(plan.pressure_date, active),
    ...(plan.validation === null ? [] : [validationChip(plan.validation, open, toggle)]),
    ...(plan.last_change === null ? [] : [changedChip(plan.last_change, now)]),
  ];
}

const LOCK_TITLES: Record<NonNullable<DocView["lock"]>["reason"], string> = {
  running: "Read-only while running",
  done: "Read-only: this plan is done",
  archive: "Read-only: archived",
  brief_of_locked_plan: "Read-only: its plan is locked",
  knowledge: "Read-only: knowledge is kept by loom",
  summaries: "Read-only: written by the summary builder",
};

function EditAction({ doc, hidden }: { doc: DocView; hidden: boolean }): JSX.Element | null {
  const canAuthor = useAtomValue(canAuthorAtom);
  const [params] = useSearchParams();
  if (doc.lock !== null) {
    return (
      <button
        type="button"
        {...HEADING_BUTTON}
        aria-disabled="true"
        title={LOCK_TITLES[doc.lock.reason]}
      >
        <Lock aria-hidden="true" />
        Edit
      </button>
    );
  }
  if (!doc.editable || !canAuthor || params.get("edit") === "1" || hidden) return null;
  return (
    <Link to={`${toAddress(doc.path)}?edit=1`} {...HEADING_BUTTON}>
      <Pencil aria-hidden="true" />
      Edit
    </Link>
  );
}

function discussDoc(store: Store, doc: DocView): void {
  store.set(chatSheetOpenAtom, true);
  // An open session on another screen has no composer here: a chip added now would wait in its
  // draft, unseen, and ride the next send into that session. Say why nothing happened instead.
  const elsewhere = store.get(elsewhereSessionAtom);
  if (elsewhere) {
    store.set(offerErrorAtom, elsewhereRefusal(elsewhere));
    return;
  }
  store.set(offerErrorAtom, null);
  store.set(addAttachmentAtom, docAttachment(doc));
  store.set(focusComposerAtom, true);
}

/** Whether a pressure test is running on the plan `doc` is, which hides Edit. */
export function usePressureActive(doc: DocView | null): boolean {
  const run = usePressureRun();
  return doc !== null && doc.plan !== null && run?.plan === doc.path && run.ended === null;
}

/** A disabled stand-in while a summary page's source document loads, so the row keeps its width. */
function PendingAction({ icon: Icon, label }: { icon: LucideIcon; label: string }): JSX.Element {
  return (
    <button type="button" {...HEADING_BUTTON} disabled>
      <Icon aria-hidden="true" />
      {label}
    </button>
  );
}

/** Edit and Discuss, shared by file mode and summary pages; `doc` is null until it loads. */
export function HeadingActions({
  doc,
  pressureActive,
}: {
  doc: DocView | null;
  pressureActive: boolean;
}): JSX.Element | null {
  const canAuthor = useAtomValue(canAuthorAtom);
  const store = useStore();
  if (doc === null) {
    return canAuthor ? (
      <>
        <PendingAction icon={Pencil} label="Edit" />
        <PendingAction icon={MessageSquarePlus} label="Discuss" />
      </>
    ) : null;
  }
  return (
    <>
      <EditAction doc={doc} hidden={pressureActive} />
      {canAuthor && (
        <button type="button" {...HEADING_BUTTON} onClick={() => discussDoc(store, doc)}>
          <MessageSquarePlus aria-hidden="true" />
          Discuss
        </button>
      )}
    </>
  );
}

export function FileDocHeading({ doc }: { doc: DocView }): JSX.Element {
  const now = useNow(60_000);
  const facts = useFeatureFacts(doc.feature);
  const docChips = useDocChips(doc, now);
  const pressureActive = usePressureActive(doc);
  const [validationPath, setValidationPath] = useState<string | null>(null);
  const validationOpen = validationPath === doc.path;
  const chips =
    doc.plan === null
      ? docChips
      : planChips(doc.plan, now, pressureActive, validationOpen, () =>
          setValidationPath((path) => (path === doc.path ? null : doc.path)),
        );
  const breadcrumb: HeadingCrumb[] = [
    ...(doc.feature === null ? [] : [featureCrumb(doc.feature)]),
    { label: AREAS[doc.kind] ?? "Files" },
    documentCrumb(doc, facts.sections),
  ];
  return (
    <DocHeading
      breadcrumb={breadcrumb}
      kind={docKind(doc.kind)}
      title={<IsolatedSentence text={doc.title} />}
      path={doc.path}
      changes={{ to: openDiff("head", doc.path) }}
      views={{
        current: "source",
        sourceTo: sourceHref(doc.path),
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
  );
}
