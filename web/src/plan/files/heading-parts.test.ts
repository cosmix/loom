import { expect, it } from "vitest";

import { pressureWhen } from "./heading-parts";

const NOW = new Date(2026, 10, 7, 9, 30);
const DAY_MONTH = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const DAY_MONTH_YEAR = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });

it("words a test from today as today, by the local calendar day", () => {
  expect(pressureWhen("2026-11-07", NOW)).toBe("today");
  expect(pressureWhen("2026-11-06", NOW)).not.toBe("today");
});

it("words a test from this year as its day and month, and from another with its year", () => {
  expect(pressureWhen("2026-11-05", NOW)).toBe(DAY_MONTH.format(new Date(2026, 10, 5)));
  expect(pressureWhen("2025-11-05", NOW)).toBe(DAY_MONTH_YEAR.format(new Date(2025, 10, 5)));
  expect(pressureWhen("2025-11-05", NOW)).toContain("2025");
});

it("shows a date in any other shape as sent", () => {
  expect(pressureWhen("2026-11-05T10:00:00Z", NOW)).toBe("2026-11-05T10:00:00Z");
  expect(pressureWhen("soon", NOW)).toBe("soon");
});
