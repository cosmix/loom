import type { JSX } from "react";
import { Link } from "react-router";

import type { NodeRef, TraceDoc } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useKeptDoc } from "@/plan/concept/passage-doc";
import { featureSourceHref } from "@/plan/feature/source-href";

import { bundleLinks, docOfPath } from "./model";
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

function Briefs({ feature, doc }: { feature: string; doc: TraceDoc }): JSX.Element | null {
  const { trace } = useTrace(feature);
  const links = trace ? bundleLinks(trace, doc.path, feature) : [];
  if (links.length === 0) return null;
  return (
    <div className="trace-briefs">
      <p>Briefs</p>
      <ul aria-label="Briefs">
        {links.map((link) => (
          <li key={link.path}>
            <Link to={link.to}>
              <Untrusted text={link.label} />
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * What sets a section or a plan's scope before its body: the stub text linked to its place, a
 * note when the stub changed since the document was settled, and a plan's brief files. Text
 * from the documents is drawn as text, never as Markdown.
 */
export function StubPanel(props: { feature: string; path: string }): JSX.Element | null {
  const { feature, path } = props;
  const { trace } = useTrace(feature);
  const doc = trace ? docOfPath(trace, path) : undefined;
  if (!doc || (doc.role !== "section" && doc.role !== "plan")) return null;
  const changed = doc.settle === "stub_changed";
  if (!doc.stub && !changed && doc.bundle.length === 0) return null;
  return (
    <div className="trace-stub">
      {doc.stub && <StubWords feature={feature} doc={doc} stub={doc.stub} />}
      {changed && (
        <p role="note" className="trace-stub-note">
          Stub changed since settled
        </p>
      )}
      <Briefs feature={feature} doc={doc} />
    </div>
  );
}
