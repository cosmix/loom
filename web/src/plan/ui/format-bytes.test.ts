import { expect, it } from "vitest";

import { formatBytes } from "./format-bytes";

it("reads sizes in binary units", () => {
  expect(formatBytes(900)).toBe("900 B");
  expect(formatBytes(1234)).toBe("1.2 KiB");
  expect(formatBytes(3 * 1024 * 1024)).toBe("3.0 MiB");
  expect(formatBytes(10 * 1024 * 1024)).toBe("10.0 MiB");
});
