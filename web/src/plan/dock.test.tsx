import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Dock } from "@/plan/dock";

const registrations = vi.hoisted(() => vi.fn());

vi.mock("@/plan/chat/panel", () => ({
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: (props: { connect?: boolean; panel?: boolean }) => {
    registrations(props);
    return null;
  },
}));

afterEach(() => registrations.mockClear());

function propsAt(pathname: string): { connect?: boolean; panel?: boolean } | undefined {
  render(
    <MemoryRouter initialEntries={[pathname]}>
      <Dock />
    </MemoryRouter>,
  );
  return registrations.mock.calls.at(-1)?.[0];
}

const connectAt = (pathname: string) => propsAt(pathname)?.connect;

describe("Dock chat connection", () => {
  it("owns the socket on the home route, where no panel mounts", () => {
    expect(connectAt("/")).toBe(true);
  });

  it("leaves the kickoff to its own socket", () => {
    expect(connectAt("/new")).toBe(false);
  });

  it("leaves the socket to the panel on a feature route", () => {
    expect(connectAt("/f/demo/product")).toBe(false);
  });

  it("reports whether a panel mounts, which the c key needs a composer for", () => {
    expect(propsAt("/")?.panel).toBe(false);
    expect(propsAt("/new")?.panel).toBe(false);
    expect(propsAt("/f/demo/product")?.panel).toBe(true);
  });
});
