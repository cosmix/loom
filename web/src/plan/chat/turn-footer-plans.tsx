import { useAtomValue, useStore } from "jotai";
import { useEffect, useState } from "react";
import { Link } from "react-router";

import type { TreeEntry } from "@/plan/api/schema-files";
import { addAttachmentAtom, MAX_ATTACHMENT_BYTES, type AttachmentDraft } from "@/plan/attachments";
import { fetchDoc, fetchTree } from "@/plan/files/api";
import { treeAtom } from "@/plan/files/atoms";
import { getImplementedBy, type ImplementedByResponse } from "@/plan/review/api";
import { connectionEpochAtom, isHolderAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";
import { draftAtom } from "./drafts";
import { Untrusted, visibleControls } from "./visible-controls";

type Plan = ImplementedByResponse["sections"][string][number];
type Store = ReturnType<typeof useStore>;

export type ImplementedBy = {
  sections: ImplementedByResponse["sections"] | null;
  error: string | null;
  retry: () => void;
};

export const sectionFile = /^doc\/product\/section_\d+\.md$/;

/** One `implemented-by` request per turn footer, shared by every changed section file. */
export function useImplementedBy(enabled: boolean): ImplementedBy {
  const [sections, setSections] = useState<ImplementedBy["sections"]>(null);
  const [error, setError] = useState<string | null>(null);
  const epoch = useAtomValue(connectionEpochAtom);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    setError(null);
    setSections(null);
    void getImplementedBy().then((result) => {
      if (!active) return;
      if (result.ok) setSections(result.value.sections);
      else setError(result.message);
    });
    return () => {
      active = false;
    };
  }, [enabled, epoch, attempt]);
  return { sections, error, retry: () => setAttempt((value) => value + 1) };
}

function planPath(entries: TreeEntry[], slug: string): string | undefined {
  for (const entry of entries) {
    if (entry.path?.endsWith(`/PLAN-${slug}.md`) || entry.path?.endsWith(`-PLAN-${slug}.md`))
      return entry.path;
    const nested = planPath(entry.children, slug);
    if (nested) return nested;
  }
  return undefined;
}

async function sourceAttachment(path: string): Promise<AttachmentDraft> {
  const result = await fetchDoc(path);
  if (!result.ok) throw new Error(result.message);
  const doc = result.value;
  if (
    doc.truncated ||
    doc.too_large ||
    new TextEncoder().encode(doc.text).length > MAX_ATTACHMENT_BYTES
  ) {
    throw new Error(`${path}: select a passage of at most 16 KiB to attach.`);
  }
  return {
    kind: "source",
    path: doc.path,
    text: doc.text,
    hash: doc.hash,
    anchor: null,
    lines: null,
    stageId: null,
  };
}

async function planAttachments(
  store: Store,
  path: string,
  plans: Plan[],
): Promise<AttachmentDraft[]> {
  let tree = store.get(treeAtom);
  if (!tree) {
    const result = await fetchTree();
    if (!result.ok) throw new Error(result.message);
    tree = result.value;
  }
  const paths = plans.map((plan) => {
    const found = planPath([...tree.plans, ...tree.archive], plan.slug);
    if (!found) throw new Error(`Cannot find the source for ${plan.slug}.`);
    return found;
  });
  return Promise.all([path, ...paths].map(sourceAttachment));
}

function FillPlans({ path, plans, followUp }: { path: string; plans: Plan[]; followUp: boolean }) {
  const store = useStore();
  const holder = useAtomValue(isHolderAtom);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fill = async () => {
    if (!store.get(isHolderAtom) || busy) return;
    setBusy(true);
    setError(null);
    try {
      const attachments = await planAttachments(store, path, plans);
      if (!store.get(isHolderAtom)) return;
      const section = path.split("/").at(-1)!.replace(/\.md$/, "");
      const text = followUp
        ? `Plan a follow-up for the change to ${section}.`
        : `Check these plans against the change to ${section}.`;
      store.set(draftAtom, (draft) => ({
        ...draft,
        text: draft.text ? `${draft.text}\n\n${text}` : text,
      }));
      attachments.forEach((attachment) => store.set(addAttachmentAtom, attachment));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };
  if (!holder) return null;
  return (
    <>
      <DisabledReason
        reason={busy ? "Loading attachments" : null}
        onClick={() => void fill()}
        size="sm"
      >
        {followUp ? "Plan a follow-up" : "Check these plans"}
      </DisabledReason>
      {error && (
        <span role="alert">
          <Untrusted text={error} />
        </span>
      )}
    </>
  );
}

function PlanGroup({ path, state, plans }: { path: string; state: string; plans: Plan[] }) {
  const followUp = state === "running" || state === "done";
  return (
    <div role="group" aria-label={`${visibleControls(state)} plans`}>
      <p>
        <span aria-hidden="true">{followUp ? "✓" : "○"}</span> <Untrusted text={state} />
        {state === "written"
          ? " — may need updating"
          : followUp
            ? " — built from earlier text"
            : ""}
      </p>
      <ul>
        {plans.map((plan) => (
          <li key={plan.slug}>
            <Link className="underline" to={`/engineering/${encodeURIComponent(plan.slug)}`}>
              <Untrusted text={plan.title} />
            </Link>
          </li>
        ))}
      </ul>
      {(state === "written" || followUp) && (
        <FillPlans path={path} plans={plans} followUp={followUp} />
      )}
    </div>
  );
}

export function ImplementingPlans({ path, loaded }: { path: string; loaded: ImplementedBy }) {
  const { sections, error, retry } = loaded;
  const number = path.match(/section_(\d+)\.md$/)?.[1];
  const plans = number ? (sections?.[String(Number(number))] ?? []) : [];
  if (error)
    return (
      <LoadState
        state="error"
        message={visibleControls(error)}
        action={{ label: "Retry", onClick: retry }}
      />
    );
  if (!sections) return <LoadState state="loading" title="Loading implementing plans" />;
  return (
    <div className="ml-3 space-y-2">
      {[...new Set(plans.map((plan) => plan.state))].map((state) => (
        <PlanGroup
          key={state}
          path={path}
          state={state}
          plans={plans.filter((plan) => plan.state === state)}
        />
      ))}
    </div>
  );
}
