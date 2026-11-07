import type { JSX } from "react";
import { Link } from "react-router";

import type { NodeRef, TraceDoc } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useKeptDoc } from "@/plan/concept/passage-doc";
import { featureSourceHref } from "@/plan/feature/source-href";

import { docOfPath, shownSettle } from "./model";
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

function StubWords({
  feature,
  doc,
  stub,
}: {
  feature: string;
  doc: TraceDoc;
  stub: NodeRef;
}): JSX.Element {
  const text = useStubText(doc, stub);
  const place = doc.role === "plan" ? "Stub in the metaplan" : "Stub in the vision";
  return (
    <div className="trace-stub-text">
      <Link to={featureSourceHref(feature, stub.path, { anchor: stub.anchor })}>{place}</Link>
      {text && (
        <p>
          <strong>
            <Untrusted text={text.title} />
          </strong>
          {text.body !== null && (
            <>
              {" "}
              <Untrusted text={text.body} />
            </>
          )}
        </p>
      )}
    </div>
  );
}

/**
 * What sets a section or a plan's scope before its body: the stub text linked to its place, with
 * a note when the stub changed since the document was settled (never for done work, which is not
 * settled). Text from the documents is drawn as text, never as Markdown.
 */
export function StubPanel(props: { feature: string; path: string }): JSX.Element | null {
  const { feature, path } = props;
  const { trace } = useTrace(feature);
  const doc = trace ? docOfPath(trace, path) : undefined;
  if (!doc || (doc.role !== "section" && doc.role !== "plan")) return null;
  const changed = shownSettle(doc) === "stub_changed";
  if (!doc.stub && !changed) return null;
  return (
    <div className="trace-stub">
      {doc.stub && <StubWords feature={feature} doc={doc} stub={doc.stub} />}
      {changed && (
        <p role="note" className="trace-stub-note">
          Stub changed since settled
        </p>
      )}
    </div>
  );
}
