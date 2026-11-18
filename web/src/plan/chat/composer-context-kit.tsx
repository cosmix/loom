import { act, renderHook, waitFor } from "@testing-library/react";
import { Provider } from "jotai";
import type { ReactNode } from "react";
import { afterEach, beforeEach, expect, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import type { setup } from "./composer-harness";
import { useSendDraft } from "./composer-hooks";
import { draftAtom } from "./drafts";
import { snapshotAtom } from "./model";
import { viewedDocumentAtom } from "./viewed-document";

export const BODY = "Operators review one section at a time.";
export const pathOf = (number: string) => `doc/product/atlas/section_${number}.md`;

export type Harness = ReturnType<typeof setup>;
type Sent = {
  command: string;
  text: string;
  feature?: string;
  attachments: Array<{ kind: string; path: string; quote: string }>;
};

/** What the stubbed `/api/doc` has been asked, and how it answers the next request. */
export const docServer = {
  asked: [] as string[],
  gate: null as Promise<void> | null,
  missing: false,
  text: `# Doc\n\n${BODY}\n`,
};

/** Replaces `fetch` with a `/api/doc` that answers from `docServer`; call once per test file. */
export function installDocServer(): void {
  beforeEach(() => {
    docServer.asked = [];
    docServer.gate = null;
    docServer.missing = false;
    docServer.text = `# Doc\n\n${BODY}\n`;
    vi.stubGlobal("fetch", async (input: RequestInfo | URL): Promise<Response> => {
      const url = new URL(typeof input === "string" ? input : input.toString(), "http://localhost");
      const path = `doc/${url.searchParams.get("path") ?? ""}`;
      docServer.asked.push(path);
      await docServer.gate;
      const body = docServer.missing
        ? { error: "not_found", message: "no such file" }
        : { ...docFixture, path, feature: "atlas", sections: [], text: docServer.text };
      return new Response(JSON.stringify(body), {
        status: docServer.missing ? 404 : 200,
        headers: { "content-type": "application/json" },
      });
    });
  });
  afterEach(() => vi.unstubAllGlobals());
}

export function view(h: Harness, number: string): void {
  act(() =>
    h.store.set(viewedDocumentAtom, {
      kind: "section",
      path: pathOf(number),
      name: `section_${number}`,
    }),
  );
}

export function sender(h: Harness) {
  initClient(h.store);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <Provider store={h.store}>{children}</Provider>
  );
  return renderHook(() => useSendDraft(), { wrapper });
}

export const sentAt = (h: Harness, index: number) => h.sent[index] as unknown as Sent;
export const pathsAt = (h: Harness, index: number) =>
  sentAt(h, index).attachments.map((a) => a.path);

/** Types `text`, presses send and answers the command, the way the server's result frame does. */
export async function deliver(h: Harness, hook: ReturnType<typeof sender>, text = "A question") {
  const before = h.sent.length;
  act(() => h.store.set(draftAtom, (draft) => ({ ...draft, text })));
  await act(async () => {
    void hook.result.current.send();
  });
  await waitFor(() => expect(h.sent).toHaveLength(before + 1));
  await h.result();
  await waitFor(() => expect(hook.result.current.pending).toBe(false));
}

export const newSession = (h: Harness, id: string) =>
  act(() => h.store.set(snapshotAtom, (s) => ({ ...s, session: { ...s.session!, id } })));
