import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { TokenFlowPanel } from "@/components/token-flow-panel";
import { applySnapshot } from "@/state/apply";

function renderPanel(mutate?: (raw: typeof fixture) => void) {
  const raw = structuredClone(fixture);
  mutate?.(raw);
  const store = createStore();
  applySnapshot(store, snapshotSchema.parse(raw));
  return render(
    <Provider store={store}>
      <TokenFlowPanel />
    </Provider>,
  );
}

afterEach(cleanup);

describe("TokenModels", () => {
  it("spaces model hues evenly without cycling after six models", () => {
    renderPanel((raw) => {
      raw.status.tokens.models = Array.from({ length: 7 }, (_, index) => ({
        model: `model-${index}`,
        effort: "high",
        total: { input: 10, cache_write: 0, cache_read: 0, output: 0 },
      }));
    });
    const pie = screen.getByRole("img", { name: "fresh tokens by model" });
    const colors = Array.from(pie.querySelectorAll("[data-model]"), (slice) =>
      slice.getAttribute("stroke")!,
    );
    expect(new Set(colors).size).toBe(7);
    const hues = colors.map((color) => Number(color.split(" ")[2]?.replace(")", "")));
    for (let index = 0; index < hues.length; index++) {
      const gap = (hues[(index + 1) % hues.length]! - hues[index]! + 360) % 360;
      expect(gap).toBeCloseTo(360 / 7);
    }
    const swatches = screen.getByRole("list", { name: "models" }).querySelectorAll("[aria-hidden]");
    swatches.forEach((swatch, index) => {
      const expected = document.createElement("span");
      expected.style.background = colors[index]!;
      expect((swatch as HTMLElement).style.background).toBe(expected.style.background);
    });
  });

  it("keeps model colours when token shares change order", () => {
    const colors = () =>
      Object.fromEntries(
        Array.from(
          screen
            .getByRole("img", { name: "fresh tokens by model" })
            .querySelectorAll("[data-model]"),
          (slice) => [slice.getAttribute("data-model"), slice.getAttribute("stroke")],
        ),
      );
    renderPanel();
    const before = colors();
    cleanup();
    renderPanel((raw) => {
      raw.status.tokens.models.find((entry) => entry.model === "gpt-6.1-sol")!.total.input =
        99999999;
    });
    expect(colors()).toEqual(before);
  });

  it("draws one pie slice per model in share order", () => {
    renderPanel();

    const pie = screen.getByRole("img", { name: "fresh tokens by model" });
    const slices = Array.from(pie.querySelectorAll("[data-model]"));
    expect(slices.map((slice) => slice.getAttribute("data-model"))).toEqual([
      "claude-sonnet-5-5",
      "claude-opus-5-5",
      "gpt-6.1-sol",
    ]);
    expect(pie.querySelectorAll("text")).toHaveLength(0);
  });

  it("lists each model's whole-percent share in the legend", () => {
    renderPanel();

    const items = within(screen.getByRole("list", { name: "models" })).getAllByRole("listitem");
    expect(items.map((item) => item.textContent)).toEqual([
      expect.stringContaining("50%"),
      expect.stringContaining("33%"),
      expect.stringContaining("17%"),
    ]);
  });

  it("makes the legend shares sum to 100 for equal models", () => {
    renderPanel((raw) => {
      raw.status.tokens.models = ["a", "b", "c"].map((model) => ({
        model,
        effort: "high",
        total: { input: 10, cache_write: 0, cache_read: 0, output: 0 },
      }));
    });

    const items = within(screen.getByRole("list", { name: "models" })).getAllByRole("listitem");
    const percents = items.map((item) => Number(/(\d+)%/.exec(item.textContent ?? "")?.[1]));
    expect(percents).toEqual([34, 33, 33]);
  });

  it("tables the totals per effort, the model cell spanning its efforts", () => {
    renderPanel();

    const table = screen.getByRole("table", { name: "token totals by model and effort" });
    const rows = within(table).getAllByRole("row");
    expect(rows).toHaveLength(5);
    const efforts = rows.slice(1).map((row) => {
      const cells = within(row).getAllByRole("cell");
      return cells[cells.length - 3]?.textContent;
    });
    expect(efforts).toEqual(["medium", "xhigh", "high", "xhigh"]);
    const opus = within(table).getByText("claude-opus-5-5");
    expect(opus.getAttribute("rowspan")).toBe("2");
  });

  it("renders no block without models", () => {
    renderPanel((raw) => {
      raw.status.tokens.models = [];
    });

    expect(screen.queryByRole("table", { name: "token totals by model and effort" })).toBeNull();
    expect(screen.queryByRole("list", { name: "models" })).toBeNull();
  });
});
