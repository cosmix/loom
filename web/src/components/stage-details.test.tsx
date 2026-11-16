import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { StageDetails } from "@/components/stage-details";

afterEach(cleanup);

const stage = snapshotSchema.parse(fixtureJson).status.stages.find(({ id }) => id === "client");
if (!stage) throw new Error("fixture stage client is missing");

it("shows the stage model followed by its effort in the folded facts", () => {
  render(<StageDetails stage={{ ...stage, model: "sonnet", effort: "high" }} level={0} />);

  const label = screen.getByText("model", { exact: true });
  expect(within(label.parentElement as HTMLElement).getByText("sonnet/high")).toBeTruthy();
});

it("shows the model alone when the effort is unknown", () => {
  render(<StageDetails stage={{ ...stage, model: "sonnet", effort: "" }} level={0} />);

  expect(screen.getByText("sonnet", { exact: true })).toBeTruthy();
});
