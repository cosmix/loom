import type { JSX } from "react";

import type { DocView } from "@/plan/api/schema-files";

import { EditBody } from "./edit-body";
import { useGone } from "./edit-flow";
import { GonePanel } from "./edit-panels";
import { useEditSession } from "./edit-session";

/** The editor for one document: source or merge view, Save, validation, leave handling. */
export function EditMode({ doc }: { doc: DocView }): JSX.Element {
  const gone = useGone(doc);
  const session = useEditSession(doc);
  if (gone) {
    return (
      <GonePanel what={gone} text={session.flow.state.text} onDiscard={session.discardToFiles} />
    );
  }
  return <EditBody doc={doc} session={session} />;
}
