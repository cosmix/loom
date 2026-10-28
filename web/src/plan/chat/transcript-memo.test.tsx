import { cleanup } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { item, mount, turn } from "./transcript-test-kit";

const footers = vi.hoisted(() => ({ seen: [] as string[] }));

vi.mock("./turn-footer", async (original) => {
  const module = await original<typeof import("./turn-footer")>();
  return {
    ...module,
    // The real footer is itself memoised, so count the transcript turn that renders it.
    TurnFooter: (props: ComponentProps<typeof module.TurnFooter>) => {
      footers.seen.push(props.turn.id);
      return <module.TurnFooter {...props} />;
    },
  };
});

afterEach(() => {
  cleanup();
  footers.seen.length = 0;
});

it("a delta re-renders only the turn it changed", () => {
  const at = (index: number) => new Date(index * 60000).toISOString();
  const turns = [1, 2, 3].map((index) =>
    turn([item("text", { id: `text-${index}`, markdown: `Reply ${index}` })], {
      id: `t${index}`,
      started_at: at(index),
      open: false,
    }),
  );
  const h = mount([], { turns });
  expect(new Set(footers.seen)).toEqual(new Set(["t1", "t2", "t3"]));
  footers.seen.length = 0;
  const changed = {
    ...turns[2]!,
    items: [...turns[2]!.items, item("text", { id: "more", markdown: "More" })],
  };
  h.update([turns[0]!, turns[1]!, changed]);
  expect(footers.seen).toEqual(["t3"]);
});
