import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, useLocation } from "react-router";
import { describe, expect, it } from "vitest";

import type { DocView } from "@/plan/api/schema-files";
import { useDocMarkdownProps } from "@/plan/files/use-doc-markdown";

const doc = {
  path: "doc/a.md",
  sections: [{ anchor: "intro" }, { anchor: "usage" }],
} as unknown as DocView;

function wrapper({ children }: { children: ReactNode }) {
  return <MemoryRouter>{children}</MemoryRouter>;
}

describe("useDocMarkdownProps", () => {
  it("resolves anchors for the open document only", () => {
    const { result } = renderHook(() => useDocMarkdownProps(doc, null), { wrapper });
    expect([...(result.current.anchorsOf("doc/a.md") ?? [])]).toEqual(["intro", "usage"]);
    expect(result.current.anchorsOf("doc/b.md")).toBeNull();
  });

  it("navigates to the given address", () => {
    const { result } = renderHook(
      () => ({ props: useDocMarkdownProps(doc, null), location: useLocation() }),
      { wrapper },
    );
    act(() => result.current.props.onNavigate("/files/doc/b.md"));
    expect(result.current.location.pathname).toBe("/files/doc/b.md");
  });
});
