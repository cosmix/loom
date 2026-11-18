import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { summaryPanelSchema, summaryQuestionSchema } from "@/plan/api/schema-summaries";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { ApprovalNotice, PendingApprovalNotice } from "./approval-notice";
import { RebuildQuestion } from "./rebuild-question";
import { panelLoadAtom } from "./state";

const question = summaryQuestionSchema.parse(questionFixture);

function makeStore(who: "holder" | "other" | "free" | "read_only" = "holder") {
  const store = createStore();
  const you = helloFixture.you;
  const holder = who === "holder" ? you : { public_id: "other-browser", name: "Dana" };
  store.set(youAtom, you);
  store.set(
    serverInfoAtom,
    serverInfoSchema.parse({
      ...helloFixture.server,
      read_only: who === "read_only" ? helloFixture.server.read_only : [],
    }),
  );
  store.set(authoringAtom, {
    holder:
      who === "free" || who === "read_only"
        ? null
        : { ...holder, away_since: null, frees_at: null },
  });
  store.set(panelLoadAtom, {
    status: "ready",
    panel: summaryPanelSchema.parse({ ...panelFixture, question }),
  });
  initClient(store);
  return store;
}

function show(ui: ReactNode, store = makeStore()) {
  return render(<Provider store={store}>{ui}</Provider>);
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("RebuildQuestion", () => {
  it.each([
    ["last", "from the last builds"],
    ["per_byte", "scaled from past builds"],
    ["starting", "starting figure"],
  ] as const)("labels the estimate based on %s", (basis, text) => {
    show(<RebuildQuestion question={{ ...question, basis }} canAnswer onAnswer={vi.fn()} />);
    expect(screen.getByText(String(question.count)).textContent).toBe("6");
    expect(screen.getByText("18,400 tokens")).toBeTruthy();
    expect(screen.getByText(/estimate:/i).textContent).toContain(text);
    const list = screen.getByRole("list", { name: "Out-of-date summaries" });
    expect(
      within(list)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(question.ids);
  });

  it("answers both choices without moving focus when the question arrives", () => {
    const onAnswer = vi.fn();
    const input = document.createElement("input");
    document.body.append(input);
    input.focus();
    show(<RebuildQuestion question={question} canAnswer onAnswer={onAnswer} />);
    expect(document.activeElement).toBe(input);
    fireEvent.click(screen.getByRole("button", { name: "Rebuild all" }));
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(onAnswer.mock.calls).toEqual([[true], [false]]);
    input.remove();
  });

  it("explains both disabled choices for a refused browser", () => {
    const onAnswer = vi.fn();
    show(
      <RebuildQuestion question={question} canAnswer={false} onAnswer={onAnswer} />,
      makeStore("other"),
    );
    for (const name of ["Rebuild all", "Not now"]) {
      const button = screen.getByRole("button", { name });
      expect(button.getAttribute("aria-disabled")).toBe("true");
      const reason = document.getElementById(button.getAttribute("aria-describedby") ?? "");
      expect(reason?.textContent).toBe("Dana holds authoring");
      fireEvent.click(button);
    }
    expect(onAnswer).not.toHaveBeenCalled();
  });

  it("falls back to an explanation when authoring has no block reason", () => {
    show(
      <RebuildQuestion question={question} canAnswer={false} onAnswer={vi.fn()} />,
      makeStore("free"),
    );
    expect(screen.getAllByText("Only the browser holding authoring can answer")).toHaveLength(2);
  });

  it("offers enabled choices when canAnswer overrides a blocked store", () => {
    const onAnswer = vi.fn();
    show(<RebuildQuestion question={question} canAnswer onAnswer={onAnswer} />, makeStore("other"));
    const button = screen.getByRole("button", { name: "Rebuild all" });
    expect(button.getAttribute("aria-disabled")).toBeNull();
    fireEvent.click(button);
    expect(onAnswer).toHaveBeenCalledWith(true);
  });
});

describe("ApprovalNotice", () => {
  it("shows the stale count, budget explanation and estimate to readers", () => {
    show(<ApprovalNotice question={question} canAnswer={false} onAnswer={vi.fn()} />);
    expect(screen.getByText("6").textContent).toBe("6");
    expect(screen.getByText(/above the budget and waiting for approval/)).toBeTruthy();
    expect(screen.getByText("18,400 tokens")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("lets the author approve the rebuild", () => {
    const onAnswer = vi.fn();
    show(<ApprovalNotice question={question} canAnswer onAnswer={onAnswer} />);
    fireEvent.click(screen.getByRole("button", { name: "Rebuild all" }));
    expect(onAnswer.mock.calls).toEqual([[true]]);
  });
});

describe("PendingApprovalNotice", () => {
  it.each(["holder", "free"] as const)(
    "posts the answer as %s and closes on success",
    async (who) => {
      const store = makeStore(who);
      const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(Response.json(panelFixture));
      vi.stubGlobal("fetch", fetchMock);
      const view = show(<PendingApprovalNotice />, store);
      fireEvent.click(screen.getByRole("button", { name: "Rebuild all" }));
      expect(fetchMock).toHaveBeenCalledWith("/api/summaries/answer", {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ question: question.id, rebuild: true }),
      });
      await waitFor(() => expect(view.container.textContent).toBe(""));
      expect(store.get(panelLoadAtom)).toEqual({
        status: "ready",
        panel: summaryPanelSchema.parse(panelFixture),
      });
    },
  );

  it.each(["other", "read_only"] as const)("shows text only to %s browsers", (who) => {
    const fetchMock = vi.fn<typeof fetch>();
    vi.stubGlobal("fetch", fetchMock);
    show(<PendingApprovalNotice />, makeStore(who));
    expect(screen.getByText("6")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("renders nothing when no question is open", () => {
    const store = makeStore();
    store.set(panelLoadAtom, { status: "ready", panel: summaryPanelSchema.parse(panelFixture) });
    const view = show(<PendingApprovalNotice />, store);
    expect(view.container.textContent).toBe("");
  });

  it("shows answer failures and keeps the approval available for retry", async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
      Response.json(
        {
          error: "refused",
          message: "Dana holds authoring\u202E",
        },
        { status: 409 },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    const store = makeStore();
    show(<PendingApprovalNotice />, store);
    fireEvent.click(screen.getByRole("button", { name: "Rebuild all" }));
    // The server's text goes through Untrusted: the control character shows as a marker.
    expect((await screen.findByRole("alert")).textContent).toBe("Dana holds authoring⟨U+202E⟩");
    expect(screen.getByRole("button", { name: "Rebuild all" })).toBeTruthy();
    act(() =>
      store.set(panelLoadAtom, {
        status: "ready",
        panel: summaryPanelSchema.parse({ ...panelFixture, question: { ...question, id: "next" } }),
      }),
    );
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
