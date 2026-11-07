import { act, fireEvent, screen } from "@testing-library/react";
import { expect, it } from "vitest";

import type { FetchView } from "@/plan/api/schema-chat";
import { harness, permission, useFakeClock } from "./permission-card-test-kit";

useFakeClock();

const docs: FetchView = {
  url: "https://docs.rs/serde",
  host: "docs.rs",
  listable: true,
  matched: null,
  rule: null,
};

function fetchCard(fetch: FetchView = docs, status: "pending" | "allowed" | "denied" = "pending") {
  return permission({
    tool: "WebFetch",
    input: JSON.stringify({ url: fetch.url, prompt: "read it" }),
    status,
    fetch,
  });
}

it("a fetch card offers the session grant and the host beside Allow and Deny", () => {
  harness([fetchCard()]).show();

  for (const name of ["Allow", "Deny", "Allow this session", "Always allow docs.rs"]) {
    expect(screen.getByRole("button", { name })).toBeTruthy();
  }
  const listed = screen.getByRole("button", { name: "Always allow docs.rs" });
  expect(listed.getAttribute("data-look")).toBe("secondary");
  const explained = document.getElementById(listed.getAttribute("aria-describedby") ?? "");
  expect(explained?.textContent).toContain("lists docs.rs and its subdomains in Settings");
});

it("a fetch without a listable host offers only the session grant", () => {
  const unlistable: FetchView[] = [
    { ...docs, url: "https://127.0.0.1/", host: null, listable: false },
    // The server keeps the name for subdomain matching, but the list cannot hold it.
    { ...docs, url: "http://localhost:8080/", host: "localhost", listable: false },
  ];
  for (const fetch of unlistable) {
    const { unmount } = harness([fetchCard(fetch)]).show();
    expect(screen.getByRole("button", { name: "Allow this session" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Always allow/ })).toBeNull();
    unmount();
  }
});

it("another tool's card has no fetch choices", () => {
  harness().show();

  expect(screen.queryByRole("button", { name: "Allow this session" })).toBeNull();
  expect(screen.queryByRole("button", { name: /^Always allow/ })).toBeNull();
});

it("the choices send their commands by request id", async () => {
  const h = harness([fetchCard()]);
  h.show();

  fireEvent.click(screen.getByRole("button", { name: "Allow this session" }));
  expect(h.send).toHaveBeenLastCalledWith(
    expect.objectContaining({
      type: "chat",
      command: "allow_session_fetches",
      request_id: "request-1",
    }),
  );
  await act(async () => h.reply());
  fireEvent.click(screen.getByRole("button", { name: "Always allow docs.rs" }));
  expect(h.send).toHaveBeenLastCalledWith(
    expect.objectContaining({ type: "chat", command: "allow_fetch_host", request_id: "request-1" }),
  );
  await act(async () => h.reply("the web fetch settings could not be saved"));
  expect(screen.getByRole("alert").textContent).toContain("could not be saved");
});

it("an observer sees no fetch choices", () => {
  harness([fetchCard()], true).show();

  expect(screen.queryByRole("button", { name: "Allow this session" })).toBeNull();
  expect(screen.getByText("Waiting for Dana to decide")).toBeTruthy();
});

it("a fetch a rule answered is one quiet line naming the URL and the rule", () => {
  const lines: [FetchView, "allowed" | "denied", string][] = [
    [
      { ...docs, rule: "listed", matched: "docs.rs" },
      "allowed",
      "Fetch allowed: https://docs.rs/serde · docs.rs is listed in Settings",
    ],
    [
      {
        ...docs,
        url: "https://api.docs.rs/v1",
        host: "api.docs.rs",
        rule: "listed",
        matched: "docs.rs",
      },
      "allowed",
      "Fetch allowed: https://api.docs.rs/v1 · docs.rs is listed in Settings",
    ],
    [
      { ...docs, rule: "always" },
      "allowed",
      "Fetch allowed: https://docs.rs/serde · Settings allow every fetch",
    ],
    [
      { ...docs, rule: "session" },
      "allowed",
      "Fetch allowed: https://docs.rs/serde · allowed for this session",
    ],
    [
      { ...docs, rule: "never" },
      "denied",
      "Fetch refused: https://docs.rs/serde · Web fetches are turned off in Settings.",
    ],
  ];
  for (const [fetch, status, text] of lines) {
    const item = fetchCard(fetch, status);
    const { unmount } = harness([item]).show(item);
    const row = screen.getByRole("group", { name: "Permission: WebFetch" });
    expect(row.textContent).toBe(text);
    expect(screen.queryByRole("button")).toBeNull();
    unmount();
  }
});
