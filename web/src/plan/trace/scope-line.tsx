import type { JSX } from "react";
import { Link } from "react-router";

import type { NodeRef, TraceDoc } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useKeptDoc } from "@/plan/concept/passage-doc";
import { featureSourceHref } from "@/plan/feature/source-href";

import { docOfPath, shownSettle } from "./model";
import { ChangesSinceSettled } from "./settled-changes";
import { SettleGlyph } from "./settle-glyph";
import { entryText, visionRowText, type StubText } from "./stub-text";
import { useTrace } from "./state";

import "./trace.css";

/** The stub's words, read from the document that holds them (vision row or metaplan entry). */
function useStubText(doc: TraceDoc, stub: NodeRef): StubText | null {
  const { doc: holder } = useKeptDoc(stub.path);
  if (!holder) return null;
  // A plan's stub is a metaplan entry; a section's is its vision row (the anchor is the table's heading).
  if (doc.role === "plan") return stub.anchor ? entryText(holder, stub.anchor) : null;
  return doc.number === null ? null : visionRowText(holder.text, doc.number);
}

function StubLine({
  feature,
  doc,
  stub,
}: {
  feature: string;
  doc: TraceDoc;
  stub: NodeRef;
}): JSX.Element {
  const text = useStubText(doc, stub);
  const place = doc.role === "plan" ? "metaplan" : "vision";
  const changed = shownSettle(doc) === "stub_changed";
  const words = text ? (text.body ?? text.title) : null;
  return (
    <div className="trace-standfirst">
      <p className="trace-scope">
        {words !== null && <Untrusted text={words} />}{" "}
        <Link
          className="trace-scope__from"
          to={featureSourceHref(feature, stub.path, { anchor: stub.anchor })}
        >
          from the {place} <span aria-hidden="true">↗</span>
        </Link>
      </p>
      {changed && (
        <p role="note" className="trace-scope__note">
          <SettleGlyph state="stub_changed" />
          The {place === "vision" ? "vision row" : "metaplan entry"} changed since you settled this{" "}
          <ChangesSinceSettled settled={doc.path} compare={stub.path} look="inline" />
        </p>
      )}
    </div>
  );
}

/**
 * The scope standfirst of a section or a plan, under its title: the stub text (the vision row's
 * Covers cell, or the metaplan entry's first paragraph) with a small link to its place, and a note
 * when the stub changed since the document was settled (never for done work, which is not
 * settled). Text from the documents is drawn as text, never as Markdown.
 */
export function ScopeLine(props: { feature: string; path: string }): JSX.Element | null {
  const { feature, path } = props;
  const { trace } = useTrace(feature);
  const doc = trace ? docOfPath(trace, path) : undefined;
  if (!doc?.stub || (doc.role !== "section" && doc.role !== "plan")) return null;
  return <StubLine feature={feature} doc={doc} stub={doc.stub} />;
}
