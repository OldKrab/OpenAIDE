import { describe, expect, it } from "vitest";
import { initialTaskNavigationRowsPerProject, taskNavigationPageSize } from "./taskNavigationPolicy";

describe("Task Navigation presentation budget", () => {
  it("allocates 20 rows for one Project, 7 for two, and 5 for three or more", () => {
    expect(initialTaskNavigationRowsPerProject(1)).toBe(20);
    expect(initialTaskNavigationRowsPerProject(2)).toBe(7);
    expect(initialTaskNavigationRowsPerProject(5)).toBe(5);
  });

  it("reveals seven more rows when there are at least three Projects", () => {
    expect(taskNavigationPageSize(1)).toBe(10);
    expect(taskNavigationPageSize(2)).toBe(10);
    expect(taskNavigationPageSize(3)).toBe(7);
    expect(taskNavigationPageSize(5)).toBe(7);
  });
});
