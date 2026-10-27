import type { ReactFlowProps } from "@xyflow/react";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import planFixture from "@/plan/api/fixtures/files-doc-plan.json";
import { docViewSchema } from "@/plan/api/schema-files";
import { StageGraph } from "@/plan/files/plan/stage-graph";

// The real React Flow renders; the wrapper only records the props the graph hands it.
const rendered = vi.hoisted(() => ({ props: [] as ReactFlowProps[] }));
vi.mock("@xyflow/react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@xyflow/react")>();
  return {
    ...actual,
    ReactFlow: (props: ReactFlowProps) => {
      rendered.props.push(props);
      return <actual.ReactFlow {...props} />;
    },
  };
});

afterEach(() => {
  cleanup();
  rendered.props.length = 0;
});

const stages = docViewSchema.parse(planFixture).plan!.stages;

describe("plan stage graph on a touch screen", () => {
  it("does not pan on drag, so a finger on the graph scrolls the reading column", () => {
    render(<StageGraph stages={stages} onSelect={vi.fn()} />);
    expect(rendered.props.length).toBeGreaterThan(0);
    for (const props of rendered.props) expect(props.panOnDrag).toBe(false);
  });

  it("lets the pane scroll vertically and pinch, against React Flow's touch-action none", () => {
    render(<StageGraph stages={stages} onSelect={vi.fn()} />);
    const wrapper = screen.getByTestId("stage-graph");
    // The backslashes keep both underscores of the class name out of Tailwind's `_` to space.
    expect(wrapper.className).toContain(
      String.raw`[&_.react-flow\_\_pane]:[touch-action:pan-y_pinch-zoom]!`,
    );
  });

  it("still reports a clicked node", () => {
    const onSelect = vi.fn();
    render(<StageGraph stages={stages} onSelect={onSelect} />);
    fireEvent.click(within(screen.getByTestId("stage-graph")).getByText("Review"));
    expect(onSelect).toHaveBeenCalledWith("review");
  });
});
