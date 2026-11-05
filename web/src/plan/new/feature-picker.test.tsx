import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { FeaturePicker, PickedFeature } from "@/plan/new/feature-picker";
import type { FollowUpFeature } from "@/plan/new/follow-up-features";

afterEach(cleanup);

const candidate = {
  kind: "finding" as const,
  stage: "Chat polish",
  stage_id: "chat-polish",
  text: "Settings statistics look bland",
  review: "doc/plans/REVIEW-PLAN-atlas-01.md",
};

const FEATURES: FollowUpFeature[] = [
  {
    name: "atlas",
    built: true,
    done: 2,
    total: 2,
    status: "done · 2 plans",
    candidates: [candidate],
  },
  {
    name: "beacon",
    built: false,
    done: 1,
    total: 3,
    status: "in progress · 1 of 3 plans",
    candidates: [],
  },
  { name: "comet", built: false, done: 0, total: 0, status: "no plans yet", candidates: [] },
];

function mount(features = FEATURES) {
  const onPick = vi.fn<(feature: string) => void>();
  const onClose = vi.fn();
  render(
    <div>
      <FeaturePicker features={features} onPick={onPick} onClose={onClose} />
      <button type="button">elsewhere</button>
    </div>,
  );
  const picker = screen.getByRole("listbox", { name: "Which feature?" });
  const active = () => document.getElementById(picker.getAttribute("aria-activedescendant")!);
  return { picker, active, onPick, onClose };
}

it("lists every feature with its mark and status, focused with the first option active", () => {
  const { picker, active } = mount();

  expect(document.activeElement).toBe(picker);
  expect(within(picker).getByText("Which feature is this for?")).toBeTruthy();
  const options = within(picker).getAllByRole("option");
  expect(options.map((option) => option.getAttribute("aria-label"))).toEqual([
    "atlas, done · 2 plans",
    "beacon, in progress · 1 of 3 plans",
    "comet, no plans yet",
  ]);
  expect(active()).toBe(options[0]);
  expect(options[0]!.getAttribute("aria-selected")).toBe("true");
  expect(options[0]!.querySelector(".kickoff-mark[data-built] svg")).toBeTruthy();
  const ring = options[1]!.querySelector(".kickoff-ring__fill");
  expect(ring?.getAttribute("stroke-dasharray")).toBe(`${1 / 3} 1`);
  expect(options[1]!.querySelector(".kickoff-feature__status > b")?.textContent).toBe(
    "in progress",
  );
  expect(within(picker).getByText("A done feature offers its open findings")).toBeTruthy();
});

it("puts every option directly in the listbox, with no list role between", () => {
  const { picker } = mount();

  for (const option of within(picker).getAllByRole("option")) {
    let node = option.parentElement;
    while (node !== picker) {
      expect(node).not.toBeNull();
      expect(["none", "presentation"]).toContain(node!.getAttribute("role"));
      node = node!.parentElement;
    }
  }
  expect(within(picker).queryByRole("list")).toBeNull();
});

it("moves with the arrows without wrapping, jumps with Home and End, and picks with Enter", () => {
  const { picker, active, onPick } = mount();
  const options = within(picker).getAllByRole("option");

  fireEvent.keyDown(picker, { key: "ArrowUp" });
  expect(active()).toBe(options[0]);
  fireEvent.keyDown(picker, { key: "ArrowDown" });
  fireEvent.keyDown(picker, { key: "ArrowDown" });
  fireEvent.keyDown(picker, { key: "ArrowDown" });
  expect(active()).toBe(options[2]);
  expect(options[2]!.getAttribute("aria-selected")).toBe("true");
  expect(options[0]!.getAttribute("aria-selected")).toBe("false");
  fireEvent.keyDown(picker, { key: "Home" });
  expect(active()).toBe(options[0]);
  fireEvent.keyDown(picker, { key: "End" });
  expect(active()).toBe(options[2]);
  fireEvent.keyDown(picker, { key: "ArrowUp" });
  fireEvent.keyDown(picker, { key: "Enter" });

  expect(onPick).toHaveBeenCalledExactlyOnceWith("beacon");
});

it("picks with Space and with a click", () => {
  const { picker, onPick } = mount();

  fireEvent.keyDown(picker, { key: " " });
  expect(onPick).toHaveBeenLastCalledWith("atlas");
  fireEvent.click(within(picker).getByRole("option", { name: /comet/ }));
  expect(onPick).toHaveBeenLastCalledWith("comet");
  expect(onPick).toHaveBeenCalledTimes(2);
});

it("closes on Escape and on a press outside, and picks nothing", () => {
  const { picker, onClose, onPick } = mount();

  fireEvent.keyDown(picker, { key: "Escape" });
  expect(onClose).toHaveBeenCalledOnce();
  fireEvent.pointerDown(within(picker).getAllByRole("option")[1]!);
  expect(onClose).toHaveBeenCalledOnce();
  fireEvent.pointerDown(screen.getByRole("button", { name: "elsewhere" }));
  expect(onClose).toHaveBeenCalledTimes(2);
  expect(onPick).not.toHaveBeenCalled();
});

it("says when no feature has a metaplan and lets nothing be picked", () => {
  const { picker, onPick, onClose } = mount([]);

  expect(within(picker).getByText("No feature has a metaplan yet.")).toBeTruthy();
  expect(within(picker).queryAllByRole("option")).toEqual([]);
  expect(picker.hasAttribute("aria-activedescendant")).toBe(false);
  fireEvent.keyDown(picker, { key: "ArrowDown" });
  fireEvent.keyDown(picker, { key: "Enter" });
  expect(onPick).not.toHaveBeenCalled();
  fireEvent.keyDown(picker, { key: "Escape" });
  expect(onClose).toHaveBeenCalledOnce();
});

it("the picked feature reads as its slug, status and a Change button", () => {
  const onChange = vi.fn();
  render(<PickedFeature feature={FEATURES[0]!} onChange={onChange} />);

  const chip = document.querySelector(".kickoff-picked")!;
  expect(chip.querySelector(".kickoff-feature__id")?.textContent).toBe("atlas");
  expect(chip.querySelector(".kickoff-feature__status")?.textContent).toBe("done · 2 plans");
  expect(chip.querySelector(".kickoff-mark[data-built]")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Change" }));
  expect(onChange).toHaveBeenCalledOnce();
});
