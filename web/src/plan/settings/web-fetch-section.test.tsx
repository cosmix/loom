import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import type { FetchPolicy } from "@/plan/api/schema-web-fetch";
import { authoringAtom, youAtom } from "@/plan/state/atoms";
import { WebFetchGroup } from "./web-fetch-section";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

type Saved = Omit<FetchPolicy, "revision">;

/** Another tab changes the policy: the server's revision moves on. */
let changeElsewhere: (next: Saved) => void = () => {};

/**
 * A server holding `saved`: GET returns it with its revision, POST lowercases the domains,
 * refuses `bad`, and answers 409 for a revision other than the current one.
 */
function stubServer(saved: Saved) {
  const posts: FetchPolicy[] = [];
  let current: FetchPolicy = { ...saved, revision: 0 };
  changeElsewhere = (next) => {
    current = { ...next, revision: current.revision + 1 };
  };
  const fetch = vi.fn(async (_path: string, init?: RequestInit) => {
    if (init?.method === "POST") {
      const body = JSON.parse(String(init.body)) as FetchPolicy;
      posts.push(body);
      if (body.revision !== current.revision) {
        const stale = { error: "stale_policy", message: "changed since they were loaded" };
        return new Response(JSON.stringify(stale), { status: 409 });
      }
      if (body.domains.includes("bad")) {
        const refusal = { error: "invalid_domain", message: '"bad" needs at least two labels' };
        return new Response(JSON.stringify(refusal), { status: 400 });
      }
      const domains = body.domains.map((domain) => domain.toLowerCase());
      current = { ...body, domains, revision: current.revision + 1 };
    }
    return new Response(JSON.stringify(current), { status: 200 });
  });
  vi.stubGlobal("fetch", fetch);
  return posts;
}

function renderGroup(holder = true) {
  const store = createStore();
  store.set(youAtom, { public_id: holder ? "holder" : "reader", name: "You" });
  store.set(authoringAtom, {
    holder: { public_id: "holder", name: "Dana", away_since: null, frees_at: null },
  });
  render(
    <Provider store={store}>
      <WebFetchGroup />
    </Provider>,
  );
}

const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;

it("shows the four modes with the saved one chosen", async () => {
  stubServer({ mode: "always", domains: [] });
  renderGroup();

  expect(screen.getByRole("heading", { name: "Web fetches" })).toBeTruthy();
  const group = await screen.findByRole("radiogroup", {
    name: "When a session fetches a web page",
  });
  const names = within(group)
    .getAllByRole("radio")
    .map((input) => input.getAttribute("value"));
  expect(names).toEqual(["ask", "listed", "always", "never"]);
  expect(radio("Always allow").checked).toBe(true);
  expect(radio("Never").getAttribute("aria-describedby")).toBeTruthy();
});

it("the holder picks a mode and edits the list, each change saved on the server", async () => {
  const posts = stubServer({ mode: "ask", domains: [] });
  renderGroup();

  fireEvent.click(await screen.findByRole("radio", { name: "Allow listed domains" }));
  await screen.findByText("No domains listed yet");
  expect(posts.at(-1)).toEqual({ mode: "listed", domains: [], revision: 0 });
  fireEvent.change(screen.getByRole("textbox", { name: "Add a domain" }), {
    target: { value: " Docs.RS " },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  const list = await screen.findByRole("list", { name: "Listed domains" });
  expect(posts.at(-1)).toEqual({ mode: "listed", domains: ["Docs.RS"], revision: 1 });
  expect(within(list).getByText("docs.rs")).toBeTruthy();
  expect((screen.getByRole("textbox", { name: "Add a domain" }) as HTMLInputElement).value).toBe(
    "",
  );
  fireEvent.click(screen.getByRole("button", { name: "Remove docs.rs" }));
  await screen.findByText("No domains listed yet");
  expect(posts.at(-1)).toEqual({ mode: "listed", domains: [], revision: 2 });
});

it("a save built on a stale read is refused, the policy reloaded and the change not retried", async () => {
  const posts = stubServer({ mode: "listed", domains: ["docs.rs"] });
  renderGroup();

  await screen.findByRole("list", { name: "Listed domains" });
  changeElsewhere({ mode: "never", domains: ["docs.rs", "api.example.com"] });
  fireEvent.change(screen.getByRole("textbox", { name: "Add a domain" }), {
    target: { value: "crates.io" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("changed since"));
  expect(posts).toHaveLength(1);
  expect(radio("Never").checked).toBe(true);
  expect(screen.queryByText("crates.io")).toBeNull();
  fireEvent.click(radio("Allow listed domains"));
  await waitFor(() => expect(radio("Allow listed domains").checked).toBe(true));
  expect(posts.at(-1)).toEqual({
    mode: "listed",
    domains: ["docs.rs", "api.example.com"],
    revision: 1,
  });
});

it("a refused entry shows the server's reason and keeps what was typed", async () => {
  stubServer({ mode: "listed", domains: ["docs.rs"] });
  renderGroup();

  const input = await screen.findByRole("textbox", { name: "Add a domain" });
  fireEvent.change(input, { target: { value: "bad" } });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("two labels"));
  expect((input as HTMLInputElement).value).toBe("bad");
  expect(screen.getByText("docs.rs")).toBeTruthy();
});

it("another browser reads the policy but cannot change it", async () => {
  const posts = stubServer({ mode: "listed", domains: ["docs.rs"] });
  renderGroup(false);

  await screen.findByRole("radio", { name: "Allow listed domains" });
  expect(radio("Allow listed domains").checked).toBe(true);
  expect(screen.getByText("Only the browser holding authoring changes these")).toBeTruthy();
  for (const input of screen.getAllByRole("radio")) {
    expect((input as HTMLInputElement).disabled).toBe(true);
  }
  expect(screen.getByText("docs.rs")).toBeTruthy();
  expect(screen.queryByRole("textbox", { name: "Add a domain" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Remove docs.rs" })).toBeNull();
  expect(posts).toEqual([]);
});
