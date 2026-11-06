import { useAtomValue } from "jotai/react";
import { Fragment, type JSX } from "react";
import { Link, useInRouterContext, useLocation } from "react-router";

import { VISION_DOC, featureOfKey, pageKeyOfPath, parsePageKey } from "@/plan/api/page-key";
import type { Sentence } from "@/plan/api/schema-concept";
import { Untrusted } from "@/plan/chat/visible-controls";
import { featureOfDocPath, featureSourceHref } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";

import { chipsFor, settleLabel, type EdgeChip, type PageKind } from "./model";
import { traceAtom } from "./state";

import "./trace.css";

type Target = { feature: string; pageKind: PageKind };

/**
 * Which feature and chip rules a sentence's summary takes. A product section reads the edges
 * covering its headings; an engineering summary (`plans/<slug>`, which names no feature) reads
 * the spec headings it implements and takes the feature from the page being viewed.
 */
function targetOf(summary: string, pathname: string): Target | null {
  const parts = parsePageKey(summary);
  if (parts?.kind === "summary" && parts.scope === "product" && parts.doc !== VISION_DOC) {
    return { feature: parts.feature, pageKind: "product" };
  }
  const engineering =
    parts?.kind === "standalone" || (parts?.kind === "summary" && parts.scope === "engineering");
  if (engineering) {
    const feature = featureOfKey(pageKeyOfPath(pathname) ?? "");
    return feature === null ? null : { feature, pageKind: "plan" };
  }
  return null;
}

function chipTo(feature: string, chip: EdgeChip): string {
  if (chip.path === null) return featureHref(feature, "findings");
  return featureSourceHref(featureOfDocPath(chip.path), chip.path, { anchor: chip.anchor });
}

function Chip({ feature, chip }: { feature: string; chip: EdgeChip }): JSX.Element {
  return (
    <Link className="trace-chip" data-kind={chip.kind} to={chipTo(feature, chip)}>
      <Untrusted text={chip.label} />
      {chip.state !== null && (
        <>
          {" "}
          <span className="trace-chip-state" data-settle={chip.state}>
            {settleLabel(chip.state)}
          </span>
        </>
      )}
    </Link>
  );
}

function ChipsOf({
  sentence,
  target,
}: {
  sentence: Pick<Sentence, "rests_on" | "citations">;
  target: Target;
}): JSX.Element | null {
  // Read, never fetch: the feature layout's spine loads the trace, and a sentence drawn outside
  // a feature page (a help scene) must stay free of requests.
  const { trace } = useAtomValue(traceAtom(target.feature));
  if (!trace) return null;
  const chips = chipsFor(trace, sentence, target.pageKind);
  if (chips.length === 0) return null;
  return (
    <span className="trace-chips" role="group" aria-label="Related documents">
      {chips.map((chip) => (
        // The space keeps neighbouring chips apart as text (screen readers, copy, `textContent`).
        <Fragment key={chip.key}>
          {" "}
          <Chip feature={target.feature} chip={chip} />
        </Fragment>
      ))}
    </span>
  );
}

function RoutedChips(props: { sentence: Sentence; summary: string }): JSX.Element | null {
  const { pathname } = useLocation();
  const target = targetOf(props.summary, pathname);
  return target && <ChipsOf sentence={props.sentence} target={target} />;
}

/**
 * The trace chips after a summary sentence: what covers the headings it rests on (product
 * pages) or what the cited plan implements (plan pages). `summary` is the sentence's summary id.
 * The chips are links, so a sentence drawn outside a router (a help scene) shows none.
 */
export function EdgeChips(props: { sentence: Sentence; summary: string }): JSX.Element | null {
  return useInRouterContext() ? <RoutedChips {...props} /> : null;
}
